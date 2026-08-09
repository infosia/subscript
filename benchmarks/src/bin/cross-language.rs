#![warn(missing_docs)]
//! Cross-language benchmark runner (`specs/blocks/benchmarks.md`).
//!
//! Measures six subjects on ten workloads in one session and writes
//! `benchmarks/results.json` and `benchmarks/README.md`:
//!
//! - **C** — the hand-written baseline (`benchmarks/workloads/c/<id>.c`), compiled
//!   `clang -O2 -ffp-contract=off`, self-timed, the 1.00x reference.
//! - **subscript-ship** — the ship tier: the workload's typed HIR emitted as C
//!   (`subscript_codegen::emit_c`), compiled `clang -std=c11 -O2 -fwrapv
//!   -ffp-contract=off` and linked with the runtime static library and the AOT
//!   timing entry, then timed on the exported workload call.
//! - **subscript-jit** — the dev tier (`subscript_codegen::jit_bench`), run in
//!   a fresh re-exec child for each workload.
//! - **LuaJIT**, **JSC**, **V8 (Node.js)** — self-timed scripts, one per
//!   language dir, located at run time via `$LUAJIT` / `$JSC` / `$NODE` or
//!   `PATH`. An absent runtime is reported as `-`, never as a failure.
//!
//! Every subject prints/returns the same integer checksum; the runner refuses
//! to report a workload's timings unless every present subject's checksum is
//! identical (the fairness invariant). Timings are never hardcoded: the runner
//! measures them live and renders the table from what it measured.
//!
//! Usage (release only — a debug runtime would be unoptimized and unfair):
//! `cargo run --offline --release -p subscript-benchmarks --bin cross-language`
//! Flags: `--warmup N` (minimum warm-up iterations; every subject also reaches
//! the 200 ms floor), `--timed M`, `--only <id>`, `--check` (validate the
//! subscript sources through the JIT and print each checksum, no timing/external
//! tools).

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use subscript_codegen::{
    emit_c, jit_bench, jit_bench_with_warmup_floor, runtime_staticlib_path, tool_output_report,
};
use subscript_compiler::{check_program, SourceFile};

/// A failure that stops one measurement; the runner never panics.
type Fail = String;

/// The ten workload ids, in report order.
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

/// One-line parameter/checksum description per workload, rendered into the
/// report so the pinned sizes are recorded next to the numbers.
fn workload_params(id: &str) -> &'static str {
    match id {
        "fib-recursive" => "naive recursion, fib(31); checksum = fib(31) = 1346269 (i32)",
        "fib-loop" => "iterative fib, INNER=32 x OUTER=3000000, masked feedback on the accumulator; checksum = accumulated i32 sum",
        "mandelbrot" => "800x800 grid, escape test x^2+y^2>=4, cap 255, f64; checksum = sum of escape counts (i64)",
        "primes" => "count primes up to 500000 by trial division (j*j<=n); checksum = count (i32)",
        "sort" => "quicksort 300000 u32 from LCG state=state*1664525+1013904223 (seed 0x12345678); checksum = order-sensitive rolling hash h=h*31+a[i] (u32 wrap)",
        "tree" => "30 full binary trees of depth 16 built/traversed/freed (subscript: reference class + Context.free; C: malloc/free; JS/Lua: GC); checksum = node-visit count (i64) = 3932130",
        "queen" => "count 13-queens solutions by bitmask backtracking; checksum = 73712 (i32)",
        "particles" => "100000 value-struct particles, 1000 steps (velocity+=acc*dt; position+=velocity*dt, dt=1.0); checksum = i32-wrapping sum of positions cast to i32. Layout: C and subscript use a packed array-of-value-structs (AoS); JS and Lua use parallel Float64Array / tables (SoA). Float64Array is the fair contiguous analog to the packed struct array, not a boxed-object strawman.",
        "callbacks" => "i32[1000000] from LCG state=state*1664525+1013904223 (seed 0x12345678), K=20 rounds; map(value,index)=(value+index) i32; filter(value,index)=((value^index)&3)!=0 (removes exactly 250000 elements per round); reduce(acc,value,index)=(acc+value+index) i32 from 0; checksum=checksum+round_result (i32 wrap)",
        "collect" => "N=20000 nodes x K=6 rounds from LCG state=state*1664525+1013904223 (seed 0x12345678); each 48-byte node owns unique strings of lengths 9/41/105/233 bytes (subscript requests 17/49/113/241 bytes, one byte past size-class payload capacities 16/48/112/240); keep exactly the nodes with (state&3)!=0 (15000 survivors/round), drop the rest, force collection (C: explicitly free), then traverse the surviving reverse-built chain; checksum per survivor in traversal order is checksum=(checksum*31+state+9+41+105+233) with i32 wrap; final checksum=1332546592",
        _ => "",
    }
}

/// The AOT timing entry: measures a warm-up phase and each timed workload call,
/// printing machine-readable durations on stderr (shared with the P4 gate
/// harness). Reused verbatim so the ship-tier span matches the gate.
const AOT_BENCH_ENTRY_C: &str = concat!(
    include_str!("../../../runtime/include/subscript_runtime.h"),
    include_str!("../../aot-entry.c")
);

/// C baseline flags (`benchmarks.md` Subjects table).
const BASELINE_CFLAGS: [&str; 3] = ["-O2", "-fwrapv", "-ffp-contract=off"];

/// Default minimum number of discarded warm-up iterations.
const DEFAULT_WARMUP: usize = 3;
/// Default timed runs (methodology floor is 11).
const DEFAULT_TIMED: usize = 11;
/// Minimum sum of measured workload execution discarded as warm-up.
const WARMUP_FLOOR: Duration = Duration::from_millis(200);
/// A spread wider than this fraction of the median flags a noisy subject.
const NOISE_LIMIT: f64 = 0.20;

/// The report subjects, in column order. The first is the 1.00x reference.
const SUBJECTS: [&str; 6] = [
    "C",
    "subscript-ship",
    "subscript-jit",
    "LuaJIT",
    "JSC",
    "V8 (Node.js)",
];

/// One subject's measured result for one workload.
#[derive(Clone)]
struct Measured {
    /// The integer checksum the subject produced.
    checksum: i128,
    /// The subject's median timed run, in seconds.
    median_s: f64,
    /// Every timed sample in execution order, in seconds.
    samples_s: Vec<f64>,
    /// Sum of measured workload-call durations discarded as warm-up.
    warmup_s: f64,
    /// Number of workload calls discarded as warm-up.
    warmup_iterations: usize,
}

impl Measured {
    /// Min/max of the timed samples, in seconds.
    fn spread(&self) -> Option<(f64, f64)> {
        stats(&self.samples_s).map(|(_, min, max)| (min, max))
    }

