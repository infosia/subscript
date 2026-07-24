//! `subscript-bindgen` command line: runs the libclang C frontend
//! (`specs/blocks/compiler.md` §13.1, §13.5) on any C header path and emits
//! the ambient `.d.ts` boundary mirror to stdout (or to a file with `-o`).
//! A header the toolchain cannot fully map fails loud with the P6.2
//! unmapped-type error naming the offending construct — it never writes a
//! silently invalid mirror.
//!
//! Usage:
//!   subscript-bindgen --header <path>            # mirror to stdout
//!   subscript-bindgen --header <path> -o <out>   # mirror to <out>
//!   subscript-bindgen <path>                     # positional form
//!   subscript-bindgen --help
//!
//! Regenerating the committed mirror:
//!   subscript-bindgen --header corpus/interop/interop.h \
//!     -o corpus/interop/interop.generated.d.ts

use std::process::ExitCode;

const USAGE: &str = "\
subscript-bindgen — emit the ambient .d.ts boundary mirror for a C header.

USAGE:
    subscript-bindgen --header <path> [-o <out>]
    subscript-bindgen <path> [-o <out>]

ARGS:
    --header <path>   Path to the C header to bind (any header file path).
    <path>            Same, given positionally.

OPTIONS:
    -o <out>          Write the mirror to <out> instead of stdout.
    -h, --help        Print this help.

The libclang frontend parses real C (preprocessor, attributes, typedefs,
nested structs, function-pointer typedefs, static const, enums, flag
typedefs). A construct the toolchain cannot map to a boundary type fails
loud, naming the offending construct; no invalid mirror is written.";

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
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--header" => {
                i += 1;
                let path = args.get(i).ok_or("`--header` requires a path")?;
                if header.replace(path).is_some() {
                    return Err("more than one header path given".to_string());
                }
            }
            "-o" => {
                i += 1;
                out = Some(args.get(i).ok_or("`-o` requires a path")?);
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}`\n\n{USAGE}"));
            }
            other if header.is_none() => header = Some(other),
            other => return Err(format!("unexpected argument `{other}`")),
        }
        i += 1;
    }
    let header = header.ok_or_else(|| format!("no header path given\n\n{USAGE}"))?;
    let src = std::fs::read_to_string(header).map_err(|e| format!("read {header}: {e}"))?;
    let mirror = subscript_bindgen::generate(&src).map_err(|e| e.to_string())?;
    match out {
        Some(path) => std::fs::write(path, mirror).map_err(|e| format!("write {path}: {e}"))?,
        None => print!("{mirror}"),
    }
    Ok(())
}
