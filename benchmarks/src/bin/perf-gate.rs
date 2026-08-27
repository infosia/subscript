#![warn(missing_docs)]
//! The P4 performance-gate harness (`specs/blocks/compiler.md` §3, §9).
//!
//! It measures three subjects on the `a22-matrix-propagation` corpus
//! entry, in one process, in one session:
//!
//! - **C baseline** — `benchmarks/a22-baseline.c`, the hand-written C
//!   implementation of the same workload, compiled here with the
//!   platform C compiler at `-O2`.
//! - **ship-tier** — the entry's typed HIR emitted as C, compiled with
//!   the baseline flags, and linked with the runtime static library.
//! - **dev-JIT** — the entry through the JIT tier
//!   (`subscript_codegen::jit_bench`).
//!
//! Every subject times the workload execution only, inside its own
//! process, with a monotonic clock: the C baseline times its workload
//! function, the ship binary times its `subscript_export_main` call, and the
//! JIT times its `main` call. Compilation, linking, process start-up,
//! Context creation, global initialization, and I/O are all outside
//! the timed span.
//!
//! Before any timing is reported, every subject's stdout bytes are
//! compared against the frozen golden
//! `corpus/accept/a22-matrix-propagation.expected`; a mismatch aborts
//! the run without a report, because subjects that compute different
//! things cannot be compared.
//!
//! Usage:
//! `cargo run --offline --release -p subscript-benchmarks --bin perf-gate [-- --warmup N --timed M]`
//!
//! # Warm-up
//!
//! §9 fixes a floor of 3 discarded warm-up runs and 11 timed runs, not
//! a ceiling, and invalidates a run whose spread exceeds ±20% of the
//! median. The floor is expressed in runs, so a subject whose workload
//! is short can still be inside the CPU's frequency/core-placement ramp
//! when its warm-up ends: its samples then decay monotonically instead
//! of scattering. The report prints every timed sample in order so that
//! case is recognizable, and the answer is the one §9 gives — redo the
//! run, with `--warmup` raised until every subject is in steady state.
//!
//! Exit status: 0 when the §3 dev-JIT threshold is met, 1 when it is
//! missed (the report is still printed — a missed threshold is a
//! measurement, not a harness failure), 2 when the measurement is void
//! (output mismatch, machine too noisy, or a toolchain error).

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use subscript_codegen::{emit_c, jit_bench, runtime_staticlib_path, tool_output_report};
use subscript_compiler::{check_program, SourceFile};

/// The corpus entry under measurement (`specs/blocks/corpus.md` §4).
const ENTRY_ID: &str = "a22-matrix-propagation";

/// The entry's source, compiled into the harness so the measured
/// program is exactly the committed corpus file.
const ENTRY_SOURCE: &str = include_str!("../../../corpus/accept/a22-matrix-propagation.ts");

/// The entry's frozen golden output (`specs/blocks/compiler.md` §2).
const ENTRY_GOLDEN: &[u8] =
    include_bytes!("../../../corpus/accept/a22-matrix-propagation.expected");

/// The hand-written C baseline, written out and compiled at run time.
const BASELINE_C: &str = include_str!("../../a22-baseline.c");

/// The timing entry program linked with the ship-tier C translation unit.
const SHIP_BENCH_ENTRY_C: &str = concat!(
    include_str!("../../../runtime/include/subscript_runtime.h"),
    include_str!("../../aot-entry.c")
);

/// Flags the C baseline is compiled with. `-O2` is the criterion's
/// (§3); `-ffp-contract=off` matches the language's f32 arithmetic,
/// which never contracts a multiply-add into an FMA.
const BASELINE_CFLAGS: [&str; 2] = ["-O2", "-ffp-contract=off"];

/// §3: dev-JIT must be within this multiple of the C baseline.
const JIT_LIMIT: f64 = 4.0;

/// §9: a spread wider than this fraction of the median invalidates the
/// run — the machine was too noisy and the measurement is redone.
const NOISE_LIMIT: f64 = 0.20;

/// Default number of discarded warm-up runs (§9 requires at least 3).
const DEFAULT_WARMUP: usize = 3;

/// Default number of timed runs (§9 requires at least 11).
const DEFAULT_TIMED: usize = 11;

/// A measurement that could not be produced. The harness never panics;
/// every failure path ends here.
type Fail = String;

