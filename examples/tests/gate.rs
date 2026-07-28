//! Differential and regeneration gates for the teaching examples.

// Referencing the package library makes Cargo propagate build.rs's native
// engine archive into this integration-test link, where its addresses are
// registered with the development tier.
extern crate subscript_examples;
// Naming the dev-dependency propagates its test-only interop archive into
// this integration-test link.
extern crate subscript_interop_fixture;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use subscript_bindgen::generate_for_header;
use subscript_codegen::{
    emit_c, run_c_aot_with_native_libraries, run_jit_with_native_libraries, NativeLibrary,
};
use subscript_compiler::{check_program, SourceFile};

const ENGINE_MIRROR_NAME: &str = "engine.generated.d.ts";
const INTEROP_MIRROR_NAME: &str = "interop.generated.d.ts";

extern "C" {
    fn engWorldCreate();
    fn engWorldRetain();
    fn engWorldRelease();
    fn engWorldSetName();
    fn engWorldSetTransform();
    fn engWorldReplaceEntities();
    fn engWorldReadEntities();
    fn engWorldApplyFlags();
    fn engWorldSetEventSink();
    fn engWorldPump();
    fn engWorldLastEvent();
    fn engWorldStep();
    fn engFrameBegin();
    fn engFrameWorld();
    fn engFrameFixedStep();
    fn engFrameIndex();

    fn subDeviceCreate();
    fn subDeviceRelease();
    fn subDeviceSubmit();
    fn subDeviceOnComplete();
    fn subDevicePump();
}

#[derive(Debug)]
struct Program {
    id: String,
    source: String,
    expected: Vec<u8>,
    uses_engine: bool,
    uses_interop: bool,
}

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repository_root() -> Result<PathBuf, String> {
    examples_root()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "examples manifest directory has no repository parent".to_string())
}

fn is_example_stem(stem: &str) -> bool {
    let Some(rest) = stem.strip_prefix('e') else {
        return false;
    };
    let Some((number, slug)) = rest.split_once('-') else {
        return false;
    };
    !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) && !slug.is_empty()
}

fn load_program(path: &Path, id: String) -> Result<Program, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("read program {}: {error}", path.display()))?;
    let expected_path = path.with_extension("expected");
    let expected = fs::read(&expected_path)
        .map_err(|error| format!("read golden {}: {error}", expected_path.display()))?;
    Ok(Program {
        id,
        uses_engine: source.contains("engWorld") || source.contains("engFrame"),
        uses_interop: source.contains("subDevice"),
        source,
        expected,
    })
}

fn discover_examples() -> Result<Vec<Program>, String> {
    let root = examples_root();
    let entries = fs::read_dir(&root)
        .map_err(|error| format!("read examples directory {}: {error}", root.display()))?;
    let mut examples = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("read examples directory entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("ts") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if !is_example_stem(stem) {
            continue;
        }
        examples.push(load_program(&path, stem.to_string())?);
    }
    examples.sort_by(|left, right| left.id.cmp(&right.id));
    if examples.is_empty() {
        return Err("no e<nn>-<slug>.ts examples found".to_string());
    }
    Ok(examples)
}

fn discover_gate_programs() -> Result<Vec<Program>, String> {
    let directory = examples_root().join("gate");
    let entries = fs::read_dir(&directory)
        .map_err(|error| format!("read gate directory {}: {error}", directory.display()))?;
    let mut programs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("read gate directory entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("ts") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        programs.push(load_program(&path, format!("gate/{stem}"))?);
    }
    programs.sort_by(|left, right| left.id.cmp(&right.id));
    if programs.is_empty() {
        return Err("no phase-proof programs found under examples/gate".to_string());
    }
    Ok(programs)
}

fn read_mirror(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("read mirror {}: {error}", path.display()))
}

fn source_files(
    id: &str,
    source: &str,
    uses_engine: bool,
    uses_interop: bool,
) -> Result<Vec<SourceFile>, String> {
    let root = examples_root();
    let mut files = Vec::new();
    if uses_engine {
        let mirror_path = root.join("engine").join(ENGINE_MIRROR_NAME);
        files.push(SourceFile::ambient(
            ENGINE_MIRROR_NAME,
            read_mirror(&mirror_path)?,
        ));
    }
    if uses_interop {
        let mirror_path = repository_root()?
            .join("corpus")
            .join("interop")
            .join(INTEROP_MIRROR_NAME);
        files.push(SourceFile::ambient(
            INTEROP_MIRROR_NAME,
            read_mirror(&mirror_path)?,
        ));
    }
    files.push(SourceFile::new(format!("{id}.ts"), source));
    Ok(files)
}

