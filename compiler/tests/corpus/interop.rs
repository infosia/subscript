//! Shared discovery of the committed interop mirrors.

use std::fs;
use std::path::{Path, PathBuf};

/// Reads every generated mirror and its ambient companion declarations.
#[allow(dead_code)]
pub fn mirrors<T>(make: impl Fn(String, String) -> T) -> Vec<T> {
    mirror_texts(&interop_directory())
        .into_iter()
        .map(|(name, text)| make(name, text))
        .collect()
}

fn interop_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop")
}

fn mirror_texts(directory: &Path) -> Vec<(String, String)> {
    let mut paths = fs::read_dir(directory)
        .expect("read interop mirrors")
        .map(|entry| entry.expect("interop mirror entry").path())
        .filter(|path| path.is_file() && path.to_string_lossy().ends_with(".d.ts"))
        .collect::<Vec<_>>();
    // Keep the existing corpus declaration order and the owner before its external mirrors.
    paths.sort_by_key(|path| {
        let name = path
            .file_name()
            .expect("mirror name")
            .to_string_lossy()
            .into_owned();
        let order = match name.as_str() {
            "wire-enum-aliases.d.ts" => 0,
            "wire-enum.generated.d.ts" => 1,
            "interop.generated.d.ts" => 2,
            _ => 3,
        };
        (order, name)
    });
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .expect("mirror name")
                .to_string_lossy()
                .into_owned();
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            (name, text)
        })
        .collect()
}

/// Reads one mirror for tests that inspect or deliberately omit declarations.
#[allow(dead_code)]
pub fn mirror<T>(name: &str, make: impl Fn(String, String) -> T) -> T {
    mirrors(|name, text| (name, text))
        .into_iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(name, text)| make(name, text))
        .expect("committed interop mirror")
}

fn identifiers(text: &str) -> std::collections::BTreeSet<&str> {
    text.lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .flat_map(|line| line.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_'))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Selects mirrors by declared names, then includes their ambient dependencies.
#[allow(dead_code)]
pub fn mirrors_for<T>(source: &str, make: impl Fn(String, String) -> T) -> Vec<T> {
    mirrors_for_in(&interop_directory(), source, make)
}

/// Selects mirrors from a supplied directory for discovery tests.
#[allow(dead_code)]
pub fn mirrors_for_in<T>(
    directory: &Path,
    source: &str,
    make: impl Fn(String, String) -> T,
) -> Vec<T> {
    let mirrors = mirror_texts(directory);
    let declarations = mirrors
        .iter()
        .map(|(_, text)| {
            text.lines()
                .filter_map(|line| {
                    let line = line.trim().strip_prefix("declare ").unwrap_or(line.trim());
                    [
                        "function ",
                        "class ",
                        "interface ",
                        "enum ",
                        "type ",
                        "const ",
                    ]
                    .iter()
                    .find_map(|prefix| line.strip_prefix(prefix))
                    .and_then(|tail| {
                        tail.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                            .next()
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut names = identifiers(source);
    let mut selected = vec![false; mirrors.len()];
    loop {
        let mut added = false;
        for (index, (_, text)) in mirrors.iter().enumerate() {
            if !selected[index] && declarations[index].iter().any(|name| names.contains(name)) {
                selected[index] = true;
                names.extend(identifiers(text));
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    // The synthetic fixture owner stays present for interop corpus entries.
    if selected.iter().any(|selected| *selected) {
        for (index, (name, _)) in mirrors.iter().enumerate() {
            if name == "interop.generated.d.ts" {
                selected[index] = true;
            }
        }
    }
    mirrors
        .into_iter()
        .zip(selected)
        .filter(|(_, selected)| *selected)
        .map(|((name, text), _)| make(name, text))
        .collect()
}
