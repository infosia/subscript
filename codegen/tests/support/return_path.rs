//! The harness of the return-path poll tests (`compiler.md` §351, §352):
//! the headroom and object size of the shapes, the count of allocating
//! instructions of one frame, the ship-tier and dev JIT runs with samples
//! of live objects and automatic collections, and the runs of one program
//! with a collection mode.

use std::path::PathBuf;

use subscript_codegen::lir::allocation::{
    instruction_allocation, InstructionAllocation, MethodBodies,
};
use subscript_codegen::lir::lower_module;
use subscript_codegen::{
    run_c_aot_configured, run_jit_configured, EntryArg, NativeLibrary, ReloadSession, RunConfig,
    RunError,
};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::context::CollectionMode;

/// The minimum headroom of each run of the bound tests.
pub const HEADROOM: usize = 64;

/// The payload size of `P`: four `i64` fields in the C layout (invariant
/// 1). The trigger counts the payload bytes that allocations request
/// (§346 rule 1).
pub const SIZE: usize = 4 * 8;

/// The most instructions with the fact "can allocate" in one function of
/// the program, other than calls of script functions. No function of a
/// shape has a loop that allocates, so a frame runs each of them once at
/// most.
pub fn per_frame_of(files: &[SourceFile]) -> usize {
    let module = lower_module(&check_program(files).expect("checks")).expect("lowers");
    let methods = MethodBodies::of_module(&module);
    module
        .functions
        .iter()
        .filter(|function| !function.exported)
        .map(|function| {
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter(|instruction| {
                    instruction_allocation(&module, &methods, function, instruction)
                        == InstructionAllocation::Can
                })
                .count()
        })
        .max()
        .unwrap_or(0)
}

/// One sample: live objects and automatic collections.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    /// Live objects of the Context.
    pub objects: u64,
    /// Automatic collections of the Context.
    pub collections: u64,
}

/// The objects over the base before the exit, and the automatic
/// collections in the call, of one depth in the ship tier.
#[derive(Debug, Clone, Copy)]
pub struct ShipFigures {
    /// The depth of the call.
    pub depth: i32,
    /// The objects over the base before the outermost exit.
    pub objects: u64,
    /// The automatic collections in the call.
    pub collections: u64,
    /// The automatic collections after the bottom of the recursion, if the
    /// program marks the bottom.
    pub after_bottom: Option<u64>,
    /// The largest payload capacity of a live allocation at the bottom, if
    /// the program marks the bottom.
    pub bottom_capacity: Option<u64>,
}

/// A new scratch directory for one test.
pub fn scratch(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "subscript-return-path-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).expect("create the scratch directory");
    directory
}

/// The ambient declaration of the bottom marker. The ship tier links a C
/// function that samples the collection count and the capacity of each
/// live allocation; the dev JIT links a function that does nothing.
pub const MARKER: &str =
    "// @subscript-c-header include=\"return_path_marker.h\"\ndeclare function markBottom(): void;";

extern "C" fn mark_bottom() {}

/// The native library of the bottom marker in the dev JIT.
pub fn dev_marker() -> NativeLibrary {
    // SAFETY: the function takes no argument, returns nothing, and lives
    // for the process.
    unsafe {
        NativeLibrary::new(
            Vec::new(),
            Vec::new(),
            vec![("markBottom".into(), mark_bottom as *const u8)],
        )
    }
}

/// Reads the sample of `depth` and `tag` from the hook output.
fn read(text: &str, name: &str, depth: i32, tag: &str) -> Sample {
    let line = text
        .lines()
        .find(|line| line.starts_with(&format!("{depth} {tag} ")))
        .unwrap_or_else(|| panic!("{name}: no {tag} sample in {text}"));
    let numbers = line
        .split_whitespace()
        .skip(2)
        .map(|value| value.parse::<u64>().expect("a count"))
        .collect::<Vec<_>>();
    Sample {
        objects: numbers[0],
        collections: numbers[1],
    }
}

