//! Ship-tier C emitter for the device-triple link
//! (`specs/blocks/compiler.md` §11).
//!
//! Emits the C translation unit for one accept-corpus entry — the ship
//! tier's HIR→C lowering — plus the host entry program, into a
//! directory. `device-link.sh` cross-compiles the pair with `clang` for
//! each device triple and links them with the runtime static library
//! cross-built for that triple; nothing here executes a produced binary
//! (compile+link is the whole criterion, §3).
//!
//! Usage:
//! `cargo run --offline -p subscript-codegen --bin emit-c -- <out-dir> [entry-id]`
//! The entry id defaults to `a01-hello`. Exit status: 0 on success,
//! 1 on a check/emit failure, 2 on usage or I/O errors.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use subscript_codegen::{emit_c, AOT_ENTRY_C};
use subscript_compiler::{check_program, SourceFile};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(out_dir) = args.next() else {
        eprintln!("usage: emit-c <out-dir> [entry-id]");
        return ExitCode::from(2);
    };
    let id = args.next().unwrap_or_else(|| "a01-hello".to_string());
    let out_dir = PathBuf::from(out_dir);
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("emit-c: create {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }

    let accept = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");
    let sources = match load_entry(&accept, &id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("emit-c: {e}");
            return ExitCode::from(2);
        }
    };

    let hir = match check_program(&sources) {
        Ok(m) => m,
        Err(diags) => {
            eprintln!(
                "emit-c: {id} did not check: {}",
                diags.first().map(|d| d.message.as_str()).unwrap_or("no diagnostic")
            );
            return ExitCode::FAILURE;
        }
    };
    let program = match emit_c(&hir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("emit-c: {id}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let entry_c = out_dir.join("entry.c");
    if let Err(e) = fs::write(&entry_c, AOT_ENTRY_C) {
        eprintln!("emit-c: write {}: {e}", entry_c.display());
        return ExitCode::from(2);
    }
    println!("wrote {}", entry_c.display());

    let src = out_dir.join(format!("{id}.c"));
    if let Err(e) = fs::write(&src, program.source.as_bytes()) {
        eprintln!("emit-c: write {}: {e}", src.display());
        return ExitCode::from(2);
    }
    println!("wrote {} ({} bytes)", src.display(), program.source.len());

    let metadata = out_dir.join(format!("{id}.alloc.h"));
    if let Err(e) = fs::write(&metadata, program.allocation_metadata_header.as_bytes()) {
        eprintln!("emit-c: write {}: {e}", metadata.display());
        return ExitCode::from(2);
    }
    println!("wrote {}", metadata.display());
    ExitCode::SUCCESS
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
        let text = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Ok(vec![SourceFile::new(format!("{id}.ts"), text)])
    }
}
