//! Stage 1 gate for compiler.md §69: each top-level corpus entry states
//! the result from stock TypeScript, and one batched process measures it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const HEADER_PREFIX: &str = "// tsc: ";

#[derive(Clone, Debug, Eq, PartialEq)]
enum TscClaim {
    Accepts,
    Rejects(BTreeSet<String>),
}

impl TscClaim {
    fn parse(text: &str) -> Result<Self, String> {
        if text == "accepts" {
            return Ok(Self::Accepts);
        }
        let Some(codes) = text.strip_prefix("rejects ") else {
            return Err("expected `accepts` or `rejects TS<code>[, TS<code> ...]`".to_string());
        };
        let codes: BTreeSet<String> = codes.split(", ").map(str::to_string).collect();
        if codes.is_empty() || codes.iter().any(|code| !is_diagnostic_code(code.as_str())) {
            return Err("expected `rejects TS<code>[, TS<code> ...]`".to_string());
        }
        Ok(Self::Rejects(codes))
    }

    fn measured(codes: BTreeSet<String>) -> Self {
        if codes.is_empty() {
            Self::Accepts
        } else {
            Self::Rejects(codes)
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Accepts => "accepts".to_string(),
            Self::Rejects(codes) => format!(
                "rejects {}",
                codes.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

fn is_diagnostic_code(code: &str) -> bool {
    code.strip_prefix("TS").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

#[derive(Debug)]
struct Entry {
    relative: String,
    absolute: PathBuf,
    accept: bool,
    external_module: bool,
    claim: TscClaim,
}

struct TempProjectDirectory(PathBuf);

impl TempProjectDirectory {
    fn create() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "subscript-tsc-corpus-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create {}: {error}", path.display()));
        Self(path)
    }
}

impl Drop for TempProjectDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate must have a workspace parent")
        .to_path_buf()
}

/// The repository-relative name of `absolute`, always with `/` separators.
/// A directory walk yields the host separator and a `tsc` diagnostic yields
/// `/`, so both spellings pass through here and name one entry on every
/// host.
fn repository_relative(root: &Path, absolute: &Path) -> Option<String> {
    Some(
        absolute
            .strip_prefix(root)
            .ok()?
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn corpus_entries(root: &Path) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for (kind, accept) in [("accept", true), ("reject", false)] {
        let directory = root.join("corpus").join(kind);
        let mut paths: Vec<PathBuf> = fs::read_dir(&directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "ts"))
            .collect();
        paths.sort();
        for absolute in paths {
            let relative = repository_relative(root, &absolute)
                .expect("corpus path must be below the workspace root");
            let source = fs::read_to_string(&absolute)
                .map_err(|error| format!("read {relative}: {error}"))?;
            for obsolete in [
                "// tsc-clean-standalone:",
                "// tsc-status:",
                "// tsc-clean:",
            ] {
                if source.lines().any(|line| line.starts_with(obsolete)) {
                    return Err(format!(
                        "{relative}: obsolete `{obsolete}` header; use `{HEADER_PREFIX}`"
                    ));
                }
            }
            let headers: Vec<&str> = source
                .lines()
                .filter_map(|line| line.strip_prefix(HEADER_PREFIX))
                .collect();
            if headers.len() != 1 {
                return Err(format!(
                    "{relative}: expected one `{HEADER_PREFIX}<claim>` header, found {}",
                    headers.len()
                ));
            }
            let claim_text = headers[0]
                .split_once("; js-comparable: ")
                .map_or(headers[0], |(claim, _)| claim);
            let claim = TscClaim::parse(claim_text)
                .map_err(|error| format!("{relative}: invalid tsc header: {error}"))?;
            if !accept {
                let expected_errors: Vec<&str> = source
                    .lines()
                    .filter_map(|line| line.strip_prefix("// expected-error: "))
                    .collect();
                if expected_errors.len() != 1 || expected_errors[0].trim().is_empty() {
                    return Err(format!(
                        "{relative}: expected one nonempty `// expected-error:` header, found {}",
                        expected_errors.len()
                    ));
                }
            }
            let external_module = source.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("import ") || line.starts_with("export ")
            });
            entries.push(Entry {
                relative,
                absolute,
                accept,
                external_module,
                claim,
            });
        }
    }
    Ok(entries)
}

fn json_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn write_projects(
    root: &Path,
    entries: &[Entry],
    temporary: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut ambient: Vec<PathBuf> = fs::read_dir(root.join("corpus/interop"))
        .map_err(|error| format!("read corpus/interop: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".d.ts"))
        })
        .collect();
    ambient.sort();
    ambient.push(root.join("prelude/lang.d.ts"));

    let modules: Vec<&PathBuf> = entries
        .iter()
        .filter(|entry| entry.external_module)
        .map(|entry| &entry.absolute)
        .collect();
    let scripts = entries
        .iter()
        .filter(|entry| !entry.external_module)
        .map(|entry| vec![&entry.absolute]);
    std::iter::once(modules)
        .chain(scripts)
        .enumerate()
        .map(|(index, entry_files)| {
            let config_path = temporary.join(format!("entry-batch-{index:03}.json"));
            let files = entry_files
                .into_iter()
                .chain(ambient.iter())
                .map(|path| json_string(path.to_string_lossy().as_ref()))
                .collect::<Vec<_>>()
                .join(",\n    ");
            let config = format!(
                "{{\n  \"compilerOptions\": {{\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2022\",\n    \"module\": \"ESNext\",\n    \"moduleResolution\": \"Bundler\",\n    \"lib\": [\"ES2022\", \"ESNext.Disposable\"],\n    \"types\": [],\n    \"forceConsistentCasingInFileNames\": true\n  }},\n  \"files\": [\n    {files}\n  ]\n}}\n"
            );
            fs::write(&config_path, config)
                .map_err(|error| format!("write {}: {error}", config_path.display()))?;
            Ok(config_path)
        })
        .collect()
}

