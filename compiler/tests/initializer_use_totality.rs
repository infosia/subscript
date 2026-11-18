//! compiler.md §332 rule 5: a new HIR use kind requires an explicit arm.
//! Cost: three small metadata-only rustc calls; no dependency build or tier compile.

use std::{collections::BTreeSet, fs, path::PathBuf, process::Command};

struct Token {
    text: String,
    children: Vec<Token>,
}

fn tokens(source: &str, offset: &mut usize, close: Option<u8>) -> Vec<Token> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    while *offset < bytes.len() {
        let byte = bytes[*offset];
        if Some(byte) == close {
            *offset += 1;
            break;
        }
        if byte.is_ascii_whitespace() {
            *offset += 1;
            continue;
        }
        if source[*offset..].starts_with("//") {
            *offset += source[*offset..]
                .find('\n')
                .unwrap_or(bytes.len() - *offset);
            continue;
        }
        if source[*offset..].starts_with("/*") {
            *offset += 2;
            let mut depth = 1;
            while depth != 0 {
                if source[*offset..].starts_with("/*") {
                    depth += 1;
                    *offset += 2;
                } else if source[*offset..].starts_with("*/") {
                    depth -= 1;
                    *offset += 2;
                } else {
                    *offset += source[*offset..].chars().next().unwrap().len_utf8();
                }
            }
            continue;
        }
        let start = *offset;
        let mut children = Vec::new();
        if byte == b'"' {
            *offset += 1;
            while bytes[*offset] != b'"' {
                if bytes[*offset] == b'\\' {
                    *offset += 1;
                }
                *offset += 1;
            }
            *offset += 1;
        } else if matches!(byte, b'(' | b'[' | b'{') {
            *offset += 1;
            let close = match byte {
                b'(' => b')',
                b'[' => b']',
                b'{' => b'}',
                _ => unreachable!(),
            };
            children = tokens(source, offset, Some(close));
        } else if byte.is_ascii_alphanumeric() || byte == b'_' {
            *offset += 1;
            while *offset < bytes.len()
                && (bytes[*offset].is_ascii_alphanumeric() || bytes[*offset] == b'_')
            {
                *offset += 1;
            }
        } else {
            *offset += source[*offset..].chars().next().unwrap().len_utf8();
        }
        let text = if children.is_empty() {
            source[start..*offset].to_string()
        } else {
            (byte as char).to_string()
        };
        out.push(Token { text, children });
    }
    out
}

fn enum_variants(source: &str, name: &str) -> BTreeSet<String> {
    let tail = source.split_once(&format!("pub enum {name}")).unwrap().1;
    let parsed = tokens(tail, &mut 0, None);
    parsed
        .iter()
        .find(|t| t.text == "{")
        .unwrap()
        .children
        .iter()
        .filter(|t| {
            t.text
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_uppercase)
        })
        .map(|t| t.text.clone())
        .collect()
}

fn match_coverage(source: &str, function: &str, alias: &str) -> (BTreeSet<String>, bool) {
    let tail = source.split_once(&format!("fn {function}(")).unwrap().1;
    let parsed = tokens(tail, &mut 0, None);
    let body = &parsed.iter().find(|t| t.text == "{").unwrap().children;
    let at = body.iter().position(|t| t.text == "match").unwrap();
    let arms = &body[at..].iter().find(|t| t.text == "{").unwrap().children;
    let mut names = BTreeSet::new();
    let mut fallback = false;
    let mut start = 0;
    while start < arms.len() {
        if arms[start].text == "," {
            start += 1;
            continue;
        }
        let arrow = (start..arms.len() - 1)
            .find(|&i| arms[i].text == "=" && arms[i + 1].text == ">")
            .unwrap();
        let pattern = &arms[start..arrow];
        for part in pattern.windows(4) {
            if part[0].text == alias && part[1].text == ":" && part[2].text == ":" {
                names.insert(part[3].text.clone());
            }
        }
        fallback |= pattern.iter().any(|t| t.text == "_");
        start = arrow + 2;
        if arms[start].text == "{" || arms[start].text == "{}" {
            start += 1;
        } else {
            start += arms[start..]
                .iter()
                .position(|t| t.text == ",")
                .unwrap_or(arms.len() - start);
        }
    }
    (names, fallback)
}

#[test]
fn a_new_hir_use_kind_does_not_build_until_the_use_match_names_it() {
    let source = include_str!("../src/check/init_effects.rs")
        .split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    let expressions = enum_variants(include_str!("../src/hir.rs"), "ExprKind");
    let statements = enum_variants(include_str!("../src/hir/statement.rs"), "Stmt");
    let mut declarations = String::new();
    let mut matches = String::new();
    for (name, variants, function, alias) in [
        ("Expression", expressions.clone(), "expr_use", "K"),
        ("Callback", expressions, "callback", "K"),
        ("Statement", statements, "stmt", "S"),
        (
            "Callee",
            enum_variants(include_str!("../src/hir.rs"), "Callee"),
            "callee",
            "C",
        ),
    ] {
        let (coverage, fallback) = match_coverage(source, function, alias);
        assert!(!variants.is_empty());
        assert_eq!(
            variants, coverage,
            "the production match names each HIR kind"
        );
        declarations.push_str(&format!(
            "enum {name} {{ {}, PROBE_KIND }}\n",
            variants.into_iter().collect::<Vec<_>>().join(",")
        ));
        let mut arms = coverage
            .into_iter()
            .map(|variant| format!("{name}::{variant} => (),"))
            .collect::<String>();
        if fallback {
            arms.push_str("_ => (),");
        }
        matches.push_str(&format!(
            "fn use_{}(value: {name}) {{ match value {{ {arms} PROBE_ARM_{name} }} }}\n",
            name.to_lowercase()
        ));
    }
    let directory = std::env::temp_dir().join(format!(
        "subscript-initializer-totality-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let compile = |injected: bool, named: bool| {
        let source = format!("#![allow(dead_code)]\n{declarations}{matches}")
            .replace("PROBE_KIND", if injected { "NewUse" } else { "" });
        let source = source
            .replace(
                "PROBE_ARM_Expression",
                if named {
                    "Expression::NewUse => (),"
                } else {
                    ""
                },
            )
            .replace(
                "PROBE_ARM_Statement",
                if named {
                    "Statement::NewUse => (),"
                } else {
                    ""
                },
            );
        let source = source.replace(
            "PROBE_ARM_Callee",
            if named { "Callee::NewUse => ()," } else { "" },
        );
        let source = source.replace(
            "PROBE_ARM_Callback",
            if named { "Callback::NewUse => ()," } else { "" },
        );
        let path = directory.join("probe.rs");
        fs::write(&path, source).unwrap();
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args([
                "--edition=2021",
                "--crate-type=lib",
                "--emit=metadata",
                "--crate-name=initializer_use_probe",
            ])
            .arg(&path)
            .arg("--out-dir")
            .arg(&directory)
            .output()
            .unwrap()
    };
    let control = compile(false, false);
    assert!(
        control.status.success(),
        "{}",
        String::from_utf8_lossy(&control.stderr)
    );
    let violating = compile(true, false);
    assert!(
        !violating.status.success(),
        "a new use kind must require an arm"
    );
    let stderr = String::from_utf8_lossy(&violating.stderr);
    assert!(
        stderr.contains("E0004") && stderr.contains("NewUse"),
        "{stderr}"
    );
    let named = compile(true, true);
    assert!(
        named.status.success(),
        "{}",
        String::from_utf8_lossy(&named.stderr)
    );
}
