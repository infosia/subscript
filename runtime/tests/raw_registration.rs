//! Section 263: a source-only check. Cost is linear in the source bytes; no compiler runs.
use std::path::Path;
use std::time::Instant;

fn tokens(source: &str) -> Vec<String> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let raw_start = if b[i..].starts_with(b"br") {
            i + 2
        } else {
            i + usize::from(b[i] == b'r')
        };
        let mut raw_quote = raw_start;
        while raw_quote < b.len() && b[raw_quote] == b'#' {
            raw_quote += 1;
        }
        if raw_start > i && raw_quote < b.len() && b[raw_quote] == b'"' {
            let hashes = raw_quote - raw_start;
            i = raw_quote + 1;
            while i < b.len() {
                if b[i] == b'"'
                    && b.get(i + 1..i + 1 + hashes)
                        .is_some_and(|tail| tail.iter().all(|&c| c == b'#'))
                {
                    i += 1 + hashes;
                    break;
                }
                i += 1;
            }
        } else if b[i] == b'\''
            && source[i + 1..]
                .chars()
                .next()
                .is_some_and(|c| c == '\\' || b.get(i + 1 + c.len_utf8()) == Some(&b'\''))
        {
            i += 1;
            if b[i] == b'\\' {
                i += 2;
                while i < b.len() && b[i] != b'\'' {
                    i += 1;
                }
            } else {
                i += source[i..].chars().next().unwrap().len_utf8();
            }
            i = (i + 1).min(b.len());
        } else if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if b[i] == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
        } else if b[i].is_ascii_alphanumeric() || b[i] == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(source[start..i].to_owned());
        } else {
            if !b[i].is_ascii_whitespace() {
                out.push((b[i] as char).to_string());
            }
            i += source[i..].chars().next().unwrap().len_utf8();
        }
    }
    out
}

