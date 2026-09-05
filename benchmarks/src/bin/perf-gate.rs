#![warn(missing_docs)]
//! The P4 performance-gate harness (`specs/blocks/compiler.md` §3, §9).
//!
//! It measures three subjects on the `a22-matrix-propagation` and `collect`
//! workloads, in one process, in one session:
//!
//! - **C baseline** — each workload's hand-written C implementation,
//!   compiled here with the platform C compiler at `-O2`.
//! - **ship-tier** — each workload's typed HIR emitted as C, compiled with
//!   the baseline flags, and linked with the runtime static library.
//! - **dev-JIT** — each workload through the JIT tier
//!   (`subscript_codegen::jit_bench`).
//! - **dev iteration** — one complete check + lower + JIT-finalize of
//!   the changed `a22` source.
//! - **hot reload** — the one-function `ENTRYLESS_V1` → `ENTRYLESS_V2`
//!   body swap from `codegen/tests/reload.rs`.
//!
//! Every execution subject times the workload execution only, inside its own
//! process, with a monotonic clock: the C baseline times its workload
//! function, the ship binary times its `subscript_export_main` call, and the
//! JIT times its `main` call. Compilation, linking, process start-up, Context
//! creation, global initialization, and I/O are all outside those execution
//! spans. The two iteration subjects state their separate spans in the
//! report.
//!
//! Before any timing is reported, every `a22` subject's stdout bytes are
//! compared against the frozen golden. Every `collect` subject must return
//! the i32 checksum `1332546592`. A mismatch aborts the run without timing.
//!
//! The binary serves two runs:
//!
//! - The default is a §9 reporting run. Excessive spread withholds that
//!   subject's timing, voids the run, and returns status 2.
//! - `--gate` selects the automatic gate run. Spread stays visible but does
//!   not control the result. The §3 thresholds return status 0 or 1.
//!
//! Output or toolchain errors return status 2 in both runs.
//!
//! Usage:
//! `cargo run --offline --release -p subscript-benchmarks --bin perf-gate [-- [--gate] [--warmup N] [--timed M]]`
//!
//! # Warm-up
//!
//! §9 requires at least 3 discarded warm-up runs and 200 ms of measured
//! workload execution. It also requires at least 11 timed runs. The harness
//! continues each execution warm-up until both floors are met. §9 does not
//! state a separate warm-up rule for one-shot compilation, so this harness
//! applies the same two floors to repeated, complete compile/reload
//! observations and discards those observations; it does not turn a compile
//! into an artificial inner loop.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use subscript_codegen::{
    emit_c, jit_bench_with_warmup_floor, jit_compile_time, runtime_staticlib_path,
    runtime_system_libraries, tool_output_report, CCompilerStyle, ReloadSession,
};
use subscript_compiler::{check_program, SourceFile};

/// The entry's frozen golden output (`specs/blocks/compiler.md` §2).
const A22_GOLDEN: &[u8] = include_bytes!("../../../corpus/accept/a22-matrix-propagation.expected");

/// The required result of the allocation workload.
const COLLECT_CHECKSUM: i32 = 1_332_546_592;

/// The timing entry program linked with the ship-tier C translation unit.
const SHIP_BENCH_ENTRY_C: &str = concat!(
    include_str!("../../../runtime/include/subscript_runtime.h"),
    include_str!("../../aot-entry.c")
);

/// Flags the C baseline is compiled with. `-O2` is the criterion's
/// (§3); `-ffp-contract=off` matches the language's f32 arithmetic,
/// which never contracts a multiply-add into an FMA.
const BASELINE_CFLAGS: [&str; 2] = ["-O2", "-ffp-contract=off"];

/// §3: both dev iteration and a one-function hot reload must fit this budget.
const DEV_ITERATION_LIMIT: Duration = Duration::from_millis(20);

/// §9's reporting-run noise limit. The gate run reports it only.
const NOISE_LIMIT: f64 = 0.20;

/// Default number of discarded warm-up runs (§9 requires at least 3).
const DEFAULT_WARMUP: usize = 3;

/// Minimum sum of measured workload execution discarded as warm-up.
const WARMUP_FLOOR: Duration = Duration::from_millis(200);

/// Default number of timed runs (§9 requires at least 11).
const DEFAULT_TIMED: usize = 11;

/// The selected `perf-gate` run contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunMode {
    /// A §9 reporting run that withholds noisy timings and voids.
    Reporting,
    /// An automatic threshold gate that reports noise without voiding.
    Gate,
}

/// Parsed command-line arguments.
struct Args {
    /// The selected run contract.
    mode: RunMode,
    /// Minimum discarded warm-up runs.
    warmup: usize,
    /// Number of timed runs.
    timed: usize,
}

/// The result selected from thresholds and sample spread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunDecision {
    /// Every threshold was met.
    Passed,
    /// One threshold was missed.
    Missed,
    /// A reporting run contained an invalid timing.
    Void,
}

/// The format of one C baseline's timing protocol.
#[derive(Clone, Copy)]
enum BaselineProtocol {
    /// A third argument sets the warm-up floor. Times use nanoseconds.
    NanosecondsWithFloor,
    /// The binary enforces the warm-up floor. Times use seconds.
    SecondsWithInternalFloor,
}

