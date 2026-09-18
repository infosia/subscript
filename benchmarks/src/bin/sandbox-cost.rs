#![warn(missing_docs)]
//! The sandbox-profile cost runner (`specs/blocks/benchmarks.md`;
//! `specs/blocks/compiler.md` §109.8 criterion 5).
//!
//! Measures the ten cross-language workloads on both subscript tiers, under
//! the default profile and under the sandbox profile, and prints one row per
//! workload and tier: default median, sandbox median, ratio. There is no
//! threshold and no record file; the number is the baseline.
//!
//! - **dev JIT** — `subscript_codegen::jit_bench_configured` in a fresh
//!   re-exec child per (workload, profile).
//! - **ship tier** — the workload's typed HIR emitted as C, compiled and
//!   linked with the runtime static library and the shared AOT timing entry,
//!   then run as a child per (workload, profile).
//!
//! Method, as `cross-language`: warm-up 3 iterations and a 200 ms measured
//! floor, 11 timed runs, median. The measured span is the exported workload
//! call alone; the §109.5 limit calls are outside it. The checksum of every
//! run of a workload must agree, or the workload's timings are withheld.
//!
//! A workload the profile rejects reports its rule code in place of the
//! sandbox median.
//!
//! A workload whose live set passes the §109.5 default quota takes a host
//! quota through `RunConfig::alloc_quota` and the ship entry's
//! `SUBSCRIPT_BENCH_ALLOC_QUOTA` macro. The `quota` column names the quota
//! each sandbox subject ran under.
//!
//! Usage (release only — a debug runtime is unoptimized and unfair):
//! `cargo run --offline --release -p subscript-benchmarks --bin sandbox-cost`
//! Flags: `--warmup N`, `--timed M`, `--only <id>`.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use subscript_codegen::{
    emit_c, host_c_compiler, jit_bench_configured, posix_feature_arguments, runtime_staticlib_path,
    runtime_system_libraries, tool_output_report, CCompilerStyle, RunConfig, RunError,
    SANDBOX_DEFAULT_ALLOC_QUOTA_BYTES, SANDBOX_DEFAULT_STACK_BUDGET_BYTES,
};
use subscript_compiler::{check_program_with, CheckOptions, Profile, SourceFile};

/// A failure that stops the run; the runner never panics.
type Fail = String;

/// The ten workload ids, in report order. `bound-call` and the `async`
/// directory are the subjects of their own runners, so they are not here.
const WORKLOADS: [&str; 10] = [
    "fib-recursive",
    "fib-loop",
    "mandelbrot",
    "primes",
    "sort",
    "tree",
    "queen",
    "particles",
    "callbacks",
    "collect",
];

/// The timing entry, shared with `cross-language` and `perf-gate`. Under
/// the sandbox profile the driver defines the two limit macros it carries.
const AOT_BENCH_ENTRY_C: &str = concat!(
    include_str!("../../../runtime/include/subscript_runtime.h"),
    include_str!("../../aot-entry.c")
);

/// Ship-tier flags, as the cross-language runner uses.
const SHIP_CFLAGS: [&str; 4] = ["-O2", "-fwrapv", "-ffp-contract=off", "-std=c11"];

/// The host allocation quota one workload needs above the §109.5 default
/// (`specs/blocks/compiler.md` §109.4 rule 5). `callbacks` keeps every
/// array of all 20 rounds live, so its live set passes 67,108,864 bytes
/// part way through and the default quota traps it.
const WORKLOAD_ALLOC_QUOTAS: [(&str, u64); 1] = [("callbacks", 268_435_456)];

/// The allocation quota the sandbox subject of `id` runs under.
fn sandbox_alloc_quota(id: &str) -> u64 {
    WORKLOAD_ALLOC_QUOTAS
        .iter()
        .find(|(workload, _)| *workload == id)
        .map_or(SANDBOX_DEFAULT_ALLOC_QUOTA_BYTES, |(_, quota)| *quota)
}

