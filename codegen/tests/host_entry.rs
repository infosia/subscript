//! The test-host source rule from compiler.md §100.2.

use std::path::{Path, PathBuf};

#[path = "../src/host_source.rs"]
mod host_source;

#[derive(Debug)]
struct Token<'a> {
    text: &'a str,
    offset: usize,
    string: bool,
}

// Lex strings and comments before punctuation, so C braces do not affect Rust scopes.
fn tokens(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if source[i..].starts_with("//") {
            i += source[i..].find('\n').unwrap_or(bytes.len() - i);
            continue;
        }
        if source[i..].starts_with("/*") {
            i += 2;
            let mut depth = 1;
            while depth > 0 && i < bytes.len() {
                if source[i..].starts_with("/*") {
                    depth += 1;
                    i += 2;
                } else if source[i..].starts_with("*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
            }
            continue;
        }
        let mut quote = i;
        if bytes[quote] == b'b' || bytes[quote] == b'c' {
            quote += 1;
        }
        let raw = bytes.get(quote) == Some(&b'r');
        if raw {
            quote += 1;
            while bytes.get(quote) == Some(&b'#') {
                quote += 1;
            }
        }
        let string = bytes.get(quote) == Some(&b'"');
        if string {
            if raw {
                let hashes = source[start..quote].chars().filter(|c| *c == '#').count();
                let close = format!("\"{}", "#".repeat(hashes));
                i = quote + 1;
                i += source[i..].find(&close).expect("closed raw string") + close.len();
            } else {
                i = quote + 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
        } else if bytes[i] == b'\'' {
            // A character literal closes after one character or one escape.
            let tail = &source[i + 1..];
            let end = if tail.starts_with('\\') {
                tail.find('\'').map(|n| i + 1 + n)
            } else {
                tail.chars().next().map(|c| i + 1 + c.len_utf8())
            };
            i = match end.filter(|end| bytes.get(*end) == Some(&b'\'')) {
                Some(end) => end + 1,
                None => i + 1, // A lifetime, not a character literal.
            };
        } else if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        } else {
            i += source[i..].chars().next().unwrap().len_utf8();
        }
        result.push(Token {
            text: &source[start..i],
            offset: start,
            string,
        });
    }
    result
}

fn string_value(literal: &str) -> String {
    let quote = literal.find('"').expect("string token");
    if literal[..quote].contains('r') {
        let hashes = literal[..quote].chars().filter(|c| *c == '#').count();
        return literal[quote + 1..literal.len() - hashes - 1].to_owned();
    }
    let mut chars = literal[quote + 1..literal.len() - 1].chars().peekable();
    let mut value = String::new();
    while let Some(c) = chars.next() {
        if c != '\\' {
            value.push(c);
            continue;
        }
        match chars.next().expect("escape") {
            'n' => value.push('\n'),
            'r' => value.push('\r'),
            't' => value.push('\t'),
            '0' => value.push('\0'),
            'x' => {
                let hex: String = chars.by_ref().take(2).collect();
                value.push(char::from(
                    u8::from_str_radix(&hex, 16).expect("hex escape"),
                ));
            }
            'u' => {
                assert_eq!(chars.next(), Some('{'));
                let hex: String = chars
                    .by_ref()
                    .take_while(|c| *c != '}')
                    .filter(|c| *c != '_')
                    .collect();
                value.push(
                    char::from_u32(u32::from_str_radix(&hex, 16).expect("Unicode escape"))
                        .expect("Unicode scalar"),
                );
            }
            '\n' | '\r' => {
                while chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                    chars.next();
                }
            }
            c => value.push(c),
        }
    }
    value
}

