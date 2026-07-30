//! Transitive on-disk program loading for the program subcommands.

use std::collections::{HashSet, VecDeque};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use subscript_compiler::{parse_import_specifiers, SourceFile};

use crate::{read_text, rejection, Failure};

/// Loads the entry, its transitive relative imports, and ambient mirrors.
pub(super) fn load_program(entry: &Path, mirrors: &[PathBuf]) -> Result<Vec<SourceFile>, Failure> {
    let mut files = Vec::with_capacity(mirrors.len() + 1);
    for path in mirrors {
        let text = read_text(path, "mirror")?;
        files.push(SourceFile::ambient(path.to_string_lossy(), text));
    }

    let entry_index = files.len();
    let entry_text = read_text(entry, "source")?;
    let entry_path = normalize_existing(entry)
        .map_err(|error| Failure::usage(format!("resolve source {}: {error}", entry.display())))?;
    files.push(SourceFile::new(entry.to_string_lossy(), entry_text));

    let mut loaded = HashSet::new();
    loaded.insert(entry_path.clone());
    let mut pending = VecDeque::from([(entry_index, entry_path)]);

    while let Some((file_index, disk_path)) = pending.pop_front() {
        let imports = parse_import_specifiers(&files[file_index])
            .map_err(|diagnostics| rejection(&files, diagnostics))?;
        if file_index == entry_index && !imports.is_empty() {
            files[entry_index].name = file_name(entry);
        }

        for specifier in imports {
            if !is_checker_resolvable(&specifier) {
                continue;
            }
            let Some(directory) = disk_path.parent() else {
                continue;
            };
            let candidate = directory.join(format!("{specifier}.ts"));
            let normalized = match normalize_existing(&candidate) {
                Ok(path) => path,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(Failure::usage(format!(
                        "resolve source {}: {error}",
                        candidate.display()
                    )));
                }
            };
            if loaded.contains(&normalized) {
                continue;
            }
            let text = match std::fs::read_to_string(&normalized) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(Failure::usage(format!(
                        "read source {}: {error}",
                        candidate.display()
                    )));
                }
            };
            loaded.insert(normalized.clone());
            let next_index = files.len();
            files.push(SourceFile::new(file_name(&candidate), text));
            pending.push_back((next_index, normalized));
        }
    }

    Ok(files)
}

fn normalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

fn is_checker_resolvable(specifier: &str) -> bool {
    specifier
        .strip_prefix("./")
        .is_some_and(|stem| !stem.is_empty() && !stem.contains('/'))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