    /// Whether the sample set extends beyond the valid +/-20% median band.
    fn noisy(&self) -> bool {
        let Some((min, max)) = self.spread() else {
            return true;
        };
        self.median_s > 0.0
            && ((max - self.median_s) / self.median_s).max((self.median_s - min) / self.median_s)
                > NOISE_LIMIT
    }
}

/// A subject's outcome for one workload: measured, absent runtime, or error.
enum Outcome {
    /// Measured successfully.
    Ok(Measured),
    /// The subject cannot run; reported as `-` with this reason.
    Unavailable(String),
    /// The runtime is present but the run failed; reported with the reason.
    Error(String),
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("benchmarks: {e}");
            ExitCode::from(2)
        }
    }
}

/// Parsed command line.
struct Args {
    warmup: usize,
    timed: usize,
    only: Option<String>,
    check: bool,
    jit_child: Option<PathBuf>,
}

/// Parses the command line.
fn parse_args() -> Result<Args, Fail> {
    let mut a = Args {
        warmup: DEFAULT_WARMUP,
        timed: DEFAULT_TIMED,
        only: None,
        check: false,
        jit_child: None,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        match flag {
            "--check" => {
                a.check = true;
                i += 1;
            }
            "--warmup" | "--timed" | "--only" | "--jit-child" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| format!("{flag} needs a value"))?;
                match flag {
                    "--warmup" => {
                        a.warmup = value.parse().map_err(|_| format!("--warmup: `{value}`"))?
                    }
                    "--timed" => {
                        a.timed = value.parse().map_err(|_| format!("--timed: `{value}`"))?
                    }
                    "--only" => a.only = Some(value.clone()),
                    _ => a.jit_child = Some(PathBuf::from(value)),
                }
                i += 2;
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    if a.warmup < 3 || a.timed < 11 {
        return Err(format!(
            "the methodology fixes floors of 3 warm-up iterations, 200 ms of \
             warm-up execution, and 11 timed runs; got {} warm-up iterations \
             and {} timed runs",
            a.warmup, a.timed
        ));
    }
    if a.jit_child.is_some() && (a.only.is_some() || a.check) {
        return Err(
            "the private --jit-child mode cannot be combined with --only or --check".to_string(),
        );
    }
    Ok(a)
}

/// Runs the whole suite.
fn run() -> Result<ExitCode, Fail> {
    if cfg!(debug_assertions) {
        return Err("this is a debug build: the tiers would call an unoptimized runtime. \
             Re-run with `cargo run --offline --release -p subscript-benchmarks --bin cross-language`"
            .to_string());
    }
    let args = parse_args()?;
    if let Some(source) = &args.jit_child {
        return run_jit_child(source, args.warmup, args.timed);
    }
    // `root` holds the generated results.json/README.md; the per-language
    // workload sources live under `root/workloads/{subscript,c,js,lua}/`.
    let root = benchmarks_dir();
    let dir = root.join("workloads");
    if !dir.is_dir() {
        return Err(format!(
            "benchmarks workloads directory {} not found; set SUBSCRIPT_BENCHMARKS_DIR to the benchmarks/ root",
            dir.display()
        ));
    }
    let ids: Vec<&str> = match &args.only {
        Some(only) => {
            if !WORKLOADS.contains(&only.as_str()) {
                return Err(format!("unknown workload `{only}`"));
            }
            vec![WORKLOADS.iter().copied().find(|w| w == only).unwrap()]
        }
        None => WORKLOADS.to_vec(),
    };

    if args.check {
        return check_only(&dir, &ids);
    }

    let tools = Tools::resolve();
    let work = WorkDir::new()?;
    let staticlib = runtime_staticlib_path().map_err(|e| format!("runtime static library: {e}"))?;

    // Per workload: subject name -> Outcome, and the agreed checksum + match.
    let mut rows: Vec<WorkloadResult> = Vec::new();
    let mut any_mismatch = false;

    for id in &ids {
        eprintln!("== {id} ==");
        let ts_path = dir.join("subscript").join(format!("{id}.ts"));
        let ts_src = std::fs::read_to_string(&ts_path)
            .map_err(|e| format!("read {}: {e}", ts_path.display()))?;
        let files = vec![SourceFile::new(format!("{id}.ts"), ts_src)];

        let c = measure_c(&tools, &dir, &work, id, args.warmup, args.timed);
        let ship = measure_ship(
            &tools,
            &files,
            &work,
            &staticlib,
            id,
            args.warmup,
            args.timed,
        );
        let jit = measure_jit(&ts_path, args.warmup, args.timed);
        let lua = measure_script(
            &tools.luajit,
            &dir,
            "lua",
            "lua",
            id,
            args.warmup,
            args.timed,
            false,
            &[],
        );
        let jsc = if *id == "collect" {
            measure_collect_jsc(&tools.jsc, &dir, args.warmup, args.timed)
        } else {
            measure_script(
                &tools.jsc,
                &dir,
                "js",
                "js",
                id,
                args.warmup,
                args.timed,
                true,
                &[],
            )
        };
        let node = if *id == "collect" {
            measure_collect_node(&tools.node, &dir, args.warmup, args.timed)
        } else {
            measure_script(
                &tools.node,
                &dir,
                "js",
                "js",
                id,
                args.warmup,
                args.timed,
                false,
                &[],
            )
        };

        let outcomes: Vec<(&str, Outcome)> = vec![
            ("C", c),
            ("subscript-ship", ship),
            ("subscript-jit", jit),
            ("LuaJIT", lua),
            ("JSC", jsc),
            ("V8 (Node.js)", node),
        ];
        for (name, o) in &outcomes {
            match o {
                Outcome::Ok(m) => eprintln!(
                    "  {name:<15} checksum={} median={:.3} ms warm-up={:.3} s ({} iterations)",
                    m.checksum,
                    m.median_s * 1000.0,
                    m.warmup_s,
                    m.warmup_iterations
                ),
                Outcome::Unavailable(reason) => eprintln!("  {name:<15} - ({reason})"),
                Outcome::Error(e) => eprintln!("  {name:<15} ERROR: {e}"),
            }
        }

        // Fairness check: every subject that ran must agree. An unavailable
        // subject contributes neither a checksum nor a timing.
        let ran: Vec<(&str, i128)> = outcomes
            .iter()
            .filter_map(|(n, o)| match o {
                Outcome::Ok(m) => Some((*n, m.checksum)),
                _ => None,
            })
            .collect();
        let checksum = ran.first().map(|(_, c)| *c);
        let mut matched = true;
        if let Some(first) = checksum {
            for (n, c) in &ran {
                if *c != first {
                    matched = false;
                    eprintln!("  CHECKSUM MISMATCH: {n} = {c}, expected {first}");
                }
            }
        }
        if !matched {
            any_mismatch = true;
        }
        rows.push(WorkloadResult {
            id: (*id).to_string(),
            checksum,
            matched,
            outcomes: outcomes
                .into_iter()
                .map(|(n, o)| (n.to_string(), o))
                .collect(),
        });
    }

    let machine = Machine::probe();
    let versions = versions(&tools);
    let generated = today();

    let json = render_json(
        &rows,
        &machine,
        &versions,
        &generated,
        args.warmup,
        args.timed,
    );
    let readme = render_readme(
        &rows,
        &machine,
        &versions,
        &generated,
        args.warmup,
        args.timed,
    );
    std::fs::write(root.join("results.json"), json.as_bytes())
        .map_err(|e| format!("write results.json: {e}"))?;
    std::fs::write(root.join("README.md"), readme.as_bytes())
        .map_err(|e| format!("write README.md: {e}"))?;
    eprintln!(
        "wrote {} and {}",
        root.join("results.json").display(),
        root.join("README.md").display()
    );

    print!("{readme}");
    if any_mismatch {
        eprintln!("benchmarks: at least one workload's subjects disagreed on the checksum; its timings are withheld.");
        Ok(ExitCode::from(1))
    } else if rows.iter().any(WorkloadResult::has_noise) {
        eprintln!("benchmarks: at least one subject exceeded the +/-20% spread limit; its timing is invalid and withheld.");
        Ok(ExitCode::from(1))
    } else if rows.iter().any(WorkloadResult::has_error) {
        eprintln!("benchmarks: at least one available subject failed; its timing is unavailable.");
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// Runs exactly one dev-JIT workload inside the private re-exec child and
/// reports the same warm-up/sample/checksum-stability protocol as the ship
/// executable. Compilation, Context construction, and process startup remain
/// outside every duration reported to the parent.
fn run_jit_child(source: &Path, warmup: usize, timed: usize) -> Result<ExitCode, Fail> {
    let ts_src = std::fs::read_to_string(source)
        .map_err(|e| format!("read JIT child source {}: {e}", source.display()))?;
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workload.ts")
        .to_string();
    let files = vec![SourceFile::new(name, ts_src)];
    let bench = jit_bench_with_warmup_floor(&files, warmup, timed, WARMUP_FLOOR)
        .map_err(|e| format!("dev-JIT: {e}"))?;

    {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(&bench.stdout)
            .map_err(|e| format!("write JIT child checksum: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("flush JIT child checksum: {e}"))?;
    }
    eprintln!(
        "warmup {} {}",
        bench.warmup_iterations,
        bench.warmup.as_nanos()
    );
    for (index, sample) in bench.samples.iter().enumerate() {
        eprintln!("sample {index} {}", sample.as_nanos());
    }
    // `jit_bench_with_warmup_floor` rejects output that changes between calls.
    eprintln!("checksum-stable 1");
    Ok(ExitCode::SUCCESS)
}

/// Runs each subscript workload through the dev JIT once and prints its
/// checksum or the diagnostic that rejected it. No timing, no external tools.
fn check_only(dir: &Path, ids: &[&str]) -> Result<ExitCode, Fail> {
    let mut ok = true;
    for id in ids {
        let ts_path = dir.join("subscript").join(format!("{id}.ts"));
        let ts_src = std::fs::read_to_string(&ts_path)
            .map_err(|e| format!("read {}: {e}", ts_path.display()))?;
        let files = vec![SourceFile::new(format!("{id}.ts"), ts_src)];
        match jit_bench(&files, 0, 1) {
            Ok(b) => {
                let s = String::from_utf8_lossy(&b.stdout);
                println!("{id:<15} checksum = {}", s.trim());
            }
            Err(e) => {
                ok = false;
                println!("{id:<15} FAILED: {e}");
            }
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// One workload's per-subject outcomes and its agreed checksum.
struct WorkloadResult {
    id: String,
    checksum: Option<i128>,
    matched: bool,
    outcomes: Vec<(String, Outcome)>,
}

impl WorkloadResult {
    /// The measured result for a subject, if it measured and the workload's
    /// checksums agreed (a mismatch withholds every timing for the workload).
    fn measured(&self, subject: &str) -> Option<&Measured> {
        if !self.matched {
            return None;
        }
        self.outcomes.iter().find_map(|(n, o)| match o {
            Outcome::Ok(m) if n == subject && !m.noisy() => Some(m),
            _ => None,
        })
    }

    /// The raw measurement, including one invalidated by excessive spread.
    fn sampled(&self, subject: &str) -> Option<&Measured> {
        self.outcomes.iter().find_map(|(n, o)| match o {
            Outcome::Ok(m) if n == subject => Some(m),
            _ => None,
        })
    }

    /// Whether this subject's sample set exceeds the spread limit.
    fn noisy(&self, subject: &str) -> bool {
        self.sampled(subject).is_some_and(Measured::noisy)
    }

    /// Whether any subject's sample set exceeds the spread limit.
    fn has_noise(&self) -> bool {
        self.outcomes.iter().any(|(_, outcome)| match outcome {
            Outcome::Ok(measured) => measured.noisy(),
            _ => false,
        })
    }

    /// Whether an available subject failed.
    fn has_error(&self) -> bool {
        self.outcomes
            .iter()
            .any(|(_, outcome)| matches!(outcome, Outcome::Error(_)))
    }

    /// Why a subject could not run (reported as `-`).
    fn unavailable_reason(&self, subject: &str) -> Option<&str> {
        self.outcomes.iter().find_map(|(n, outcome)| match outcome {
            Outcome::Unavailable(reason) if n == subject => Some(reason.as_str()),
            _ => None,
        })
    }
}

// ---- measurement ----------------------------------------------------------

/// Median of finite, non-negative samples in seconds, with min/max.
fn stats(samples_s: &[f64]) -> Option<(f64, f64, f64)> {
    if samples_s.is_empty()
        || samples_s
            .iter()
            .any(|sample| !sample.is_finite() || *sample < 0.0)
    {
        return None;
    }
    let mut s = samples_s.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = s.len() / 2;
    let median = if s.len() % 2 == 1 {
        s[mid]
    } else {
        (s[mid - 1] + s[mid]) / 2.0
    };
    Some((median, s[0], s[s.len() - 1]))
}

/// Compiles and times the hand-written C baseline.
fn measure_c(
    tools: &Tools,
    dir: &Path,
    work: &WorkDir,
    id: &str,
    warmup: usize,
    timed: usize,
) -> Outcome {
    let Some(cc) = &tools.cc else {
        return Outcome::Unavailable("clang is not installed".to_string());
    };
    let src = dir.join("c").join(format!("{id}.c"));
    let exe = work
        .path
        .join(format!("c-{id}{}", std::env::consts::EXE_SUFFIX));
    let build = Command::new(cc)
        .args(BASELINE_CFLAGS)
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output();
    match build {
        Ok(o) if o.status.success() => {}
        Ok(o) => return Outcome::Error(format!("compile failed:\n{}", tool_output_report(&o))),
        Err(e) => return Outcome::Error(format!("clang could not run: {e}")),
    }
    let argv: Vec<std::ffi::OsString> = vec![warmup.to_string().into(), timed.to_string().into()];
    run_self_timed(&exe, &argv, warmup, timed)
}

/// Emits C for the workload, compiles + links it with the runtime static
/// library and the AOT timing entry, and times the exported workload call.
fn measure_ship(
    tools: &Tools,
    files: &[SourceFile],
    work: &WorkDir,
    staticlib: &Path,
    id: &str,
    warmup: usize,
    timed: usize,
) -> Outcome {
    let Some(cc) = &tools.cc else {
        return Outcome::Unavailable("clang is not installed".to_string());
    };
    let module = match check_program(files) {
        Ok(m) => m,
        Err(diags) => {
            return Outcome::Error(format!(
                "did not check: {}",
                diags.first().map(|d| d.message.clone()).unwrap_or_default()
            ))
        }
    };
    let c_source = match emit_c(&module) {
        Ok(p) => p.source,
        Err(e) => return Outcome::Error(format!("C emission: {e}")),
    };
    let src = work.path.join(format!("ship-{id}.c"));
    let entry = work.path.join(format!("ship-entry-{id}.c"));
    let exe = work
        .path
        .join(format!("ship-{id}{}", std::env::consts::EXE_SUFFIX));
    if let Err(e) = std::fs::write(&src, c_source.as_bytes()) {
        return Outcome::Error(format!("write emitted C: {e}"));
    }
    if let Err(e) = std::fs::write(&entry, AOT_BENCH_ENTRY_C.as_bytes()) {
        return Outcome::Error(format!("write entry: {e}"));
    }
    let build = Command::new(cc)
        .arg("-std=c11")
        .args(BASELINE_CFLAGS)
        .arg("-fwrapv")
        .arg(&src)
        .arg(&entry)
        .arg(staticlib)
        .args(runtime_system_libs())
        .arg("-o")
        .arg(&exe)
        .output();
    match build {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            return Outcome::Error(format!("compile/link failed:\n{}", tool_output_report(&o)))
        }
        Err(e) => return Outcome::Error(format!("clang could not run: {e}")),
    }
    match run_looped_binary(&exe, warmup, timed, WARMUP_FLOOR) {
        Ok(run) => finish(
            &run.stdout,
            &run.samples,
            run.warmup,
            run.warmup_iterations,
            warmup,
            timed,
        ),
        Err(e) => Outcome::Error(e),
    }
}

/// Times the dev-tier JIT in a fresh process for this workload.
fn measure_jit(source: &Path, warmup: usize, timed: usize) -> Outcome {
    match run_jit_process(source, warmup, timed) {
        Ok(run) => finish(
            &run.stdout,
            &run.samples,
            run.warmup,
            run.warmup_iterations,
            warmup,
            timed,
        ),
        Err(e) => Outcome::Error(format!("dev-JIT child: {e}")),
    }
}

/// Runs a self-timed script under an interpreter, parsing `<checksum> <median>`
/// from its stdout. A missing interpreter is unavailable.
fn measure_script(
    tool: &Option<PathBuf>,
    dir: &Path,
    subdir: &str,
    ext: &str,
    id: &str,
    warmup: usize,
    timed: usize,
    dashdash: bool,
    prefix_args: &[&str],
) -> Outcome {
    let Some(exe) = tool else {
        return Outcome::Unavailable(format!("{subdir} runtime is not installed"));
    };
    let script = dir.join(subdir).join(format!("{id}.{ext}"));
    // The self-timed subject reads its warm-up/timed counts from argv, so it runs
    // the same schedule as the subscript tiers. `jsc` exposes script arguments
    // only when they follow `--`; node and luajit take them directly.
    let mut argv: Vec<std::ffi::OsString> =
        prefix_args.iter().map(std::ffi::OsString::from).collect();
    argv.push(script.into_os_string());
    if dashdash {
        argv.push("--".into());
    }
    argv.push(warmup.to_string().into());
    argv.push(timed.to_string().into());
    run_self_timed(exe, &argv, warmup, timed)
}

/// Runs `collect` under Node only when `--expose-gc` really exposes the
/// synchronous `globalThis.gc` hook. A runtime without it is an honest `-`;
/// the workload is never approximated by allocation pressure.
fn measure_collect_node(
    tool: &Option<PathBuf>,
    dir: &Path,
    warmup: usize,
    timed: usize,
) -> Outcome {
    let Some(exe) = tool else {
        return Outcome::Unavailable("Node.js is not installed".to_string());
    };
    let probe = Command::new(exe)
        .args([
            "--expose-gc",
            "-e",
            "if (typeof globalThis.gc !== 'function') process.exit(1)",
        ])
        .output();
    if !matches!(probe, Ok(ref out) if out.status.success()) {
        return Outcome::Unavailable(
            "this Node.js cannot expose forced collection with --expose-gc".to_string(),
        );
    }
    measure_script(
        tool,
        dir,
        "js",
        "js",
        "collect",
        warmup,
        timed,
        false,
        &["--expose-gc"],
    )
}

/// Runs `collect` under JSC when either the shell's `gc()` hook is already
/// present or `--useDollarVM=true` exposes `$vm.gc()`.
fn measure_collect_jsc(tool: &Option<PathBuf>, dir: &Path, warmup: usize, timed: usize) -> Outcome {
    let Some(exe) = tool else {
        return Outcome::Unavailable("JavaScriptCore is not installed".to_string());
    };
    let probe_source = "if (typeof gc !== 'function' && \
        (typeof $vm !== 'object' || typeof $vm.gc !== 'function')) \
        throw new Error('forced collection unavailable')";
    let default_probe = Command::new(exe).args(["-e", probe_source]).output();
    if matches!(default_probe, Ok(ref out) if out.status.success()) {
        return measure_script(tool, dir, "js", "js", "collect", warmup, timed, true, &[]);
    }
    let dollar_vm_probe = Command::new(exe)
        .args(["--useDollarVM=true", "-e", probe_source])
        .output();
    if matches!(dollar_vm_probe, Ok(ref out) if out.status.success()) {
        return measure_script(
            tool,
            dir,
            "js",
            "js",
            "collect",
            warmup,
            timed,
            true,
            &["--useDollarVM=true"],
        );
    }
    Outcome::Unavailable(
        "this JSC shell exposes neither gc() nor --useDollarVM=true/$vm.gc()".to_string(),
    )
}

/// Runs `exe args…`, expecting `<checksum> <median_seconds>` on stdout and
/// `warmup` / per-sample records on stderr.
fn run_self_timed(
    exe: &Path,
    args: &[std::ffi::OsString],
    minimum_warmup: usize,
    timed: usize,
) -> Outcome {
    let out = match Command::new(exe).args(args).output() {
        Ok(o) => o,
        Err(e) => return Outcome::Error(format!("run {}: {e}", exe.display())),
    };
    if !out.status.success() {
        return Outcome::Error(format!(
            "{} exited with {}: {}",
            exe.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.trim();
    let mut parts = line.split_whitespace();
    let (Some(cs), Some(md), None) = (parts.next(), parts.next(), parts.next()) else {
        return Outcome::Error(format!("unparseable output `{line}`"));
    };
    let checksum: i128 = match cs.parse() {
        Ok(v) => v,
        Err(_) => return Outcome::Error(format!("checksum `{cs}` is not an integer")),
    };
    let median_s: f64 = match md.parse() {
        Ok(v) => v,
        Err(_) => return Outcome::Error(format!("median `{md}` is not a number")),
    };
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut warmup_report: Option<(usize, f64)> = None;
    let mut samples_s = Vec::with_capacity(timed);
    for record in stderr.lines() {
        let fields = record.split_whitespace().collect::<Vec<_>>();
        match fields.as_slice() {
            ["warmup", iterations, seconds] => {
                if warmup_report.is_some() {
                    return Outcome::Error("duplicate warm-up report".to_string());
                }
                let iterations = match iterations.parse() {
                    Ok(value) => value,
                    Err(_) => {
                        return Outcome::Error(format!(
                            "warm-up iterations `{iterations}` is not an integer"
                        ))
                    }
                };
                let seconds = match seconds.parse() {
                    Ok(value) => value,
                    Err(_) => {
                        return Outcome::Error(format!("warm-up time `{seconds}` is not a number"))
                    }
                };
                warmup_report = Some((iterations, seconds));
            }
            ["sample", index, seconds] => {
                let index: usize = match index.parse() {
                    Ok(value) => value,
                    Err(_) => {
                        return Outcome::Error(format!("sample index `{index}` is not an integer"))
                    }
                };
                if index != samples_s.len() {
                    return Outcome::Error(format!(
                        "sample index {index} is out of sequence; expected {}",
                        samples_s.len()
                    ));
                }
                let seconds = match seconds.parse() {
                    Ok(value) => value,
                    Err(_) => return Outcome::Error(format!("sample `{seconds}` is not a number")),
                };
                samples_s.push(seconds);
            }
            _ => return Outcome::Error(format!("unparseable stderr record `{record}`")),
        }
    }
    let Some((warmup_iterations, warmup_s)) = warmup_report else {
        return Outcome::Error("subject did not report its measured warm-up time".to_string());
    };
    finish_measurement(
        checksum,
        samples_s,
        warmup_s,
        warmup_iterations,
        minimum_warmup,
        timed,
        Some(median_s),
    )
}

/// Builds a `Measured` from a runner-timed subscript tier.
fn finish(
    stdout: &[u8],
    samples: &[Duration],
    warmup: Duration,
    warmup_iterations: usize,
    minimum_warmup: usize,
    timed: usize,
) -> Outcome {
    let s = String::from_utf8_lossy(stdout);
    let checksum: i128 = match s.trim().parse() {
        Ok(v) => v,
        Err(_) => return Outcome::Error(format!("checksum `{}` is not an integer", s.trim())),
    };
    finish_measurement(
        checksum,
        samples.iter().map(Duration::as_secs_f64).collect(),
        warmup.as_secs_f64(),
        warmup_iterations,
        minimum_warmup,
        timed,
        None,
    )
}

/// Validates the common timing contract and constructs a successful result.
fn finish_measurement(
    checksum: i128,
    samples_s: Vec<f64>,
    warmup_s: f64,
    warmup_iterations: usize,
    minimum_warmup: usize,
    timed: usize,
    reported_median_s: Option<f64>,
) -> Outcome {
    if !warmup_s.is_finite() || warmup_s < WARMUP_FLOOR.as_secs_f64() {
        return Outcome::Error(format!(
            "warm-up = {warmup_s:.3} s across {warmup_iterations} iterations; \
             the floor is {:.3} s",
            WARMUP_FLOOR.as_secs_f64()
        ));
    }
    let required_iterations = minimum_warmup.max(DEFAULT_WARMUP);
    if warmup_iterations < required_iterations {
        return Outcome::Error(format!(
            "warm-up used {warmup_iterations} iterations; expected at least \
             {required_iterations}"
        ));
    }
    if samples_s.len() != timed {
        return Outcome::Error(format!(
            "subject gave {} timed samples, expected {timed}",
            samples_s.len()
        ));
    }
    let Some((median_s, _, _)) = stats(&samples_s) else {
        return Outcome::Error("timed samples are empty or invalid".to_string());
    };
    if let Some(reported) = reported_median_s {
        if !reported.is_finite() || (reported - median_s).abs() > 0.000_000_1 {
            return Outcome::Error(format!(
                "reported median {reported:.9} s disagrees with the sample \
                 median {median_s:.9} s"
            ));
        }
    }
    Outcome::Ok(Measured {
        checksum,
        median_s,
        samples_s,
        warmup_s,
        warmup_iterations,
    })
}

/// One parsed runner-timed execution.
struct LoopedRun {
    stdout: Vec<u8>,
    samples: Vec<Duration>,
    warmup: Duration,
    warmup_iterations: usize,
}

/// Runs a looped-entry binary (the ship subject) and parses its warm-up,
/// per-sample, and checksum-stability protocol.
fn run_looped_binary(
    exe: &Path,
    warmup: usize,
    timed: usize,
    warmup_floor: Duration,
) -> Result<LoopedRun, Fail> {
    let out = Command::new(exe)
        .arg(warmup.to_string())
        .arg(timed.to_string())
        .arg(warmup_floor.as_nanos().to_string())
        .output()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    parse_looped_output(&exe.display().to_string(), out, timed)
}

/// Re-execs this runner in its private one-workload JIT mode.
fn run_jit_process(source: &Path, warmup: usize, timed: usize) -> Result<LoopedRun, Fail> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("locate the cross-language executable: {e}"))?;
    let out = Command::new(&exe)
        .arg("--jit-child")
        .arg(source)
        .arg("--warmup")
        .arg(warmup.to_string())
        .arg("--timed")
        .arg(timed.to_string())
        .output()
        .map_err(|e| {
            format!(
                "run {} --jit-child {}: {e}",
                exe.display(),
                source.display()
            )
        })?;
    let label = format!("{} --jit-child {}", exe.display(), source.display());
    parse_looped_output(&label, out, timed)
}

/// Parses the looped subscript-subject protocol from a completed child.
fn parse_looped_output(
    label: &str,
    out: std::process::Output,
    timed: usize,
) -> Result<LoopedRun, Fail> {
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return Err(format!(
            "{label} exited with {}: {}",
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
                let ns = ns
                    .parse()
                    .map_err(|_| format!("bad warm-up time `{line}`"))?;
                warmup_report = Some((iterations, Duration::from_nanos(ns)));
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
                let ns: u64 = ns.parse().map_err(|_| format!("bad sample `{line}`"))?;
                samples.push(Duration::from_nanos(ns));
            }
            ["checksum-stable", flag] => stable = Some(*flag == "1"),
            _ => return Err(format!("unexpected stderr line `{line}`")),
        }
    }
    if stable != Some(true) {
        return Err(format!("{label} produced unstable output across runs"));
    }
    if samples.len() != timed {
        return Err(format!(
            "{label} gave {} samples, expected {timed}",
            samples.len()
        ));
    }
    let Some((warmup_iterations, warmup)) = warmup_report else {
        return Err(format!("{label} did not report its measured warm-up time"));
    };
    Ok(LoopedRun {
        stdout: out.stdout,
        samples,
        warmup,
        warmup_iterations,
    })
}

// ---- environment / tools --------------------------------------------------

/// Located interpreters/compilers (env override, then `PATH`, then a known OS
/// path for JSC). Absent tools are `None` and reported as `-`.
struct Tools {
    cc: Option<PathBuf>,
    node: Option<PathBuf>,
    luajit: Option<PathBuf>,
    jsc: Option<PathBuf>,
}

impl Tools {
    fn resolve() -> Tools {
        Tools {
            cc: resolve_clang(),
            node: env_or_path("NODE", "node"),
            luajit: env_or_path("LUAJIT", "luajit"),
            jsc: resolve_jsc(),
        }
    }
}

/// Resolves clang: `$CC` verbatim, else `clang` on `PATH`, else — on
/// Windows only — the standard LLVM install
/// (`%ProgramFiles%\LLVM\bin\clang.exe`). `None` when none is found
/// (reported as `-`, mirroring `codegen::aot::host_c_compiler` but
/// without that function's fail-loud bare-name fallback, since an
/// absent compiler here is a skipped subject, not an error).
fn resolve_clang() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("CC") {
        return Some(PathBuf::from(v));
    }
    if let Some(p) = find_on_path("clang") {
        return Some(p);
    }
    #[cfg(windows)]
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        let llvm = PathBuf::from(pf).join("LLVM").join("bin").join("clang.exe");
        if llvm.is_file() {
            return Some(llvm);
        }
    }
    None
}

/// `$VAR` verbatim if set, else `name` on `PATH`, else `None`.
fn env_or_path(var: &str, name: &str) -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(var) {
        return Some(PathBuf::from(v));
    }
    find_on_path(name)
}

/// JSC: `$JSC`, else `jsc` on `PATH`, else the macOS framework helper (an OS
/// location, not a developer path).
fn resolve_jsc() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("JSC") {
        return Some(PathBuf::from(v));
    }
    if let Some(p) = find_on_path("jsc") {
        return Some(p);
    }
    let helper =
        PathBuf::from("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc");
    helper.is_file().then_some(helper)
}

/// Finds `name` on `PATH`.
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

/// System import libraries a manual clang link of the runtime static library
/// needs on windows-msvc (mirrors `codegen::aot`). Empty elsewhere.
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

/// The `benchmarks/` root (`$SUBSCRIPT_BENCHMARKS_DIR`, else this crate's own
/// directory — the benchmarks crate lives at the benchmarks root). Holds the
/// generated results.json/README.md and the `workloads/` subtree.
/// `CARGO_MANIFEST_DIR` is a build-time value, not a committed path.
fn benchmarks_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SUBSCRIPT_BENCHMARKS_DIR") {
        return PathBuf::from(d);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A temporary directory outside the repository, removed on drop.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn new() -> Result<WorkDir, Fail> {
        let path =
            std::env::temp_dir().join(format!("subscript-benchmarks-{}", std::process::id()));
        std::fs::create_dir_all(&path).map_err(|e| format!("temp dir: {e}"))?;
        Ok(WorkDir { path })
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---- machine / versions ---------------------------------------------------

/// Machine facts recorded in the report.
struct Machine {
    arch: String,
    os: String,
    cpu: String,
    cores: String,
    power: String,
}

impl Machine {
    fn probe() -> Machine {
        Machine {
            arch: std::env::consts::ARCH.to_string(),
            os: std::env::consts::OS.to_string(),
            cpu: cpu_brand(),
            cores: sysctl("hw.ncpu")
                .or_else(|| {
                    std::thread::available_parallelism()
                        .ok()
                        .map(|n| n.to_string())
                })
                .unwrap_or_else(|| "unknown".to_string()),
            power: ac_power(),
        }
    }
}

/// The CPU identifier for the report. `machdep.cpu.brand_string` is populated
/// on Intel macs but empty on Apple Silicon, so fall back to `hw.model` (e.g.
/// `Mac14,2`), then the build arch, reporting the first non-empty value.
fn cpu_brand() -> String {
    sysctl("machdep.cpu.brand_string")
        .or_else(|| sysctl("hw.model"))
        .or_else(|| {
            std::env::var("PROCESSOR_IDENTIFIER")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| std::env::consts::ARCH.to_string())
}

/// Reads a `sysctl -n <key>` value (macOS); `None` when unavailable.
fn sysctl(key: &str) -> Option<String> {
    let out = Command::new("sysctl").arg("-n").arg(key).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Whether the machine is on AC power, parsed from `pmset -g ps` (macOS).
fn ac_power() -> String {
    match Command::new("pmset").arg("-g").arg("ps").output() {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.contains("AC Power") {
                "AC Power".to_string()
            } else if s.contains("Battery Power") {
                "Battery Power".to_string()
            } else {
                "unknown".to_string()
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

/// First line of a command's output, for version banners.
fn first_line(cmd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    let text = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    String::from_utf8_lossy(&text)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Runtime version banners for the report.
fn versions(tools: &Tools) -> Vec<(String, String)> {
    let mut v = Vec::new();
    v.push((
        "C".to_string(),
        tools
            .cc
            .as_ref()
            .and_then(|c| first_line(c, &["--version"]))
            .unwrap_or_else(|| "absent".to_string()),
    ));
    v.push(("subscript".to_string(), subscript_version()));
    v.push((
        "LuaJIT".to_string(),
        tools
            .luajit
            .as_ref()
            .and_then(|c| first_line(c, &["-v"]))
            .unwrap_or_else(|| "absent".to_string()),
    ));
    v.push(("JSC".to_string(), jsc_version(tools)));
    v.push((
        "V8 (Node.js)".to_string(),
        tools
            .node
            .as_ref()
            .and_then(|c| first_line(c, &["--version"]))
            .map(|s| format!("Node.js {s}"))
            .unwrap_or_else(|| "absent".to_string()),
    ));
    v
}

/// subscript provenance: the repo's short git commit, when available.
fn subscript_version() -> String {
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    format!("subscript @ {hash} (dev-JIT: Cranelift; ship: HIR->C->clang)")
}

/// JSC has no version flag; label it with the macOS product version (an OS
/// fact, not a developer-specific one).
fn jsc_version(tools: &Tools) -> String {
    if tools.jsc.is_none() {
        return "absent".to_string();
    }
    match sw_vers() {
        Some(v) => format!("JavaScriptCore (macOS {v})"),
        None => "JavaScriptCore (system)".to_string(),
    }
}

/// macOS product version via `sw_vers -productVersion`.
fn sw_vers() -> Option<String> {
    let out = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The UTC date `YYYY-MM-DD` the snapshot was captured.
fn today() -> String {
    Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

// ---- rendering ------------------------------------------------------------

/// A cell for the ratio/absolute table: `-` when absent, `withheld` on a
/// checksum mismatch, else `ratio (median ms)` (or `1.00x (median ms)` for C).
fn cell(row: &WorkloadResult, subject: &str, baseline: Option<f64>) -> String {
    if let Some(m) = row.measured(subject) {
        let ms = m.median_s * 1000.0;
        match baseline {
            Some(b) if b > 0.0 => format!("{:.2}x ({:.3} ms)", m.median_s / b, ms),
            _ => format!("{:.3} ms", ms),
        }
    } else if row.unavailable_reason(subject).is_some() {
        "-".to_string()
    } else if row.noisy(subject) {
        "invalid (noise)".to_string()
    } else if !row.matched {
        "withheld".to_string()
    } else {
        // Present but errored.
        "error".to_string()
    }
}

/// A cell for the measured warm-up table.
fn warmup_cell(row: &WorkloadResult, subject: &str) -> String {
    if let Some(measured) = row.sampled(subject) {
        format!(
            "{:.3} s ({} iterations)",
            measured.warmup_s, measured.warmup_iterations
        )
    } else if row.unavailable_reason(subject).is_some() {
        "-".to_string()
    } else {
        "error".to_string()
    }
}

/// Renders `README.md`.
fn render_readme(
    rows: &[WorkloadResult],
    machine: &Machine,
    versions: &[(String, String)],
    generated: &str,
    warmup: usize,
    timed: usize,
) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Cross-language benchmarks — captured results");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Snapshot captured {generated}. Measured live by the runner \
         (`benchmarks/src/bin/cross-language.rs`), never hardcoded; re-run with \
         `cargo run --offline --release -p subscript-benchmarks --bin cross-language`. \
         Contract: `specs/blocks/benchmarks.md`."
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "## Machine");
    let _ = writeln!(s);
    let _ = writeln!(s, "- host: {} / {}", machine.arch, machine.os);
    let _ = writeln!(
        s,
        "- CPU: {} ({} logical cores)",
        machine.cpu, machine.cores
    );
    let _ = writeln!(s, "- power: {}", machine.power);
    let _ = writeln!(s);
    let _ = writeln!(s, "## Runtimes");
    let _ = writeln!(s);
    for (name, ver) in versions {
        let _ = writeln!(s, "- **{name}**: {ver}");
    }
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "## Method\n\n\
         Every subject that runs discards at least {warmup} warm-up iterations \
         and continues until measured workload execution reaches the {:.0} ms \
         floor, then performs {timed} timed runs and reports the median. \
         `--warmup` is the minimum iteration count; the time floor is always \
         additional. The runner rejects a subject that reports less than the \
         floor or fewer than the requested iterations. Every workload/subject \
         measurement runs in a fresh process; the runner re-execs itself for \
         each subscript-jit workload. Only workload execution is timed. C is \
         the 1.00x reference; every other subject is `ratio \
         (median)`. C, LuaJIT, JSC, and V8 self-time and report every sample; \
         the two subscript tiers are timed by the runner (the language has no \
         clock primitive). Every subject that runs computes the identical \
         integer checksum for a workload — unavailable subjects contribute no \
         checksum, and the runner withholds a workload's timings if any \
         measured checksum differs.\n\n\
         **Span note.** The C/LuaJIT/JSC/V8 subjects time only the `workload()` \
         call and print the checksum afterward; the two subscript tiers time the \
         whole exported `main()`, which includes formatting and writing the \
         one-line integer checksum to the runtime sink. That is a \
         sub-microsecond step inside subscript's span but outside the others' — \
         a conservative difference that penalizes subscript, retained because \
         the ship-tier AOT timing entry and `jit_bench` are shared with the P4 \
         performance gate and time the exported entry by contract.",
        WARMUP_FLOOR.as_secs_f64() * 1000.0
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "## Results");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "| Workload | Checksum | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
    for row in rows {
        let baseline = row.measured("C").map(|m| m.median_s);
        let checksum = row
            .checksum
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            row.id,
            if row.matched {
                checksum
            } else {
                format!("{checksum} (MISMATCH)")
            },
            cell(row, "C", baseline),
            cell(row, "subscript-ship", baseline),
            cell(row, "subscript-jit", baseline),
            cell(row, "LuaJIT", baseline),
            cell(row, "JSC", baseline),
            cell(row, "V8 (Node.js)", baseline),
        );
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "## Measured warm-up");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "| Workload | C | subscript-ship | subscript-jit | LuaJIT | JSC | V8 (Node.js) |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|");
    for row in rows {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} |",
            row.id,
            warmup_cell(row, "C"),
            warmup_cell(row, "subscript-ship"),
            warmup_cell(row, "subscript-jit"),
            warmup_cell(row, "LuaJIT"),
            warmup_cell(row, "JSC"),
            warmup_cell(row, "V8 (Node.js)"),
        );
    }
    if rows.iter().any(|row| row.id == "callbacks") {
        let _ = writeln!(
            s,
            "\n**callbacks interpretation.** This workload measures what the \
             idiomatic callback spelling costs against a hand-written loop, \
             not a codegen deficit."
        );
    }
    if let Some(row) = rows.iter().find(|row| row.id == "collect") {
        let ran = row
            .outcomes
            .iter()
            .filter_map(|(subject, outcome)| {
                matches!(outcome, Outcome::Ok(_)).then_some(subject.as_str())
            })
            .collect::<Vec<_>>();
        let unavailable = row
            .outcomes
            .iter()
            .filter_map(|(subject, outcome)| match outcome {
                Outcome::Unavailable(reason) => Some(format!("{subject} ({reason})")),
                _ => None,
            })
            .collect::<Vec<_>>();
        let failed = row
            .outcomes
            .iter()
            .filter_map(|(subject, outcome)| match outcome {
                Outcome::Error(reason) => Some(format!("{subject} ({reason})")),
                _ => None,
            })
            .collect::<Vec<_>>();
        let _ = writeln!(
            s,
            "\n**collect interpretation.** This is not a cross-runtime “GC \
             speed” claim; it compares reclaiming the pinned graph in each \
             runtime's own explicit idiom. Ran: {}. Could not run: {}. Failed: {}.",
            if ran.is_empty() {
                "none".to_string()
            } else {
                ran.join(", ")
            },
            if unavailable.is_empty() {
                "none".to_string()
            } else {
                unavailable.join("; ")
            },
            if failed.is_empty() {
                "none".to_string()
            } else {
                failed.join("; ")
            },
        );
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "## Workload parameters\n");
    for id in WORKLOADS {
        if rows.iter().any(|r| r.id == id) {
            let _ = writeln!(s, "- **{id}** — {}", workload_params(id));
        }
    }
    // Noise note for every subject's recorded sample set.
    let mut noisy: Vec<String> = Vec::new();
    for row in rows {
        for subject in SUBJECTS {
            if let Some(m) = row.sampled(subject) {
                if let Some((min, max)) = m.spread() {
                    if m.median_s > 0.0 {
                        let spread =
                            ((max - m.median_s) / m.median_s).max((m.median_s - min) / m.median_s);
                        if spread > NOISE_LIMIT {
                            noisy.push(format!("{}/{subject} ({:.0}%)", row.id, spread * 100.0));
                        }
                    }
                }
            }
        }
    }
    let _ = writeln!(s);
    if noisy.is_empty() {
        let _ = writeln!(
            s,
            "Noise: every recorded sample set is within +/-{:.0}% of its median.",
            NOISE_LIMIT * 100.0
        );
    } else {
        let _ = writeln!(
            s,
            "Noise: wider than +/-{:.0}% spread for {} — those timings are invalid and withheld.",
            NOISE_LIMIT * 100.0,
            noisy.join(", ")
        );
    }
    s
}

/// Renders `results.json` (hand-serialized; all values are controlled).
fn render_json(
    rows: &[WorkloadResult],
    machine: &Machine,
    versions: &[(String, String)],
    generated: &str,
    warmup: usize,
    timed: usize,
) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    let _ = writeln!(s, "  \"generated\": {},", jstr(generated));
    let _ = writeln!(
        s,
        "  \"procedure\": {},",
        jstr(&format!(
            "{warmup} minimum warm-up iterations and {:.0} ms measured warm-up \
             execution, {timed} timed runs, median reported",
            WARMUP_FLOOR.as_secs_f64() * 1000.0
        ))
    );
    let _ = writeln!(s, "  \"machine\": {{");
    let _ = writeln!(s, "    \"arch\": {},", jstr(&machine.arch));
    let _ = writeln!(s, "    \"os\": {},", jstr(&machine.os));
    let _ = writeln!(s, "    \"cpu\": {},", jstr(&machine.cpu));
    let _ = writeln!(s, "    \"cores\": {},", jstr(&machine.cores));
    let _ = writeln!(s, "    \"power\": {}", jstr(&machine.power));
    let _ = writeln!(s, "  }},");
    let _ = writeln!(s, "  \"versions\": {{");
    for (i, (name, ver)) in versions.iter().enumerate() {
        let comma = if i + 1 < versions.len() { "," } else { "" };
        let _ = writeln!(s, "    {}: {}{comma}", jstr(name), jstr(ver));
    }
    let _ = writeln!(s, "  }},");
    let _ = writeln!(
        s,
        "  \"subjects\": [{}],",
        SUBJECTS
            .iter()
            .map(|s| jstr(s))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(s, "  \"workloads\": [");
    for (ri, row) in rows.iter().enumerate() {
        let _ = writeln!(s, "    {{");
        let _ = writeln!(s, "      \"id\": {},", jstr(&row.id));
        let _ = writeln!(s, "      \"params\": {},", jstr(workload_params(&row.id)));
        let _ = writeln!(
            s,
            "      \"checksum\": {},",
            row.checksum
                .map(|c| c.to_string())
                .unwrap_or_else(|| "null".to_string())
        );
        let _ = writeln!(s, "      \"checksum_match\": {},", row.matched);
        let _ = writeln!(s, "      \"medians_s\": {{");
        let subs = &SUBJECTS;
        for (si, subject) in subs.iter().enumerate() {
            let comma = if si + 1 < subs.len() { "," } else { "" };
            let value = if let Some(m) = row.measured(subject) {
                format!("{:.9}", m.median_s)
            } else {
                "null".to_string()
            };
            let _ = writeln!(s, "        {}: {value}{comma}", jstr(subject));
        }
        let _ = writeln!(s, "      }},");
        let _ = writeln!(s, "      \"warmups_s\": {{");
        for (si, subject) in subs.iter().enumerate() {
            let comma = if si + 1 < subs.len() { "," } else { "" };
            let value = row
                .sampled(subject)
                .map(|measured| format!("{:.9}", measured.warmup_s))
                .unwrap_or_else(|| "null".to_string());
            let _ = writeln!(s, "        {}: {value}{comma}", jstr(subject));
        }
        let _ = writeln!(s, "      }},");
        let _ = writeln!(s, "      \"warmup_iterations\": {{");
        for (si, subject) in subs.iter().enumerate() {
            let comma = if si + 1 < subs.len() { "," } else { "" };
            let value = row
                .sampled(subject)
                .map(|measured| measured.warmup_iterations.to_string())
                .unwrap_or_else(|| "null".to_string());
            let _ = writeln!(s, "        {}: {value}{comma}", jstr(subject));
        }
        let _ = writeln!(s, "      }},");
        let _ = writeln!(s, "      \"samples_s\": {{");
        for (si, subject) in subs.iter().enumerate() {
            let comma = if si + 1 < subs.len() { "," } else { "" };
            let value = row
                .sampled(subject)
                .map(|measured| {
                    format!(
                        "[{}]",
                        measured
                            .samples_s
                            .iter()
                            .map(|sample| format!("{sample:.9}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .unwrap_or_else(|| "null".to_string());
            let _ = writeln!(s, "        {}: {value}{comma}", jstr(subject));
        }
        let _ = writeln!(s, "      }},");
        let _ = writeln!(s, "      \"ranges_s\": {{");
        for (si, subject) in subs.iter().enumerate() {
            let comma = if si + 1 < subs.len() { "," } else { "" };
            let value = row
                .sampled(subject)
                .and_then(Measured::spread)
                .map(|(min, max)| format!("[{min:.9}, {max:.9}]"))
                .unwrap_or_else(|| "null".to_string());
            let _ = writeln!(s, "        {}: {value}{comma}", jstr(subject));
        }
        let _ = writeln!(s, "      }},");
        let _ = writeln!(s, "      \"unavailable_reasons\": {{");
        for (si, subject) in subs.iter().enumerate() {
            let comma = if si + 1 < subs.len() { "," } else { "" };
            let value = row
                .unavailable_reason(subject)
                .map(jstr)
                .unwrap_or_else(|| "null".to_string());
            let _ = writeln!(s, "        {}: {value}{comma}", jstr(subject));
        }
        let _ = writeln!(s, "      }}");
        let comma = if ri + 1 < rows.len() { "," } else { "" };
        let _ = writeln!(s, "    }}{comma}");
    }
    let _ = writeln!(s, "  ]");
    s.push_str("}\n");
    s
}

/// Minimal JSON string escaping (quotes, backslash, control chars).
fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
