//! Measures the §94.4 async workloads on the ship tier.
//!
//! One timed iteration is one whole program run on a fresh Context: creation,
//! `subscript_init`, the exported `main`, the other exported async functions,
//! every host checkpoint to quiescence, and Context release. Code compilation
//! is outside the timed span.
//!
//! The comparison across revisions runs this same binary in each checkout,
//! with the same warm-up and timed counts, on one host. It prints a
//! `workload <name> median <ns> min <ns> max <ns>` line per workload so a
//! ratio is computed from two runs of the same command.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use subscript_codegen::{
    emit_c, host_c_compiler, runtime_staticlib_path, runtime_system_libraries, tool_output_report,
    CCompilerStyle,
};
use subscript_compiler::{check_program, SourceFile};

/// The timing entry linked with the emitted ship-tier translation unit.
const ENTRY_C: &str = concat!(
    include_str!("../../../runtime/include/subscript_runtime.h"),
    include_str!("../../async-cost-entry.c")
);

/// §3's criterion flags, as every other ship-tier subject uses.
const CFLAGS: [&str; 3] = ["-O2", "-ffp-contract=off", "-fwrapv"];

/// §94.4: at least three warm-up runs and eleven timed runs. §9 adds a
/// measured warm-up floor, which these short workloads need more than three
/// iterations to meet.
const DEFAULT_WARMUP: usize = 3;
const DEFAULT_TIMED: usize = 11;
/// §9's measured warm-up floor, in nanoseconds.
const WARMUP_FLOOR_NS: u64 = 200_000_000;

/// §9's noise limit, reported rather than gated here.
const NOISE_LIMIT: f64 = 0.20;

struct Workload {
    name: &'static str,
    path: &'static str,
    shape: &'static str,
}

const WORKLOADS: [Workload; 3] = [
    Workload {
        name: "settled-awaits",
        path: "benchmarks/workloads/subscript/async/settled-awaits.ts",
        shape: "200,000 awaits of an already-completed handle",
    },
    Workload {
        name: "held-handles",
        path: "benchmarks/workloads/subscript/async/held-handles.ts",
        shape: "20 rounds of 2,000 independently held handles",
    },
    Workload {
        name: "deep-chains",
        path: "benchmarks/workloads/subscript/async/deep-chains.ts",
        shape: "400 chains of depth 200",
    },
];

struct Measurement {
    samples: Vec<u64>,
    warmup_iterations: u64,
    warmup_elapsed: u64,
    stdout: String,
    checkpoints: u64,
    unfinished: u64,
    live_bytes: u64,
    live_allocations: u64,
    stable: bool,
    compile: std::time::Duration,
}

fn main() -> ExitCode {
    let mut warmup = DEFAULT_WARMUP;
    let mut timed = DEFAULT_TIMED;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--warmup" => warmup = parse_count(args.next().as_deref()),
            "--timed" => timed = parse_count(args.next().as_deref()),
            other => {
                eprintln!("async-cost: unknown argument `{other}`");
                return ExitCode::from(2);
            }
        }
    }
    match run(warmup, timed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("async-cost: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse_count(value: Option<&str>) -> usize {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("async-cost: expected a positive count");
            std::process::exit(2);
        })
}

