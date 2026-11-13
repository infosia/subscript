//! Transitive on-disk program loading for the program subcommands.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use subscript_compiler::{discover_module_sources, SourceFile};

use crate::{file_stamp, read_text, rejection, Failure, FileStamp};

pub(super) struct LoadedProgram {
    pub files: Vec<SourceFile>,
    pub tried_paths: Vec<(PathBuf, FileStamp)>,
}

fn record_path(tried: &mut Vec<(PathBuf, FileStamp)>, path: &Path) {
    let path = watch_path(path);
    let stamp = file_stamp(&path);
    tried.push((path, stamp));
}

/// Loads the entry, its transitive relative imports and re-exports, and ambient mirrors.
pub(super) fn load_program(entry: &Path, mirrors: &[PathBuf]) -> Result<LoadedProgram, Failure> {
    let mut tried = Vec::new();
    match load_program_traced(entry, mirrors, &mut tried) {
        Ok(files) => Ok(LoadedProgram {
            files,
            tried_paths: tried,
        }),
        Err(mut failure) => {
            failure.tried_paths = tried;
            Err(failure)
        }
    }
}

fn load_program_traced(
    entry: &Path,
    mirrors: &[PathBuf],
    tried: &mut Vec<(PathBuf, FileStamp)>,
) -> Result<Vec<SourceFile>, Failure> {
    let mut files = Vec::with_capacity(mirrors.len() + 1);
    for path in mirrors {
        record_path(tried, path);
        let text = read_text(path, "mirror")?;
        files.push(SourceFile::ambient(path.to_string_lossy(), text));
    }

    let entry_index = files.len();
    record_path(tried, entry);
    let entry_text = read_text(entry, "source")?;
    let entry_path = normalize_existing(entry)
        .map_err(|error| Failure::usage(format!("resolve source {}: {error}", entry.display())))?;
    files.push(SourceFile::entry(entry.to_string_lossy(), entry_text));

    let source = files.remove(entry_index);
    let has_dependencies = std::cell::Cell::new(false);
    let mut discovered = discover_module_sources(
        (entry_path, source),
        |disk_path, specifier| {
            has_dependencies.set(true);
            if !specifier
                .strip_prefix("./")
                .is_some_and(|stem| !stem.is_empty() && !stem.contains('/'))
            {
                return Ok(None);
            }
            let Some(directory) = disk_path.parent() else {
                return Ok(None);
            };
            let candidate = directory.join(format!("{}.ts", &specifier[2..]));
            record_path(tried, &candidate);
            let normalized = match normalize_existing(&candidate) {
                Ok(path) => path,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return Ok(None);
                }
                Err(error) => {
                    return Err(Failure::usage(format!(
                        "resolve source {}: {error}",
                        candidate.display()
                    )))
                }
            };
            let text = match std::fs::read_to_string(&normalized) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return Ok(None);
                }
                Err(error) => {
                    return Err(Failure::usage(format!(
                        "read source {}: {error}",
                        candidate.display()
                    )))
                }
            };
            Ok(Some((
                normalized,
                SourceFile::new(file_name(&candidate), text),
            )))
        },
        |sources, diagnostics| {
            let mut all = files.clone();
            all.extend_from_slice(sources);
            rejection(&all, diagnostics)
        },
    )?;
    if has_dependencies.get() {
        discovered[0].name = file_name(entry);
    }
    files.extend(discovered);
    Ok(files)
}

pub(super) fn watch_path(path: &Path) -> PathBuf {
    normalize_existing(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        }
    })
}

fn normalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
