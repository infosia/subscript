//! macOS arm64 linked-binary size gate for the P23 RegExp surface.
//!
//! The two committed programs are a matched pair: the regex side adds
//! only one RegExp call and otherwise reaches the same runtime surfaces.
//! This gate is target-specific because it consumes Apple's link-map
//! format and measures `-Wl,-dead_strip` followed by Apple's `strip`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use subscript_codegen::{emit_c, runtime_staticlib_path, tool_output_report, AOT_ENTRY_C};
use subscript_compiler::{check_program, SourceFile};

const BASELINE_SOURCE: &str = include_str!("../../regex-size/baseline.ts");
const REGEX_SOURCE: &str = include_str!("../../regex-size/with-regex.ts");

const DELTA_MIN: u64 = 560_000;
const DELTA_MAX: u64 = 700_000;
const REGRESS_MIN: u64 = 490_000;
const REGRESS_MAX: u64 = 515_000;

// The baseline has one MiB of upward tolerance for unrelated code-generation
// movement. That is wide relative to ordinary drift but cannot hide the
// 4,194,304-byte astral table returning on both sides of the matched pair.
const BASELINE_ABSOLUTE_TOLERANCE: u64 = 1_048_576;
const REFERENCE_BASELINE_BYTES: u64 = 605_992;
// The regex reference diagnoses a matched-pair delta failure; the baseline
// reference additionally enforces the absolute-size guard above.
const REFERENCE_REGEX_BYTES: u64 = 1_221_000;