/// One measured subject: its timed samples and the bytes it printed.
struct Subject {
    /// Display name in the report.
    name: &'static str,
    /// What exactly was timed, stated in the report.
    span: &'static str,
    /// Exact stdout bytes the workload produced.
    stdout: Vec<u8>,
    /// One duration per timed run, warm-up already discarded.
    samples: Vec<Duration>,
    /// Time spent preparing executable code, reported but never gated.
    prepare: Vec<(&'static str, Duration)>,
}

/// Median, minimum, and maximum of a subject's samples, in seconds.
struct Stats {
    /// Median sample (the reported figure, §9).
    median: f64,
    /// Fastest sample.
    min: f64,
    /// Slowest sample.
    max: f64,
}

impl Stats {
    /// Computes the statistics of `samples`.
    ///
    /// The median of an even count is the mean of the two middle
    /// samples.
    fn of(samples: &[Duration]) -> Result<Stats, Fail> {
        if samples.is_empty() {
            return Err("a subject produced no timed samples".to_string());
        }
        let mut secs: Vec<f64> = samples.iter().map(Duration::as_secs_f64).collect();
        secs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = secs.len() / 2;
        let median = match secs.len() % 2 {
            1 => *secs.get(mid).unwrap_or(&0.0),
            _ => (secs.get(mid - 1).unwrap_or(&0.0) + secs.get(mid).unwrap_or(&0.0)) / 2.0,
        };
        Ok(Stats {
            median,
            min: *secs.first().unwrap_or(&0.0),
            max: *secs.last().unwrap_or(&0.0),
        })
    }

    /// Largest relative deviation of `min`/`max` from the median.
    fn spread(&self) -> f64 {
        if self.median <= 0.0 {
            return f64::INFINITY;
        }
        let high = (self.max - self.median) / self.median;
        let low = (self.median - self.min) / self.median;
        high.max(low)
    }
}

/// Formats a duration in milliseconds with three decimals.
fn ms(seconds: f64) -> String {
    format!("{:.3} ms", seconds * 1000.0)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("bench: {e}");
            ExitCode::from(2)
        }
    }
}

/// Parses the command line into `(warmup, timed)`.
fn parse_args() -> Result<(usize, usize), Fail> {
    let mut warmup = DEFAULT_WARMUP;
    let mut timed = DEFAULT_TIMED;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let flag = args.get(i).map(String::as_str).unwrap_or_default();
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        let parsed: usize = value
            .parse()
            .map_err(|_| format!("{flag}: `{value}` is not a count"))?;
        match flag {
            "--warmup" => warmup = parsed,
            "--timed" => timed = parsed,
            other => return Err(format!("unknown argument `{other}`")),
        }
        i += 2;
    }
    if warmup < 3 || timed < 11 {
        return Err(format!(
            "the methodology fixes a floor of 3 warm-up and 11 timed runs (compiler.md §9); got {warmup} and {timed}"
        ));
    }
    Ok((warmup, timed))
}

/// Measures every subject, verifies the outputs, and prints the report.
fn run() -> Result<ExitCode, Fail> {
    if cfg!(debug_assertions) {
        return Err(
            "this is a debug build: the runtime the tiers call would be unoptimized. \
             Re-run with `cargo run --offline --release -p subscript-benchmarks --bin perf-gate`"
                .to_string(),
        );
    }
    let (warmup, timed) = parse_args()?;
    let workdir = WorkDir::new()?;
    let files = vec![SourceFile::new(format!("{ENTRY_ID}.ts"), ENTRY_SOURCE)];

    let c = measure_c(workdir.path(), warmup, timed)?;
    let ship = measure_ship(&files, workdir.path(), warmup, timed)?;
    let jit = measure_jit(&files, warmup, timed)?;
    let subjects: [&Subject; 3] = [&c, &ship, &jit];

    for s in subjects {
        if s.stdout != ENTRY_GOLDEN {
            return Err(format!(
                "{} printed {:?}, the golden is {:?}; the subjects do not compute the same thing, so no timing is reported",
                s.name,
                String::from_utf8_lossy(&s.stdout),
                String::from_utf8_lossy(ENTRY_GOLDEN)
            ));
        }
    }

    let mut report = String::new();
    let outcome = write_report(&mut report, &subjects, warmup, timed)?;
    let mut out = std::io::stdout();
    out.write_all(report.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| format!("writing the report: {e}"))?;
    Ok(outcome)
}

