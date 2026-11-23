//! The program of a directory corpus entry (compiler.md §354 rule 10).
//!
//! Each loader of the corpus that reads a directory entry uses this file,
//! so every loader names the files and resolves the imports in one way.

use std::fs;
use std::path::Path;

use subscript_compiler::SourceFile;

/// Each `.ts` file below `directory`, named by its path from `directory`
/// with `/` separators, sorted.
///
/// # Errors
/// Returns the first directory read error, or a name that is not UTF-8.
pub fn module_names(directory: &Path) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut pending = vec![(directory.to_path_buf(), String::new())];
    while let Some((path, prefix)) = pending.pop() {
        let entries =
            fs::read_dir(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read {}: {error}", path.display()))?;
            let file_name = entry.file_name();
            let file_name = file_name
                .to_str()
                .ok_or_else(|| format!("{}: a name is not UTF-8", path.display()))?;
            let name = format!("{prefix}{file_name}");
            if entry.path().is_dir() {
                pending.push((entry.path(), format!("{name}/")));
            } else if name.ends_with(".ts") {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}

/// The source files of the directory entry, in discovery order from
/// `main.ts`. Each import resolves with
/// [`subscript_compiler::resolve_module_specifier`].
///
/// # Errors
/// Returns an error when `main.ts` is absent, a read fails, a source does
/// not parse, or no import reaches a `.ts` file of the directory.
pub fn load(directory: &Path) -> Result<Vec<SourceFile>, String> {
    let names = module_names(directory)?;
    let entry = "main.ts".to_owned();
    if !names.contains(&entry) {
        return Err("a directory entry requires main.ts".to_owned());
    }
    let read = |name: &str| {
        fs::read_to_string(directory.join(name)).map_err(|error| format!("read {name}: {error}"))
    };
    let source = SourceFile::new(&entry, read(&entry)?);
    let discovered = subscript_compiler::discover_module_sources(
        (entry, source),
        |importer: &String, specifier| {
            let Some(key) = subscript_compiler::resolve_module_specifier(importer, specifier)
            else {
                return Ok(None);
            };
            let name = format!("{key}.ts");
            if !names.contains(&name) {
                return Ok(None);
            }
            let text = read(&name)?;
            Ok(Some((name.clone(), SourceFile::new(name, text))))
        },
        |_, diagnostics| format!("{diagnostics:?}"),
    )?;
    for name in &names {
        if !discovered.iter().any(|source| &source.name == name) {
            return Err(format!("unreachable corpus source {name}"));
        }
    }
    Ok(discovered)
}