fn main() -> ExitCode {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        println!(
            "regex-size-gate: skipped (requires macOS arm64; host is {} {})",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        return ExitCode::SUCCESS;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("regex-size-gate: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    assert_matched_sources()?;
    let staticlib = runtime_staticlib_path().map_err(|error| error.to_string())?;
    let work = WorkDir::new()?;
    let baseline = link_subject(
        &work.path,
        "baseline",
        "baseline.ts",
        BASELINE_SOURCE,
        &staticlib,
    )?;
    let regex = link_subject(
        &work.path,
        "with-regex",
        "with-regex.ts",
        REGEX_SOURCE,
        &staticlib,
    )?;
    if baseline.stdout != regex.stdout {
        return Err(format!(
            "matched programs disagree: baseline stdout {:?}, regex stdout {:?}",
            String::from_utf8_lossy(&baseline.stdout),
            String::from_utf8_lossy(&regex.stdout)
        ));
    }
    let delta = regex.bytes.checked_sub(baseline.bytes).ok_or_else(|| {
        format!(
            "regex side is smaller than baseline: baseline={} B, regex={} B",
            baseline.bytes, regex.bytes
        )
    })?;
    let regress = regress_live_bytes(&regex.map)?;

    println!("baseline stripped: {} B", baseline.bytes);
    println!("regex stripped:    {} B", regex.bytes);
    println!("regex delta:       {} B", delta);
    println!("regress link map:  {} B", regress);

    let baseline_shift = baseline.bytes as i128 - REFERENCE_BASELINE_BYTES as i128;
    if baseline_shift > BASELINE_ABSOLUTE_TOLERANCE as i128 {
        return Err(format!(
            "baseline side moved up by {baseline_shift} B: absolute baseline {} B exceeds \
             reference {} B + tolerance {} B; regex={} B, delta={} B",
            baseline.bytes,
            REFERENCE_BASELINE_BYTES,
            BASELINE_ABSOLUTE_TOLERANCE,
            regex.bytes,
            delta
        ));
    }
    if !(DELTA_MIN..=DELTA_MAX).contains(&delta) {
        let regex_shift = regex.bytes as i128 - REFERENCE_REGEX_BYTES as i128;
        let side = if baseline_shift.abs() > regex_shift.abs() {
            "baseline side"
        } else {
            "regex side"
        };
        return Err(format!(
            "{side} moved farther from the matched-pair reference: regex delta {delta} B is \
             outside {DELTA_MIN}..={DELTA_MAX} B; baseline={} B (shift {baseline_shift:+}), \
             regex={} B (shift {regex_shift:+})",
            baseline.bytes, regex.bytes
        ));
    }
    if !(REGRESS_MIN..=REGRESS_MAX).contains(&regress) {
        let direction = if regress < REGRESS_MIN { "down" } else { "up" };
        return Err(format!(
            "regex side moved {direction}: live regress contribution {regress} B is outside \
             {REGRESS_MIN}..={REGRESS_MAX} B; baseline={} B, regex={} B, delta={} B",
            baseline.bytes, regex.bytes, delta
        ));
    }
    Ok(())
}

fn assert_matched_sources() -> Result<(), String> {
    const EXTRA: &str = "  matched = new RegExp(pattern, flags).test(subject);\n";
    let without_call = REGEX_SOURCE.replacen(EXTRA, "", 1);
    if without_call != BASELINE_SOURCE || !REGEX_SOURCE.contains(EXTRA) {
        return Err(
            "regex-size sources drifted: with-regex.ts must differ by exactly the RegExp call"
                .to_string(),
        );
    }
    Ok(())
}

struct Linked {
    bytes: u64,
    map: PathBuf,
    stdout: Vec<u8>,
}

fn link_subject(
    work: &Path,
    id: &str,
    file: &str,
    source: &str,
    staticlib: &Path,
) -> Result<Linked, String> {
    let module = check_program(&[SourceFile::new(file, source)]).map_err(|diagnostics| {
        format!(
            "{file} did not check: {}",
            diagnostics
                .first()
                .map(|diagnostic| diagnostic.message.as_str())
                .unwrap_or("no diagnostic")
        )
    })?;
    let emitted = emit_c(&module).map_err(|error| format!("emit {file}: {error}"))?;
    let program = work.join(format!("{id}.c"));
    let entry = work.join(format!("{id}-entry.c"));
    let executable = work.join(id);
    let map = work.join(format!("{id}.map"));
    std::fs::write(&program, emitted.source)
        .map_err(|error| format!("write {}: {error}", program.display()))?;
    std::fs::write(&entry, AOT_ENTRY_C)
        .map_err(|error| format!("write {}: {error}", entry.display()))?;

    let compiler = std::env::var_os("CC").unwrap_or_else(|| "clang".into());
    let link = Command::new(&compiler)
        .args([
            "-std=c11",
            "-arch",
            "arm64",
            "-O2",
            "-fwrapv",
            "-ffp-contract=off",
            "-Wl,-dead_strip",
        ])
        .arg(format!("-Wl,-map,{}", map.display()))
        .arg(&program)
        .arg(&entry)
        .arg(staticlib)
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|error| format!("run compiler {:?}: {error}", compiler))?;
    if !link.status.success() {
        return Err(format!("link {id} failed:\n{}", tool_output_report(&link)));
    }
    let strip = Command::new("strip")
        .arg(&executable)
        .output()
        .map_err(|error| format!("strip {id}: {error}"))?;
    if !strip.status.success() {
        return Err(format!(
            "strip {id} failed:\n{}",
            String::from_utf8_lossy(&strip.stderr)
        ));
    }
    let bytes = std::fs::metadata(&executable)
        .map_err(|error| format!("stat {}: {error}", executable.display()))?
        .len();
    let run = Command::new(&executable)
        .output()
        .map_err(|error| format!("run {id}: {error}"))?;
    if !run.status.success() {
        return Err(format!(
            "{id} exited {}:\n{}",
            run.status,
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(Linked {
        bytes,
        map,
        stdout: run.stdout,
    })
}

fn regress_live_bytes(map: &Path) -> Result<u64, String> {
    let bytes = std::fs::read(map).map_err(|error| format!("read {}: {error}", map.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let mut regress_files = HashSet::new();
    let mut in_objects = false;
    for line in text.lines() {
        if line == "# Object files:" {
            in_objects = true;
            continue;
        }
        if in_objects && line.starts_with("# Sections:") {
            break;
        }
        if in_objects && line.contains("regress") {
            if let Some(index) = map_index(line) {
                regress_files.insert(index);
            }
        }
    }
    if regress_files.is_empty() {
        return Err("regex link map contains no regress object files".to_string());
    }

    let mut bytes = 0u64;
    let mut in_symbols = false;
    for line in text.lines() {
        if line == "# Symbols:" {
            in_symbols = true;
            continue;
        }
        if in_symbols && line.starts_with("# Dead Stripped Symbols:") {
            break;
        }
        if !in_symbols {
            continue;
        }
        let Some(index) = map_index(line) else {
            continue;
        };
        if !regress_files.contains(&index) {
            continue;
        }
        let Some(size) = line.split_whitespace().nth(1) else {
            continue;
        };
        bytes = bytes
            .checked_add(parse_hex(size)?)
            .ok_or_else(|| "regress link-map byte sum overflowed".to_string())?;
    }
    Ok(bytes)
}

fn map_index(line: &str) -> Option<u32> {
    let start = line.find('[')?;
    let end = line[start + 1..].find(']')? + start + 1;
    line[start + 1..end].trim().parse().ok()
}

fn parse_hex(value: &str) -> Result<u64, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("link-map size is not hexadecimal: {value}"))?;
    u64::from_str_radix(digits, 16)
        .map_err(|error| format!("invalid link-map size {value}: {error}"))
}

struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn new() -> Result<Self, String> {
        let path =
            std::env::temp_dir().join(format!("subscript-regex-size-{}", std::process::id()));
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_sources_are_a_matched_pair() {
        assert_matched_sources().expect("matched regex-size sources");
    }

    #[test]
    fn parses_apple_link_map_indices_and_hex_sizes() {
        assert_eq!(map_index("[  27] archive(regress.o)"), Some(27));
        assert_eq!(map_index("0x1 0x20 [  27] symbol"), Some(27));
        assert_eq!(parse_hex("0x20").expect("hex"), 32);
    }
}