/// Runs the export `run(depth)` of a program for each depth in the ship
/// tier, in one build, in automatic mode. A pre-entry host hook collects,
/// samples, brackets the export with one script entry, and samples between
/// the export and the exit. If `marked`, the program calls `markBottom` at
/// the bottom of the recursion, and the hook reports the collections and
/// the largest live capacity at that call.
pub fn ship_run(
    name: &str,
    files: &[SourceFile],
    depths: &[i32],
    marked: bool,
) -> Vec<ShipFigures> {
    ship_run_linked(name, files, depths, marked, Vec::new())
}

/// [`ship_run`] with more native libraries linked to the program, such as
/// the interop fixture.
pub fn ship_run_linked(
    name: &str,
    files: &[SourceFile],
    depths: &[i32],
    marked: bool,
    libraries: Vec<NativeLibrary>,
) -> Vec<ShipFigures> {
    let text = ship_text(name, files, depths, HEADROOM, libraries);
    depths
        .iter()
        .map(|&depth| {
            let base = read(&text, name, depth, "base");
            let exit = read(&text, name, depth, "before-exit");
            let bottom = read(&text, name, depth, "bottom");
            assert_eq!(
                marked,
                bottom.collections != u64::MAX,
                "{name}: the marker ran at depth {depth}"
            );
            ShipFigures {
                depth,
                objects: exit.objects - base.objects,
                collections: exit.collections - base.collections,
                after_bottom: marked.then(|| exit.collections - bottom.collections),
                bottom_capacity: marked.then_some(bottom.objects),
            }
        })
        .collect()
}

/// The samples of one depth at the calls of `markBottom` in the ship tier.
#[derive(Debug, Clone, Copy)]
pub struct BottomFigures {
    /// The depth of the call.
    pub depth: i32,
    /// The calls of `markBottom`.
    pub calls: u64,
    /// The live objects over the base at the first call.
    pub first: u64,
    /// The live objects over the base at the last call.
    pub last: u64,
    /// The automatic collections between the first and the last call.
    pub between: u64,
    /// The objects over the base before the outermost exit.
    pub objects: u64,
    /// The automatic collections after the first call.
    pub after_first: u64,
    /// The largest payload capacity of a live allocation at the calls.
    pub capacity: u64,
    /// The live bytes of the Context at the first call, as the ship tier
    /// counts them (§18.2d).
    pub first_bytes: u64,
}

/// Runs the export `run(depth)` of a program that calls `markBottom` one
/// time or more, as [`ship_run`] does, with a minimum headroom of
/// `headroom` bytes. The hook samples the live objects and the automatic
/// collections at the first and the last call of `markBottom`.
pub fn ship_bottom_run(
    name: &str,
    files: &[SourceFile],
    depths: &[i32],
    headroom: usize,
) -> Vec<BottomFigures> {
    let text = ship_text(name, files, depths, headroom, Vec::new());
    depths
        .iter()
        .map(|&depth| {
            let base = read(&text, name, depth, "base");
            let exit = read(&text, name, depth, "before-exit");
            let line = text
                .lines()
                .find(|line| line.starts_with(&format!("{depth} bottom ")))
                .unwrap_or_else(|| panic!("{name}: no bottom sample in {text}"));
            let numbers = line
                .split_whitespace()
                .skip(2)
                .map(|value| value.parse::<u64>().expect("a count"))
                .collect::<Vec<_>>();
            let [capacity, last_collections, first_collections, first, last, calls, first_bytes] =
                numbers[..]
            else {
                panic!("{name}: a bottom sample has seven numbers: {line}");
            };
            assert!(calls > 0, "{name}: the marker ran at depth {depth}");
            BottomFigures {
                depth,
                calls,
                first: first - base.objects,
                last: last - base.objects,
                between: last_collections - first_collections,
                objects: exit.objects - base.objects,
                after_first: exit.collections - first_collections,
                capacity,
                first_bytes,
            }
        })
        .collect()
}

