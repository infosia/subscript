//! Shared filesystem entry for the ship-tier C emitter.

use std::fmt;
use std::path::{Path, PathBuf};

use subscript_compiler::{check_program, Diagnostic, SourceFile};

use crate::{emit_c, emit_c_without_main, AOT_ENTRY_C};

/// Files written by [`emit_c_files`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmittedCFiles {
    /// Generated host entry, when entry generation was enabled.
    pub entry: Option<PathBuf>,
    /// Generated program translation unit.
    pub source: PathBuf,
    /// Generated allocation-metadata header.
    pub allocation_metadata: PathBuf,
    /// Byte length of the generated program translation unit.
    pub source_len: usize,
}

/// Failure from [`emit_c_files`].
#[derive(Debug)]
#[non_exhaustive]
pub enum EmitCFilesError {
    /// The front end rejected the source program.
    Diagnostics(Vec<Diagnostic>),
    /// The checked program could not be lowered to C.
    Emission(String),
    /// An output directory or artifact could not be written.
    Io {
        /// Operation that failed.
        action: &'static str,
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying filesystem error.
        source: std::io::Error,
    },
}

impl fmt::Display for EmitCFilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostics(diagnostics) => {
                write!(f, "program did not check")?;
                for diagnostic in diagnostics {
                    write!(f, "\n  {diagnostic}")?;
                }
                Ok(())
            }
            Self::Emission(message) => f.write_str(message),
            Self::Io {
                action,
                path,
                source,
            } => write!(f, "{action} {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for EmitCFilesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Diagnostics(_) | Self::Emission(_) => None,
        }
    }
}

/// Checks `files`, emits ship-tier C, and writes its complete artifact set.
///
/// The program translation unit is `<label>.c`, allocation metadata is
/// `<label>.alloc.h`, and the optional generated host entry is `entry.c`.
/// The directory is created when absent. Existing files with those names
/// are replaced; no path outside `out_dir` is written when `label` is a
/// plain file stem.
///
/// # Errors
///
/// Returns [`EmitCFilesError::Diagnostics`] for front-end rejection,
/// [`EmitCFilesError::Emission`] for C-lowering failure, and
/// [`EmitCFilesError::Io`] for directory or artifact write failures.
pub fn emit_c_files(
    files: &[SourceFile],
    out_dir: &Path,
    label: &str,
    write_entry: bool,
) -> Result<EmittedCFiles, EmitCFilesError> {
    std::fs::create_dir_all(out_dir).map_err(|source| EmitCFilesError::Io {
        action: "create",
        path: out_dir.to_path_buf(),
        source,
    })?;
    let hir = check_program(files).map_err(EmitCFilesError::Diagnostics)?;
    let program = if write_entry {
        emit_c(&hir)
    } else {
        emit_c_without_main(&hir)
    }
    .map_err(EmitCFilesError::Emission)?;

    let entry = if write_entry {
        let path = out_dir.join("entry.c");
        write(&path, AOT_ENTRY_C.as_bytes())?;
        Some(path)
    } else {
        None
    };
    let source = out_dir.join(format!("{label}.c"));
    write(&source, program.source.as_bytes())?;
    let allocation_metadata = out_dir.join(format!("{label}.alloc.h"));
    write(
        &allocation_metadata,
        program.allocation_metadata_header.as_bytes(),
    )?;

    Ok(EmittedCFiles {
        entry,
        source,
        allocation_metadata,
        source_len: program.source.len(),
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), EmitCFilesError> {
    std::fs::write(path, bytes).map_err(|source| EmitCFilesError::Io {
        action: "write",
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Result<Self, String> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "subscript-emit-files-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path)
                .map_err(|error| format!("create {}: {error}", path.display()))?;
            Ok(Self(path))
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn shared_emitter_writes_the_direct_emitter_bytes() -> Result<(), String> {
        let directory = TestDir::new()?;
        let files = [SourceFile::new(
            "main.ts",
            "export function main(): void {\n  print(\"shared\");\n}\n",
        )];
        let hir = check_program(&files).map_err(|diagnostics| {
            diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        let direct = emit_c(&hir)?;
        let written =
            emit_c_files(&files, &directory.0, "program", true).map_err(|e| e.to_string())?;

        let source = std::fs::read(&written.source)
            .map_err(|error| format!("read {}: {error}", written.source.display()))?;
        assert_eq!(source, direct.source.as_bytes());
        assert_eq!(written.source_len, source.len());
        assert_eq!(
            std::fs::read(
                written
                    .entry
                    .as_ref()
                    .ok_or_else(|| "entry path missing".to_string())?
            )
            .map_err(|error| format!("read entry: {error}"))?,
            AOT_ENTRY_C.as_bytes()
        );
        assert_eq!(
            std::fs::read(&written.allocation_metadata)
                .map_err(|error| format!("read metadata: {error}"))?,
            direct.allocation_metadata_header.as_bytes()
        );
        Ok(())
    }

    #[test]
    fn shared_emitter_reports_diagnostics() -> Result<(), String> {
        let directory = TestDir::new()?;
        let files = [SourceFile::new("bad.ts", "const bad: number = 1;\n")];
        let error = emit_c_files(&files, &directory.0, "program", false)
            .expect_err("invalid program must be rejected");
        assert!(matches!(error, EmitCFilesError::Diagnostics(_)));
        Ok(())
    }

    #[test]
    fn shared_emitter_reports_output_io_errors() -> Result<(), String> {
        let directory = TestDir::new()?;
        let file = directory.0.join("not-a-directory");
        std::fs::write(&file, b"x")
            .map_err(|error| format!("write {}: {error}", file.display()))?;
        let files = [SourceFile::new(
            "main.ts",
            "export function main(): void {}\n",
        )];
        let error =
            emit_c_files(&files, &file, "program", false).expect_err("file is not a directory");
        assert!(matches!(error, EmitCFilesError::Io { .. }));
        Ok(())
    }
}