fn diagnostic_codes(
    root: &Path,
    entries: &[Entry],
    output: &str,
) -> Result<BTreeMap<String, BTreeSet<String>>, Vec<String>> {
    let entry_names: BTreeSet<&str> = entries
        .iter()
        .map(|entry| entry.relative.as_str())
        .collect();
    let mut codes = BTreeMap::<String, BTreeSet<String>>::new();
    let mut unowned = Vec::new();
    for line in output.lines().filter(|line| line.contains("error TS")) {
        let Some(marker) = line.find("): error TS") else {
            unowned.push(line.to_string());
            continue;
        };
        let Some(position_start) = line[..marker].rfind('(') else {
            unowned.push(line.to_string());
            continue;
        };
        let source_path = Path::new(&line[..position_start]);
        let absolute = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            root.join(source_path)
        };
        let Some(relative) = repository_relative(root, &absolute) else {
            unowned.push(line.to_string());
            continue;
        };
        if !entry_names.contains(relative.as_str()) {
            unowned.push(line.to_string());
            continue;
        }
        let code_start = marker + "): error ".len();
        let Some(code) = line[code_start..].split(':').next() else {
            unowned.push(line.to_string());
            continue;
        };
        if !is_diagnostic_code(code) {
            unowned.push(line.to_string());
            continue;
        }
        codes.entry(relative).or_default().insert(code.to_string());
    }
    if unowned.is_empty() {
        Ok(codes)
    } else {
        Err(unowned)
    }
}

#[test]
fn every_corpus_tsc_header_matches_measured_tsc() {
    let root = project_root();
    let entries = corpus_entries(&root).unwrap_or_else(|error| panic!("{error}"));
    let temporary = TempProjectDirectory::create();
    let projects =
        write_projects(&root, &entries, &temporary.0).unwrap_or_else(|error| panic!("{error}"));
    // `node_modules/.bin/tsc` is a POSIX shell script. Windows cannot
    // execute it (`os error 193`); npm writes `tsc.cmd` beside it for that
    // host.
    let tsc = root.join(if cfg!(windows) {
        "node_modules/.bin/tsc.cmd"
    } else {
        "node_modules/.bin/tsc"
    });
    assert!(
        tsc.is_file(),
        "the pinned TypeScript compiler is absent at {}",
        tsc.display()
    );

    let started = Instant::now();
    let output = Command::new(&tsc)
        .arg("--build")
        .arg("--pretty")
        .arg("false")
        .args(&projects)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", tsc.display()));
    let elapsed = started.elapsed();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = diagnostic_codes(&root, &entries, &combined).unwrap_or_else(|unowned| {
        panic!(
            "tsc emitted diagnostics that belong to no corpus entry:\n{}",
            unowned.join("\n")
        )
    });
    assert!(
        output.status.success() || !codes.is_empty(),
        "tsc exited with {} and emitted no attributed diagnostic:\n{combined}",
        output.status
    );

    let mut disagreements = Vec::new();
    for entry in &entries {
        let actual = TscClaim::measured(codes.get(&entry.relative).cloned().unwrap_or_default());
        if entry.claim != actual {
            disagreements.push(format!(
                "{}: header says `{}`; tsc said `{}`",
                entry.relative,
                entry.claim.display(),
                actual.display()
            ));
        }
        if entry.accept && actual != TscClaim::Accepts {
            disagreements.push(format!(
                "{}: an accept entry must type-check; tsc said `{}`",
                entry.relative,
                actual.display()
            ));
        }
    }
    eprintln!(
        "tsc corpus gate: {} entries measured in {:.3}s",
        entries.len(),
        elapsed.as_secs_f64()
    );
    assert!(
        disagreements.is_empty(),
        "tsc corpus header disagreement(s):\n{}",
        disagreements.join("\n")
    );
}

#[test]
fn tsc_claim_parser_requires_an_outcome_and_valid_diagnostic_codes() {
    assert_eq!(TscClaim::parse("accepts"), Ok(TscClaim::Accepts));
    assert_eq!(
        TscClaim::parse("rejects TS1238, TS2322"),
        Ok(TscClaim::Rejects(BTreeSet::from([
            "TS1238".to_string(),
            "TS2322".to_string()
        ])))
    );
    for invalid in ["", "clean", "rejects", "rejects S100", "rejects TS"] {
        assert!(TscClaim::parse(invalid).is_err(), "accepted `{invalid}`");
    }
}

#[test]
fn diagnostic_parser_attributes_codes_and_reports_unowned_errors() {
    let root = Path::new("/workspace");
    let entry = Entry {
        relative: "corpus/reject/r01.ts".to_string(),
        absolute: root.join("corpus/reject/r01.ts"),
        accept: false,
        external_module: true,
        claim: TscClaim::Accepts,
    };
    let measured = diagnostic_codes(
        root,
        &[entry],
        "/workspace/corpus/reject/r01.ts(2,3): error TS1234: bad\n",
    )
    .expect("diagnostic must belong to r01");
    assert_eq!(
        measured["corpus/reject/r01.ts"],
        BTreeSet::from(["TS1234".to_string()])
    );
    assert!(diagnostic_codes(root, &[], "error TS9999: global failure\n").is_err());
}