fn engine_library() -> NativeLibrary {
    let directory = examples_root().join("engine");
    let symbols = vec![
        ("engWorldCreate".to_string(), engWorldCreate as *const u8),
        ("engWorldRetain".to_string(), engWorldRetain as *const u8),
        ("engWorldRelease".to_string(), engWorldRelease as *const u8),
        ("engWorldSetName".to_string(), engWorldSetName as *const u8),
        (
            "engWorldSetTransform".to_string(),
            engWorldSetTransform as *const u8,
        ),
        (
            "engWorldReplaceEntities".to_string(),
            engWorldReplaceEntities as *const u8,
        ),
        (
            "engWorldReadEntities".to_string(),
            engWorldReadEntities as *const u8,
        ),
        (
            "engWorldApplyFlags".to_string(),
            engWorldApplyFlags as *const u8,
        ),
        (
            "engWorldSetEventSink".to_string(),
            engWorldSetEventSink as *const u8,
        ),
        ("engWorldPump".to_string(), engWorldPump as *const u8),
        (
            "engWorldLastEvent".to_string(),
            engWorldLastEvent as *const u8,
        ),
        ("engWorldStep".to_string(), engWorldStep as *const u8),
        ("engFrameBegin".to_string(), engFrameBegin as *const u8),
        ("engFrameWorld".to_string(), engFrameWorld as *const u8),
        (
            "engFrameFixedStep".to_string(),
            engFrameFixedStep as *const u8,
        ),
        ("engFrameIndex".to_string(), engFrameIndex as *const u8),
    ];
    // SAFETY: build.rs links these static-lifetime functions into the test
    // process, and every address has the signature declared by engine.h and
    // its committed mirror.
    unsafe {
        NativeLibrary::new(
            vec![directory.clone()],
            vec![directory.join("engine.c")],
            symbols,
        )
    }
}

fn interop_library() -> Result<NativeLibrary, String> {
    let directory = repository_root()?.join("corpus").join("interop");
    let symbols = vec![
        ("subDeviceCreate".to_string(), subDeviceCreate as *const u8),
        (
            "subDeviceRelease".to_string(),
            subDeviceRelease as *const u8,
        ),
        ("subDeviceSubmit".to_string(), subDeviceSubmit as *const u8),
        (
            "subDeviceOnComplete".to_string(),
            subDeviceOnComplete as *const u8,
        ),
        ("subDevicePump".to_string(), subDevicePump as *const u8),
    ];
    // SAFETY: the test-only fixture crate links these static-lifetime
    // functions into the test process, and every address has the signature
    // declared by interop.h and its committed mirror.
    Ok(unsafe {
        NativeLibrary::new(
            vec![directory.clone()],
            vec![directory.join("interop.c")],
            symbols,
        )
    })
}

fn native_libraries(uses_engine: bool, uses_interop: bool) -> Result<Vec<NativeLibrary>, String> {
    let mut libraries = Vec::new();
    if uses_engine {
        libraries.push(engine_library());
    }
    if uses_interop {
        libraries.push(interop_library()?);
    }
    Ok(libraries)
}

fn run_jit_on_fresh_thread(program: &Program) -> Result<Vec<u8>, String> {
    let id = program.id.clone();
    let source = program.source.clone();
    let uses_engine = program.uses_engine;
    let uses_interop = program.uses_interop;
    thread::spawn(move || {
        let files = source_files(&id, &source, uses_engine, uses_interop)?;
        let libraries = native_libraries(uses_engine, uses_interop)?;
        run_jit_with_native_libraries(&files, &libraries)
            .map_err(|error| format!("{id}: dev-JIT run failed: {error}"))
    })
    .join()
    .map_err(|_| format!("{}: dev-JIT thread panicked", program.id))?
}

