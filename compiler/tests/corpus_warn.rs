//! Warning corpus and zero-warning precision gates.

use std::fs;
use std::path::{Path, PathBuf};

use subscript_compiler::{check_program, check_warnings, SourceFile, WarnCode, Warning};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn corpus_dir() -> PathBuf {
    repository_root().join("corpus")
}

const EXPECTED: &[(&str, WarnCode, u32)] = &[
    ("w01-loop-allocation-unreleased.ts", WarnCode::W001, 17),
    ("w02-use-after-free.ts", WarnCode::W002, 18),
    (
        "w03-fresh-callback-userdata-loop.ts",
        WarnCode::W003,
        19,
    ),
];

fn checked_warnings(files: Vec<SourceFile>, label: &str) -> Vec<Warning> {
    let module = check_program(&files).unwrap_or_else(|diagnostics| {
        panic!("{label} was rejected: {diagnostics:?}");
    });
    check_warnings(&module)
}

fn read_source(path: &Path, name: impl Into<String>) -> SourceFile {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    SourceFile::new(name, source)
}

fn interop_mirror() -> SourceFile {
    let path = corpus_dir().join("interop/interop.generated.d.ts");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    SourceFile::ambient("interop.generated.d.ts", source)
}

fn accept_sources(name: &str, path: &Path) -> Vec<SourceFile> {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    const INTEROP_TOKENS: &[&str] = &[
        "subDevice",
        "subChainPayloadValue",
        "subSlice",
        "SubDrawList",
        "subDrawListTotal",
        "SUB_ACCESS",
        "subAccessMatches",
        "subBulk",
        "subBoundaryString",
        "SUB_STAGE",
        "subStageMatches",
        "subFutureMake",
        "subStatsMake",
        "SubQueryStatus",
    ];
    let mut files = Vec::new();
    if INTEROP_TOKENS.iter().any(|token| source.contains(token)) {
        files.push(interop_mirror());
    }
    files.push(SourceFile::new(name, source));
    files
}

#[test]
fn every_warning_entry_is_accepted_and_fires_at_its_pinned_line() {
    let directory = corpus_dir().join("warn");
    for (file, code, line) in EXPECTED {
        let path = directory.join(file);
        let warnings = checked_warnings(accept_sources(file, &path), file);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.code == *code && warning.pos.line == *line),
            "{file}: expected {code} at line {line}, got {warnings:?}"
        );
    }
}

#[test]
fn warning_table_covers_every_warning_corpus_entry() {
    let directory = corpus_dir().join("warn");
    let mut actual = fs::read_dir(&directory)
        .expect("read corpus/warn")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".ts"))
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = EXPECTED
        .iter()
        .map(|(file, _, _)| (*file).to_string())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(actual, expected, "warning corpus and table disagree");
}

#[test]
fn accept_corpus_and_examples_have_zero_warnings() {
    let accept = corpus_dir().join("accept");
    let mut accept_entries = fs::read_dir(&accept)
        .expect("read corpus/accept")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".ts"))
        .collect::<Vec<_>>();
    accept_entries.sort();

    let mut checked_files = 0_usize;
    for name in &accept_entries {
        let warnings = checked_warnings(
            accept_sources(name, &accept.join(name)),
            &format!("corpus/accept/{name}"),
        );
        assert!(
            warnings.is_empty(),
            "corpus/accept/{name} produced warnings: {warnings:?}"
        );
        checked_files += 1;
    }

    let modules = accept.join("a19-modules");
    let module_files = vec![
        read_source(&modules.join("main.ts"), "main.ts"),
        read_source(&modules.join("math.ts"), "math.ts"),
    ];
    let warnings = checked_warnings(module_files, "corpus/accept/a19-modules");
    assert!(
        warnings.is_empty(),
        "corpus/accept/a19-modules produced warnings: {warnings:?}"
    );
    checked_files += 2;
    assert_eq!(checked_files, 99, "accept source-file count changed");

    let examples = repository_root().join("examples");
    let engine_mirror_path = examples.join("engine/engine.generated.d.ts");
    let engine_mirror_source = fs::read_to_string(&engine_mirror_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", engine_mirror_path.display()));
    let mut example_entries = fs::read_dir(&examples)
        .expect("read examples")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('e') && name.ends_with(".ts"))
        .collect::<Vec<_>>();
    example_entries.sort();

    for name in &example_entries {
        let path = examples.join(name);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let mut files = Vec::new();
        if source.contains("engineWorld") || source.contains("engineFrame") {
            files.push(SourceFile::ambient(
                "engine.generated.d.ts",
                engine_mirror_source.clone(),
            ));
        }
        files.push(SourceFile::new(name, source));
        let warnings = checked_warnings(files, &format!("examples/{name}"));
        assert!(
            warnings.is_empty(),
            "examples/{name} produced warnings: {warnings:?}"
        );
    }
    assert_eq!(example_entries.len(), 10, "numbered example count changed");

    for relative in ["hot-reload/demo.ts", "rust-host/logic.ts"] {
        let path = examples.join(relative);
        let warnings = checked_warnings(
            vec![read_source(
                &path,
                path.file_name()
                    .expect("example file name")
                    .to_string_lossy(),
            )],
            &format!("examples/{relative}"),
        );
        assert!(
            warnings.is_empty(),
            "examples/{relative} produced warnings: {warnings:?}"
        );
    }
}
