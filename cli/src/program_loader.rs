//! Transitive on-disk program loading for the program subcommands.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use subscript_compiler::{
    discover_module_sources, resolve_module_specifier, Diagnostic, SourceFile,
};

use crate::{file_stamp, read_text, rejection, Failure, FileStamp};

pub(super) struct LoadedProgram {
    pub files: Vec<SourceFile>,
    pub tried_paths: Vec<(PathBuf, FileStamp)>,
    /// The canonical path of each program file that is not ambient, the
    /// entry first, in the order of `files`.
    pub program_paths: Vec<PathBuf>,
}

/// A file that the loader read. The identity is the canonical path; the
/// name is the provisional name, relative to the directory of the entry.
#[derive(Clone)]
struct LoadedFile {
    path: PathBuf,
    name: String,
}

impl PartialEq for LoadedFile {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for LoadedFile {}

impl Hash for LoadedFile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
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
        Ok((files, program_paths)) => Ok(LoadedProgram {
            files,
            tried_paths: tried,
            program_paths,
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
) -> Result<(Vec<SourceFile>, Vec<PathBuf>), Failure> {
    let mut files = Vec::with_capacity(mirrors.len() + 1);
    for path in mirrors {
        record_path(tried, path);
        let text = read_text(path, "mirror")?;
        files.push(SourceFile::ambient(path.to_string_lossy(), text));
    }

    record_path(tried, entry);
    let entry_text = read_text(entry, "source")?;
    let entry_path = normalize_existing(entry)
        .map_err(|error| Failure::usage(format!("resolve source {}: {error}", entry.display())))?;
    let entry_directory = entry_path.parent().map(Path::to_path_buf).ok_or_else(|| {
        Failure::usage(format!(
            "source {} has no parent directory",
            entry.display()
        ))
    })?;
    // The provisional name of the entry is its file name as given.
    let entry_file = LoadedFile {
        name: file_name(entry),
        path: entry_path.clone(),
    };
    let source = SourceFile::entry(entry_file.name.clone(), entry_text);

    // The canonical path of each provisional name that the load closure gave.
    let mut paths_by_name = HashMap::from([(entry_file.name.clone(), entry_file.path.clone())]);
    let mut has_dependencies = false;
    // The largest count of leading `..` segments in a key that loaded a file.
    let mut climb = 0;
    let discovered = discover_module_sources(
        (entry_file, source),
        |importer: &LoadedFile, specifier| {
            has_dependencies = true;
            // compiler.md §354 rules 6 and 8: the resolver of the compiler
            // gives the key; the key is a path from the directory of the entry.
            let Some(key) = resolve_module_specifier(&importer.name, specifier) else {
                return Ok(None);
            };
            // An empty key or a last `..` segment names a directory, not a file.
            if key.is_empty() || key.rsplit('/').next() == Some("..") {
                return Ok(None);
            }
            let candidate = key_path(&entry_directory, &key);
            record_path(tried, &candidate);
            let normalized = match normalize_existing(&candidate) {
                Ok(path) => path,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return Ok(None);
                }
                Err(error) => {
                    return Err(LoadError::Failure(Failure::usage(format!(
                        "resolve source {}: {error}",
                        candidate.display()
                    ))))
                }
            };
            if !normalized.is_file() {
                return Ok(None);
            }
            let text = match std::fs::read_to_string(&normalized) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return Ok(None);
                }
                Err(error) => {
                    return Err(LoadError::Failure(Failure::usage(format!(
                        "read source {}: {error}",
                        candidate.display()
                    ))))
                }
            };
            climb = climb.max(
                key.split('/')
                    .take_while(|segment| *segment == "..")
                    .count(),
            );
            let name = format!("{key}.ts");
            paths_by_name
                .entry(name.clone())
                .or_insert_with(|| normalized.clone());
            Ok(Some((
                LoadedFile {
                    path: normalized,
                    name: name.clone(),
                },
                SourceFile::new(name, text),
            )))
        },
        |sources, diagnostics| LoadError::Parse(sources.to_vec(), diagnostics),
    );
    let (mut discovered, parse_diagnostics) = match discovered {
        Ok(discovered) => (discovered, None),
        Err(LoadError::Failure(failure)) => return Err(failure),
        Err(LoadError::Parse(sources, diagnostics)) => (sources, Some(diagnostics)),
    };

    let mut program_paths = Vec::with_capacity(discovered.len());
    program_paths.push(entry_path);
    for source in discovered.iter().skip(1) {
        let path = paths_by_name
            .get(&source.name)
            .cloned()
            .ok_or_else(|| Failure::usage(format!("source {} has no loaded path", source.name)))?;
        program_paths.push(path);
    }
    // A program of one file with no import keeps the entry name as given.
    let names = if has_dependencies {
        let provisional: Vec<&str> = discovered
            .iter()
            .map(|source| source.name.as_str())
            .collect();
        rebased_names(&entry_directory, climb, &provisional)?
    } else {
        vec![entry.to_string_lossy().into_owned()]
    };
    let mut renamed = HashMap::new();
    for (source, name) in discovered.iter_mut().zip(names) {
        renamed.insert(std::mem::replace(&mut source.name, name.clone()), name);
    }
    files.extend(discovered);
    if let Some(mut diagnostics) = parse_diagnostics {
        // A parse diagnostic names the file by the name that the program has.
        for diagnostic in &mut diagnostics {
            if let Some(name) = renamed.get(&diagnostic.pos.file) {
                diagnostic.pos.file.clone_from(name);
            }
        }
        return Err(rejection(&files, diagnostics));
    }
    Ok((files, program_paths))
}