fn violations(file: &str, source: &str) -> Vec<String> {
    let tokens = tokens(source);
    let main = ["int ", "main(void)"].concat();
    let mut errors = Vec::new();
    let mut owner_end = 0;
    for (index, token) in tokens.iter().enumerate() {
        // Exempt only the production entry and the shared helper's implementation.
        if file == "codegen/src/ship.rs"
            && matches!(token.text, "fn" | "const")
            && tokens.get(index + 1).is_some_and(|next| {
                (token.text == "fn" && next.text == "host_entry")
                    || (token.text == "const" && next.text == "AOT_ENTRY_C")
            })
        {
            let mut depth = 0;
            let mut opened = false;
            for (end, next) in tokens.iter().enumerate().skip(index + 2) {
                if !next.string {
                    match next.text {
                        "{" => {
                            depth += 1;
                            opened = true;
                        }
                        "}" => {
                            depth -= 1;
                        }
                        ";" if token.text == "const" && depth == 0 => {
                            owner_end = end + 1;
                            break;
                        }
                        _ => {}
                    }
                    if token.text == "fn" && opened && depth == 0 {
                        owner_end = end + 1;
                        break;
                    }
                }
            }
        }
        if !token.string
            || index < owner_end
            || host_source::main_body_start(&string_value(token.text)).is_none()
        {
            continue;
        }
        // §100.2 covers byte-exact sink comparisons. This one layout probe uses
        // lines(), not sink bytes, and §11c excludes it on windows-msvc.
        // Exempt its single entry fragment, not other bodies in the same file.
        if file == "codegen/tests/offsetof_layout.rs" && token.text == format!("\"{main} {{\\n\"") {
            continue;
        }
        // This literal is a Rust fake compiler driver, compiled by rustc.
        // Exempt only that source literal, not other bodies in the resolver test.
        if file == "codegen/clang_resolver.rs"
            && string_value(token.text)
                .starts_with("\nuse std::ffi::OsStr;\nuse std::path::Path;\n\nfn main()")
        {
            continue;
        }
        // Require the body at the helper boundary. A helper call elsewhere in
        // the file cannot authorize this literal, including an aliased raw body.
        let wrapped =
            index >= 2 && tokens[index - 1].text == "(" && tokens[index - 2].text == "host_entry";
        if !wrapped {
            let line = source[..token.offset]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                + 1;
            errors.push(format!("{file}:{line}: §100.2: pass the C host body directly to host_entry before compilation"));
        }
    }
    errors
}

fn sources(directory: &Path, paths: &mut Vec<PathBuf>) {
    // Include tracked and new source files. Exclude ignored build products and scratch checkouts.
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
        ])
        .output()
        .expect("workspace Rust source inventory");
    assert!(
        output.status.success(),
        "workspace Rust source inventory failed"
    );
    paths.extend(
        output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| directory.join(std::str::from_utf8(path).expect("Rust source path"))),
    );
}

#[test]
fn all_test_host_bodies_use_the_helper_and_bypass_is_rejected() {
    let body = format!("int {} {{ return 0; }}", "main(void)");
    let raw = format!("let host = r#\"{body}\"#;\nstd::fs::write(\"host.c\", host).unwrap();\nhost_c_compiler().unwrap().command().arg(\"host.c\").output().unwrap();");
    let errors = violations("bypass.rs", &raw);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("bypass.rs:1: §100.2:"));
    let wrapped = format!("let host = host_entry(r#\"{body}\"#);");
    assert!(violations("wrapped.rs", &wrapped).is_empty());
    let mixed = format!("{wrapped}\n{raw}\n{raw}");
    assert_eq!(violations("mixed.rs", &mixed).len(), 2);
    let escaped = format!("\"{}\"", body.replace('m', "\\u{6d}"));
    assert_eq!(violations("escaped.rs", &escaped).len(), 1);
    let comments = format!("/* outer /* {raw} */ {raw} */\n// {body}\n{wrapped}");
    assert!(violations("comments.rs", &comments).is_empty());

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut paths = Vec::new();
    sources(root, &mut paths);
    paths.sort();
    paths.dedup();
    let errors: Vec<_> = paths
        .iter()
        .flat_map(|path| {
            let file = subscript_compiler::repository_relative(root, path).unwrap();
            violations(&file, &std::fs::read_to_string(path).expect("Rust source"))
        })
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn host_entry_owns_the_guard_and_rejects_invalid_bodies() {
    use subscript_codegen::host_entry;
    assert!(host_entry("void run(void) {}").is_err());
    let body = format!("int {} {{ _setmode(0, 0); }}", "main(void)");
    assert!(host_entry(&body).is_err());
    let body = format!("int {} {{ return 0; }}", "main(void)");
    let host = host_entry(&body).unwrap();
    assert!(host.starts_with(subscript_codegen::HOST_HEADER_C));
    assert!(host.contains(
        "#ifdef _WIN32\n#include <stdio.h>\n#include <fcntl.h>\n#include <io.h>\n#endif"
    ));
    assert!(host.contains("#ifdef _WIN32\n    (void)_setmode(_fileno(stdout), _O_BINARY);\n#endif"));
    assert_eq!(host.matches("_setmode").count(), 1);
    assert!(host.ends_with(" return 0; }"));
}

