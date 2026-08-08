#![warn(missing_docs)]
//! Boundary-price decomposition benchmark (`specs/blocks/benchmarks.md`).
//!
//! On Unix this emits the real Subscript subject, builds it and four matched
//! C subjects with the ship compiler flags, runs every subject in a fresh
//! process, verifies checksums, and reports the boundary-layer deltas.

#[cfg(not(unix))]
fn main() {
    println!(
        "bound-call: skipped (requires Unix; host is {} {})",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

#[cfg(unix)]
use std::io::Write as _;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, ExitCode};

#[cfg(unix)]
use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, emit_c, host_c_compiler,
    include_directory_arg, runtime_staticlib_path, runtime_system_libraries,
    tool_output_report, HostCCompiler, AOT_ENTRY_C,
};
#[cfg(unix)]
use subscript_compiler::{check_program, SourceFile};

#[cfg(unix)]
type Fail = String;

#[cfg(unix)]
const WORKLOAD_SOURCE: &str = include_str!("../../workloads/subscript/bound-call.ts");
#[cfg(unix)]
const MIRROR_SOURCE: &str = include_str!("../../boundary-noop.generated.d.ts");

#[cfg(unix)]
const SAMPLE_COUNT: usize = 15;
#[cfg(unix)]
const WARMUP_FLOOR_MS: u64 = 200;
#[cfg(unix)]
const SPREAD_LIMIT: f64 = 0.20;
#[cfg(unix)]
const QUANTUM_LIMIT: f64 = 0.01;

#[cfg(unix)]
const MIMIC_C: &str = r#"
#include "boundary-noop.h"

#include <stdint.h>
#include <stddef.h>

extern void *subscript_rt_ctx_new(void);
extern void subscript_rt_ctx_release(void *ctx);
extern void subscript_rt_ctx_enter_script(void *ctx);
extern void subscript_rt_ctx_exit_script(void *ctx);
extern void subscript_rt_shadow_push(void *ctx, void *base, uint64_t slots);
extern void subscript_rt_shadow_pop(void *ctx);
extern void *subscript_rt_array_new(void *ctx, uint64_t elem_size, uint32_t pos_id);
extern int32_t subscript_rt_array_push(void *ctx, void *a, const void *src, uint32_t pos_id);
extern int32_t subscript_rt_array_len(void *ctx, const void *a);
extern const void *subscript_rt_array_data(void *ctx, const void *a);

