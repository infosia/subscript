//! Deterministic corpus mutations must return from the public checker (§90).

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use subscript_compiler::{check_program, SourceFile};

const SAMPLES: usize = 12;
const PUNCTUATION: &[u8] = b"{}[]();,:=!?";

fn sources(dir: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read corpus directory") {
        let path = entry.expect("read corpus entry").path();
        if path.is_dir() {
            sources(&path, paths);
        } else if path.extension().is_some_and(|extension| extension == "ts") {
            paths.push(path);
        }
    }
}

fn source_file(path: &Path, source: String) -> SourceFile {
    let name = path.to_str().expect("UTF-8 corpus path");
    if name.ends_with(".d.ts") {
        SourceFile::ambient(name, source)
    } else {
        SourceFile::new(name, source)
    }
}

fn sample(index: usize, count: usize, length: usize) -> usize {
    if count <= 1 {
        0
    } else {
        index * (length - 1) / (count - 1)
    }
}

#[test]
fn corpus_mutations_do_not_panic() {
    let started = Instant::now();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    let mut paths = Vec::new();
    for arm in ["accept", "reject", "warn", "trap", "interop"] {
        sources(&root.join(arm), &mut paths);
    }
    paths.sort();
    let originals: Vec<_> = paths
        .iter()
        .map(|path| source_file(path, fs::read_to_string(path).expect("read corpus source")))
        .collect();
    let current = Arc::new(Mutex::new(String::new()));
    let panics = Arc::new(Mutex::new(Vec::new()));
    let hook_current = Arc::clone(&current);
    let hook_panics = Arc::clone(&panics);
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        hook_panics
            .lock()
            .expect("panic records lock")
            .push(format!(
                "{}: {info}",
                hook_current.lock().expect("input label lock")
            ));
    }));

    // Restore the process hook before any test assertion can fail.
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut total = 0;
        let mut caught = 0;
        for (path, original) in paths.iter().zip(&originals) {
            let mut files: Vec<_> = paths
                .iter()
                .zip(&originals)
                .filter(|(sibling, file)| {
                    file.dts && sibling.parent() == path.parent() && *sibling != path
                })
                .map(|(_, file)| file.clone())
                .collect();
            files.push(original.clone());
            let text = &original.source;
            let mut run = |kind: &str, position: usize, mutation: String| {
                *current.lock().expect("input label lock") = format!(
                    "{}:{kind}@{position}",
                    path.strip_prefix(&root)
                        .expect("corpus-relative path")
                        .display()
                );
                files.last_mut().expect("mutation source").source = mutation;
                total += 1;
                if panic::catch_unwind(AssertUnwindSafe(|| check_program(&files))).is_err() {
                    caught += 1;
                }
            };
            let boundaries: Vec<_> = text.char_indices().map(|(offset, _)| offset).collect();
            for index in 0..SAMPLES {
                let position = boundaries
                    .get(sample(index, SAMPLES, boundaries.len().max(1)))
                    .copied()
                    .unwrap_or(0);
                run("truncate", position, text[..position].to_owned());
            }
            // ASCII bytes permit a one-byte edit without invalid UTF-8.
            let bytes: Vec<_> = text
                .bytes()
                .enumerate()
                .filter_map(|(offset, byte)| byte.is_ascii().then_some(offset))
                .collect();
            if !bytes.is_empty() {
                for index in 0..SAMPLES {
                    let position = bytes[sample(index, SAMPLES, bytes.len())];
                    let mut mutation = text.as_bytes().to_vec();
                    let mut replacement = PUNCTUATION[index % PUNCTUATION.len()];
                    if replacement == mutation[position] {
                        replacement = PUNCTUATION[(index + 1) % PUNCTUATION.len()];
                    }
                    mutation[position] = replacement;
                    run(
                        "replace",
                        position,
                        String::from_utf8(mutation).expect("ASCII edit"),
                    );
                }
            }
            let lines: Vec<_> = text.split_inclusive('\n').collect();
            let count = lines.len().min(SAMPLES);
            let mut offsets = Vec::with_capacity(lines.len() + 1);
            offsets.push(0);
            for line in &lines {
                offsets.push(offsets.last().expect("line offset") + line.len());
            }
            for index in 0..count {
                let line = sample(index, count, lines.len());
                let mut mutation = text.clone();
                mutation.replace_range(offsets[line]..offsets[line + 1], "");
                run("delete-line", line + 1, mutation);
            }
        }
        (total, caught)
    }));
    panic::set_hook(previous_hook);
    let records = panics.lock().expect("panic records lock");
    for record in records.iter() {
        eprintln!("{record}");
    }
    let (total, caught) = outcome.expect("mutation harness must complete");
    println!(
        "robustness: {} sources, {total} inputs, {caught} panics, {:?}",
        paths.len(),
        started.elapsed()
    );
    assert_eq!(caught, 0, "checker panics:\n{}", records.join("\n"));
    assert!(
        records.is_empty(),
        "panic hook recorded an unexpected panic"
    );
}