fn assert_programs_match(programs: &[Program], set_name: &str) {
    let mut failures = Vec::new();
    let mut compared = 0usize;
    for program in programs {
        let jit = match run_jit_on_fresh_thread(program) {
            Ok(output) => output,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let files = match source_files(
            &program.id,
            &program.source,
            program.uses_engine,
            program.uses_interop,
        ) {
            Ok(files) => files,
            Err(error) => {
                failures.push(format!("{}: {error}", program.id));
                continue;
            }
        };
        let libraries = match native_libraries(program.uses_engine, program.uses_interop) {
            Ok(libraries) => libraries,
            Err(error) => {
                failures.push(format!("{}: {error}", program.id));
                continue;
            }
        };
        let ship = match run_c_aot_with_native_libraries(&files, &libraries) {
            Ok(output) => output,
            Err(error) => {
                failures.push(format!("{}: ship-C-AOT run failed: {error}", program.id));
                continue;
            }
        };
        compared += 1;
        if jit != ship {
            failures.push(format!(
                "{}: dev-JIT output {:?} != ship-C-AOT output {:?}",
                program.id,
                String::from_utf8_lossy(&jit),
                String::from_utf8_lossy(&ship)
            ));
        }
        if jit != program.expected {
            failures.push(format!(
                "{}: dev-JIT output {:?} != golden {:?}",
                program.id,
                String::from_utf8_lossy(&jit),
                String::from_utf8_lossy(&program.expected)
            ));
        }
        if ship != program.expected {
            failures.push(format!(
                "{}: ship-C-AOT output {:?} != golden {:?}",
                program.id,
                String::from_utf8_lossy(&ship),
                String::from_utf8_lossy(&program.expected)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} {set_name} differential failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        compared,
        programs.len(),
        "every discovered {set_name} program must run on both tiers"
    );
}

#[test]
fn every_example_matches_dev_jit_ship_c_aot_and_golden() {
    let examples = discover_examples().unwrap_or_else(|error| panic!("discover examples: {error}"));
    assert_programs_match(&examples, "example");
}

#[test]
fn every_phase_gate_program_matches_dev_jit_ship_c_aot_and_golden() {
    let programs =
        discover_gate_programs().unwrap_or_else(|error| panic!("discover gate programs: {error}"));
    assert_programs_match(&programs, "phase gate");
}

#[test]
fn capstone_host_builds_runs_and_matches_golden() {
    let host = examples_root().join("host");
    let script = host.join("build.sh");
    let expected_path = host.join("expected.txt");
    let expected = fs::read(&expected_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", expected_path.display()));
    let output = Command::new("sh")
        .arg(&script)
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", script.display()));
    assert!(
        output.status.success(),
        "capstone build/run failed with {}:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        expected,
        "capstone stdout differs from {}:\nactual:\n{}",
        expected_path.display(),
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn derived_example_set_excludes_phase_gate_programs() {
    let examples = discover_examples().unwrap_or_else(|error| panic!("discover examples: {error}"));
    let gate_programs =
        discover_gate_programs().unwrap_or_else(|error| panic!("discover gate programs: {error}"));
    let example_ids = examples
        .iter()
        .map(|program| program.id.as_str())
        .collect::<Vec<_>>();
    println!("derived example set: {}", example_ids.join(", "));
    assert!(
        example_ids.iter().all(|id| is_example_stem(id)),
        "the derived example set contains an unnumbered program: {example_ids:?}"
    );
    assert!(
        gate_programs
            .iter()
            .all(|program| program.id.starts_with("gate/")
                && !example_ids.contains(&program.id.as_str())),
        "a phase-gate program entered the derived example set"
    );
}

#[test]
fn engine_mirror_regenerates_byte_identically() {
    let engine = examples_root().join("engine");
    let header_path = engine.join("engine.h");
    let mirror_path = engine.join(ENGINE_MIRROR_NAME);
    let header = fs::read_to_string(&header_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", header_path.display()));
    let committed = fs::read_to_string(&mirror_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", mirror_path.display()));
    let regenerated = generate_for_header(&header, "engine.h")
        .unwrap_or_else(|error| panic!("regenerate {}: {error}", mirror_path.display()));
    assert_eq!(
        regenerated, committed,
        "engine mirror is stale; regenerate with \
         `cargo run --offline -p subscript-bindgen -- --header \
         examples/engine/engine.h -o examples/engine/engine.generated.d.ts`"
    );
}

#[test]
fn two_header_gate_emits_both_provenance_vocabularies() {
    let programs =
        discover_gate_programs().unwrap_or_else(|error| panic!("discover gate programs: {error}"));
    let program = programs
        .iter()
        .find(|program| program.uses_engine && program.uses_interop)
        .unwrap_or_else(|| panic!("no phase-gate program binds both engine.h and interop.h"));
    let files = source_files(
        &program.id,
        &program.source,
        program.uses_engine,
        program.uses_interop,
    )
    .unwrap_or_else(|error| panic!("{}: load sources: {error}", program.id));
    let module = check_program(&files)
        .unwrap_or_else(|diagnostics| panic!("{}: check failed: {diagnostics:?}", program.id));
    let c = emit_c(&module)
        .unwrap_or_else(|error| panic!("{}: emit C failed: {error}", program.id))
        .source;

    let engine_include = c
        .find("#include \"engine.h\"")
        .unwrap_or_else(|| panic!("{}: emitted C lacks engine.h include", program.id));
    let interop_include = c
        .find("#include \"interop.h\"")
        .unwrap_or_else(|| panic!("{}: emitted C lacks interop.h include", program.id));
    assert!(
        engine_include < interop_include,
        "{}: mirror include order was not preserved",
        program.id
    );
    for spelling in [
        "((EngEntityStateView){",
        "((EngEntityStateOut){ (EngEntityState*)",
        "((SubBufferView){",
        "engWorldReplaceEntities(",
        "subDeviceSubmit(",
    ] {
        assert!(
            c.contains(spelling),
            "{}: emitted C lacks `{spelling}`",
            program.id
        );
    }
}
