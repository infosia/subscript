//! Retained ship-target object cross-check (`specs/blocks/compiler.md` §8.1).
//!
//! Emits one retained Cranelift-object cross-check object per ship target
//! for shape parity: the `aarch64-apple-ios` (Mach-O) and
//! `aarch64-linux-android` (ELF) device triples, plus the
//! `x86_64-unknown-linux-gnu` (ELF) host target. The actual ship path is
//! `subscript emit` followed by clang; see `device-link.sh`, which does not
//! consume these objects. This binary also writes the generated C entry
//! program next to the cross-check objects, but executes nothing.
//!
//! Usage:
//! `cargo run --offline -p subscript-codegen --bin emit-object -- <out-dir> [entry-id]`
//! The entry id defaults to `a01-hello`. Exit status: 0 on success,
//! 1 on a compile/emit failure, 2 on usage or I/O errors.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use subscript_codegen::{emit_object, AOT_ENTRY_C};
use subscript_compiler::SourceFile;

/// Ship-target triples retained for Cranelift-object shape parity.
const SHIP_TARGET_TRIPLES: [&str; 3] = [
    "aarch64-apple-ios",
    "aarch64-linux-android",
    "x86_64-unknown-linux-gnu",
];

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(out_dir) = args.next() else {
        eprintln!("usage: emit-object <out-dir> [entry-id]");
        return ExitCode::from(2);
    };
    let id = args.next().unwrap_or_else(|| "a01-hello".to_string());
    let out_dir = PathBuf::from(out_dir);
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("emit-object: create {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }

    let accept = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");
    let sources = match load_entry(&accept, &id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("emit-object: {e}");
            return ExitCode::from(2);
        }
    };

    let entry_c = out_dir.join("entry.c");
    if let Err(e) = fs::write(&entry_c, AOT_ENTRY_C) {
        eprintln!("emit-object: write {}: {e}", entry_c.display());
        return ExitCode::from(2);
    }
    println!("wrote {}", entry_c.display());

    for triple in SHIP_TARGET_TRIPLES {
        let object = match emit_object(&sources, Some(triple)) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("emit-object: {id} for {triple}: {e}");
                return ExitCode::FAILURE;
            }
        };
        let path = out_dir.join(format!("{id}-{triple}.o"));
        if let Err(e) = fs::write(&path, &object.bytes) {
            eprintln!("emit-object: write {}: {e}", path.display());
            return ExitCode::from(2);
        }
        println!("wrote {} ({} bytes)", path.display(), object.bytes.len());
    }
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
        let text =
            fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Ok(vec![SourceFile::new(format!("{id}.ts"), text)])
    }
}