/// What one subject of the matrix measures: the profile it compiles
/// under, the limit the host sets for it, and the procedure.
#[derive(Clone, Copy)]
struct Subject {
    /// The compile profile (§109.1).
    profile: Profile,
    /// The host allocation quota in bytes (§109.5). `None` leaves the
    /// limit to the profile, which is what a default-profile subject
    /// runs under.
    quota: Option<u64>,
    /// Minimum discarded warm-up iterations.
    warmup: usize,
    /// Timed runs.
    timed: usize,
}

/// Default minimum number of discarded warm-up iterations.
const DEFAULT_WARMUP: usize = 3;
/// Default timed runs (the methodology floor is 11).
const DEFAULT_TIMED: usize = 11;
/// Minimum sum of measured workload execution discarded as warm-up.
const WARMUP_FLOOR: Duration = Duration::from_millis(200);

/// The two profiles, in column order.
const PROFILES: [(&str, Profile); 2] =
    [("default", Profile::Default), ("sandbox", Profile::Sandbox)];

/// The two tiers, in row order.
const TIERS: [&str; 2] = ["dev-JIT", "ship-C-AOT"];

/// What one (workload, tier, profile) measurement produced.
enum Outcome {
    /// The median timed run, in seconds, with the checksum it produced.
    Ok {
        /// The integer checksum every run of this subject produced.
        checksum: i128,
        /// The median of the timed samples, in seconds.
        median_s: f64,
    },
    /// The profile rejected the workload; the string is the rule code.
    Rejected(String),
    /// The subject failed; the string is the reason.
    Error(String),
}

impl Outcome {
    /// The median, when this subject was measured.
    fn median_s(&self) -> Option<f64> {
        match self {
            Outcome::Ok { median_s, .. } => Some(*median_s),
            Outcome::Rejected(_) | Outcome::Error(_) => None,
        }
    }

    /// The checksum, when this subject was measured.
    fn checksum(&self) -> Option<i128> {
        match self {
            Outcome::Ok { checksum, .. } => Some(*checksum),
            Outcome::Rejected(_) | Outcome::Error(_) => None,
        }
    }

    /// The cell this subject renders in the table.
    fn cell(&self) -> String {
        match self {
            Outcome::Ok { median_s, .. } => format!("{:.3} ms", median_s * 1e3),
            Outcome::Rejected(code) => format!("rejected {code}"),
            Outcome::Error(_) => "error".to_string(),
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("sandbox-cost: {error}");
            ExitCode::from(2)
        }
    }
}

/// The parsed command line.
struct Args {
    /// Minimum warm-up iterations per subject.
    warmup: usize,
    /// Timed runs per subject.
    timed: usize,
    /// Measure this workload alone.
    only: Option<String>,
    /// The private one-workload dev-JIT child: the source it measures.
    jit_child: Option<PathBuf>,
    /// The profile the private child measures under.
    child_profile: Profile,
    /// The allocation quota the private child sets, in bytes. `None`
    /// leaves the limit to the profile (§109.5).
    child_alloc_quota: Option<u64>,
}