/// The result contract for one workload.
#[derive(Clone, Copy)]
enum ExpectedOutput {
    /// Every subject must match these exact bytes.
    Bytes(&'static [u8]),
    /// Every subject must print this i32 checksum.
    I32(i32),
}

/// One performance-gate workload.
struct Workload {
    /// Short name in the report.
    name: &'static str,
    /// Source name in diagnostics and the report.
    source_path: &'static str,
    /// Source text compiled into the harness.
    source: &'static str,
    /// C baseline name in diagnostics and the report.
    baseline_path: &'static str,
    /// C baseline source text compiled into the harness.
    baseline_c: &'static str,
    /// C baseline argument and timing protocol.
    baseline_protocol: BaselineProtocol,
    /// Result that every subject must produce.
    expected_output: ExpectedOutput,
    /// Maximum ship-tier ratio to the C baseline, by host ISA.
    ship_limit: ShipLimit,
    /// Maximum dev-tier ratio to the C baseline.
    dev_limit: f64,
    /// Description of the C timed span.
    c_span: &'static str,
}

/// A ship-tier ceiling that differs by host instruction set
/// (`specs/blocks/compiler.md` §3). The emitted C is one text, and it does
/// not cost the same on both: §10a measures out-of-line growable-array
/// access and copy-heavy value-class parameter passing as costs clang
/// optimizes on arm64 and does not on x86-64. One number over two
/// instruction sets therefore states a criterion neither host owns.
#[derive(Debug, Clone, Copy)]
struct ShipLimit {
    /// The gated ratio on aarch64, where §3 keeps 1.5x.
    aarch64: f64,
    /// The gated ratio on x86-64.
    x86_64: f64,
}

impl ShipLimit {
    /// One ratio for every host, where the instruction set does not change
    /// the measured cost.
    const fn uniform(limit: f64) -> Self {
        Self {
            aarch64: limit,
            x86_64: limit,
        }
    }

    /// The ceiling for the host this binary runs on. A host that is
    /// neither aarch64 nor x86-64 reads the aarch64 number, and §3 has no
    /// measurement for it; the report names the host, so the reader sees
    /// which number applied.
    fn here(self) -> f64 {
        if cfg!(target_arch = "x86_64") {
            self.x86_64
        } else {
            self.aarch64
        }
    }

