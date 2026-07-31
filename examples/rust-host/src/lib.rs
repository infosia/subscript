#![warn(missing_docs)]
//! A minimal pure-Rust host for an embedded subscript program.

use subscript_codegen::{ReloadError, ReloadSession};
use subscript_compiler::{check_program, render_diagnostics, SourceFile};

const LOGIC_V1: &str = include_str!("../logic.ts");

const LOGIC_V2: &str = "\
let ticks: i32 = 0;

function doubled(value: i32): i32 {
  return value * 10;
}

export function update(): void {
  ticks += 1;
  print(`tick=${ticks}, helper=${doubled(ticks)}`);
}

export function main(): void {}
";

const LOGIC_V3: &str = "\
let ticks: i32 = 0;

function doubled(value: i64): i64 {
  return value * 10;
}

export function update(): void {
  ticks += 1;
  print(`tick=${ticks}, helper=${doubled(ticks as i64)}`);
}

export function main(): void {}
";

fn source_files(source: &str) -> Vec<SourceFile> {
    vec![SourceFile::new("logic.ts", source)]
}

fn drive_frame(session: &mut ReloadSession, stdout: &mut Vec<u8>) -> Result<(), String> {
    session
        .call_export("update")
        .map_err(|error| format!("call export `update`: {error}"))?;
    stdout.extend(session.take_output());
    Ok(())
}

/// Runs the complete embedding and hot-reload example in memory.
///
/// The returned tuple contains the script's stdout bytes and the host's
/// expected stderr lines. Keeping process I/O outside this function makes the
/// flow deterministic and directly testable.
///
/// # Errors
///
/// Returns rendered checker diagnostics or a host/JIT error if a required
/// compile, call, accepted reload, or expected refusal does not occur.
pub fn run() -> Result<(Vec<u8>, Vec<String>), String> {
    // 1. Build and check the source set before giving it to the JIT.
    let files = source_files(LOGIC_V1);
    check_program(&files).map_err(|diagnostics| render_diagnostics(&files, &diagnostics))?;

    // 2. Start one live Context and let the host drive three frames.
    let mut session =
        ReloadSession::new(&files).map_err(|error| format!("start reload session: {error}"))?;
    let mut stdout = Vec::new();
    let mut stderr_lines = Vec::new();
    for _ in 0..3 {
        drive_frame(&mut session, &mut stdout)?;
    }

    // 3. Swap function bodies, then drive two more frames. The global tick
    // counter continues from three because reload preserves the Context.
    let v2_files = source_files(LOGIC_V2);
    session
        .reload(&v2_files)
        .map_err(|error| format!("reload V2: {error}"))?;
    for _ in 0..2 {
        drive_frame(&mut session, &mut stdout)?;
    }

    // 4. A signature edit changes the declaration hash and is refused.
    let v3_files = source_files(LOGIC_V3);
    let refusal = match session.reload(&v3_files) {
        Err(error @ ReloadError::DeclarationChanged { .. }) => error.to_string(),
        Err(error) => return Err(format!("reload V3 failed unexpectedly: {error}")),
        Ok(()) => return Err("reload V3 unexpectedly accepted a declaration change".to_string()),
    };
    stderr_lines.push(refusal);

    // The refused swap leaves V2 and its Context live for one final frame.
    drive_frame(&mut session, &mut stdout)?;

    Ok((stdout, stderr_lines))
}