fn candidates(source: &str) -> Vec<String> {
    let t = tokens(source);
    let mut found = Vec::new();
    for i in 0..t.len().saturating_sub(3) {
        if t[i] != "pub" {
            continue;
        }
        // Section 263 also covers the public RunConfig field.
        if t[i + 1] == "file_provider" && t[i + 2] == ":" {
            found.push("file_provider".to_owned());
        }
        let mut qualifier = i + 1;
        while matches!(
            t.get(qualifier).map(String::as_str),
            Some("const" | "async" | "extern")
        ) {
            // The tokenizer removes the ABI string after extern.
            qualifier += 1;
        }
        if t.get(qualifier).map(String::as_str) != Some("fn") {
            continue;
        }
        let name = if t[qualifier + 1] == "$" {
            &t[qualifier + 2]
        } else {
            &t[qualifier + 1]
        };
        let mut angle_depth = 0usize;
        let Some(start) = (qualifier + 2..t.len()).find(|&j| {
            match t[j].as_str() {
                "<" => angle_depth += 1,
                ">" if angle_depth > 0 && t[j - 1] != "-" => angle_depth -= 1,
                "(" if angle_depth == 0 => return true,
                _ => {}
            }
            false
        }) else {
            continue;
        };
        let mut depth = 1;
        let mut end = start + 1;
        while end < t.len() && depth > 0 {
            match t[end].as_str() {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            end += 1;
        }
        let args = &t[start + 1..end - 1];
        let raw = args
            .windows(2)
            .any(|w| w[0] == "*" && matches!(w[1].as_str(), "mut" | "const"));
        let held = args.iter().any(|s| {
            matches!(
                s.as_str(),
                "NonNull"
                    | "FileProvider"
                    | "TrapObserver"
                    | "PrintObserver"
                    | "DiagnosticsObserver"
            )
        });
        let address = args
            .windows(3)
            .any(|w| matches!(w[0].as_str(), "base" | "address") && w[1] == ":" && w[2] == "usize");
        if raw || held || address {
            let name = if t[qualifier + 1] == "$" {
                let m = (0..i).rfind(|&j| t[j] == "macro_rules").unwrap();
                format!("{}::{name}", t[m + 2])
            } else {
                name.clone()
            };
            found.push(name);
        }
    }
    found
}

const ALLOW: &[(&str, &str)] = &[
    (
        "runtime/src/ffi/conversions.rs::math_ffi_unary::sym",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/ffi/conversions.rs::math_ffi_binary::sym",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/ffi/numbers.rs::subscript_rt_fmod",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/ffi/numbers.rs::subscript_rt_math_clz32",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/ffi/numbers.rs::subscript_rt_math_imul",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/ffi/numbers.rs::subscript_rt_math_fround",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/ffi/numbers.rs::subscript_rt_math_f32_to_bits",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/ffi/numbers.rs::subscript_rt_math_f32_from_bits",
        "Ignores the Context pointer; the operation is pure.",
    ),
    (
        "runtime/src/context/async_scheduler.rs::async_is_stale",
        "Uses the frame address only as a lookup key during the call.",
    ),
    (
        "runtime/src/exception.rs::async_complete_exception",
        "Uses the frame address only as a lookup key during the call.",
    ),
    (
        "runtime/src/registration.rs::release_callback_registration",
        "Uses the registration address only as a lookup key during the call.",
    ),
];

fn check(found: &[String], allow: &[(&str, &str)]) -> Result<(), String> {
    let mut errors = Vec::new();
    for name in found {
        if !allow
            .iter()
            .any(|(key, reason)| *key == name && !reason.is_empty())
        {
            errors.push(format!("unlisted: {name}"));
        }
    }
    for (name, _) in allow {
        if found.iter().filter(|key| *key == name).count() != 1 {
            errors.push(format!("stale: {name}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn scan(root: &Path, dir: &Path, found: &mut Vec<String>, bytes: &mut usize) {
    let mut entries = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            scan(root, &path, found, bytes);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let source = std::fs::read_to_string(&path).unwrap();
            *bytes += source.len();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            found.extend(
                candidates(&source)
                    .into_iter()
                    .map(|name| format!("{relative}::{name}")),
            );
        }
    }
}

#[test]
fn public_raw_registration() {
    let start = Instant::now();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut found = Vec::new();
    let mut bytes = 0;
    for dir in ["runtime/src", "codegen/src"] {
        scan(root, &root.join(dir), &mut found, &mut bytes);
    }
    eprintln!("section 263: {bytes} source bytes in {:?}", start.elapsed());
    if let Err(errors) = check(&found, ALLOW) {
        panic!("{errors}");
    }
}

#[test]
fn controls() {
    assert!(check(&candidates("pub fn keep(p: *mut u8) {}"), &[]).is_err());
    for qualifiers in ["extern \"C\"", "const", "async", "const extern \"C\""] {
        assert!(check(
            &candidates(&format!("pub {qualifiers} fn keep(p: *mut u8) {{}}")),
            &[]
        )
        .is_err());
    }
    assert!(candidates("pub unsafe extern \"C\" fn keep(p: *mut u8) {}").is_empty());
    assert!(check(&candidates("pub fn keep<F: Fn()>(p: *mut u8) {}"), &[]).is_err());
    assert_eq!(candidates("pub fn root(address: usize) {}"), ["root"]);
    for ty in [
        "NonNull<u8>",
        "FileProvider",
        "TrapObserver",
        "PrintObserver",
        "DiagnosticsObserver",
    ] {
        assert_eq!(
            candidates(&format!("pub fn keep(p: Option<{ty}>) {{}}")),
            ["keep"]
        );
    }
    assert!(check(&[], &[("gone", "Uses the pointer only during the call.")]).is_err());
    assert!(
        candidates("pub(crate) fn keep(p: *mut u8) {} pub unsafe fn other(p: *const u8) {}")
            .is_empty()
    );
}