/// Builds the report and returns the process exit status it implies.
///
/// The report contains the C baseline, the ship tier, and dev-JIT.
/// The retained §3 dev-JIT threshold gates the run.
fn write_report(
    report: &mut String,
    subjects: &[&Subject],
    warmup: usize,
    timed: usize,
) -> Result<ExitCode, Fail> {
    let stats: Vec<Stats> = subjects
        .iter()
        .map(|s| Stats::of(&s.samples))
        .collect::<Result<_, _>>()?;
    let baseline = stats
        .first()
        .ok_or_else(|| "no baseline statistics".to_string())?
        .median;
    if baseline <= 0.0 {
        return Err("the C baseline measured zero time; the clock is unusable".to_string());
    }

    let w = |r: &mut String, line: std::fmt::Arguments<'_>| -> Result<(), Fail> {
        writeln!(r, "{line}").map_err(|e| format!("formatting the report: {e}"))
    };

    w(report, format_args!("== subscript P4 performance gate =="))?;
    w(
        report,
        format_args!("entry:       corpus/accept/{ENTRY_ID}.ts"),
    )?;
    w(
        report,
        format_args!(
            "host:        {} / {}, release build",
            std::env::consts::ARCH,
            std::env::consts::OS
        ),
    )?;
    w(
        report,
        format_args!(
            "C compiler:  {} [{}]",
            cc_version(),
            BASELINE_CFLAGS.join(" ")
        ),
    )?;
    w(
        report,
        format_args!(
            "procedure:   {warmup} warm-up runs discarded, {timed} timed runs, median reported"
        ),
    )?;
    w(
        report,
        format_args!("output:      every subject matches corpus/accept/{ENTRY_ID}.expected"),
    )?;
    w(report, format_args!(""))?;
    w(
        report,
        format_args!(
            "{:<10} {:>12} {:>12} {:>12} {:>9} {:>10}",
            "subject", "median", "min", "max", "spread", "vs C"
        ),
    )?;
    for (s, st) in subjects.iter().zip(stats.iter()) {
        w(
            report,
            format_args!(
                "{:<10} {:>12} {:>12} {:>12} {:>8.1}% {:>9.2}x",
                s.name,
                ms(st.median),
                ms(st.min),
                ms(st.max),
                st.spread() * 100.0,
                st.median / baseline
            ),
        )?;
    }

    w(report, format_args!(""))?;
    w(
        report,
        format_args!("timed runs in order (ms), so a warm-up ramp is visible as such:"),
    )?;
    for s in subjects.iter() {
        let mut line = String::new();
        for d in &s.samples {
            let _ = write!(line, " {:.3}", d.as_secs_f64() * 1000.0);
        }
        w(report, format_args!("  {:<10}{}", s.name, line))?;
    }

    w(report, format_args!(""))?;
    w(report, format_args!("timed span per subject:"))?;
    for s in subjects.iter() {
        w(report, format_args!("  {:<10} {}", s.name, s.span))?;
    }

    w(report, format_args!(""))?;
    w(
        report,
        format_args!("preparation time (reported, not gated - compiler.md §9):"),
    )?;
    for s in subjects.iter() {
        for (label, d) in &s.prepare {
            w(
                report,
                format_args!("  {:<10} {:<34} {}", s.name, label, ms(d.as_secs_f64())),
            )?;
        }
    }

    w(report, format_args!(""))?;
    w(
        report,
        format_args!("threshold (compiler.md §3, pre-registered):"),
    )?;
    let (Some(jit), Some(jit_stats)) = (subjects.get(2), stats.get(2)) else {
        return Err("the dev-JIT subject is missing from the report".to_string());
    };
    let jit_ratio = jit_stats.median / baseline;
    let threshold_met = jit_ratio <= JIT_LIMIT;
    w(
        report,
        format_args!(
            "  {:<10} {:>5.2}x of C, limit {:.2}x  {}",
            jit.name,
            jit_ratio,
            JIT_LIMIT,
            if threshold_met { "MET" } else { "MISSED" }
        ),
    )?;

    w(report, format_args!(""))?;
    let noisy: Vec<&str> = subjects
        .iter()
        .zip(stats.iter())
        .filter(|(_, st)| st.spread() > NOISE_LIMIT)
        .map(|(s, _)| s.name)
        .collect();
    if noisy.is_empty() {
        w(
            report,
            format_args!(
                "noise check: every subject's spread is within +/-{:.0}% of its median (compiler.md §9)",
                NOISE_LIMIT * 100.0
            ),
        )?;
        w(
            report,
            format_args!(
                "result:      {}",
                if threshold_met {
                    "the threshold was met"
                } else {
                    "a threshold was missed; compiler.md §3 names the backend decision this reopens"
                }
            ),
        )?;
        Ok(if threshold_met {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        })
    } else {
        w(
            report,
            format_args!(
                "noise check: FAILED for {} - spread wider than +/-{:.0}% of the median, so this run is void (compiler.md §9).\n             Redo it. Samples that decay monotonically are a warm-up ramp, not machine noise:\n             raise --warmup (§9's 3 is a floor) until every subject is in steady state.",
                noisy.join(", "),
                NOISE_LIMIT * 100.0
            ),
        )?;
        Ok(ExitCode::from(2))
    }
}

