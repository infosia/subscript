//! Deterministic corpus mutations must return from the public checker (§90).

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
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

struct Input {
    label: String,
    files: Vec<SourceFile>,
}

struct PanicRecord {
    label: String,
    message: String,
}

thread_local! {
    static CURRENT_INPUT: RefCell<String> = const { RefCell::new(String::new()) };
}

// Both tests take this lock for the entire hook lifetime. The hook is process-wide.
static HOOK_LOCK: Mutex<()> = Mutex::new(());

fn run_inputs(
    source_count: usize,
    workers: usize,
    build: impl Fn(usize) -> Vec<Input> + Sync,
    check: impl Fn(&Input) + Sync,
) -> (usize, usize, Vec<PanicRecord>) {
    let panics = Arc::new(Mutex::new(Vec::new()));
    let hook_panics = Arc::clone(&panics);
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let label = CURRENT_INPUT.with(|current| current.borrow().clone());
        hook_panics
            .lock()
            .expect("panic records lock")
            .push(PanicRecord {
                label,
                message: info.to_string(),
            });
    }));
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let next = AtomicUsize::new(0);
        let caught = AtomicUsize::new(0);
        let total = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let (next, caught, total, build, check) = (&next, &caught, &total, &build, &check);
                scope.spawn(move || loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= source_count {
                        break;
                    }
                    let inputs = build(index);
                    total.fetch_add(inputs.len(), Ordering::Relaxed);
                    for input in &inputs {
                        CURRENT_INPUT.with(|current| current.replace(input.label.clone()));
                        if panic::catch_unwind(AssertUnwindSafe(|| check(input))).is_err() {
                            caught.fetch_add(1, Ordering::Relaxed);
                        }
                        CURRENT_INPUT.with(|current| current.borrow_mut().clear());
                    }
                });
            }
        });
        (total.into_inner(), caught.into_inner())
    }));
    // Restore the process hook before any test assertion can fail.
    panic::set_hook(previous_hook);
    let (total, caught) = outcome.expect("mutation harness must complete");
    let records = std::mem::take(&mut *panics.lock().expect("panic records lock"));
    (total, caught, records)
}

#[test]
fn pool_records_only_the_panicking_input_label() {
    let _hook_guard = HOOK_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let workers = 2;
    let chosen = "control/source.ts:replace@0";
    let started = Barrier::new(workers);
    let labeled = Barrier::new(workers);
    let finished = Barrier::new(workers);
    let checked = AtomicUsize::new(0);
    struct FinishOnUnwind<'a>(&'a Barrier);
    impl Drop for FinishOnUnwind<'_> {
        fn drop(&mut self) {
            self.0.wait();
        }
    }
    let (total, caught, records) = run_inputs(
        workers,
        workers,
        |index| {
            // The other worker cannot set its label until the chosen label is set.
            if index != 0 {
                started.wait();
            }
            vec![Input {
                label: format!("control/source.ts:replace@{index}"),
                files: Vec::new(),
            }]
        },
        |input| {
            checked.fetch_add(1, Ordering::Relaxed);
            if input.label == chosen {
                let _finish = FinishOnUnwind(&finished);
                started.wait();
                // All other labels are now set and remain set through the panic hook.
                labeled.wait();
                panic!("chosen control input");
            }
            labeled.wait();
            finished.wait();
        },
    );
    assert_eq!(total, workers);
    assert_eq!(checked.into_inner(), total);
    assert_eq!(caught, 1);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].label, chosen);
    assert!(records[0].message.contains("chosen control input"));
}

#[test]
fn corpus_mutations_do_not_panic() {
    let _hook_guard = HOOK_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
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
    let mut companions: HashMap<_, Vec<usize>> = HashMap::new();
    for (index, path) in paths.iter().enumerate() {
        companions.entry(path.parent()).or_default().push(index);
    }
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    let (total, caught, records) = run_inputs(
        paths.len(),
        workers,
        |source_index| {
            let path = &paths[source_index];
            let original = &originals[source_index];
            let mut inputs = Vec::new();
            let mut files: Vec<_> = companions[&path.parent()]
                .iter()
                .copied()
                .filter(|&index| {
                    (originals[index].dts
                        || path.parent().is_some_and(|parent| {
                            parent.parent() == Some(root.join("reject").as_path())
                        }))
                        && paths[index] != *path
                })
                .map(|index| originals[index].clone())
                .collect();
            files.push(original.clone());
            let text = &original.source;
            let mut run = |kind: &str, position: usize, mutation: String| {
                let label = format!(
                    "{}:{kind}@{position}",
                    path.strip_prefix(&root)
                        .expect("corpus-relative path")
                        .display()
                );
                files.last_mut().expect("mutation source").source = mutation;
                inputs.push(Input {
                    label,
                    files: files.clone(),
                });
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
            inputs
        },
        |input| {
            let _ = check_program(&input.files);
        },
    );
    let records: Vec<_> = records
        .iter()
        .map(|record| format!("{}: {}", record.label, record.message))
        .collect();
    for record in &records {
        eprintln!("{record}");
    }
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
