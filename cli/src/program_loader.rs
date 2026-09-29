//! Transitive on-disk program loading for the program subcommands.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use subscript_compiler::{discover_module_sources, SourceFile};

use crate::{read_text, rejection, Failure};

/// Loads the entry, its transitive relative imports and re-exports, and ambient mirrors.
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
            let candidate = directory.join(format!("{specifier}.ts"));
            let normalized = match normalize_existing(&candidate) {
                Ok(path) => path,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(Failure::usage(format!(
                        "resolve source {}: {error}",
                        candidate.display()
                    )))
                }
            };
            let text = match std::fs::read_to_string(&normalized) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
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

fn normalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