fn run(warmup: usize, timed: usize) -> Result<(), String> {
    let root = repository_root();
    // One directory per driver process, so two revisions measured on one
    // host never share an output path.
    let dir = std::env::var_os("SUBSCRIPT_ASYNC_COST_DIR").map_or_else(
        || std::env::temp_dir().join(format!("subscript-async-cost-{}", std::process::id())),
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    println!("== subscript §94.4 async cost ==");
    println!(
        "host:        {} / {}",
        std::env::consts::ARCH,
        std::env::consts::OS
    );
    println!(
        "procedure:   warm-up until {warmup} iterations and {:.3} ms of measured execution, \
         then {timed} timed runs, median reported",
        WARMUP_FLOOR_NS as f64 / 1e6
    );
    println!("timed span:  one whole program run on a fresh Context, release included");
    println!();
    for workload in &WORKLOADS {
        let measurement = measure(&root, &dir, workload, warmup, timed)?;
        report(workload, &measurement);
    }
    Ok(())
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the benchmarks crate sits in the repository")
        .to_path_buf()
}

fn measure(
    root: &Path,
    dir: &Path,
    workload: &Workload,
    warmup: usize,
    timed: usize,
) -> Result<Measurement, String> {
    let path = root.join(workload.path);
    let source = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let files = [SourceFile::new(workload.path, &source)];
    let module = check_program(&files).map_err(|diagnostics| {
        format!(
            "{} did not check: {}",
            workload.name,
            diagnostics
                .first()
                .map_or_else(|| "no diagnostic".to_string(), |d| d.message.clone())
        )
    })?;
    let emitted = emit_c(&module).map_err(|error| format!("C emission: {error}"))?;
    let program = dir.join(format!("{}.c", workload.name));
    let entry = dir.join(format!("{}-entry.c", workload.name));
    let exe = dir.join(format!("{}{}", workload.name, std::env::consts::EXE_SUFFIX));
    std::fs::write(&program, emitted.source.as_bytes())
        .map_err(|error| format!("write {}: {error}", program.display()))?;
    std::fs::write(&entry, ENTRY_C.as_bytes())
        .map_err(|error| format!("write {}: {error}", entry.display()))?;
    let staticlib =
        runtime_staticlib_path().map_err(|error| format!("runtime static library: {error}"))?;
    let compiler =
        host_c_compiler().map_err(|error| format!("the platform C compiler: {error}"))?;

    let started = Instant::now();
    let build = compiler
        .command()
        .arg("-std=c11")
        .args(cfg!(target_os = "linux").then_some("-D_POSIX_C_SOURCE=199309L"))
        // A revision whose runtime predates `async_unfinished` compiles the
        // same entry without that read, so one driver source measures both.
        .args(
            std::env::var_os("SUBSCRIPT_ASYNC_COST_NO_UNFINISHED")
                .map(|_| "-DSUBSCRIPT_ASYNC_COST_NO_UNFINISHED"),
        )
        .args(CFLAGS)
        .arg(&program)
        .arg(&entry)
        .arg(&staticlib)
        .args(runtime_system_libraries(CCompilerStyle::Unix))
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|error| format!("the platform C compiler could not be run: {error}"))?;
    let compile = started.elapsed();
    if !build.status.success() {
        return Err(format!(
            "compiling {} failed:\n{}",
            workload.name,
            tool_output_report(&build)
        ));
    }

    let run = Command::new(&exe)
        .arg(warmup.to_string())
        .arg(timed.to_string())
        .arg(WARMUP_FLOOR_NS.to_string())
        .output()
        .map_err(|error| format!("running {}: {error}", exe.display()))?;
    if !run.status.success() {
        return Err(format!(
            "{} exited with {}:\n{}",
            workload.name,
            run.status,
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    let mut measurement = Measurement {
        samples: Vec::new(),
        warmup_iterations: 0,
        warmup_elapsed: 0,
        stdout: String::from_utf8_lossy(&run.stdout).into_owned(),
        checkpoints: 0,
        unfinished: 0,
        live_bytes: 0,
        live_allocations: 0,
        stable: false,
        compile,
    };
    for line in String::from_utf8_lossy(&run.stderr).lines() {
        let mut fields = line.split_whitespace();
        match (fields.next(), fields.next(), fields.next()) {
            (Some("sample"), Some(_), Some(value)) => {
                measurement.samples.push(parse_u64(workload.name, value)?);
            }
            (Some("warmup"), Some(iterations), Some(elapsed)) => {
                measurement.warmup_iterations = parse_u64(workload.name, iterations)?;
                measurement.warmup_elapsed = parse_u64(workload.name, elapsed)?;
            }
            (Some("checkpoints"), Some(value), None) => {
                measurement.checkpoints = parse_u64(workload.name, value)?;
            }
            (Some("unfinished"), Some(value), None) => {
                measurement.unfinished = parse_u64(workload.name, value)?;
            }
            (Some("live-bytes"), Some(value), None) => {
                measurement.live_bytes = parse_u64(workload.name, value)?;
            }
            (Some("live-allocations"), Some(value), None) => {
                measurement.live_allocations = parse_u64(workload.name, value)?;
            }
            (Some("checksum-stable"), Some(value), None) => {
                measurement.stable = value == "1";
            }
            _ => {}
        }
    }
    if measurement.samples.len() != timed {
        return Err(format!(
            "{}: {} timed samples for {timed} runs",
            workload.name,
            measurement.samples.len()
        ));
    }
    Ok(measurement)
}

fn parse_u64(workload: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{workload}: `{value}` is not a count"))
}

fn report(workload: &Workload, measurement: &Measurement) {
    let mut samples = measurement.samples.clone();
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let min = samples[0];
    let max = samples[samples.len() - 1];
    let spread = (max - min) as f64 / median as f64;
    println!(
        "workload {} median {median} min {min} max {max}",
        workload.name
    );
    println!("  shape:            {}", workload.shape);
    println!(
        "  warm-up:          {} iterations, {:.3} ms of measured execution (floors: {} iterations, {:.3} ms)",
        measurement.warmup_iterations,
        measurement.warmup_elapsed as f64 / 1e6,
        DEFAULT_WARMUP,
        WARMUP_FLOOR_NS as f64 / 1e6
    );
    println!("  median:           {:.3} ms", median as f64 / 1e6);
    println!(
        "  range:            {:.3} ms to {:.3} ms, spread (max-min)/median {:.1}%{}",
        min as f64 / 1e6,
        max as f64 / 1e6,
        spread * 100.0,
        if spread > NOISE_LIMIT {
            "  (above the 20% limit of compiler.md section 9: not valid for acceptance)"
        } else {
            ""
        }
    );
    println!(
        "  timed samples in order (ms): {}",
        measurement
            .samples
            .iter()
            .map(|sample| format!("{:.3}", *sample as f64 / 1e6))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("  output:           {:?}", measurement.stdout);
    println!("  output stable:    {}", measurement.stable);
    println!("  checkpoints:      {}", measurement.checkpoints);
    println!("  unfinished:       {}", measurement.unfinished);
    println!(
        "  Context payload:  {} live bytes, {} live allocations at release",
        measurement.live_bytes, measurement.live_allocations
    );
    println!(
        "  compile + link:   {:.3} ms (outside the timed span)",
        measurement.compile.as_secs_f64() * 1e3
    );
    println!(
        "  note:             Rust scheduler storage and queue capacity are \
         outside these Context counters."
    );
    println!();
}
