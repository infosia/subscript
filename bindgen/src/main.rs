//! `bindgen` command line: reads a synthetic C interop header and writes
//! the ambient `.d.ts` mirror to stdout (or to a file with `-o`).
//!
//! Usage:
//!   bindgen <header.h>            # mirror to stdout
//!   bindgen <header.h> -o <out>   # mirror to <out>
//!
//! Regenerating the committed mirror:
//!   bindgen corpus/interop/interop.h -o corpus/interop/interop.generated.d.ts

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut header: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                out = Some(args.get(i).ok_or("`-o` requires a path")?);
            }
            other if header.is_none() => header = Some(other),
            other => return Err(format!("unexpected argument `{other}`")),
        }
        i += 1;
    }
    let header = header.ok_or("usage: bindgen <header.h> [-o <out>]")?;
    let src = std::fs::read_to_string(header).map_err(|e| format!("read {header}: {e}"))?;
    let mirror = subscript_bindgen::generate(&src).map_err(|e| e.to_string())?;
    match out {
        Some(path) => std::fs::write(path, mirror).map_err(|e| format!("write {path}: {e}"))?,
        None => print!("{mirror}"),
    }
    Ok(())
}
