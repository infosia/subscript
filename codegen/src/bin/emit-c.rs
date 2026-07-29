//! Ship-tier C emitter for the device-triple link
//! (`specs/blocks/compiler.md` §11).
//!
//! Emits a ship-tier C translation unit and, unless suppressed, the host
//! entry program into a directory. Input is either one accept-corpus entry
//! or explicit script and ambient-mirror paths. `device-link.sh` uses the
//! corpus form and cross-compiles the pair for each device triple.
//!
//! Usage:
//! `emit-c <out-dir> [entry-id] [--no-entry]`
//! `emit-c <out-dir> --source <path>... [--mirror <path>...] [--no-entry]`
//! The corpus entry id defaults to `a01-hello`. Exit status: 0 on
//! success, 1 on a check/emit failure, 2 on usage or I/O errors.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use subscript_codegen::{emit_c_files, EmitCFilesError};
use subscript_compiler::SourceFile;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(out_dir) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let mut entry_id = None;
    let mut source_paths = Vec::new();
    let mut mirror_paths = Vec::new();
    let mut write_entry = true;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" => {
                let Some(path) = args.next() else {
                    eprintln!("emit-c: --source requires a path");
                    usage();
                    return ExitCode::from(2);
                };
                source_paths.push(PathBuf::from(path));
            }
            "--mirror" => {
                let Some(path) = args.next() else {
                    eprintln!("emit-c: --mirror requires a path");
                    usage();
                    return ExitCode::from(2);
                };
                mirror_paths.push(PathBuf::from(path));
            }
            "--no-entry" => write_entry = false,
            flag if flag.starts_with('-') => {
                eprintln!("emit-c: unknown option `{flag}`");
                usage();
                return ExitCode::from(2);
            }
            id if entry_id.is_none() => entry_id = Some(id.to_string()),
            other => {
                eprintln!("emit-c: unexpected argument `{other}`");
                usage();
                return ExitCode::from(2);
            }
        }
    }
    if !source_paths.is_empty() && entry_id.is_some() {
        eprintln!("emit-c: an entry id cannot be combined with --source");
        usage();
        return ExitCode::from(2);
    }
    if source_paths.is_empty() && !mirror_paths.is_empty() {
        eprintln!("emit-c: --mirror requires at least one --source");
        usage();
        return ExitCode::from(2);
    }

    let out_dir = PathBuf::from(out_dir);
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("emit-c: create {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }

    let (label, sources) = if source_paths.is_empty() {
        let id = entry_id.unwrap_or_else(|| "a01-hello".to_string());
        let accept = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");
        let sources = match load_entry(&accept, &id) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("emit-c: {e}");
                return ExitCode::from(2);
            }
        };
        (id, sources)
    } else {
        let sources = match load_explicit(&mirror_paths, &source_paths) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("emit-c: {e}");
                return ExitCode::from(2);
            }
        };
        ("program".to_string(), sources)
    };

    let emitted = match emit_c_files(&sources, &out_dir, &label, write_entry) {
        Ok(emitted) => emitted,
        Err(EmitCFilesError::Diagnostics(diags)) => {
            eprintln!(
                "emit-c: {label} did not check: {}",
                diags
                    .first()
                    .map(|d| d.message.as_str())
                    .unwrap_or("no diagnostic")
            );
            return ExitCode::FAILURE;
        }
        Err(EmitCFilesError::Emission(e)) => {
            eprintln!("emit-c: {label}: {e}");
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("emit-c: {error}");
            return ExitCode::from(2);
        }
    };

    if let Some(entry) = emitted.entry {
        println!("wrote {}", entry.display());
    }
    println!(
        "wrote {} ({} bytes)",
        emitted.source.display(),
        emitted.source_len
    );
    println!("wrote {}", emitted.allocation_metadata.display());
    ExitCode::SUCCESS
}

fn usage() {
    eprintln!(
        "usage: emit-c <out-dir> [entry-id] [--no-entry]\n\
         \x20      emit-c <out-dir> --source <path>... [--mirror <path>...] [--no-entry]"
    );
}

fn load_explicit(mirrors: &[PathBuf], sources: &[PathBuf]) -> Result<Vec<SourceFile>, String> {
    let mut out = Vec::with_capacity(mirrors.len() + sources.len());
    for path in mirrors {
        let text =
            fs::read_to_string(path).map_err(|e| format!("read mirror {}: {e}", path.display()))?;
        out.push(SourceFile::ambient(path.to_string_lossy(), text));
    }
    for path in sources {
        let text =
            fs::read_to_string(path).map_err(|e| format!("read source {}: {e}", path.display()))?;
        out.push(SourceFile::new(path.to_string_lossy(), text));
    }
    Ok(out)
}

/// Loads one accept-corpus entry (a multi-file entry is a directory).
fn load_entry(accept: &std::path::Path, id: &str) -> Result<Vec<SourceFile>, String> {
    let dir = accept.join(id);
    if dir.is_dir() {
        let mut names: Vec<String> = fs::read_dir(&dir)
            .map_err(|e| format!("read {}: {e}", dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".ts"))
            .collect();
        names.sort();
        names.sort_by_key(|n| !n.contains("main"));
        let mut out = Vec::new();
        for n in names {
            let text = fs::read_to_string(dir.join(&n)).map_err(|e| format!("read {n}: {e}"))?;
            out.push(SourceFile::new(n, text));
        }
        Ok(out)
    } else {
        let path = accept.join(format!("{id}.ts"));
        let text =
            fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Ok(vec![SourceFile::new(format!("{id}.ts"), text)])
    }
}