#[test]
fn c_definitions_share_recognition_and_compile() {
    use subscript_codegen::{add_c11_optimized_flags, host_c_compiler, host_entry};
    let directory = Scratch::new();
    let compiler = host_c_compiler().expect("C compiler");
    for signature in [
        "int  main(void)",
        "int main( void )",
        "int\nmain\t(\nvoid\n)",
        "int /* type */ main(/* argument */ void)",
        "signed int main(int argc, char **argv)",
        "int ((main))(void)",
        "int (main(void))",
        "typedef int result; result main(void)",
        "int main(argc, argv) int argc; char **argv;",
        "int (main(argc, argv)) int argc; char **argv;",
        "int ma\\\nin(void)",
    ] {
        let body = format!("{signature} {{ return 0; }}");
        let raw = format!("let body = r#\"{body}\"#;");
        assert_eq!(violations("definition.rs", &raw).len(), 1, "{signature}");
        let wrapped = format!("let body = host_entry(r#\"{body}\"#);");
        assert!(
            violations("definition.rs", &wrapped).is_empty(),
            "{signature}"
        );
        let host = host_entry(&body).expect("recognized definition");
        assert_eq!(host.matches("_setmode").count(), 1);
        // Compile both the bypass and the helper result with the selected host compiler.
        for source in [&body, &host] {
            let path = directory.0.join("host.c");
            std::fs::write(&path, source).unwrap();
            let mut command = compiler.command();
            add_c11_optimized_flags(&mut command, compiler.style());
            command
                .arg(if compiler.style().is_msvc() {
                    "/Zs"
                } else {
                    "-fsyntax-only"
                })
                .arg(&path);
            let output = command.output().expect("compile main spelling");
            assert!(
                output.status.success(),
                "{signature}: {}",
                subscript_codegen::tool_output_report(&output)
            );
        }
    }
    let signature = ["int ", "main(void)"].concat();
    for body in [
        format!("{signature};"),
        format!("// {signature} {{ return 0; }}"),
        format!("/* {signature} {{ return 0; }} */"),
        format!("const char *s = \"{signature} {{ return 0; }}\";"),
        "int domain(void) { return 0; }".to_owned(),
    ] {
        assert!(host_entry(&body).is_err(), "{body}");
        let raw = format!("let body = r#\"{body}\"#;");
        assert!(violations("non-definition.rs", &raw).is_empty(), "{body}");
    }
}

#[test]
fn c_punctuators_and_attributes_preserve_the_insertion_offset() {
    for (signature, open, close) in [
        ("int main(void)", "<%", "%>"),
        ("int main(void)", "??<", "??>"),
        ("int ma??/\nin(void)", "{", "}"),
        ("int main(void) __attribute__((unused))", "{", "}"),
    ] {
        let prefix = format!("{signature} {open}");
        let body = format!("{prefix} return 0; {close}");
        assert_eq!(host_source::main_body_start(&body), Some(prefix.len()));
        let host = subscript_codegen::host_entry(&body).unwrap();
        assert!(host.contains(&format!("{prefix}\n#ifdef _WIN32")));
        assert_eq!(
            violations("punctuator.rs", &format!("r#\"{body}\"#")).len(),
            1
        );
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "subscript-host-entry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove scratch source tree");
    }
}

#[test]
fn every_new_source_directory_reports_an_injected_body_in_a_scratch_copy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut paths = Vec::new();
    sources(root, &mut paths);
    paths.sort();
    let mut directories = std::collections::BTreeSet::new();
    let scratch = Scratch::new();
    let mut found = Vec::new();
    for path in paths {
        let relative = path.strip_prefix(root).unwrap();
        if relative.starts_with("codegen/src")
            || relative.starts_with("codegen/tests")
            || !directories.insert(relative.parent().unwrap().to_owned())
        {
            continue;
        }
        let copy = scratch.0.join(relative);
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::copy(&path, &copy).unwrap();
        let file = subscript_compiler::repository_relative(root, &path).unwrap();
        let mut source = std::fs::read_to_string(&copy).unwrap();
        assert!(violations(&file, &source).is_empty(), "{file}");
        for signature in ["int  main(void)", "int main( void )"] {
            source.push_str(&format!(
                "\nfn injected() {{ let body = r#\"{signature} {{ return 0; }}\"#; }}\n"
            ));
        }
        std::fs::write(&copy, source).unwrap();
        let errors = violations(&file, &std::fs::read_to_string(copy).unwrap());
        assert_eq!(errors.len(), 2, "{file}: {errors:?}");
        found.extend(errors);
    }
    for required in [
        "benchmarks/src/bin",
        "examples/tests",
        "compiler/src",
        "runtime/src",
        "bindgen/src",
        "cli/src",
        "spike/mobile-link/src",
        "codegen",
    ] {
        assert!(
            directories.contains(Path::new(required)),
            "unread source directory: {required}"
        );
    }
    println!(
        "{} scratch directories reported {} injected host bodies:\n{}",
        directories.len(),
        found.len(),
        found.join("\n")
    );
}