/// Builds and runs the ship program of [`ship_run`], with `libraries` linked
/// after the hooks, and returns its output.
fn ship_text(
    name: &str,
    files: &[SourceFile],
    depths: &[i32],
    headroom: usize,
    libraries: Vec<NativeLibrary>,
) -> String {
    let directory = scratch(name);
    let hooks = directory.join("return_path_hooks.c");
    let calls = depths
        .iter()
        .map(|depth| {
            format!(
                "  subscript_rt_ctx_collect(ctx);\n  sample(ctx, {depth}, \"base\");\n\
                 \x20 bottom_collections = UINT64_MAX; bottom_capacity = 0; bottom_calls = 0;\n\
                 \x20 subscript_rt_ctx_enter_script(ctx);\n\
                 \x20 subscript_export_run(ctx, {depth});\n\
                 \x20 printf(\"%d bottom %llu %llu %llu %llu %llu %llu %llu\\n\", {depth},\n\
                 \x20   (unsigned long long)bottom_capacity, (unsigned long long)bottom_collections,\n\
                 \x20   (unsigned long long)first_collections, (unsigned long long)first_objects,\n\
                 \x20   (unsigned long long)last_objects, (unsigned long long)bottom_calls,\n\
                 \x20   (unsigned long long)first_bytes);\n\
                 \x20 sample(ctx, {depth}, \"before-exit\");\n\
                 \x20 subscript_rt_ctx_exit_script(ctx);\n"
            )
        })
        .collect::<String>();
    std::fs::write(
        directory.join("return_path_marker.h"),
        "void markBottom(void);\n",
    )
    .expect("write the marker header");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdint.h>\n\
             #include <stdio.h>\n\
             #include \"program.h\"\n\
             uint64_t subscript_rt_automatic_collection_count(const subscript_rt_context* ctx);\n\
             static subscript_rt_context* current;\n\
             static uint64_t bottom_collections;\n\
             static uint64_t bottom_capacity;\n\
             static uint64_t bottom_calls;\n\
             static uint64_t first_collections;\n\
             static uint64_t first_objects;\n\
             static uint64_t last_objects;\n\
             static uint64_t first_bytes;\n\
             static void capacity(void* userdata, uint32_t class_id, uint32_t pos_id, uint64_t bytes) {{\n\
               (void)userdata; (void)class_id; (void)pos_id;\n\
               if (bytes > bottom_capacity) {{ bottom_capacity = bytes; }}\n\
             }}\n\
             void markBottom(void) {{\n\
               bottom_collections = subscript_rt_automatic_collection_count(current);\n\
               last_objects = subscript_rt_ctx_live_allocations(current);\n\
               if (bottom_calls == 0) {{\n\
                 first_collections = bottom_collections; first_objects = last_objects;\n\
                 first_bytes = subscript_rt_ctx_live_bytes(current);\n\
               }}\n\
               bottom_calls += 1;\n\
               (void)subscript_rt_ctx_visit_live_allocations(current, capacity, NULL);\n\
             }}\n\
             void subscript_test_return_path_setup(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, 1, {headroom}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n\
             static void sample(subscript_rt_context* ctx, int depth, const char* tag) {{\n\
               printf(\"%d %s %llu %llu\\n\", depth, tag,\n\
                 (unsigned long long)subscript_rt_ctx_live_allocations(ctx),\n\
                 (unsigned long long)subscript_rt_automatic_collection_count(ctx));\n\
             }}\n\
             void subscript_test_return_path_run(subscript_rt_context* ctx) {{\n\
               current = ctx;\n\
             {calls}\
               fflush(stdout);\n\
             }}\n"
        ),
    )
    .expect("write the hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the C source defines the host hooks and the marker, which the
    // C build links. The symbol entry names the marker for the check of
    // foreign symbols; the dev JIT function lives for the process.
    let mut library = vec![unsafe {
        NativeLibrary::new(
            vec![repo.join("runtime/include"), directory.clone()],
            vec![hooks],
            vec![("markBottom".into(), mark_bottom as *const u8)],
        )
    }];
    library.extend(libraries);
    let mut config = RunConfig::default().with_native_libraries(&library);
    config.pre_init_hook = Some("subscript_test_return_path_setup");
    config.pre_entry_hook = Some("subscript_test_return_path_run");
    let output = run_c_aot_configured(files, config).expect("the ship tier runs");
    let _ = std::fs::remove_dir_all(&directory);
    let text = String::from_utf8(output.stdout).expect("utf-8 output");
    assert!(!text.contains("setup-refused"), "{name}: {text}");
    text
}

/// The objects over the base before the outermost exit of one depth in the
/// dev JIT, and the automatic collections and outermost exits of a run of
/// `main` at that depth. The program has the exports `settle`, which
/// collects, `probe(depth)`, which runs the shape and then stops on an
/// index trap, and `main`. A Context with a trap runs no collection at the
/// outermost exit (§342 rule 8), so the live count after `probe` is the
/// live count before the exit.
pub fn dev_run(
    name: &str,
    files: &[SourceFile],
    depth: i32,
    libraries: &[NativeLibrary],
) -> (usize, u64, u64) {
    let mut config = RunConfig::default().with_native_libraries(libraries);
    config.collection = Some((CollectionMode::Automatic, HEADROOM));
    let mut session = ReloadSession::new_configured(files, config).expect("the session starts");
    session.call_export("settle").expect("settle runs");
    let base = session.live_allocations();
    match session.call_export_with("probe", &[EntryArg::I32(depth)]) {
        Err(RunError::Trap(_)) => {}
        other => panic!("{name}: probe({depth}) does not trap: {other:?}"),
    }
    let objects = session.live_allocations() - base;
    let mut config = RunConfig::default().with_native_libraries(libraries);
    config.collection = Some((CollectionMode::Automatic, HEADROOM));
    config.memory_accounting = true;
    let run = run_jit_configured(files, config).expect("main runs");
    let accounting = run.memory_accounting.expect("accounting");
    (
        objects,
        accounting.automatic_collections,
        accounting.outermost_exits,
    )
}

/// The live objects over the base in the dev JIT when the program stops at
/// the call `stop` of its bottom marker, with a minimum headroom of
/// `headroom` bytes. The program has the exports `settle`, which collects,
/// and `probe(depth, stop)`, which runs the shape and stops on an index
/// trap at that call. A Context with a trap runs no collection at the
/// outermost exit (§342 rule 8).
pub fn dev_bottom(
    name: &str,
    files: &[SourceFile],
    depth: i32,
    stop: i32,
    headroom: usize,
    libraries: &[NativeLibrary],
) -> usize {
    let mut config = RunConfig::default().with_native_libraries(libraries);
    config.collection = Some((CollectionMode::Automatic, headroom));
    let mut session = ReloadSession::new_configured(files, config).expect("the session starts");
    session.call_export("settle").expect("settle runs");
    let base = session.live_allocations();
    match session.call_export_with("probe", &[EntryArg::I32(depth), EntryArg::I32(stop)]) {
        Err(RunError::Trap(_)) => {}
        other => panic!("{name}: probe({depth}, {stop}) does not trap: {other:?}"),
    }
    session.live_allocations() - base
}

/// The ship tier with the collection mode set before the initializer.
pub fn ship_output(source: &str, mode: i32) -> String {
    let directory = scratch(&format!("roots-{mode}"));
    let hooks = directory.join("return_path_roots.c");
    std::fs::write(
        &hooks,
        format!(
            "#include <stdio.h>\n\
             #include \"subscript_runtime.h\"\n\
             void subscript_test_return_path_mode(subscript_rt_context* ctx) {{\n\
               if (subscript_rt_ctx_set_collection(ctx, {mode}, {HEADROOM}u) != 0) {{ printf(\"setup-refused\\n\"); }}\n\
             }}\n"
        ),
    )
    .expect("write the hook source");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    // SAFETY: the library defines only a host hook, which the C build links.
    let library = [unsafe {
        NativeLibrary::new(vec![repo.join("runtime/include")], vec![hooks], Vec::new())
    }];
    let mut config = RunConfig::default().with_native_libraries(&library);
    config.pre_init_hook = Some("subscript_test_return_path_mode");
    let files = [SourceFile::new("return-path-roots.ts", source)];
    let output = run_c_aot_configured(&files, config).expect("the ship tier runs");
    let _ = std::fs::remove_dir_all(&directory);
    String::from_utf8(output.stdout).expect("utf-8 output")
}

/// The dev JIT with a collection mode. Memory accounting keeps the run in
/// the test process.
pub fn dev_output(source: &str, mode: CollectionMode) -> String {
    let files = [SourceFile::new("return-path-roots.ts", source)];
    let mut config = RunConfig::default();
    config.collection = Some((mode, HEADROOM));
    config.memory_accounting = true;
    let run = run_jit_configured(&files, config).expect("the dev JIT runs");
    String::from_utf8(run.stdout).expect("utf-8 output")
}