/// A failure of the discovery: a failure of the file system, or the parse
/// diagnostics with the files that the discovery read.
enum LoadError {
    Failure(Failure),
    Parse(Vec<SourceFile>, Vec<Diagnostic>),
}

/// The disk path of the module `key`, a `/`-separated path from `directory`
/// that can start with `..` segments.
///
/// `directory` is canonical, so a `..` segment removes its last component.
/// A Windows canonical path has the `\\?\` prefix, which turns off the
/// interpretation of `..` by the file system *(docs)*, so the loader removes
/// the component itself.
fn key_path(directory: &Path, key: &str) -> PathBuf {
    let mut path = directory.to_path_buf();
    let mut segments = key.split('/').peekable();
    while let Some(segment) = segments.next() {
        if segment == ".." {
            path.pop();
        } else if segments.peek().is_some() {
            path.push(segment);
        } else {
            path.push(format!("{segment}.ts"));
        }
    }
    path
}

/// The name of each file: its provisional name rebased onto the root of
/// the names, with `/` separators (compiler.md §354 rule 8).
///
/// The root is the directory of the entry moved up by `climb`, the largest
/// count of leading `..` segments of a key that loaded a file. The name is
/// the last `climb` components of `entry_directory`, joined with the
/// provisional name and normalized by the rules of `module_key`, with the
/// `.ts` suffix kept.
///
/// Each provisional name is a key that `resolve_module_specifier` gave from
/// the provisional name of the importer, with `.ts`. The rebase adds one
/// prefix to every name, and no name has more leading `..` segments than
/// the prefix has segments. A resolution against the directory of a name
/// is lexical, so it commutes with the addition of the prefix. Thus the key
/// that the resolver gives from the final name of the importer is the key
/// of the final name of the file that the loader read. The names come only
/// from specifiers, not from the file system, so a difference of case or a
/// symbolic link does not change this. One file that two spellings reach
/// keeps the first name; the identity of a file is its canonical path.
fn rebased_names(
    entry_directory: &Path,
    climb: usize,
    provisional: &[&str],
) -> Result<Vec<String>, Failure> {
    let components: Vec<_> = entry_directory.components().collect();
    let start = components.len().saturating_sub(climb);
    let prefix = &components[start..];
    if prefix.len() < climb
        || !prefix
            .iter()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(Failure::usage(format!(
            "an import leaves the file system root above {}",
            entry_directory.display()
        )));
    }
    let prefix: Vec<String> = prefix
        .iter()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    Ok(provisional
        .iter()
        .map(|name| {
            let mut segments: Vec<&str> = Vec::new();
            for segment in prefix.iter().map(String::as_str).chain(name.split('/')) {
                match segment {
                    "" | "." => {}
                    ".." if segments.last().is_some_and(|last| *last != "..") => {
                        segments.pop();
                    }
                    segment => segments.push(segment),
                }
            }
            segments.join("/")
        })
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    // compiler.md §354 rule 8. Cost (debug): below 0.01 s; string and path
    // operations only.
    #[test]
    fn names_are_provisional_names_rebased_onto_the_root() -> Result<(), String> {
        let base = std::env::temp_dir().join("subscript-names-root");
        let rebase = |directory: &Path, climb: usize, names: &[&str]| {
            rebased_names(directory, climb, names).map_err(|failure| failure.message)
        };
        assert_eq!(
            rebase(&base.join("other"), 1, &["up.ts", "../sub/x.ts"])?,
            ["other/up.ts", "sub/x.ts"]
        );
        assert_eq!(
            rebase(&base.join("sub"), 1, &["m.ts", "../sub/x.ts"])?,
            ["sub/m.ts", "sub/x.ts"]
        );
        assert_eq!(
            rebase(&base, 0, &["main.ts", "a/x.ts"])?,
            ["main.ts", "a/x.ts"]
        );
        // A name keeps the spelling of the specifier, not of the file on
        // disk, so the key of each name agrees with the resolver.
        let names = rebase(&base.join("other"), 1, &["up.ts", "../Sub/Util.ts"])?;
        assert_eq!(names, ["other/up.ts", "Sub/Util.ts"]);
        assert_eq!(
            resolve_module_specifier(&names[0], "../Sub/Util").as_deref(),
            Some(subscript_compiler::module_key(&names[1]).as_str())
        );
        assert_eq!(
            key_path(&base.join("other"), "../sub/x"),
            base.join("sub").join("x.ts")
        );
        Ok(())
    }

    // compiler.md §354 rule 8. Cost (debug): below 0.01 s.
    #[test]
    fn a_root_above_the_file_system_root_is_a_usage_failure() {
        let failure = rebased_names(Path::new("/a"), 2, &["m.ts", "../../x.ts"]);
        assert!(
            failure.is_err_and(|failure| failure.code == crate::USAGE_ERROR
                && failure.message.contains("leaves the file system root"))
        );
    }

    // compiler.md §354 rules 8 and 11. Cost (debug): below 0.01 s; four
    // small files, no compile.
    #[test]
    fn the_watch_set_holds_each_file_in_a_subdirectory() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "subscript-loader-subdirectory-{}",
            std::process::id()
        ));
        let write = |relative: &str, text: &str| -> Result<(), String> {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().ok_or("parent")?).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| e.to_string())
        };
        write("sub/x.ts", "export function x(): i32 { return 7; }\n")?;
        write(
            "other/up.ts",
            "import { x } from '../sub/x';\nimport { y } from './missing/y';\n",
        )?;
        let loaded = load_program(&root.join("other/up.ts"), &[]);
        let canonical = std::fs::canonicalize(&root).map_err(|e| e.to_string());
        let _ = std::fs::remove_dir_all(&root);
        let (loaded, canonical) = (loaded.map_err(|failure| failure.message)?, canonical?);
        let names: Vec<&str> = loaded.files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names, ["other/up.ts", "sub/x.ts"]);
        let x = canonical.join("sub").join("x.ts");
        assert_eq!(
            loaded.program_paths,
            [canonical.join("other").join("up.ts"), x.clone()]
        );
        let tried: Vec<&PathBuf> = loaded.tried_paths.iter().map(|(path, _)| path).collect();
        assert!(tried.contains(&&x), "{tried:?}");
        // A candidate that does not exist stays watched.
        let missing = canonical.join("other").join("missing").join("y.ts");
        assert!(tried.contains(&&missing), "{tried:?}");
        Ok(())
    }

    // Cost (debug): below 0.01 s; path operations only.
    #[cfg(windows)]
    #[test]
    fn verbatim_paths_are_named_with_slashes() {
        // The `\\?\` prefix and `\` separators of a canonical path do not
        // reach a name.
        let names = rebased_names(Path::new(r"\\?\C:\a\sub"), 1, &["m.ts", "../b/x.ts"])
            .map_err(|failure| failure.message);
        assert_eq!(
            names.ok(),
            Some(vec!["sub/m.ts".to_owned(), "b/x.ts".to_owned()])
        );
        let failure = rebased_names(Path::new(r"\\?\C:\a"), 2, &["m.ts", "../../x.ts"]);
        assert!(failure.is_err());
        assert_eq!(
            key_path(Path::new(r"\\?\C:\a\sub"), "../b/x"),
            PathBuf::from(r"\\?\C:\a\b\x.ts")
        );
    }
}