/// First line of the C compiler's version banner, for the report.
fn cc_version() -> String {
    Command::new(host_cc())
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Finds `name` on `PATH`, returning its full path when an executable
/// file exists. On Windows the `.exe` extension is tried as well.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) { &["", ".exe"] } else { &[""] };
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let cand = dir.join(format!("{name}{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Resolves the C compiler used for the C subjects. The compiler is clang
/// (compiler.md §11), matching the ship path: `$CC`
/// verbatim when set, else `clang` on `PATH`, else — on Windows only — the
/// standard LLVM install (`%ProgramFiles%\LLVM\bin\clang.exe`). Falls back
/// to the bare name `clang`, so a missing toolchain surfaces as a clear
/// error rather than a silent skip.
fn host_cc() -> std::ffi::OsString {
    if let Some(cc) = std::env::var_os("CC") {
        return cc;
    }
    if let Some(p) = find_on_path("clang") {
        return p.into_os_string();
    }
    #[cfg(windows)]
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        let llvm = PathBuf::from(pf).join("LLVM").join("bin").join("clang.exe");
        if llvm.is_file() {
            return llvm.into_os_string();
        }
    }
    "clang".into()
}

/// System import libraries the linked program needs on windows-msvc that
/// clang's own defaults do not supply. The runtime static library embeds
/// Rust `std`, which references these; `rustc` passes them automatically
/// when it links, so a manual clang link of the staticlib must add them
/// (mirrors `subscript_codegen::runtime_system_libraries`). Empty on every other
/// target.
fn runtime_system_libs() -> &'static [&'static str] {
    if cfg!(all(windows, target_env = "msvc")) {
        &[
            "-lkernel32",
            "-lntdll",
            "-luserenv",
            "-lws2_32",
            "-ldbghelp",
        ]
    } else {
        &[]
    }
}

/// A temporary directory outside the repository, removed on drop.
struct WorkDir {
    /// Directory path.
    path: PathBuf,
}

impl WorkDir {
    /// Creates the directory under the platform temporary directory.
    fn new() -> Result<WorkDir, Fail> {
        let path = std::env::temp_dir().join(format!("subscript-perf-gate-{}", std::process::id()));
        std::fs::create_dir_all(&path).map_err(|e| format!("temp dir: {e}"))?;
        Ok(WorkDir { path })
    }

    /// The directory path.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        // Best effort: a leftover temporary directory does not
        // invalidate a measurement.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Writes `contents` to `path`.
fn write_file(path: &Path, contents: &[u8]) -> Result<(), Fail> {
    std::fs::write(path, contents).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Runs a subject binary with the run counts and returns its stdout
/// bytes and its timed samples, parsed from the `sample <i> <ns>` lines
/// it writes to stderr.
fn run_subject(exe: &Path, warmup: usize, timed: usize) -> Result<(Vec<u8>, Vec<Duration>), Fail> {
    let out = Command::new(exe)
        .arg(warmup.to_string())
        .arg(timed.to_string())
        .output()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return Err(format!(
            "{} exited with {}: {}",
            exe.display(),
            out.status,
            stderr.trim()
        ));
    }
    let mut samples = Vec::with_capacity(timed);
    let mut stable = None;
    for line in stderr.lines() {
        let mut parts = line.split_whitespace();
        match (parts.next(), parts.next(), parts.next()) {
            (Some("sample"), Some(_), Some(ns)) => {
                let ns: u64 = ns
                    .parse()
                    .map_err(|_| format!("unreadable sample line `{line}`"))?;
                samples.push(Duration::from_nanos(ns));
            }
            (Some("checksum-stable"), Some(flag), None) => stable = Some(flag == "1"),
            _ => return Err(format!("unexpected line on stderr: `{line}`")),
        }
    }
    if stable != Some(true) {
        return Err(format!(
            "{} did not report a stable result across its runs",
            exe.display()
        ));
    }
    if samples.len() != timed {
        return Err(format!(
            "{} reported {} samples, expected {timed}",
            exe.display(),
            samples.len()
        ));
    }
    Ok((out.stdout, samples))
}

/// Compiles and measures the hand-written C baseline.
fn measure_c(dir: &Path, warmup: usize, timed: usize) -> Result<Subject, Fail> {
    let source = dir.join("a22-baseline.c");
    let exe = dir.join(format!("a22-baseline{}", std::env::consts::EXE_SUFFIX));
    write_file(&source, BASELINE_C.as_bytes())?;

    let started = Instant::now();
    let build = Command::new(host_cc())
        .args(BASELINE_CFLAGS)
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|e| format!("the platform C compiler could not be run: {e}"))?;
    let compile = started.elapsed();
    if !build.status.success() {
        return Err(format!(
            "compiling the C baseline failed:\n{}",
            tool_output_report(&build)
        ));
    }

    let (stdout, samples) = run_subject(&exe, warmup, timed)?;
    Ok(Subject {
        name: "C",
        span: "the workload call: array construction, 100 propagation iterations, checksum",
        stdout,
        samples,
        prepare: vec![("compile (cc)", compile)],
    })
}

/// Emits C from the entry's typed HIR (the ship tier — compiler.md §11),
/// compiles it with the same flags as the hand C baseline, links it with
/// the runtime static library, and measures it under the same harness
/// protocol (`specs/tracking/p4-performance.md`).
///
/// The emitted translation unit carries the language's semantics (C2
/// value-class copies, checked growable-array indexing and push growth,
/// f32-precision arithmetic) and calls the runtime for arrays, strings,
/// and Q14 formatting, so this measures the shipped path through clang,
/// timed on the whole `subscript_export_main` call.
fn measure_ship(
    files: &[SourceFile],
    dir: &Path,
    warmup: usize,
    timed: usize,
) -> Result<Subject, Fail> {
    let started = Instant::now();
    let module = check_program(files).map_err(|diags| {
        format!(
            "the entry did not check for C emission: {}",
            diags
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_else(|| "no diagnostic".to_string())
        )
    })?;
    let c_source = emit_c(&module)
        .map_err(|e| format!("C emission: {e}"))?
        .source;
    let emit = started.elapsed();

    let source = dir.join("a22-ship.c");
    let entry = dir.join("ship-entry.c");
    let exe = dir.join(format!("a22-ship{}", std::env::consts::EXE_SUFFIX));
    write_file(&source, c_source.as_bytes())?;
    write_file(&entry, SHIP_BENCH_ENTRY_C.as_bytes())?;
    let staticlib = runtime_staticlib_path().map_err(|e| format!("runtime static library: {e}"))?;

    let started = Instant::now();
    let build = Command::new(host_cc())
        // Match the ship path's dialect (run_c_aot, compiler block §11):
        // the emitted C is C11. The hand baseline keeps its §9-recorded
        // flags; this subject stands in for the ship tier, so it tracks
        // the ship tier's `-std`.
        .arg("-std=c11")
        .args(BASELINE_CFLAGS)
        // The emitted ship-tier C requires two's-complement signed
        // wrap; `-fwrapv` makes signed overflow defined (matching the
        // language and the run_c_aot gate build).
        .arg("-fwrapv")
        .arg(&source)
        .arg(&entry)
        .arg(&staticlib)
        .args(runtime_system_libs())
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|e| format!("the platform C compiler could not be run: {e}"))?;
    let compile = started.elapsed();
    if !build.status.success() {
        return Err(format!(
            "compiling the emitted C failed:\n{}",
            tool_output_report(&build)
        ));
    }

    let (stdout, samples) = run_subject(&exe, warmup, timed)?;
    Ok(Subject {
        name: "ship-tier",
        span: "the subscript_export_main call in the ship-tier binary (linked with the runtime)",
        stdout,
        samples,
        prepare: vec![("check + emit C", emit), ("compile + link (cc)", compile)],
    })
}

/// Measures the dev-tier JIT on the entry.
fn measure_jit(files: &[SourceFile], warmup: usize, timed: usize) -> Result<Subject, Fail> {
    let b = jit_bench(files, warmup, timed).map_err(|e| format!("dev-JIT run: {e}"))?;
    Ok(Subject {
        name: "dev-JIT",
        span: "the main call in this process",
        stdout: b.stdout,
        samples: b.samples,
        prepare: vec![("check + lower + finalize (JIT compile)", b.compile)],
    })
}