int main(void) {
    void *ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    subscript_rt_ctx_enter_script(ctx);

    void *roots[2] = { NULL, NULL };
    subscript_rt_shadow_push(ctx, roots, 2u);
    roots[0] = bnBindGroupCreate();
    if (*(const uint32_t *)ctx != 0u) goto trapped;
    roots[1] = subscript_rt_array_new(ctx, sizeof(uint32_t), 0u);
    if (*(const uint32_t *)ctx != 0u) goto trapped;
    {
        uint32_t value = 7u;
        (void)subscript_rt_array_push(ctx, roots[1], &value, 1u);
    }
    if (*(const uint32_t *)ctx != 0u) goto trapped;

    while (bnMoreSamples() != 0) {
        int64_t t0 = bnNow();
        if (*(const uint32_t *)ctx != 0u) goto trapped;
        for (int32_t i = 0; i < 1000; ++i) {
            uint32_t index = 3u;
            void *group = roots[0];
            void *array = roots[1];
            const void *data = subscript_rt_array_data(ctx, array);
            size_t count = (size_t)subscript_rt_array_len(ctx, array);
            bnSetBindGroup(index, group, (BnOffsets){ data, count });
            if (*(const uint32_t *)ctx != 0u) goto trapped;
            bnDraw(1u, 2u, 3u, 4u);
            if (*(const uint32_t *)ctx != 0u) goto trapped;
        }
        {
            int64_t t1 = bnNow();
            if (*(const uint32_t *)ctx != 0u) goto trapped;
            bnRecordSample(t0, t1);
            if (*(const uint32_t *)ctx != 0u) goto trapped;
        }
    }

    bnReport();
    bnBindGroupRelease(roots[0]);
    subscript_rt_shadow_pop(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_release(ctx);
    return 0;

trapped:
    if (roots[0] != NULL) bnBindGroupRelease(roots[0]);
    subscript_rt_shadow_pop(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_release(ctx);
    return 3;
}
"#;

#[cfg(unix)]
const NO_TRAP_C: &str = r#"
#include "boundary-noop.h"

#include <stdint.h>
#include <stddef.h>

extern void *subscript_rt_ctx_new(void);
extern void subscript_rt_ctx_release(void *ctx);
extern void subscript_rt_ctx_enter_script(void *ctx);
extern void subscript_rt_ctx_exit_script(void *ctx);
extern void subscript_rt_shadow_push(void *ctx, void *base, uint64_t slots);
extern void subscript_rt_shadow_pop(void *ctx);
extern void *subscript_rt_array_new(void *ctx, uint64_t elem_size, uint32_t pos_id);
extern int32_t subscript_rt_array_push(void *ctx, void *a, const void *src, uint32_t pos_id);
extern int32_t subscript_rt_array_len(void *ctx, const void *a);
extern const void *subscript_rt_array_data(void *ctx, const void *a);

int main(void) {
    void *ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    subscript_rt_ctx_enter_script(ctx);

    void *roots[2] = { NULL, NULL };
    subscript_rt_shadow_push(ctx, roots, 2u);
    roots[0] = bnBindGroupCreate();
    if (*(const uint32_t *)ctx != 0u) goto trapped;
    roots[1] = subscript_rt_array_new(ctx, sizeof(uint32_t), 0u);
    if (*(const uint32_t *)ctx != 0u) goto trapped;
    {
        uint32_t value = 7u;
        (void)subscript_rt_array_push(ctx, roots[1], &value, 1u);
    }
    if (*(const uint32_t *)ctx != 0u) goto trapped;

    while (bnMoreSamples() != 0) {
        int64_t t0 = bnNow();
        if (*(const uint32_t *)ctx != 0u) goto trapped;
        for (int32_t i = 0; i < 1000; ++i) {
            uint32_t index = 3u;
            void *group = roots[0];
            void *array = roots[1];
            const void *data = subscript_rt_array_data(ctx, array);
            size_t count = (size_t)subscript_rt_array_len(ctx, array);
            bnSetBindGroup(index, group, (BnOffsets){ data, count });
            bnDraw(1u, 2u, 3u, 4u);
        }
        {
            int64_t t1 = bnNow();
            if (*(const uint32_t *)ctx != 0u) goto trapped;
            bnRecordSample(t0, t1);
            if (*(const uint32_t *)ctx != 0u) goto trapped;
        }
    }

    bnReport();
    bnBindGroupRelease(roots[0]);
    subscript_rt_shadow_pop(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_release(ctx);
    return 0;

trapped:
    if (roots[0] != NULL) bnBindGroupRelease(roots[0]);
    subscript_rt_shadow_pop(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_release(ctx);
    return 3;
}
"#;

#[cfg(unix)]
const HOISTED_C: &str = r#"
#include "boundary-noop.h"

#include <stdint.h>
#include <stddef.h>

extern void *subscript_rt_ctx_new(void);
extern void subscript_rt_ctx_release(void *ctx);
extern void subscript_rt_ctx_enter_script(void *ctx);
extern void subscript_rt_ctx_exit_script(void *ctx);
extern void subscript_rt_shadow_push(void *ctx, void *base, uint64_t slots);
extern void subscript_rt_shadow_pop(void *ctx);
extern void *subscript_rt_array_new(void *ctx, uint64_t elem_size, uint32_t pos_id);
extern int32_t subscript_rt_array_push(void *ctx, void *a, const void *src, uint32_t pos_id);
extern int32_t subscript_rt_array_len(void *ctx, const void *a);
extern const void *subscript_rt_array_data(void *ctx, const void *a);

int main(void) {
    void *ctx = subscript_rt_ctx_new();
    if (ctx == NULL) return 2;
    subscript_rt_ctx_enter_script(ctx);

    void *roots[2] = { NULL, NULL };
    subscript_rt_shadow_push(ctx, roots, 2u);
    roots[0] = bnBindGroupCreate();
    if (*(const uint32_t *)ctx != 0u) goto trapped;
    roots[1] = subscript_rt_array_new(ctx, sizeof(uint32_t), 0u);
    if (*(const uint32_t *)ctx != 0u) goto trapped;
    {
        uint32_t value = 7u;
        (void)subscript_rt_array_push(ctx, roots[1], &value, 1u);
    }
    if (*(const uint32_t *)ctx != 0u) goto trapped;

    void *array = roots[1];
    const void *data = subscript_rt_array_data(ctx, array);
    size_t count = (size_t)subscript_rt_array_len(ctx, array);
    while (bnMoreSamples() != 0) {
        int64_t t0 = bnNow();
        if (*(const uint32_t *)ctx != 0u) goto trapped;
        for (int32_t i = 0; i < 1000; ++i) {
            uint32_t index = 3u;
            void *group = roots[0];
            bnSetBindGroup(index, group, (BnOffsets){ data, count });
            if (*(const uint32_t *)ctx != 0u) goto trapped;
            bnDraw(1u, 2u, 3u, 4u);
            if (*(const uint32_t *)ctx != 0u) goto trapped;
        }
        {
            int64_t t1 = bnNow();
            if (*(const uint32_t *)ctx != 0u) goto trapped;
            bnRecordSample(t0, t1);
            if (*(const uint32_t *)ctx != 0u) goto trapped;
        }
    }

    bnReport();
    bnBindGroupRelease(roots[0]);
    subscript_rt_shadow_pop(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_release(ctx);
    return 0;

trapped:
    if (roots[0] != NULL) bnBindGroupRelease(roots[0]);
    subscript_rt_shadow_pop(ctx);
    subscript_rt_ctx_exit_script(ctx);
    subscript_rt_ctx_release(ctx);
    return 3;
}
"#;

#[cfg(unix)]
const FLOOR_C: &str = r#"
#include "boundary-noop.h"

#include <stdint.h>

int main(void) {
    BnBindGroup group = bnBindGroupCreate();
    uint32_t seven = 7u;
    while (bnMoreSamples() != 0) {
        int64_t t0 = bnNow();
        for (int32_t i = 0; i < 1000; ++i) {
            bnSetBindGroup(3u, group, (BnOffsets){ &seven, 1u });
            bnDraw(1u, 2u, 3u, 4u);
        }
        bnRecordSample(t0, bnNow());
    }
    bnReport();
    bnBindGroupRelease(group);
    return 0;
}
"#;

#[cfg(unix)]
struct WorkDir {
    path: PathBuf,
}

#[cfg(unix)]
impl WorkDir {
    fn new() -> Result<Self, Fail> {
        let base = std::env::temp_dir();
        for attempt in 0..1000u32 {
            let path = base.join(format!(
                "subscript-bound-call-{}-{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("create {}: {error}", path.display())),
            }
        }
        Err("could not create a unique bound-call temporary directory".to_string())
    }
}

#[cfg(unix)]
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(unix)]
struct Report {
    checksum: u64,
    warmup_ms: u64,
    quantum_ns: u64,
    samples_ns: Vec<u64>,
}

#[cfg(unix)]
struct Stats {
    median_ns: f64,
    iqr_percent: f64,
    spread: f64,
}

#[cfg(unix)]
impl Stats {
    fn of(samples: &[u64]) -> Result<Self, Fail> {
        if samples.len() != SAMPLE_COUNT {
            return Err(format!(
                "expected {SAMPLE_COUNT} timed samples, got {}",
                samples.len()
            ));
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let median_ns = sorted[SAMPLE_COUNT / 2] as f64;
        if median_ns <= 0.0 {
            return Err("a timed sample median was zero".to_string());
        }
        let q1 = sorted[3] as f64;
        let q3 = sorted[11] as f64;
        let spread = (sorted[SAMPLE_COUNT - 1] - sorted[0]) as f64 / median_ns;
        Ok(Self {
            median_ns,
            iqr_percent: (q3 - q1) / median_ns * 100.0,
            spread,
        })
    }
}

#[cfg(unix)]
struct Measurement {
    name: &'static str,
    report: Report,
    stats: Stats,
}

#[cfg(unix)]
struct Subject<'a> {
    name: &'static str,
    source: &'a str,
    runtime: bool,
}

#[cfg(unix)]
fn main() -> ExitCode {
    if cfg!(debug_assertions) {
        eprintln!(
            "bound-call: this benchmark requires a release build; run `cargo run --offline \
             --release -p subscript-benchmarks --bin bound-call`"
        );
        return ExitCode::from(2);
    }
    match run() {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(2),
        Err(error) => {
            eprintln!("bound-call: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(unix)]
fn run() -> Result<bool, Fail> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let work = WorkDir::new()?;
    let compiler = host_c_compiler().map_err(|error| format!("C compiler: {error}"))?;
    let runtime = runtime_staticlib_path().map_err(|error| format!("runtime staticlib: {error}"))?;
    let backend_object = compile_backend(&compiler, &manifest, &work.path)?;

    let script_source = emit_script()?;
    let program = work.path.join("script.c");
    let entry = work.path.join("script-entry.c");
    write_file(&program, script_source.as_bytes())?;
    write_file(&entry, AOT_ENTRY_C.as_bytes())?;
    let script_exe = work.path.join("script");
    link_sources(
        &compiler,
        &manifest,
        &work.path,
        &[&program, &entry],
        &backend_object,
        Some(&runtime),
        &script_exe,
        "script",
    )?;

    let subjects = [
        Subject {
            name: "mimic",
            source: MIMIC_C,
            runtime: true,
        },
        Subject {
            name: "no-trap",
            source: NO_TRAP_C,
            runtime: true,
        },
        Subject {
            name: "hoisted",
            source: HOISTED_C,
            runtime: true,
        },
        Subject {
            name: "floor",
            source: FLOOR_C,
            runtime: false,
        },
    ];

    let mut executables = Vec::with_capacity(subjects.len());
    for subject in &subjects {
        let source = work.path.join(format!("{}.c", subject.name));
        let executable = work.path.join(subject.name);
        write_file(&source, subject.source.as_bytes())?;
        link_sources(
            &compiler,
            &manifest,
            &work.path,
            &[&source],
            &backend_object,
            subject.runtime.then_some(runtime.as_path()),
            &executable,
            subject.name,
        )?;
        executables.push((subject.name, executable));
    }

    let mut measurements = Vec::with_capacity(5);
    measurements.push(run_subject("script", &script_exe)?);
    for (name, executable) in &executables {
        measurements.push(run_subject(name, executable)?);
    }
    verify_checksums(&measurements)?;

    // The contract permits this instead of statically linking the backend into
    // the Rust runner: build floor once more, then run it with inherited stdio.
    let inherited_exe = work.path.join("floor-inherited-stdio");
    let floor_source = work.path.join("floor.c");
    link_sources(
        &compiler,
        &manifest,
        &work.path,
        &[&floor_source],
        &backend_object,
        None,
        &inherited_exe,
        "floor-inherited-stdio",
    )?;
    println!("floor-inherited-stdio (secondary observation):");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("flush stdout: {error}"))?;
    let status = Command::new(&inherited_exe)
        .status()
        .map_err(|error| format!("run {}: {error}", inherited_exe.display()))?;
    if !status.success() {
        return Err(format!("{} exited with {status}", inherited_exe.display()));
    }

    Ok(print_report(&compiler, &measurements))
}

#[cfg(unix)]
fn emit_script() -> Result<String, Fail> {
    let files = [
        SourceFile::ambient("boundary-noop.generated.d.ts", MIRROR_SOURCE),
        SourceFile::new("bound-call.ts", WORKLOAD_SOURCE),
    ];
    let module = check_program(&files).map_err(|diagnostics| {
        format!(
            "bound-call.ts did not check: {}",
            diagnostics
                .first()
                .map(|diagnostic| diagnostic.message.as_str())
                .unwrap_or("no diagnostic")
        )
    })?;
    emit_c(&module)
        .map(|program| program.source)
        .map_err(|error| format!("emit bound-call.ts: {error}"))
}

#[cfg(unix)]
fn compile_backend(
    compiler: &HostCCompiler,
    manifest: &Path,
    work: &Path,
) -> Result<PathBuf, Fail> {
    let source = manifest.join("boundary-noop.c");
    let object = work.join("boundary-noop.o");
    let mut command = compiler.command();
    add_c11_optimized_flags(&mut command, compiler.style());
    command
        .arg(include_directory_arg(compiler.style(), manifest))
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object);
    let output = command.output().map_err(|error| {
        format!(
            "the C compiler `{}` could not compile the backend: {error}",
            compiler.program().to_string_lossy()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "compiling boundary-noop.c failed:\n{}",
            tool_output_report(&output)
        ));
    }
    Ok(object)
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
fn link_sources(
    compiler: &HostCCompiler,
    manifest: &Path,
    work: &Path,
    sources: &[&Path],
    backend_object: &Path,
    runtime: Option<&Path>,
    executable: &Path,
    label: &str,
) -> Result<(), Fail> {
    let mut command = compiler.command();
    add_c11_optimized_flags(&mut command, compiler.style());
    command.arg(include_directory_arg(compiler.style(), manifest));
    command.args(sources);
    command.arg(backend_object);
    if let Some(runtime) = runtime {
        command.arg(runtime);
        command.args(runtime_system_libraries(compiler.style()));
    }
    add_executable_output(&mut command, executable, compiler.style());
    let output = command.output().map_err(|error| {
        format!(
            "the C compiler `{}` could not build {label}: {error}",
            compiler.program().to_string_lossy()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "compiling/linking {label} failed (temporary directory {}):\n{}",
            work.display(),
            tool_output_report(&output)
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn write_file(path: &Path, contents: &[u8]) -> Result<(), Fail> {
    std::fs::write(path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(unix)]
fn run_subject(name: &'static str, executable: &Path) -> Result<Measurement, Fail> {
    let output = Command::new(executable)
        .output()
        .map_err(|error| format!("run {}: {error}", executable.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} exited with {}:\n{}",
            executable.display(),
            output.status,
            tool_output_report(&output)
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("{name} stdout was not UTF-8: {error}"))?;
    let line = stdout
        .lines()
        .find(|line| line.starts_with("bound-call "))
        .ok_or_else(|| format!("{name} did not print a bound-call report: {stdout:?}"))?;
    let report = parse_report(name, line)?;
    if report.warmup_ms < WARMUP_FLOOR_MS {
        return Err(format!(
            "{name} reported only {} ms of warm-up; the floor is {WARMUP_FLOOR_MS} ms",
            report.warmup_ms
        ));
    }
    let stats = Stats::of(&report.samples_ns)?;
    Ok(Measurement {
        name,
        report,
        stats,
    })
}

#[cfg(unix)]
fn parse_report(name: &str, line: &str) -> Result<Report, Fail> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 5 || fields[0] != "bound-call" {
        return Err(format!("{name} printed an unreadable report line: `{line}`"));
    }
    let checksum = parse_field(name, fields[1], "checksum")?;
    let warmup_ms = parse_field(name, fields[2], "warmup_ms")?;
    let quantum_ns = parse_field(name, fields[3], "quantum_ns")?;
    let sample_text = fields[4]
        .strip_prefix("samples_ns=")
        .ok_or_else(|| format!("{name} report lacks samples_ns: `{line}`"))?;
    let samples_ns = sample_text
        .split(',')
        .map(|sample| {
            sample
                .parse::<u64>()
                .map_err(|_| format!("{name} has an invalid sample `{sample}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if samples_ns.len() != SAMPLE_COUNT {
        return Err(format!(
            "{name} reported {} samples, expected {SAMPLE_COUNT}",
            samples_ns.len()
        ));
    }
    Ok(Report {
        checksum,
        warmup_ms,
        quantum_ns,
        samples_ns,
    })
}

#[cfg(unix)]
fn parse_field(name: &str, field: &str, key: &str) -> Result<u64, Fail> {
    field
        .strip_prefix(key)
        .and_then(|rest| rest.strip_prefix('='))
        .ok_or_else(|| format!("{name} report lacks {key}: `{field}`"))?
        .parse()
        .map_err(|_| format!("{name} has an invalid {key}: `{field}`"))
}

#[cfg(unix)]
fn verify_checksums(measurements: &[Measurement]) -> Result<(), Fail> {
    let first = measurements
        .first()
        .ok_or_else(|| "no measurements were run".to_string())?;
    for measurement in measurements.iter().skip(1) {
        if measurement.report.checksum != first.report.checksum {
            return Err(format!(
                "checksum mismatch: {}={} but {}={}",
                first.name,
                first.report.checksum,
                measurement.name,
                measurement.report.checksum
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn print_report(compiler: &HostCCompiler, measurements: &[Measurement]) -> bool {
    println!();
    println!("== bound-call boundary price ==");
    println!(
        "compiler: {} [-std=c11 -O2 -fwrapv -ffp-contract=off]",
        compiler.program().to_string_lossy()
    );
    println!("variant | median ns/region | ns/pair | ns/call | IQR% | quantum_ns | checksum");
    for measurement in measurements {
        println!(
            "{} | {:.0} | {:.3} | {:.3} | {:.2} | {} | {}",
            measurement.name,
            measurement.stats.median_ns,
            measurement.stats.median_ns / 1000.0,
            measurement.stats.median_ns / 2000.0,
            measurement.stats.iqr_percent,
            measurement.report.quantum_ns,
            measurement.report.checksum
        );
    }

    println!();
    println!("deltas | ns/pair | ns/call");
    for (label, left, right) in [
        ("script-mimic", 0usize, 1usize),
        ("mimic-no-trap", 1usize, 2usize),
        ("mimic-hoisted", 1usize, 3usize),
        ("hoisted-floor", 3usize, 4usize),
    ] {
        let delta = measurements[left].stats.median_ns - measurements[right].stats.median_ns;
        println!("{label} | {:.3} | {:.3}", delta / 1000.0, delta / 2000.0);
    }

    println!();
    println!("warm-up and timed samples (ns, in order):");
    for measurement in measurements {
        let samples = measurement
            .report
            .samples_ns
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{} warmup_ms={} quantum_ns={} samples_ns={samples}",
            measurement.name, measurement.report.warmup_ms, measurement.report.quantum_ns
        );
    }

    let mut invalid = false;
    for measurement in measurements {
        if measurement.stats.spread > SPREAD_LIMIT {
            invalid = true;
            println!(
                "validity: {} measured-but-invalid; spread {:.2}% exceeds {:.0}%",
                measurement.name,
                measurement.stats.spread * 100.0,
                SPREAD_LIMIT * 100.0
            );
        }
        let quantum_ratio = measurement.report.quantum_ns as f64 / measurement.stats.median_ns;
        if quantum_ratio > QUANTUM_LIMIT {
            invalid = true;
            println!(
                "validity: {} measured-but-invalid; clock quantum {} ns is {:.2}% of the median region, exceeding {:.0}%",
                measurement.name,
                measurement.report.quantum_ns,
                quantum_ratio * 100.0,
                QUANTUM_LIMIT * 100.0
            );
        }
    }
    if !invalid {
        println!(
            "validity: every variant spread is within {:.0}% and clock quantum is within {:.0}% of its median region",
            SPREAD_LIMIT * 100.0,
            QUANTUM_LIMIT * 100.0
        );
    }

    let script = measurements[0].stats.median_ns;
    let mimic = measurements[1].stats.median_ns;
    let copy_difference = (script - mimic).abs() / script;
    if copy_difference > SPREAD_LIMIT {
        invalid = true;
        println!(
            "decomposition: invalid; script and mimic differ by {:.2}% (> {:.0}%)",
            copy_difference * 100.0,
            SPREAD_LIMIT * 100.0
        );
    } else {
        println!(
            "decomposition: valid; script and mimic differ by {:.2}%",
            copy_difference * 100.0
        );
    }
    invalid
}