/// Parses the command line.
fn parse_args() -> Result<Args, Fail> {
    let mut parsed = Args {
        warmup: DEFAULT_WARMUP,
        timed: DEFAULT_TIMED,
        only: None,
        jit_child: None,
        child_profile: Profile::Default,
        child_alloc_quota: None,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < argv.len() {
        let flag = argv[index].as_str();
        let value = || -> Result<String, Fail> {
            argv.get(index + 1)
                .cloned()
                .ok_or_else(|| format!("`{flag}` needs a value"))
        };
        match flag {
            "--warmup" => {
                parsed.warmup = parse_count(&value()?)?;
                index += 2;
            }
            "--timed" => {
                parsed.timed = parse_count(&value()?)?;
                index += 2;
            }
            "--only" => {
                parsed.only = Some(value()?);
                index += 2;
            }
            "--jit-child" => {
                parsed.jit_child = Some(PathBuf::from(value()?));
                index += 2;
            }
            "--profile" => {
                parsed.child_profile = profile_by_name(&value()?)?;
                index += 2;
            }
            "--alloc-quota" => {
                let bytes = value()?;
                parsed.child_alloc_quota = Some(
                    bytes
                        .parse()
                        .map_err(|_| format!("`{bytes}` is not a byte count"))?,
                );
                index += 2;
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(parsed)
}

/// Reads a positive count.
fn parse_count(value: &str) -> Result<usize, Fail> {
    value
        .parse()
        .map_err(|_| format!("`{value}` is not a count"))
}

/// Reads a profile name. The default profile's name here is `default`; the
/// CLI's default profile has no name (§109.1 rule 1).
fn profile_by_name(value: &str) -> Result<Profile, Fail> {
    PROFILES
        .iter()
        .find(|(name, _)| *name == value)
        .map(|(_, profile)| *profile)
        .ok_or_else(|| format!("unknown profile `{value}`"))
}

/// Runs the whole matrix, or the private child when asked.
fn run() -> Result<ExitCode, Fail> {
    let args = parse_args()?;
    if let Some(source) = &args.jit_child {
        return run_jit_child(
            source,
            args.child_profile,
            args.child_alloc_quota,
            args.warmup,
            args.timed,
        );
    }
    let root = repository_root()?;
    let workloads = match &args.only {
        Some(only) => {
            if !WORKLOADS.contains(&only.as_str()) {
                return Err(format!("unknown workload `{only}`"));
            }
            vec![only.as_str()]
        }
        None => WORKLOADS.to_vec(),
    };
    let work = std::env::temp_dir().join(format!("subscript-sandbox-cost-{}", std::process::id()));
    std::fs::create_dir_all(&work)
        .map_err(|error| format!("create {}: {error}", work.display()))?;
    let staticlib =
        runtime_staticlib_path().map_err(|error| format!("runtime static library: {error}"))?;

    println!("== subscript sandbox-profile cost ==");
    println!(
        "host:        {} / {}",
        std::env::consts::ARCH,
        std::env::consts::OS
    );
    println!(
        "procedure:   warm-up until {} iterations and {:.0} ms of measured execution, then {} timed runs, median",
        args.warmup,
        WARMUP_FLOOR.as_secs_f64() * 1e3,
        args.timed
    );
    println!(
        "timed span:  the exported workload call alone; the profile limits are set outside it"
    );
    println!(
        "limits:      stack budget {} bytes (§109.5 default); the quota column names each row's quota, §109.5 default {} bytes",
        SANDBOX_DEFAULT_STACK_BUDGET_BYTES, SANDBOX_DEFAULT_ALLOC_QUOTA_BYTES
    );
    println!();

    let mut rows = Vec::new();
    let mut failed = false;
    for id in &workloads {
        let path = root
            .join("benchmarks/workloads/subscript")
            .join(format!("{id}.ts"));
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let files = vec![SourceFile::new(format!("{id}.ts"), source)];
        // §109.4 rule 5: the quota is the host's fact. The sandbox subject
        // runs under the workload's quota; the default subject keeps the
        // Context no limit, which is what the ratio compares against.
        let quota = sandbox_alloc_quota(id);
        let mut outcomes = Vec::new();
        for tier in TIERS {
            for (name, profile) in PROFILES {
                let subject = Subject {
                    profile,
                    quota: (profile == Profile::Sandbox).then_some(quota),
                    warmup: args.warmup,
                    timed: args.timed,
                };
                let outcome = match tier {
                    "dev-JIT" => measure_jit(&path, subject),
                    _ => measure_ship(&files, &work, &staticlib, id, subject),
                };
                if let Outcome::Error(reason) = &outcome {
                    eprintln!("sandbox-cost: {id} {tier} {name}: {reason}");
                    failed = true;
                }
                outcomes.push((tier, name, outcome));
            }
        }
        let checksums: Vec<i128> = outcomes
            .iter()
            .filter_map(|(_, _, outcome)| outcome.checksum())
            .collect();
        let agreed = checksums.windows(2).all(|pair| pair[0] == pair[1]);
        if !agreed {
            eprintln!("sandbox-cost: {id}: the runs disagreed on the checksum: {checksums:?}");
            failed = true;
        }
        rows.push(Row {
            id: (*id).to_string(),
            outcomes,
            checksum: checksums.first().copied(),
            agreed,
            quota,
        });
    }
    let _ = std::fs::remove_dir_all(&work);

    report(&rows);
    if failed {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// One workload's four measurements.
struct Row {
    /// The workload id.
    id: String,
    /// One entry per (tier, profile) pair, in measurement order.
    outcomes: Vec<(&'static str, &'static str, Outcome)>,
    /// The checksum the measured subjects produced.
    checksum: Option<i128>,
    /// Whether every measured subject produced that checksum.
    agreed: bool,
    /// The allocation quota the sandbox subjects ran under, in bytes.
    quota: u64,
}

impl Row {
    /// The outcome of one (tier, profile) pair.
    fn outcome(&self, tier: &str, profile: &str) -> Option<&Outcome> {
        self.outcomes
            .iter()
            .find(|(row_tier, row_profile, _)| *row_tier == tier && *row_profile == profile)
            .map(|(_, _, outcome)| outcome)
    }

    /// The quota cell: the bytes a measured sandbox subject ran under.
    /// A subject the profile rejected ran under none.
    fn quota_cell(&self, sandbox: &Outcome) -> String {
        match sandbox {
            Outcome::Ok { .. } => self.quota.to_string(),
            Outcome::Rejected(_) | Outcome::Error(_) => "-".to_string(),
        }
    }

    /// The checksum cell.
    fn checksum_cell(&self) -> String {
        match (self.checksum, self.agreed) {
            (Some(value), true) => value.to_string(),
            (Some(_), false) => "disagreed".to_string(),
            (None, _) => "-".to_string(),
        }
    }
}

/// Prints the table: one row per workload and tier.
fn report(rows: &[Row]) {
    println!(
        "{:<15} {:<11} {:>14} {:>14} {:>7} {:>10}  checksum",
        "workload", "tier", "default", "sandbox", "ratio", "quota"
    );
    for row in rows {
        for tier in TIERS {
            let (Some(default), Some(sandbox)) =
                (row.outcome(tier, "default"), row.outcome(tier, "sandbox"))
            else {
                continue;
            };
            let ratio = match (default.median_s(), sandbox.median_s()) {
                (Some(base), Some(under)) if base > 0.0 => format!("{:.2}x", under / base),
                _ => "-".to_string(),
            };
            println!(
                "{:<15} {tier:<11} {:>14} {:>14} {ratio:>7} {:>10}  {}",
                row.id,
                default.cell(),
                sandbox.cell(),
                row.quota_cell(sandbox),
                row.checksum_cell(),
            );
        }
    }
}

/// The repository root.
fn repository_root() -> Result<PathBuf, Fail> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "the benchmarks crate has no repository parent".to_string())
}

/// Measures one dev-JIT subject in a fresh child process.
fn measure_jit(source: &Path, subject: Subject) -> Outcome {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => return Outcome::Error(format!("locate this executable: {error}")),
    };
    let name = PROFILES
        .iter()
        .find(|(_, candidate)| *candidate == subject.profile)
        .map_or("default", |(name, _)| *name);
    let mut command = Command::new(&exe);
    command
        .arg("--jit-child")
        .arg(source)
        .arg("--profile")
        .arg(name)
        .arg("--warmup")
        .arg(subject.warmup.to_string())
        .arg("--timed")
        .arg(subject.timed.to_string());
    if let Some(bytes) = subject.quota {
        command.arg("--alloc-quota").arg(bytes.to_string());
    }
    let output = command.output();
    match output {
        Ok(output) => parse_child(output, subject.timed),
        Err(error) => Outcome::Error(format!("run the dev-JIT child: {error}")),
    }
}

/// Emits the workload's C under `profile`, links it with the timing entry,
/// and runs it as a child process.
fn measure_ship(
    files: &[SourceFile],
    work: &Path,
    staticlib: &Path,
    id: &str,
    subject: Subject,
) -> Outcome {
    let module = match check_program_with(files, &CheckOptions::with_profile(subject.profile)) {
        Ok(module) => module,
        Err(diagnostics) => return rejected(&diagnostics),
    };
    // §109.1 rule 2: the checked module carries the profile from here on.
    let sandbox = module.profile == Profile::Sandbox;
    let source = match emit_c(&module) {
        Ok(program) => program.source,
        Err(error) => return Outcome::Error(format!("C emission: {error}")),
    };
    let name = if sandbox { "sandbox" } else { "default" };
    let program = work.join(format!("{id}-{name}.c"));
    let entry = work.join(format!("{id}-{name}-entry.c"));
    let exe = work.join(format!("{id}-{name}{}", std::env::consts::EXE_SUFFIX));
    if let Err(error) = std::fs::write(&program, source.as_bytes()) {
        return Outcome::Error(format!("write {}: {error}", program.display()));
    }
    if let Err(error) = std::fs::write(&entry, AOT_BENCH_ENTRY_C.as_bytes()) {
        return Outcome::Error(format!("write {}: {error}", entry.display()));
    }
    let compiler = match host_c_compiler() {
        Ok(compiler) => compiler,
        Err(error) => return Outcome::Error(format!("the platform C compiler: {error}")),
    };
    let mut build = compiler.command();
    build.args(SHIP_CFLAGS).args(posix_feature_arguments());
    if sandbox {
        // The entry sets these on every fresh Context, before the first
        // script call (§109.5). The ship entry receives the host's quota
        // the way it receives the default.
        let alloc_quota = subject.quota.unwrap_or(SANDBOX_DEFAULT_ALLOC_QUOTA_BYTES);
        build.arg(format!("-DSUBSCRIPT_BENCH_ALLOC_QUOTA={alloc_quota}"));
        build.arg(format!(
            "-DSUBSCRIPT_BENCH_STACK_BUDGET={SANDBOX_DEFAULT_STACK_BUDGET_BYTES}"
        ));
    }
    let built = build
        .arg(&program)
        .arg(&entry)
        .arg(staticlib)
        .args(runtime_system_libraries(CCompilerStyle::Unix))
        .arg("-o")
        .arg(&exe)
        .output();
    match built {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            return Outcome::Error(format!(
                "compile/link failed:\n{}",
                tool_output_report(&output)
            ))
        }
        Err(error) => return Outcome::Error(format!("the C compiler could not run: {error}")),
    }
    let run = Command::new(&exe)
        .arg(subject.warmup.to_string())
        .arg(subject.timed.to_string())
        .arg(WARMUP_FLOOR.as_nanos().to_string())
        .output();
    match run {
        Ok(output) => parse_child(output, subject.timed),
        Err(error) => Outcome::Error(format!("run {}: {error}", exe.display())),
    }
}

/// Turns a rejection into the outcome that names its first rule code.
fn rejected(diagnostics: &[subscript_compiler::Diagnostic]) -> Outcome {
    diagnostics.first().map_or_else(
        || Outcome::Error("the checker rejected the program with no diagnostic".to_string()),
        |diagnostic| Outcome::Rejected(format!("{:?}", diagnostic.code)),
    )
}

/// Parses the shared child protocol: the checksum on stdout, and
/// `warmup`/`sample`/`checksum-stable` lines on stderr.
fn parse_child(output: std::process::Output, timed: usize) -> Outcome {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        for line in stderr.lines() {
            if let Some(code) = line.strip_prefix("rejected ") {
                return Outcome::Rejected(code.to_string());
            }
        }
        return Outcome::Error(format!(
            "child exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    let mut samples = Vec::with_capacity(timed);
    let mut warmup = None;
    let mut stable = None;
    for line in stderr.lines() {
        match line.split_whitespace().collect::<Vec<_>>().as_slice() {
            ["warmup", iterations, nanos] => {
                let iterations: usize = match iterations.parse() {
                    Ok(value) => value,
                    Err(_) => return Outcome::Error(format!("bad warm-up line `{line}`")),
                };
                let nanos: u64 = match nanos.parse() {
                    Ok(value) => value,
                    Err(_) => return Outcome::Error(format!("bad warm-up line `{line}`")),
                };
                warmup = Some((iterations, Duration::from_nanos(nanos)));
            }
            ["sample", _, nanos] => match nanos.parse::<u64>() {
                Ok(value) => samples.push(Duration::from_nanos(value).as_secs_f64()),
                Err(_) => return Outcome::Error(format!("bad sample line `{line}`")),
            },
            ["checksum-stable", flag] => stable = Some(*flag == "1"),
            _ => return Outcome::Error(format!("unexpected child line `{line}`")),
        }
    }
    if stable != Some(true) {
        return Outcome::Error("the child produced unstable output across runs".to_string());
    }
    let Some((iterations, elapsed)) = warmup else {
        return Outcome::Error("the child reported no warm-up".to_string());
    };
    if iterations < DEFAULT_WARMUP || elapsed < WARMUP_FLOOR {
        return Outcome::Error(format!(
            "warm-up was {iterations} iterations and {:.3} s; the floors are {DEFAULT_WARMUP} and {:.3} s",
            elapsed.as_secs_f64(),
            WARMUP_FLOOR.as_secs_f64()
        ));
    }
    if samples.len() != timed {
        return Outcome::Error(format!("{} timed samples for {timed} runs", samples.len()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Ok(checksum) = text.trim().parse::<i128>() else {
        return Outcome::Error(format!("checksum `{}` is not an integer", text.trim()));
    };
    let Some(median_s) = median(&samples) else {
        return Outcome::Error("the timed samples are empty or invalid".to_string());
    };
    Outcome::Ok { checksum, median_s }
}

/// The median of the timed samples, in seconds.
fn median(samples_s: &[f64]) -> Option<f64> {
    if samples_s.is_empty()
        || samples_s
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return None;
    }
    let mut sorted = samples_s.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let middle = sorted.len() / 2;
    Some(if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    })
}

/// Measures one dev-JIT workload in this private child process and reports
/// the same protocol the linked ship binary reports.
fn run_jit_child(
    source: &Path,
    profile: Profile,
    quota: Option<u64>,
    warmup: usize,
    timed: usize,
) -> Result<ExitCode, Fail> {
    let text = std::fs::read_to_string(source)
        .map_err(|error| format!("read {}: {error}", source.display()))?;
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workload.ts")
        .to_string();
    let files = vec![SourceFile::new(name, text)];
    let mut config = RunConfig::with_profile(profile);
    if let Some(bytes) = quota {
        config = config.with_alloc_quota(bytes);
    }
    let bench = match jit_bench_configured(&files, config, warmup, timed, WARMUP_FLOOR) {
        Ok(bench) => bench,
        Err(RunError::Rejected(diagnostics)) => {
            let code = diagnostics
                .first()
                .map_or_else(|| "none".to_string(), |d| format!("{:?}", d.code));
            eprintln!("rejected {code}");
            return Ok(ExitCode::from(3));
        }
        Err(error) => return Err(format!("dev-JIT: {error}")),
    };
    {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(&bench.stdout)
            .map_err(|error| format!("write the child checksum: {error}"))?;
        stdout
            .flush()
            .map_err(|error| format!("flush the child checksum: {error}"))?;
    }
    eprintln!(
        "warmup {} {}",
        bench.warmup_iterations,
        bench.warmup.as_nanos()
    );
    for (index, sample) in bench.samples.iter().enumerate() {
        eprintln!("sample {index} {}", sample.as_nanos());
    }
    // `jit_bench_configured` refuses output that changes between calls.
    eprintln!("checksum-stable 1");
    Ok(ExitCode::SUCCESS)
}