    /// Whether this workload's ceiling differs by host.
    fn is_scoped(self) -> bool {
        self.aarch64 != self.x86_64
    }
}

/// The workloads that the P4 gate measures.
const WORKLOADS: [Workload; 2] = [
    Workload {
        name: "a22",
        source_path: "corpus/accept/a22-matrix-propagation.ts",
        source: include_str!("../../../corpus/accept/a22-matrix-propagation.ts"),
        baseline_path: "benchmarks/a22-baseline.c",
        baseline_c: include_str!("../../a22-baseline.c"),
        baseline_protocol: BaselineProtocol::NanosecondsWithFloor,
        expected_output: ExpectedOutput::Bytes(A22_GOLDEN),
        ship_limit: ShipLimit {
            aarch64: 1.50,
            x86_64: 2.50,
        },
        dev_limit: 25.0,
        c_span: "the workload call: array construction, 100 propagation iterations, checksum",
    },
    Workload {
        name: "collect",
        source_path: "benchmarks/workloads/subscript/collect.ts",
        source: include_str!("../../workloads/subscript/collect.ts"),
        baseline_path: "benchmarks/workloads/c/collect.c",
        baseline_c: include_str!("../../workloads/c/collect.c"),
        baseline_protocol: BaselineProtocol::SecondsWithInternalFloor,
        expected_output: ExpectedOutput::I32(COLLECT_CHECKSUM),
        ship_limit: ShipLimit::uniform(7.50),
        dev_limit: 8.50,
        c_span: "the workload call: graph construction, explicit reclamation, traversal, checksum",
    },
];

/// First revision of the exact one-function reload shape exercised by
/// `codegen/tests/reload.rs::entryless_session_observes_an_accepted_body_swap`.
const RELOAD_V1: &str = "\
let initialized: i32 = 41;
export function frame(): void {
  print(`frame v1: ${initialized}`);
}
export function shutdown(): void {
  print(\"shutdown v1\");
}
";

/// Second revision of the reload shape. Only `frame`'s body changes.
const RELOAD_V2: &str = "\
let initialized: i32 = 41;
export function frame(): void {
  print(`frame v2: ${initialized + 1}`);
}
export function shutdown(): void {
  print(\"shutdown v1\");
}
";

/// Output proving that the newly finalized `frame` body is runnable.
const RELOAD_GOLDEN: &[u8] = b"frame v2: 42\n";

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
    /// Sum of the measured workload-call durations discarded as warm-up.
    warmup: Duration,
    /// Number of workload calls discarded as warm-up.
    warmup_iterations: usize,
    /// Time spent preparing executable code, reported but never gated.
    prepare: Vec<(&'static str, Duration)>,
}

/// The three execution subjects for one workload.
struct WorkloadRun {
    /// Workload contract and thresholds.
    workload: &'static Workload,
    /// C, ship-tier, and dev-JIT subjects, in that order.
    subjects: [Subject; 3],
}

/// One one-shot timing subject measured by repeating the complete operation.
struct OneShotSubject {
    /// Display name in the report.
    name: &'static str,
    /// What exactly was timed, stated in the report.
    span: &'static str,
    /// One duration per timed complete operation.
    samples: Vec<Duration>,
    /// Sum of discarded complete-operation durations.
    warmup: Duration,
    /// Number of complete operations discarded as warm-up.
    warmup_iterations: usize,
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

/// Selects the run result from the threshold result and observed spreads.
fn decide_run(mode: RunMode, thresholds_met: bool, spreads: &[f64]) -> RunDecision {
    if mode == RunMode::Reporting && spreads.iter().any(|spread| *spread > NOISE_LIMIT) {
        RunDecision::Void
    } else if thresholds_met {
        RunDecision::Passed
    } else {
        RunDecision::Missed
    }
}

/// Whether a reporting run must withhold this sample set.
fn timing_is_withheld(mode: RunMode, spread: f64) -> bool {
    mode == RunMode::Reporting && spread > NOISE_LIMIT
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

/// Parses the command line.
fn parse_args() -> Result<Args, Fail> {
    parse_args_from(std::env::args().skip(1))
}

/// Parses command-line values from an iterator.
fn parse_args_from(args: impl IntoIterator<Item = String>) -> Result<Args, Fail> {
    let mut mode = RunMode::Reporting;
    let mut warmup = DEFAULT_WARMUP;
    let mut timed = DEFAULT_TIMED;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--gate" => {
                if mode == RunMode::Gate {
                    return Err("`--gate` cannot be repeated".to_string());
                }
                mode = RunMode::Gate;
            }
            "--warmup" | "--timed" => {
                let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
                let parsed: usize = value
                    .parse()
                    .map_err(|_| format!("{flag}: `{value}` is not a count"))?;
                if flag == "--warmup" {
                    warmup = parsed;
                } else {
                    timed = parsed;
                }
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    if warmup < 3 || timed < 11 {
        return Err(format!(
            "the methodology fixes a floor of 3 warm-up and 11 timed runs (compiler.md §9); got {warmup} and {timed}"
        ));
    }
    Ok(Args {
        mode,
        warmup,
        timed,
    })
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
    let args = parse_args()?;
    let warmup = args.warmup;
    let timed = args.timed;
    let workdir = WorkDir::new()?;
    let mut workload_runs = Vec::with_capacity(WORKLOADS.len());
    for workload in &WORKLOADS {
        let files = vec![SourceFile::new(workload.source_path, workload.source)];
        let subjects = [
            measure_c(workload, workdir.path(), warmup, timed)?,
            measure_ship(workload, &files, workdir.path(), warmup, timed)?,
            measure_jit(&files, warmup, timed)?,
        ];
        validate_execution_subjects(workload, &subjects, warmup)?;
        workload_runs.push(WorkloadRun { workload, subjects });
    }

    let a22 = WORKLOADS
        .first()
        .ok_or_else(|| "the a22 workload is missing".to_string())?;
    let a22_files = vec![SourceFile::new(a22.source_path, a22.source)];
    let iteration = measure_one_shot(
        "dev-iteration",
        "changed a22 source: check + lower + finalize",
        warmup,
        timed,
        || jit_compile_time(&a22_files).map_err(|e| format!("dev iteration: {e}")),
    )?;
    let reload = measure_reload(warmup, timed)?;
    let one_shots: [&OneShotSubject; 2] = [&iteration, &reload];

    for s in one_shots {
        if s.warmup_iterations < warmup || s.warmup < WARMUP_FLOOR {
            return Err(format!(
                "{} warmed up for {} across {} complete operations; the one-shot interpretation of compiler.md §9 requires at least {} across at least {warmup} operations",
                s.name,
                ms(s.warmup.as_secs_f64()),
                s.warmup_iterations,
                ms(WARMUP_FLOOR.as_secs_f64())
            ));
        }
    }

    let mut report = String::new();
    let outcome = write_report(
        &mut report,
        &workload_runs,
        &one_shots,
        args.mode,
        warmup,
        timed,
    )?;
    let mut out = std::io::stdout();
    out.write_all(report.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| format!("writing the report: {e}"))?;
    Ok(outcome)
}

/// Verifies the warm-up and result contracts for one workload.
fn validate_execution_subjects(
    workload: &Workload,
    subjects: &[Subject; 3],
    warmup: usize,
) -> Result<(), Fail> {
    for subject in subjects {
        if subject.warmup_iterations < warmup || subject.warmup < WARMUP_FLOOR {
            return Err(format!(
                "{}/{} warmed up for {} across {} iterations; compiler.md §9 requires at least {} across at least {warmup} iterations",
                workload.name,
                subject.name,
                ms(subject.warmup.as_secs_f64()),
                subject.warmup_iterations,
                ms(WARMUP_FLOOR.as_secs_f64())
            ));
        }
    }

    match workload.expected_output {
        ExpectedOutput::Bytes(expected) => {
            for subject in subjects {
                if subject.stdout != expected {
                    return Err(format!(
                        "{}/{} printed {:?}, expected {:?}; no timing is reported for this workload",
                        workload.name,
                        subject.name,
                        String::from_utf8_lossy(&subject.stdout),
                        String::from_utf8_lossy(expected)
                    ));
                }
            }
        }
        ExpectedOutput::I32(expected) => {
            let results = subjects
                .iter()
                .map(|subject| {
                    let text = String::from_utf8_lossy(&subject.stdout);
                    let result = text.trim().parse::<i32>().map_err(|_| {
                        format!(
                            "{}/{} printed {:?}, not an i32 checksum; no timing is reported for this workload",
                            workload.name, subject.name, text
                        )
                    })?;
                    Ok((subject.name, result))
                })
                .collect::<Result<Vec<_>, Fail>>()?;
            if results.iter().any(|(_, result)| *result != expected) {
                let observed = results
                    .iter()
                    .map(|(name, result)| format!("{name}={result}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "{} subjects did not agree on required i32 checksum {expected}: {observed}; no timing is reported for this workload",
                    workload.name
                ));
            }
        }
    }
    Ok(())
}

/// Builds the report and returns the process exit status it implies.
///
/// The report contains all execution and iteration subjects.
fn write_report(
    report: &mut String,
    workload_runs: &[WorkloadRun],
    one_shots: &[&OneShotSubject],
    mode: RunMode,
    warmup: usize,
    timed: usize,
) -> Result<ExitCode, Fail> {
    let workload_stats: Vec<Vec<Stats>> = workload_runs
        .iter()
        .map(|run| {
            run.subjects
                .iter()
                .map(|subject| Stats::of(&subject.samples))
                .collect::<Result<_, _>>()
        })
        .collect::<Result<_, _>>()?;
    let one_shot_stats: Vec<Stats> = one_shots
        .iter()
        .map(|s| Stats::of(&s.samples))
        .collect::<Result<_, _>>()?;
    let spreads = workload_stats
        .iter()
        .flatten()
        .chain(one_shot_stats.iter())
        .map(Stats::spread)
        .collect::<Vec<_>>();

    let w = |r: &mut String, line: std::fmt::Arguments<'_>| -> Result<(), Fail> {
        writeln!(r, "{line}").map_err(|e| format!("formatting the report: {e}"))
    };

    w(report, format_args!("== subscript P4 performance gate =="))?;
    w(report, format_args!("workloads:"))?;
    for run in workload_runs {
        w(
            report,
            format_args!(
                "  {:<8} source {}, C baseline {}",
                run.workload.name, run.workload.source_path, run.workload.baseline_path
            ),
        )?;
    }
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
            "procedure:   at least {warmup} warm-up runs and {} measured warm-up, {timed} timed runs, median reported",
            ms(WARMUP_FLOOR.as_secs_f64())
        ),
    )?;
    w(report, format_args!("output:"))?;
    for run in workload_runs {
        match run.workload.expected_output {
            ExpectedOutput::Bytes(_) => w(
                report,
                format_args!(
                    "  {:<8} every subject matches corpus/accept/a22-matrix-propagation.expected",
                    run.workload.name
                ),
            )?,
            ExpectedOutput::I32(expected) => w(
                report,
                format_args!(
                    "  {:<8} every subject agrees on i32 checksum {expected}",
                    run.workload.name
                ),
            )?,
        }
    }
    w(report, format_args!(""))?;
    w(
        report,
        format_args!(
            "{:<9} {:<10} {:>12} {:>12} {:>12} {:>9} {:>10}",
            "workload", "subject", "median", "min", "max", "spread", "vs C"
        ),
    )?;
    for (run, stats) in workload_runs.iter().zip(workload_stats.iter()) {
        let baseline_stats = stats
            .first()
            .ok_or_else(|| format!("{} has no baseline statistics", run.workload.name))?;
        let baseline = baseline_stats.median;
        if baseline <= 0.0 {
            return Err(format!(
                "{} C baseline measured zero time; the clock is unusable",
                run.workload.name
            ));
        }
        let baseline_withheld = timing_is_withheld(mode, baseline_stats.spread());
        for (subject, subject_stats) in run.subjects.iter().zip(stats.iter()) {
            let withheld = timing_is_withheld(mode, subject_stats.spread());
            if withheld {
                w(
                    report,
                    format_args!(
                        "{:<9} {:<10} {:>12} {:>12} {:>12} {:>8.1}% {:>10}",
                        run.workload.name,
                        subject.name,
                        "withheld",
                        "withheld",
                        "withheld",
                        subject_stats.spread() * 100.0,
                        "withheld"
                    ),
                )?;
            } else if baseline_withheld {
                w(
                    report,
                    format_args!(
                        "{:<9} {:<10} {:>12} {:>12} {:>12} {:>8.1}% {:>10}",
                        run.workload.name,
                        subject.name,
                        ms(subject_stats.median),
                        ms(subject_stats.min),
                        ms(subject_stats.max),
                        subject_stats.spread() * 100.0,
                        "withheld"
                    ),
                )?;
            } else {
                w(
                    report,
                    format_args!(
                        "{:<9} {:<10} {:>12} {:>12} {:>12} {:>8.1}% {:>9.2}x",
                        run.workload.name,
                        subject.name,
                        ms(subject_stats.median),
                        ms(subject_stats.min),
                        ms(subject_stats.max),
                        subject_stats.spread() * 100.0,
                        subject_stats.median / baseline
                    ),
                )?;
            }
        }
    }

    w(report, format_args!(""))?;
    w(report, format_args!("measured warm-up per subject:"))?;
    for run in workload_runs {
        for subject in &run.subjects {
            w(
                report,
                format_args!(
                    "  {:<9} {:<10} {:>12} across {} iterations",
                    run.workload.name,
                    subject.name,
                    ms(subject.warmup.as_secs_f64()),
                    subject.warmup_iterations
                ),
            )?;
        }
    }

    w(report, format_args!(""))?;
    w(
        report,
        format_args!("timed runs in order (ms), so a warm-up ramp is visible as such:"),
    )?;
    for (run, stats) in workload_runs.iter().zip(workload_stats.iter()) {
        for (subject, subject_stats) in run.subjects.iter().zip(stats.iter()) {
            if timing_is_withheld(mode, subject_stats.spread()) {
                w(
                    report,
                    format_args!(
                        "  {:<9} {:<10} withheld (noise)",
                        run.workload.name, subject.name
                    ),
                )?;
                continue;
            }
            let mut line = String::new();
            for duration in &subject.samples {
                let _ = write!(line, " {:.3}", duration.as_secs_f64() * 1000.0);
            }
            w(
                report,
                format_args!("  {:<9} {:<10}{}", run.workload.name, subject.name, line),
            )?;
        }
    }

    w(report, format_args!(""))?;
    w(report, format_args!("timed span per subject:"))?;
    for run in workload_runs {
        for subject in &run.subjects {
            w(
                report,
                format_args!(
                    "  {:<9} {:<10} {}",
                    run.workload.name, subject.name, subject.span
                ),
            )?;
        }
    }

    w(report, format_args!(""))?;
    w(report, format_args!("one-shot iteration timings:"))?;
    w(
        report,
        format_args!(
            "{:<15} {:>12} {:>12} {:>12} {:>9}",
            "subject", "median", "min", "max", "spread"
        ),
    )?;
    for (s, st) in one_shots.iter().zip(one_shot_stats.iter()) {
        if timing_is_withheld(mode, st.spread()) {
            w(
                report,
                format_args!(
                    "{:<15} {:>12} {:>12} {:>12} {:>8.1}%",
                    s.name,
                    "withheld",
                    "withheld",
                    "withheld",
                    st.spread() * 100.0
                ),
            )?;
        } else {
            w(
                report,
                format_args!(
                    "{:<15} {:>12} {:>12} {:>12} {:>8.1}%",
                    s.name,
                    ms(st.median),
                    ms(st.min),
                    ms(st.max),
                    st.spread() * 100.0
                ),
            )?;
        }
    }
    w(report, format_args!(""))?;
    w(report, format_args!("one-shot measured warm-up:"))?;
    for s in one_shots.iter() {
        w(
            report,
            format_args!(
                "  {:<15} {:>12} across {} complete operations",
                s.name,
                ms(s.warmup.as_secs_f64()),
                s.warmup_iterations
            ),
        )?;
    }
    w(report, format_args!(""))?;
    w(report, format_args!("one-shot timed runs in order (ms):"))?;
    for (s, st) in one_shots.iter().zip(one_shot_stats.iter()) {
        if timing_is_withheld(mode, st.spread()) {
            w(report, format_args!("  {:<15} withheld (noise)", s.name))?;
            continue;
        }
        let mut line = String::new();
        for d in &s.samples {
            let _ = write!(line, " {:.3}", d.as_secs_f64() * 1000.0);
        }
        w(report, format_args!("  {:<15}{}", s.name, line))?;
    }
    w(report, format_args!(""))?;
    w(report, format_args!("one-shot timed span per subject:"))?;
    for s in one_shots.iter() {
        w(report, format_args!("  {:<15} {}", s.name, s.span))?;
    }
    w(report, format_args!(""))?;
    w(
        report,
        format_args!(
            "one-shot warm-up decision: compiler.md §9 gives no separate rule for a one-shot compile."
        ),
    )?;
    w(
        report,
        format_args!(
            "  This harness repeats and discards complete operations until both its 3-operation and {} measured-span floors are met, then times {timed} fresh complete operations; no inner loop is added.",
            ms(WARMUP_FLOOR.as_secs_f64())
        ),
    )?;

    w(report, format_args!(""))?;
    w(
        report,
        format_args!("auxiliary preparation time (single observations, reported, not gated):"),
    )?;
    for run in workload_runs {
        for subject in &run.subjects {
            for (label, duration) in &subject.prepare {
                w(
                    report,
                    format_args!(
                        "  {:<9} {:<10} {:<34} {}",
                        run.workload.name,
                        subject.name,
                        label,
                        ms(duration.as_secs_f64())
                    ),
                )?;
            }
        }
    }

    w(report, format_args!(""))?;
    w(
        report,
        format_args!("thresholds (compiler.md §3, pre-registered):"),
    )?;
    for workload in WORKLOADS.iter().filter(|w| w.ship_limit.is_scoped()) {
        w(
            report,
            format_args!(
                "  {} ship-tier is scoped by host ISA: aarch64 {:.2}x, x86-64 {:.2}x. \
                 §10a names the x86-64 cost, and it is open.",
                workload.name, workload.ship_limit.aarch64, workload.ship_limit.x86_64
            ),
        )?;
    }
    let (Some(iteration), Some(iteration_stats), Some(reload), Some(reload_stats)) = (
        one_shots.first(),
        one_shot_stats.first(),
        one_shots.get(1),
        one_shot_stats.get(1),
    ) else {
        return Err("an iteration subject is missing from the report".to_string());
    };
    let mut thresholds_met = true;
    for (run, stats) in workload_runs.iter().zip(workload_stats.iter()) {
        let (Some(baseline), Some(ship), Some(dev)) = (stats.first(), stats.get(1), stats.get(2))
        else {
            return Err(format!(
                "{} tier statistics are missing from the report",
                run.workload.name
            ));
        };
        if baseline.median <= 0.0 {
            return Err(format!(
                "{} C baseline measured zero time; the clock is unusable",
                run.workload.name
            ));
        }
        let ship_ratio = ship.median / baseline.median;
        let dev_ratio = dev.median / baseline.median;
        let ship_limit = run.workload.ship_limit.here();
        let ship_met = ship_ratio <= ship_limit;
        let dev_met = dev_ratio <= run.workload.dev_limit;
        thresholds_met = thresholds_met && ship_met && dev_met;
        let baseline_withheld = timing_is_withheld(mode, baseline.spread());
        if baseline_withheld || timing_is_withheld(mode, ship.spread()) {
            w(
                report,
                format_args!(
                    "  {:<9} {:<15} {:>16}, limit {:.2}x  WITHHELD",
                    run.workload.name, "ship-tier", "noise", ship_limit
                ),
            )?;
        } else {
            w(
                report,
                format_args!(
                    "  {:<9} {:<15} {:>7.2}x of C, limit {:.2}x  {}",
                    run.workload.name,
                    "ship-tier",
                    ship_ratio,
                    ship_limit,
                    if ship_met { "MET" } else { "MISSED" }
                ),
            )?;
        }
        if baseline_withheld || timing_is_withheld(mode, dev.spread()) {
            w(
                report,
                format_args!(
                    "  {:<9} {:<15} {:>16}, limit {:.2}x  WITHHELD",
                    run.workload.name, "dev-JIT", "noise", run.workload.dev_limit
                ),
            )?;
        } else {
            w(
                report,
                format_args!(
                    "  {:<9} {:<15} {:>7.2}x of C, limit {:.2}x  {}",
                    run.workload.name,
                    "dev-JIT",
                    dev_ratio,
                    run.workload.dev_limit,
                    if dev_met { "MET" } else { "MISSED" }
                ),
            )?;
        }
    }
    let iteration_met = iteration_stats.median <= DEV_ITERATION_LIMIT.as_secs_f64();
    let reload_met = reload_stats.median <= DEV_ITERATION_LIMIT.as_secs_f64();
    thresholds_met = thresholds_met && iteration_met && reload_met;
    if timing_is_withheld(mode, iteration_stats.spread()) {
        w(
            report,
            format_args!(
                "  {:<9} {:<15} {:>10}, limit {}  WITHHELD",
                "a22",
                iteration.name,
                "noise",
                ms(DEV_ITERATION_LIMIT.as_secs_f64())
            ),
        )?;
    } else {
        w(
            report,
            format_args!(
                "  {:<9} {:<15} {:>10}, limit {}  {}",
                "a22",
                iteration.name,
                ms(iteration_stats.median),
                ms(DEV_ITERATION_LIMIT.as_secs_f64()),
                if iteration_met { "MET" } else { "MISSED" }
            ),
        )?;
    }
    if timing_is_withheld(mode, reload_stats.spread()) {
        w(
            report,
            format_args!(
                "  {:<9} {:<15} {:>10}, limit {}  WITHHELD",
                "a22",
                reload.name,
                "noise",
                ms(DEV_ITERATION_LIMIT.as_secs_f64())
            ),
        )?;
    } else {
        w(
            report,
            format_args!(
                "  {:<9} {:<15} {:>10}, limit {}  {}",
                "a22",
                reload.name,
                ms(reload_stats.median),
                ms(DEV_ITERATION_LIMIT.as_secs_f64()),
                if reload_met { "MET" } else { "MISSED" }
            ),
        )?;
    }
    w(report, format_args!(""))?;
    let mut noisy = Vec::new();
    for (run, stats) in workload_runs.iter().zip(workload_stats.iter()) {
        noisy.extend(
            run.subjects
                .iter()
                .zip(stats.iter())
                .filter(|(_, subject_stats)| subject_stats.spread() > NOISE_LIMIT)
                .map(|(subject, _)| format!("{}/{}", run.workload.name, subject.name)),
        );
    }
    noisy.extend(
        one_shots
            .iter()
            .zip(one_shot_stats.iter())
            .filter(|(_, subject_stats)| subject_stats.spread() > NOISE_LIMIT)
            .map(|(subject, _)| format!("a22/{}", subject.name)),
    );
    if noisy.is_empty() {
        w(
            report,
            format_args!(
                "noise check: every subject's spread is within +/-{:.0}% of its median (compiler.md §9)",
                NOISE_LIMIT * 100.0
            ),
        )?;
    } else if mode == RunMode::Reporting {
        w(
            report,
            format_args!(
                "noise check: FAILED for {} - spread wider than +/-{:.0}% of the median, so this run is void (compiler.md §9).\n             Redo it when the machine is under lower load.",
                noisy.join(", "),
                NOISE_LIMIT * 100.0
            ),
        )?;
    } else {
        w(
            report,
            format_args!(
                "noise check: {} exceeded +/-{:.0}% of the median; this is reported and not gated (compiler.md §9)",
                noisy.join(", "),
                NOISE_LIMIT * 100.0
            ),
        )?;
    }
    match decide_run(mode, thresholds_met, &spreads) {
        RunDecision::Passed => {
            w(
                report,
                format_args!("result:      all gated criteria were met"),
            )?;
            Ok(ExitCode::SUCCESS)
        }
        RunDecision::Missed => {
            w(
                report,
                format_args!(
                    "result:      a gated criterion was missed; compiler.md §3 names the backend decision this reopens"
                ),
            )?;
            Ok(ExitCode::from(1))
        }
        RunDecision::Void => Ok(ExitCode::from(2)),
    }
}

/// Repeats a complete one-shot operation for §9 warm-up and timed samples.
fn measure_one_shot<F>(
    name: &'static str,
    span: &'static str,
    warmup: usize,
    timed: usize,
    mut measure: F,
) -> Result<OneShotSubject, Fail>
where
    F: FnMut() -> Result<Duration, Fail>,
{
    let mut warmup_elapsed = Duration::ZERO;
    let mut warmup_iterations = 0;
    while warmup_iterations < warmup || warmup_elapsed < WARMUP_FLOOR {
        warmup_elapsed += measure()?;
        warmup_iterations += 1;
    }
    let mut samples = Vec::with_capacity(timed);
    for _ in 0..timed {
        samples.push(measure()?);
    }
    Ok(OneShotSubject {
        name,
        span,
        samples,
        warmup: warmup_elapsed,
        warmup_iterations,
    })
}

/// Measures the exact accepted one-function body swap from the reload suite.
fn measure_reload(warmup: usize, timed: usize) -> Result<OneShotSubject, Fail> {
    let before = vec![SourceFile::new("live.ts", RELOAD_V1)];
    let after = vec![SourceFile::new("live.ts", RELOAD_V2)];
    measure_one_shot(
        "hot-reload",
        "reload.rs::entryless_session_observes_an_accepted_body_swap (`frame` only): check + lower + finalize + atomic table swap",
        warmup,
        timed,
        || {
            // Session construction is deliberately outside the timed span:
            // the subject is an edit applied to an already-running program.
            // A fresh session makes every observation the suite's exact V1 ->
            // V2 transition and avoids accumulating old generations.
            let mut session = ReloadSession::new(&before)
                .map_err(|e| format!("hot-reload session setup: {e}"))?;
            let started = Instant::now();
            session
                .reload(&after)
                .map_err(|e| format!("hot reload: {e}"))?;
            let elapsed = started.elapsed();
            session
                .call_export("frame")
                .map_err(|e| format!("run reloaded `frame`: {e}"))?;
            let output = session.take_output();
            if output != RELOAD_GOLDEN {
                return Err(format!(
                    "reloaded `frame` printed {:?}, expected {:?}",
                    String::from_utf8_lossy(&output),
                    String::from_utf8_lossy(RELOAD_GOLDEN)
                ));
            }
            Ok(elapsed)
        },
    )
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

/// One parsed subject execution.
struct SubjectRun {
    /// Exact stdout bytes from the workload.
    stdout: Vec<u8>,
    /// One duration per timed workload call.
    samples: Vec<Duration>,
    /// Sum of measured workload-call durations discarded as warm-up.
    warmup: Duration,
    /// Number of workload calls discarded as warm-up.
    warmup_iterations: usize,
}

/// Parses one protocol time value.
fn parse_protocol_duration(value: &str, protocol: BaselineProtocol) -> Result<Duration, Fail> {
    match protocol {
        BaselineProtocol::NanosecondsWithFloor => value
            .parse::<u64>()
            .map(Duration::from_nanos)
            .map_err(|_| format!("time `{value}` is not nanoseconds")),
        BaselineProtocol::SecondsWithInternalFloor => {
            let seconds = value
                .parse::<f64>()
                .map_err(|_| format!("time `{value}` is not seconds"))?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(format!("time `{value}` is not a nonnegative duration"));
            }
            Ok(Duration::from_secs_f64(seconds))
        }
    }
}

/// Normalizes the result bytes from one C baseline protocol.
fn normalize_baseline_stdout(stdout: Vec<u8>, protocol: BaselineProtocol) -> Result<Vec<u8>, Fail> {
    match protocol {
        BaselineProtocol::NanosecondsWithFloor => Ok(stdout),
        BaselineProtocol::SecondsWithInternalFloor => {
            let text = String::from_utf8_lossy(&stdout);
            let mut fields = text.split_whitespace();
            let (Some(checksum), Some(median), None) =
                (fields.next(), fields.next(), fields.next())
            else {
                return Err(format!(
                    "C baseline output {:?} is not `<checksum> <median>`",
                    text.trim()
                ));
            };
            let checksum = checksum
                .parse::<i32>()
                .map_err(|_| format!("C baseline checksum `{checksum}` is not an i32"))?;
            let median = median
                .parse::<f64>()
                .map_err(|_| format!("C baseline median `{median}` is not seconds"))?;
            if !median.is_finite() || median < 0.0 {
                return Err(format!(
                    "C baseline median `{median}` is not a nonnegative duration"
                ));
            }
            Ok(format!("{checksum}\n").into_bytes())
        }
    }
}

/// Runs a subject binary and parses its warm-up and timed samples.
fn run_subject(
    exe: &Path,
    warmup: usize,
    timed: usize,
    protocol: BaselineProtocol,
) -> Result<SubjectRun, Fail> {
    let mut command = Command::new(exe);
    command.arg(warmup.to_string()).arg(timed.to_string());
    if matches!(protocol, BaselineProtocol::NanosecondsWithFloor) {
        command.arg(WARMUP_FLOOR.as_nanos().to_string());
    }
    let out = command
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
    let mut warmup_report = None;
    for line in stderr.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        match fields.as_slice() {
            ["warmup", iterations, ns] => {
                if warmup_report.is_some() {
                    return Err("duplicate warm-up report".to_string());
                }
                let iterations = iterations
                    .parse()
                    .map_err(|_| format!("bad warm-up iterations `{line}`"))?;
                let duration = parse_protocol_duration(ns, protocol)
                    .map_err(|_| format!("bad warm-up time `{line}`"))?;
                warmup_report = Some((iterations, duration));
            }
            ["sample", index, ns] => {
                let index: usize = index
                    .parse()
                    .map_err(|_| format!("bad sample index `{line}`"))?;
                if index != samples.len() {
                    return Err(format!(
                        "sample index {index} is out of sequence; expected {}",
                        samples.len()
                    ));
                }
                let duration = parse_protocol_duration(ns, protocol)
                    .map_err(|_| format!("unreadable sample line `{line}`"))?;
                samples.push(duration);
            }
            ["checksum-stable", flag]
                if matches!(protocol, BaselineProtocol::NanosecondsWithFloor) =>
            {
                stable = Some(*flag == "1");
            }
            _ => return Err(format!("unexpected line on stderr: `{line}`")),
        }
    }
    if matches!(protocol, BaselineProtocol::NanosecondsWithFloor) && stable != Some(true) {
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
    let Some((warmup_iterations, warmup)) = warmup_report else {
        return Err(format!(
            "{} did not report its measured warm-up time",
            exe.display()
        ));
    };
    Ok(SubjectRun {
        stdout: normalize_baseline_stdout(out.stdout, protocol)?,
        samples,
        warmup,
        warmup_iterations,
    })
}

/// Compiles and measures the hand-written C baseline.
fn measure_c(
    workload: &Workload,
    dir: &Path,
    warmup: usize,
    timed: usize,
) -> Result<Subject, Fail> {
    let source = dir.join(format!("{}-baseline.c", workload.name));
    let exe = dir.join(format!(
        "{}-baseline{}",
        workload.name,
        std::env::consts::EXE_SUFFIX
    ));
    write_file(&source, workload.baseline_c.as_bytes())?;

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

    let run = run_subject(&exe, warmup, timed, workload.baseline_protocol)?;
    Ok(Subject {
        name: "C",
        span: workload.c_span,
        stdout: run.stdout,
        samples: run.samples,
        warmup: run.warmup,
        warmup_iterations: run.warmup_iterations,
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
    workload: &Workload,
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

    let source = dir.join(format!("{}-ship.c", workload.name));
    let entry = dir.join(format!("{}-ship-entry.c", workload.name));
    let exe = dir.join(format!(
        "{}-ship{}",
        workload.name,
        std::env::consts::EXE_SUFFIX
    ));
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
        // Strict C11 hides the POSIX clock declarations in glibc.
        // SHIP_BENCH_ENTRY_C concatenates the runtime header before aot-entry.c.
        // That header selects glibc features, so a macro inside aot-entry.c is too late.
        .args(cfg!(target_os = "linux").then_some("-D_POSIX_C_SOURCE=199309L"))
        .args(BASELINE_CFLAGS)
        // The emitted ship-tier C requires two's-complement signed
        // wrap; `-fwrapv` makes signed overflow defined (matching the
        // language and the run_c_aot gate build).
        .arg("-fwrapv")
        .arg(&source)
        .arg(&entry)
        .arg(&staticlib)
        .args(runtime_system_libraries(CCompilerStyle::Unix))
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

    let run = run_subject(&exe, warmup, timed, BaselineProtocol::NanosecondsWithFloor)?;
    Ok(Subject {
        name: "ship-tier",
        span: "the subscript_export_main call in the ship-tier binary (linked with the runtime)",
        stdout: run.stdout,
        samples: run.samples,
        warmup: run.warmup,
        warmup_iterations: run.warmup_iterations,
        prepare: vec![("check + emit C", emit), ("compile + link (cc)", compile)],
    })
}

/// Measures the dev-tier JIT on the entry.
fn measure_jit(files: &[SourceFile], warmup: usize, timed: usize) -> Result<Subject, Fail> {
    let b = jit_bench_with_warmup_floor(files, warmup, timed, WARMUP_FLOOR)
        .map_err(|e| format!("dev-JIT run: {e}"))?;
    Ok(Subject {
        name: "dev-JIT",
        span: "the main call in this process",
        stdout: b.stdout,
        samples: b.samples,
        warmup: b.warmup,
        warmup_iterations: b.warmup_iterations,
        prepare: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_reporting_run_passes_when_thresholds_are_met() {
        let spreads = [NOISE_LIMIT - 0.01];
        assert_eq!(
            decide_run(RunMode::Reporting, true, &spreads),
            RunDecision::Passed
        );
    }

    #[test]
    fn clean_reporting_run_reports_a_missed_threshold() {
        let spreads = [NOISE_LIMIT - 0.01];
        assert_eq!(
            decide_run(RunMode::Reporting, false, &spreads),
            RunDecision::Missed
        );
    }

    #[test]
    fn gate_noise_does_not_void_a_missed_threshold() {
        let spreads = [NOISE_LIMIT + 0.01];
        assert_eq!(
            decide_run(RunMode::Gate, false, &spreads),
            RunDecision::Missed
        );
    }

    #[test]
    fn reporting_noise_voids_before_a_missed_threshold() {
        let spreads = [NOISE_LIMIT + 0.01];
        assert_eq!(
            decide_run(RunMode::Reporting, false, &spreads),
            RunDecision::Void
        );
    }

    #[test]
    fn only_noisy_reporting_timings_are_withheld() {
        let under_limit = NOISE_LIMIT - 0.01;
        let over_limit = NOISE_LIMIT + 0.01;

        assert!(!timing_is_withheld(RunMode::Reporting, under_limit));
        assert!(timing_is_withheld(RunMode::Reporting, over_limit));
        assert!(!timing_is_withheld(RunMode::Gate, under_limit));
        assert!(!timing_is_withheld(RunMode::Gate, over_limit));
    }

    #[test]
    fn an_empty_spread_list_never_voids_either_run() {
        for mode in [RunMode::Reporting, RunMode::Gate] {
            assert_eq!(decide_run(mode, true, &[]), RunDecision::Passed);
            assert_eq!(decide_run(mode, false, &[]), RunDecision::Missed);
        }
    }

    #[test]
    fn excessive_spread_voids_only_the_reporting_run() {
        let spreads = [NOISE_LIMIT + 0.01];
        assert_eq!(
            decide_run(RunMode::Reporting, true, &spreads),
            RunDecision::Void
        );
        assert_eq!(
            decide_run(RunMode::Gate, true, &spreads),
            RunDecision::Passed
        );
    }
}
