//! §262: scan all tracked shell files. Cost: one git process and linear source traversal.

use std::time::Instant;

// §262: remove shell quotes and comments; keep command boundaries as tokens.
fn tokens(source: &str) -> Vec<String> {
    let source = source.replace("\\\n", "");
    let mut result = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut chars = source.chars();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                word.push(c);
            }
        } else {
            match c {
                '\'' | '"' => quote = Some(c),
                '#' if word.is_empty() => {
                    for next in chars.by_ref() {
                        if next == '\n' {
                            break;
                        }
                    }
                    result.push(";".into());
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        word.push(next);
                    }
                }
                ';' | '&' | '|' | '(' | ')' | '\n' => {
                    if !word.is_empty() {
                        result.push(std::mem::take(&mut word));
                    }
                    result.push(";".into());
                }
                c if c.is_whitespace() => {
                    if !word.is_empty() {
                        result.push(std::mem::take(&mut word));
                    }
                }
                _ => word.push(c),
            }
        }
    }
    if !word.is_empty() {
        result.push(word);
    }
    result
}

fn build_commands(source: &str) -> Vec<Vec<String>> {
    let tokens = tokens(source);
    let mut checks = Vec::new();
    for command in tokens.split(|token| token == ";") {
        for pair in command.windows(2) {
            if matches!(pair[0].as_str(), "cargo" | "$CARGO" | "${CARGO}") && pair[1] == "build" {
                checks.push(command.to_vec());
            }
        }
    }
    checks
}

fn build_checks(source: &str) -> Vec<bool> {
    build_commands(source)
        .iter()
        .map(|command| {
            command
                .iter()
                .any(|arg| arg == "--target-dir" || arg.starts_with("--target-dir="))
        })
        .collect()
}

// §262: these scripts do not read the build output directly.
const ALLOWLIST: &[(&str, &str)] = &[(
    "tools/gate.sh",
    "The script does not read build output; cargo test reuses the cargo build target directory.",
)];

fn violations(scripts: &[(&str, Vec<bool>)], allowlist: &[(&str, &str)]) -> Vec<String> {
    let mut matched = vec![false; allowlist.len()];
    let mut errors = Vec::new();
    for (path, checks) in scripts {
        for (index, present) in checks.iter().enumerate() {
            if !present {
                if let Some(row) = allowlist.iter().position(|(file, _)| file == path) {
                    matched[row] = true;
                } else {
                    errors.push(format!(
                        "{path}: cargo build #{} lacks --target-dir",
                        index + 1
                    ));
                }
            }
        }
    }
    for ((file, reason), matched) in allowlist.iter().zip(matched) {
        if reason.trim().is_empty() {
            errors.push(format!("allowlist row lacks a reason: {file}"));
        }
        if !matched {
            errors.push(format!("stale allowlist row: {file}"));
        }
    }
    errors
}

#[test]
fn scripts_build_into_the_directory_they_read() {
    let started = Instant::now();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let output = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", "*.sh"])
        .current_dir(root)
        .output()
        .expect("git ls-files must run");
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let paths = String::from_utf8(output.stdout).unwrap();
    let mut bytes = 0;
    let mut files = 0;
    let mut builds = 0;
    let mut scripts = Vec::new();
    for path in paths.split('\0').filter(|path| !path.is_empty()) {
        let source = std::fs::read_to_string(root.join(path)).unwrap();
        files += 1;
        bytes += source.len();
        let commands = build_commands(&source);
        builds += commands.len();
        for (index, command) in commands.iter().enumerate() {
            eprintln!("{path}: build #{}: {}", index + 1, command.join(" "));
        }
        scripts.push((path, build_checks(&source)));
    }
    eprintln!(
        "§262 script scan: {files} files, {bytes} bytes, {builds} builds, {:?}",
        started.elapsed()
    );
    let errors = violations(&scripts, ALLOWLIST);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn reports_a_build_without_target_dir() {
    for source in [
        "cargo build --offline",
        "(cd dir && cargo build --offline)",
        "if cargo build; then :; fi",
        "exec cargo build",
        "\"$CARGO\" build",
        "${CARGO} build",
        "cargo \\\n build --offline",
        "cargo build # --target-dir ignored",
    ] {
        assert_eq!(build_checks(source), vec![false], "{source}");
        assert_eq!(
            build_checks(&source.replace("build", "build --target-dir target")),
            vec![true],
            "{source}"
        );
    }
    assert!(build_checks("# cargo build\necho '# cargo build'").is_empty());
    assert_eq!(
        build_checks("cargo build; cargo build --target-dir target"),
        vec![false, true]
    );
}

#[test]
fn reports_stale_allowlist_rows() {
    let allowlist = &[("control.sh", "The script does not read build output.")];
    assert!(violations(&[("control.sh", build_checks("cargo build"))], allowlist).is_empty());
    for scripts in [
        vec![],
        vec![("control.sh", build_checks("echo done"))],
        vec![(
            "control.sh",
            build_checks("cargo build --target-dir target"),
        )],
        vec![("other.sh", build_checks("cargo build"))],
    ] {
        assert!(violations(&scripts, allowlist).contains(&"stale allowlist row: control.sh".into()));
    }
    assert_eq!(
        violations(&[("other.sh", build_checks("cargo build"))], &[]),
        vec!["other.sh: cargo build #1 lacks --target-dir"]
    );
}
