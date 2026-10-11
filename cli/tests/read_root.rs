//! `specs/blocks/compiler.md` §187 rules 3 and 6 to 15, end to end: the
//! read root of each position is the code-generation fact; the call writes
//! back the members that C changed in each scratch copy of a non-`const`
//! pointer and skips a userdata slot and a callback field; no call builds
//! a struct cycle, and a `const` cycle of byte-copied structs passes as
//! script memory; a by-value struct whose bytes the call copies passes
//! them; a pair keeps its script array at every reach, a pair whose
//! elements the call writes back is rejected, and a writable pair of
//! validated elements has no read lowering; a result lowers by its read
//! facts; a scalar fixed array in a scratch struct copies as bytes; a link
//! to an embedded header passes its declared type and reads no class id,
//! so C memory passes as it is; and the
//! binder's write validation rejects no member that the pass decision
//! lowers. Each case binds a subset of the read-root fixture header, checks
//! the mirror, and runs the accepted programs in the dev JIT and in C AOT
//! against the fixture's C implementation.
//!
//! A rejected form has no mirror, because the binder rejects it first. Its
//! checker case binds the `const` twin of the declaration and changes its
//! `const` record back to `false`: that mirror is the binder's mirror of
//! the rejected header.
//!
//! Cost: each bind is one in-memory libclang parse; each check is in
//! memory. Each run compiles one program in the dev JIT and one in C AOT
//! (one C compiler process and one program process each). Each accepted
//! case runs its program and a control, so the printed values come from C.
//! On Apple arm64 one program in both tiers takes about 0.4 s, which the C
//! AOT run dominates; the file (23 tests) runs in 3.6 s in parallel.
#![cfg(not(all(windows, target_env = "msvc")))]

// Naming the fixture crate links its native archive into this test, so
// the dev JIT can call the read-root functions.
extern crate subscript_interop_fixture;

use std::path::PathBuf;

use subscript_bindgen::{boundary_passes, generate_for_header};
use subscript_boundary::{member_pass, struct_pass, FieldShape, StructPass, StructView};
use subscript_codegen::{
    run_c_aot_with_native_libraries, run_jit_with_native_libraries, NativeLibrary,
};
use subscript_compiler::boundary_pass::Classes;
use subscript_compiler::lir::ForeignTypeProvenance;
use subscript_compiler::{check_program, SourceFile};
use subscript_interop_fixture::{READ_ROOT_DIRECTORY, READ_ROOT_HEADER};

extern "C" {
    fn subReadRootHolderFill();
    fn subReadRootHolderSum();
    fn subReadRootOuterTouch();
    fn subReadRootOuterSum();
    fn subReadRootNamedFill();
    fn subReadRootDeviceNamedFill();
    fn subReadRootDevice();
    fn subReadRootDeviceId();
    fn subReadRootCmdRun();
    fn subReadRootOuterBump();
    fn subReadRootItemListFill();
    fn subReadRootItemSpanTouch();
    fn subReadRootLinkedUse();
    fn subReadRootExtUse();
    fn subReadRootNamedUdFill();
    fn subReadRootUdFill();
    fn subReadRootRender();
    fn subReadRootRenderConst();
    fn subReadRootJobRun();
    fn subReadRootJobSet();
    fn subReadRootNodeBump();
    fn subReadRootCycleUse();
    fn subReadRootFtSum();
    fn subReadRootFtSumP();
    fn subReadRootSceneDeform();
    fn subReadRootNWalk();
    fn subReadRootSWalk();
    fn subReadRootFrGet();
    fn subReadRootFbGet();
    fn subReadRootBoundsRender();
    fn subReadRootTrSum();
    fn subReadRootTrVal();
    fn subReadRootTrFill();
    fn subReadRootRecFill();
    fn subReadRootRecTouch();
    fn subReadRootRecCGet();
    fn subReadRootHUse();
    fn subReadRootGUse();
    fn subReadRootPDraw();
    fn subReadRootPDrawV();
    fn subReadRootPNamedUse();
    fn subReadRootPSubmit();
    fn subReadRootPStep();
    fn subReadRootCSum();
    fn subReadRootCross();
    fn subReadRootMousePos();
    fn subReadRootLabelLen();
    fn subReadRootDrawMeshInstanced();
    fn subReadRootLinkedExt();
    fn subReadRootWTouch();
    fn subReadRootWRead();
    fn subReadRootIvAt();
    fn subReadRootIvSet();
    fn subReadRootIvSum();
    fn subReadRootIvBump();
    fn subReadRootLabLen();
    fn subReadRootNSum();
    fn subReadRootRenderScene();
}

const HEADER_NAME: &str = "read-root.h";

const TAIL: &str = "with no read lowering; a struct that C writes and the script reads must \
                    not hold one (compiler.md §187)";

/// The function prototypes of the fixture header, each on its own line.
const FUNCTIONS: [&str; 58] = [
    "subReadRootHolderFill",
    "subReadRootHolderSum",
    "subReadRootOuterTouch",
    "subReadRootOuterSum",
    "subReadRootNamedFill",
    "subReadRootDeviceNamedFill",
    "subReadRootDevice",
    "subReadRootDeviceId",
    "subReadRootCmdRun",
    "subReadRootOuterBump",
    "subReadRootItemListFill",
    "subReadRootItemSpanTouch",
    "subReadRootLinkedUse",
    "subReadRootExtUse",
    "subReadRootNamedUdFill",
    "subReadRootUdFill",
    "subReadRootRender",
    "subReadRootRenderConst",
    "subReadRootJobRun",
    "subReadRootJobSet",
    "subReadRootNodeBump",
    "subReadRootCycleUse",
    "subReadRootFtSum",
    "subReadRootFtSumP",
    "subReadRootSceneDeform",
    "subReadRootNWalk",
    "subReadRootSWalk",
    "subReadRootFrGet",
    "subReadRootFbGet",
    "subReadRootBoundsRender",
    "subReadRootTrSum",
    "subReadRootTrVal",
    "subReadRootTrFill",
    "subReadRootRecFill",
    "subReadRootRecTouch",
    "subReadRootRecCGet",
    "subReadRootHUse",
    "subReadRootGUse",
    "subReadRootPDraw",
    "subReadRootPDrawV",
    "subReadRootPNamedUse",
    "subReadRootPSubmit",
    "subReadRootPStep",
    "subReadRootCSum",
    "subReadRootCross",
    "subReadRootMousePos",
    "subReadRootLabelLen",
    "subReadRootDrawMeshInstanced",
    "subReadRootLinkedExt",
    "subReadRootWTouch",
    "subReadRootWRead",
    "subReadRootIvAt",
    "subReadRootIvSet",
    "subReadRootIvSum",
    "subReadRootIvBump",
    "subReadRootLabLen",
    "subReadRootNSum",
    "subReadRootRenderScene",
];

/// The declarations of the fixture header that a subset holds only when
/// `keep` names them: the structs that hold a string view, or embed or link
/// one that does (the binder binds one only when a foreign pointer
/// parameter passes it), and the declarations of single cases: the job
/// callback, the struct cycles, the fixed array, the scalar-pair scene,
/// the nested pairs, the results, the scalar fixed arrays, and the `CEnum`
/// pairs.
const CASE_DECLARATIONS: [&str; 62] = [
    "RrNamed",
    "RrDeviceNamed",
    "RrCmd",
    "RrBase",
    "RrExt",
    "RrLinked",
    "RrNamedUd",
    "RrJobCallback",
    "RrCount",
    "RrJob",
    "RrNode",
    "RrCycleA",
    "RrCycleB",
    "RrFt",
    "RrDMesh",
    "RrDScene",
    "RrNLeaf",
    "RrNMid",
    "RrNTop",
    "RrSMid",
    "RrSTop",
    "RrFp",
    "RrFr",
    "RrFb",
    "RrBMesh",
    "RrBScene",
    "RrTr",
    "RrModeC",
    "RrRec",
    "RrRecC",
    "RrHBase",
    "RrHExt",
    "RrHHolder",
    "RrGBase",
    "RrGExt",
    "RrGHolder",
    "RrPMesh",
    "RrPNamed",
    "RrPJob",
    "RrPBody",
    "RrPWorld",
    "RrCNode",
    "RrVec3",
    "RrVMesh",
    "RrEntity",
    "RrVec2",
    "RrLabel",
    "RrMatrix",
    "RrRMesh",
    "RrModel",
    "RrWExt",
    "RrWHolder",
    "RrIV2",
    "RrSpr",
    "RrLab",
    "RrTile",
    "RrNodeV",
    "RrNView",
    "RrScMaterial",
    "RrScMesh",
    "RrScEntity",
    "RrScScene",
];

/// The fixture header with only the prototypes of `keep`, and only the
/// case declarations that `keep` names.
fn subset(keep: &[&str]) -> String {
    READ_ROOT_HEADER
        .lines()
        .filter(|line| {
            let function = FUNCTIONS
                .iter()
                .find(|function| line.contains(&format!(" {function}(")));
            let declaration = CASE_DECLARATIONS.iter().find(|name| {
                line.ends_with(&format!(" {name};"))
                    || line.contains(&format!("(*{name})"))
                    || line.contains(&format!("@subscript-cenum {name} "))
            });
            function
                .or(declaration)
                .is_none_or(|declared| keep.contains(declared))
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

fn bind(header: &str) -> Result<String, String> {
    generate_for_header(header, HEADER_NAME).map_err(|error| error.0)
}

fn files(mirror: &str, program: &str) -> Vec<SourceFile> {
    vec![
        SourceFile::ambient("read-root.generated.d.ts", mirror.to_string()),
        SourceFile::new("main.ts", program.to_string()),
    ]
}

/// The checker's diagnostics for `program` against `mirror`.
fn check(mirror: &str, program: &str) -> Vec<String> {
    match check_program(&files(mirror, program)) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect(),
    }
}

fn library() -> NativeLibrary {
    library_with(&[])
}

/// The fixture library with `includes` as further include directories.
fn library_with(includes: &[PathBuf]) -> NativeLibrary {
    let directory = PathBuf::from(READ_ROOT_DIRECTORY);
    let symbols = vec![
        (
            "subReadRootHolderFill".into(),
            subReadRootHolderFill as *const u8,
        ),
        (
            "subReadRootHolderSum".into(),
            subReadRootHolderSum as *const u8,
        ),
        (
            "subReadRootOuterTouch".into(),
            subReadRootOuterTouch as *const u8,
        ),
        (
            "subReadRootOuterSum".into(),
            subReadRootOuterSum as *const u8,
        ),
        (
            "subReadRootNamedFill".into(),
            subReadRootNamedFill as *const u8,
        ),
        (
            "subReadRootDeviceNamedFill".into(),
            subReadRootDeviceNamedFill as *const u8,
        ),
        ("subReadRootDevice".into(), subReadRootDevice as *const u8),
        (
            "subReadRootDeviceId".into(),
            subReadRootDeviceId as *const u8,
        ),
        ("subReadRootCmdRun".into(), subReadRootCmdRun as *const u8),
        (
            "subReadRootOuterBump".into(),
            subReadRootOuterBump as *const u8,
        ),
        (
            "subReadRootItemListFill".into(),
            subReadRootItemListFill as *const u8,
        ),
        (
            "subReadRootItemSpanTouch".into(),
            subReadRootItemSpanTouch as *const u8,
        ),
        (
            "subReadRootLinkedUse".into(),
            subReadRootLinkedUse as *const u8,
        ),
        ("subReadRootExtUse".into(), subReadRootExtUse as *const u8),
        (
            "subReadRootNamedUdFill".into(),
            subReadRootNamedUdFill as *const u8,
        ),
        ("subReadRootUdFill".into(), subReadRootUdFill as *const u8),
        ("subReadRootRender".into(), subReadRootRender as *const u8),
        (
            "subReadRootRenderConst".into(),
            subReadRootRenderConst as *const u8,
        ),
        ("subReadRootJobRun".into(), subReadRootJobRun as *const u8),
        ("subReadRootJobSet".into(), subReadRootJobSet as *const u8),
        (
            "subReadRootNodeBump".into(),
            subReadRootNodeBump as *const u8,
        ),
        (
            "subReadRootCycleUse".into(),
            subReadRootCycleUse as *const u8,
        ),
        ("subReadRootFtSum".into(), subReadRootFtSum as *const u8),
        ("subReadRootFtSumP".into(), subReadRootFtSumP as *const u8),
        (
            "subReadRootSceneDeform".into(),
            subReadRootSceneDeform as *const u8,
        ),
        ("subReadRootNWalk".into(), subReadRootNWalk as *const u8),
        ("subReadRootSWalk".into(), subReadRootSWalk as *const u8),
        ("subReadRootFrGet".into(), subReadRootFrGet as *const u8),
        ("subReadRootFbGet".into(), subReadRootFbGet as *const u8),
        (
            "subReadRootBoundsRender".into(),
            subReadRootBoundsRender as *const u8,
        ),
        ("subReadRootTrSum".into(), subReadRootTrSum as *const u8),
        ("subReadRootTrVal".into(), subReadRootTrVal as *const u8),
        ("subReadRootTrFill".into(), subReadRootTrFill as *const u8),
        ("subReadRootRecFill".into(), subReadRootRecFill as *const u8),
        (
            "subReadRootRecTouch".into(),
            subReadRootRecTouch as *const u8,
        ),
        ("subReadRootRecCGet".into(), subReadRootRecCGet as *const u8),
        ("subReadRootHUse".into(), subReadRootHUse as *const u8),
        ("subReadRootGUse".into(), subReadRootGUse as *const u8),
        ("subReadRootPDraw".into(), subReadRootPDraw as *const u8),
        ("subReadRootPDrawV".into(), subReadRootPDrawV as *const u8),
        (
            "subReadRootPNamedUse".into(),
            subReadRootPNamedUse as *const u8,
        ),
        ("subReadRootPSubmit".into(), subReadRootPSubmit as *const u8),
        ("subReadRootPStep".into(), subReadRootPStep as *const u8),
        ("subReadRootCSum".into(), subReadRootCSum as *const u8),
        ("subReadRootCross".into(), subReadRootCross as *const u8),
        (
            "subReadRootMousePos".into(),
            subReadRootMousePos as *const u8,
        ),
        (
            "subReadRootLabelLen".into(),
            subReadRootLabelLen as *const u8,
        ),
        (
            "subReadRootDrawMeshInstanced".into(),
            subReadRootDrawMeshInstanced as *const u8,
        ),
        (
            "subReadRootLinkedExt".into(),
            subReadRootLinkedExt as *const u8,
        ),
        ("subReadRootWTouch".into(), subReadRootWTouch as *const u8),
        ("subReadRootWRead".into(), subReadRootWRead as *const u8),
        ("subReadRootIvAt".into(), subReadRootIvAt as *const u8),
        ("subReadRootIvSet".into(), subReadRootIvSet as *const u8),
        ("subReadRootIvSum".into(), subReadRootIvSum as *const u8),
        ("subReadRootIvBump".into(), subReadRootIvBump as *const u8),
        ("subReadRootLabLen".into(), subReadRootLabLen as *const u8),
        ("subReadRootNSum".into(), subReadRootNSum as *const u8),
        (
            "subReadRootRenderScene".into(),
            subReadRootRenderScene as *const u8,
        ),
    ];
    // SAFETY: the fixture crate links these static-lifetime functions into
    // this test process, and each address implements the C signature that
    // `read-root.h` declares and the bound mirror names.
    unsafe {
        NativeLibrary::new(
            std::iter::once(directory.clone())
                .chain(includes.iter().cloned())
                .collect(),
            vec![directory.join("read-root.c")],
            symbols,
        )
    }
}

/// Runs `program` in both tiers and returns `(dev JIT, C AOT)` stdout.
fn run_both(mirror: &str, program: &str) -> (String, String) {
    let files = files(mirror, program);
    let libraries = [library()];
    let jit = run_jit_with_native_libraries(&files, &libraries)
        .unwrap_or_else(|error| panic!("dev JIT: {error}"));
    let aot = run_c_aot_with_native_libraries(&files, &libraries)
        .unwrap_or_else(|error| panic!("C AOT: {error}"));
    (
        String::from_utf8_lossy(&jit).into_owned(),
        String::from_utf8_lossy(&aot).into_owned(),
    )
}

/// The binder's mirror of a rejected header: the mirror of its `const`
/// twin, with the `const` record of the twin set back to `false`.
fn rejected_mirror(keep: &[&str], from: &str, to: &str, record: &str) -> String {
    let twin = subset(keep).replace(from, to);
    assert_ne!(twin, subset(keep), "the twin changes `{from}`");
    let mirror = bind(&twin).unwrap_or_else(|error| panic!("twin: {error}"));
    let true_record = format!("{record} const=true");
    assert!(mirror.contains(&true_record), "{mirror}");
    mirror.replace(&true_record, &format!("{record} const=false"))
}

/// Asserts that the binder rejects `keep` and the checker rejects the
/// binder's mirror of it, each with `message`.
fn assert_rejected(keep: &[&str], twin: (&str, &str, &str), message: &str) {
    let expected = format!("{message} {TAIL}");
    assert_eq!(bind(&subset(keep)), Err(expected.clone()), "binder");
    let mirror = rejected_mirror(keep, twin.0, twin.1, twin.2);
    let program = "export function main(): void {}\n";
    assert_eq!(check(&mirror, program), vec![expected], "checker");
}

/// Runs `program(call)` with `call` and with no call, in both tiers, and
/// asserts each output in both tiers. The run with no call is the control:
/// it shows that the values of the first run come from C.
fn assert_runs(mirror: &str, program: impl Fn(&str) -> String, call: &str, outputs: [&str; 2]) {
    for (call, expected) in [(call, outputs[0]), ("", outputs[1])] {
        let expected = format!("{expected}\n");
        assert_eq!(
            run_both(mirror, &program(call)),
            (expected.clone(), expected),
            "{call:?}"
        );
    }
}

/// §187 rule 7, 187.3 item 4: `subReadRootHolderFill` replaces `out->p`.
/// The copy-back keeps the script link, so the replacement is lost and
/// the script `RrPoint` keeps its value; `k` is read.
fn a_fill_keeps_the_script_link_of_a_pointer_member() {
    let mirror = bind(&subset(&["subReadRootHolderFill"])).expect("the fill binds");
    let program = |call: &str| {
        format!(
            "export function main(): void {{\n  \
             const holder = new RrHolder(0, new RrPoint(70));\n  {call}\n  \
             const p = holder.p;\n  \
             print(`${{holder.k}} ${{p === null ? -1 : p.x}}`);\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootHolderFill(holder);",
        ["5 70", "0 70"],
    );
}

/// §187 rule 7: the call passes the target `RrInner` as a scratch copy and
/// writes it back, from a by-value root (`subReadRootOuterTouch`) and from
/// a `const` pointer root (`subReadRootOuterBump`). `RrQ` copies its bytes,
/// so the call passes its script memory. A null root builds no target, and
/// the call writes back nothing.
fn a_pointer_target_is_written_back() {
    let program = |call: &str| {
        format!(
            "export function main(): void {{\n  \
             const outer = new RrOuter(1, new RrInner(2, new RrQ(3)));\n  {call}\n  \
             const inner = outer.inner;\n  \
             if (inner !== null) {{\n    const q = inner.q;\n    \
             print(`${{inner.x}} ${{q === null ? -1 : q.y}}`);\n  }}\n}}\n"
        )
    };
    let mirror = bind(&subset(&["subReadRootOuterTouch"])).expect("the by-value root binds");
    assert_runs(
        &mirror,
        program,
        "subReadRootOuterTouch(outer);",
        ["9 11", "2 3"],
    );
    let mirror = bind(&subset(&["subReadRootOuterBump"])).expect("the pointer root binds");
    // A null root builds no target: the call writes back nothing.
    assert_runs(
        &mirror,
        program,
        "subReadRootOuterBump(outer);\n  subReadRootOuterBump(null);",
        ["12 13", "2 3"],
    );
}

/// §187 rule 7: a read-only host with a non-`const` pointer graph. The
/// call passes `RrScene` and `RrMesh` as scratch copies and writes both
/// back; `RrMaterial` copies its bytes.
fn a_scene_graph_is_written_back_at_every_depth() {
    let mirror = bind(&subset(&["subReadRootRender"])).expect("the scene graph binds");
    let program = |call: &str| {
        let call = if call.is_empty() {
            "const id = scene.id;".to_string()
        } else {
            format!("const id = {call}")
        };
        format!(
            "export function main(): void {{\n  \
             const scene = new RrScene(7, new RrMesh(3, new RrMaterial(1.5)));\n  {call}\n  \
             const mesh = scene.mesh;\n  \
             if (mesh !== null) {{\n    const material = mesh.mat;\n    \
             print(`${{id}} ${{mesh.count}} ${{material === null ? -1 : material.r}}`);\n  \
             }}\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootRender(scene);",
        ["7 4 3", "7 3 1.5"],
    );
}

/// §187 rules 7 and 11: a pair whose elements the call builds as scratch
/// copies and writes back is rejected: a `const` pair of `RrItem`, whose
/// `inner` C can write, at a fill (`subReadRootItemListFill`); a by-value
/// descriptor of mutable `RrItem` elements (`subReadRootItemSpanTouch`);
/// a mutable pair of struct elements that hold a pair, at a `const` root
/// (`subReadRootNWalk`, and its scalar twin `subReadRootSWalk`); and a
/// `const` pair whose elements link a mesh that C can write
/// (`subReadRootRenderScene`). The binder rejects each header, and the
/// checker rejects the binder's mirror of it, with one text. A `const`
/// pair names the member that needs the write-back. Each twin with
/// `const` pointers binds.
fn a_pair_whose_elements_are_written_back_is_rejected() {
    let lead = "a pair whose elements the call copies in and back one by one. The cost grows \
                with the count, and the call site does not show it.";
    let rule = "(compiler.md §187 rule 11)";
    let mutable = format!(
        "{lead} Declare the element pointer `const`, or use elements that copy their bytes \
         {rule}"
    );
    let reaches = |member: &str| {
        format!(
            "{lead} C can write the scratch target of `{member}`; a `const` on that pointer, \
             or on a pointer between, makes the pair readable {rule}"
        )
    };
    let item_const = (
        "int32_t k; RrInner *inner; } RrItem;",
        "int32_t k; const RrInner *inner; } RrItem;",
    );
    let item_record = "// @subscript-c-member aggregate=\"RrItem\" member=\"inner\"";
    let span = format!(
        "{lead} Declare the element pointer `const`, and put a `const` on `RrItem.inner` or \
         on a pointer between; or use elements that copy their bytes {rule}"
    );
    // (keep, the replacements of the `const` twin, the records that the
    // checker case sets back to `false`, the text after the function)
    type Case<'a> = (
        &'a [&'a str],
        &'a [(&'a str, &'a str)],
        &'a [&'a str],
        String,
    );
    let cases: [Case<'_>; 5] = [
        (
            &["subReadRootItemListFill"],
            &[item_const],
            &[item_record],
            format!(
                "parameter `out` reads `RrItemList.items`, {}",
                reaches("RrItem.inner")
            ),
        ),
        (
            &["subReadRootItemSpanTouch"],
            &[
                item_const,
                (
                    "RrItem *items; size_t count;",
                    "const RrItem *items; size_t count;",
                ),
            ],
            &[
                item_record,
                "// @subscript-c-descriptor function=\"subReadRootItemSpanTouch\" \
                 parameter=\"span\" aggregate=\"RrItemSpan\" element=\"RrItem\"",
            ],
            format!("parameter `span` passes {span}"),
        ),
        (
            &["subReadRootNWalk", "RrNLeaf", "RrNMid", "RrNTop"],
            &[("RrNMid *mids;", "const RrNMid *mids;")],
            &["// @subscript-c-member aggregate=\"RrNTop\" member=\"mids\""],
            format!("parameter `top` reads `RrNTop.mids`, {mutable}"),
        ),
        (
            &["subReadRootSWalk", "RrSMid", "RrSTop"],
            &[("RrSMid *mids;", "const RrSMid *mids;")],
            &["// @subscript-c-member aggregate=\"RrSTop\" member=\"mids\""],
            format!("parameter `top` reads `RrSTop.mids`, {mutable}"),
        ),
        (
            RENDER_SCENE,
            &[RENDER_SCENE_CONST],
            &["// @subscript-c-member aggregate=\"RrScEntity\" member=\"mesh\""],
            format!(
                "parameter `s` reads `RrScScene.entities`, {}",
                reaches("RrScEntity.mesh")
            ),
        ),
    ];
    for (keep, twins, records, text) in cases {
        let function = keep[0];
        let expected = format!("foreign function `{function}` {text}");
        assert_eq!(bind(&subset(keep)), Err(expected.clone()), "binder");
        let header = subset(keep);
        let twin = twins
            .iter()
            .fold(header.clone(), |twin, (from, to)| twin.replace(from, to));
        assert_ne!(twin, header, "{function}: the twin changes the header");
        let mut mirror = bind(&twin).unwrap_or_else(|error| panic!("{function} twin: {error}"));
        for record in records {
            let true_record = format!("{record} const=true");
            assert!(mirror.contains(&true_record), "{mirror}");
            mirror = mirror.replace(&true_record, &format!("{record} const=false"));
        }
        let program = "export function main(): void {}\n";
        assert_eq!(
            check(&mirror, program),
            vec![expected],
            "{function}: checker"
        );
    }
}

/// The scene of `subReadRootRenderScene` (§187 rule 11).
const RENDER_SCENE: &[&str] = &[
    "subReadRootRenderScene",
    "RrScMaterial",
    "RrScMesh",
    "RrScEntity",
    "RrScScene",
];

/// The `const` twin of the scene: `RrScEntity.mesh` is `const`, so the
/// build of an element writes back no target.
const RENDER_SCENE_CONST: (&str, &str) = ("RrScMesh *mesh;", "const RrScMesh *mesh;");

/// §187 rule 11: the `const` twin of `subReadRootRenderScene` binds and
/// runs in both tiers. The call passes the `const` pair of `RrScEntity` as
/// a scratch array and each mesh as a scratch copy that it does not write
/// back. Each material copies its bytes, so C writes `r` in the script
/// memory. The control has no call.
fn a_const_scene_twin_binds_and_runs() {
    let mirror = bind(&subset(RENDER_SCENE).replace(RENDER_SCENE_CONST.0, RENDER_SCENE_CONST.1))
        .expect("the const scene binds");
    let program = |call: &str| {
        let call = if call.is_empty() { "-1" } else { call };
        format!(
            "export function main(): void {{\n  \
             const entities: RrScEntity[] = [\
             new RrScEntity(1, new RrScMesh(10, new RrScMaterial(1.5))), \
             new RrScEntity(2, new RrScMesh(20, null))];\n  \
             const sum = {call};\n  const mesh = entities[0].mesh;\n  \
             const material = mesh === null ? null : mesh.material;\n  \
             print(`${{sum}} ${{material === null ? -1 : material.r}}`);\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootRenderScene(new RrScScene(0, entities))",
        ["33 2.5", "-1 1.5"],
    );
}

/// §187 rule 11: a `const` pair of struct elements whose build writes back
/// no target is read at every reach. `subReadRootNSum` reads `m` and the
/// first leaf of each `RrNMid` through a `const` pair, which the call
/// passes as a scratch array, and the `const` leaves of each element as
/// script memory. The control has no call.
fn a_const_pair_of_struct_elements_is_read() {
    let mirror = bind(&subset(&[
        "subReadRootNSum",
        "RrNLeaf",
        "RrNMid",
        "RrNView",
    ]))
    .expect("the const pair binds");
    let program = |call: &str| {
        let call = if call.is_empty() { "-1" } else { call };
        format!(
            "export function main(): void {{\n  \
             const mids: RrNMid[] = [new RrNMid(1, [new RrNLeaf(2)]), \
             new RrNMid(3, [new RrNLeaf(4)])];\n  \
             print(`${{{call}}} ${{mids[1].m}}`);\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootNSum(new RrNView(0, mids))",
        ["10 3", "-1 3"],
    );
}

/// §187 rule 14: a link to an embedded header (§33 rule 9) passes its
/// declared type, by the pass of that type, and the call reads no class
/// id: a scratch copy of a header that holds a view (`subReadRootHUse`,
/// `subReadRootLinkedExt`), and script memory for a family that copies its
/// bytes, where a C cast to the extension reads the extension
/// (`subReadRootGUse`, `7701`). A C cast to an extension that does not copy
/// its bytes reads past the scratch copy of the header (187.3 item 13);
/// that value is not stable, so no case asserts it.
fn a_link_to_an_embedded_header_passes_its_declared_type() {
    let extension_user = "int32_t subReadRootHExtLen(const RrHExt *e);\n";
    let mirror = bind(&format!(
        "{}{extension_user}",
        subset(&["subReadRootHUse", "RrHBase", "RrHExt", "RrHHolder"])
    ))
    .expect("the link to a header binds");
    let program = "export function main(): void {\n  \
                   print(`${subReadRootHUse(new RrHHolder(1, new RrHBase(5)))}`);\n}\n";
    assert_eq!(run_both(&mirror, program), ("5\n".into(), "5\n".into()));
    let mirror = bind(&subset(&[
        "subReadRootLinkedExt",
        "subReadRootExtUse",
        "RrBase",
        "RrExt",
        "RrLinked",
    ]))
    .expect("a header that holds a view binds");
    let program = "export function main(): void {\n  \
                   print(`${subReadRootLinkedExt(new RrLinked(1, new RrBase(5, \"abcd\")))}`);\n}\n";
    assert_eq!(
        run_both(&mirror, program),
        ("5004\n".into(), "5004\n".into())
    );
    let mirror = bind(&subset(&[
        "subReadRootGUse",
        "RrGBase",
        "RrGExt",
        "RrGHolder",
    ]))
    .expect("a family that copies its bytes binds");
    let program = |link: &str| {
        format!(
            "export function main(): void {{\n  \
             const e = new RrGExt(new RrGBase(2), 77);\n  \
             print(`${{subReadRootGUse(new RrGHolder(1, {link}))}}`);\n}}\n"
        )
    };
    for (link, expected) in [("e.base", "7701\n"), ("new RrGBase(5)", "5\n")] {
        assert_eq!(
            run_both(&mirror, &program(link)),
            (expected.to_string(), expected.to_string()),
            "{link}"
        );
    }
}

/// §187 rule 14 with headers that a math or a graphics header declares: a
/// `Vec3` that `RrEntity` extends (`subReadRootCross`, in one mirror and
/// across two), a `Vec2` that `RrLabel` extends (`subReadRootMousePos`), a
/// `Matrix` that `RrModel` extends (`subReadRootDrawMeshInstanced`), and a
/// non-`const` link through which C writes a member of the extension
/// (`subReadRootWTouch`, read again by `subReadRootWRead`): each header
/// copies its bytes, so the call passes script memory. Each runs with a
/// plain header box beside the extension's box.
fn a_header_of_a_math_or_graphics_header_binds_and_runs() {
    let mirror = bind(&format!(
        "{}int32_t subReadRootHExtLen(const RrHExt *e);\n",
        subset(&[
            "subReadRootCross",
            "subReadRootMousePos",
            "subReadRootLabelLen",
            "subReadRootDrawMeshInstanced",
            "subReadRootWTouch",
            "subReadRootWRead",
            "RrVec3",
            "RrVMesh",
            "RrEntity",
            "RrVec2",
            "RrLabel",
            "RrMatrix",
            "RrRMesh",
            "RrModel",
            "RrWExt",
            "RrWHolder",
            "RrHBase",
            "RrHExt",
        ])
    ))
    .expect("the math and graphics headers bind");
    let program = "export function main(): void {\n  \
                   const out = new RrVec3(0, 0, 0);\n  \
                   const e = new RrEntity(new RrVec3(1, 0, 0), null);\n  \
                   subReadRootCross(e.pos, new RrVec3(0, 1, 0), out);\n  \
                   const plain = new RrVec3(0, 0, 0);\n  \
                   subReadRootCross(new RrVec3(0, 1, 0), new RrVec3(1, 0, 0), plain);\n  \
                   print(`${out.z} ${plain.z}`);\n  \
                   const v = new RrVec2(0, 0);\n  subReadRootMousePos(v);\n  \
                   const l = new RrLabel(new RrVec2(1, 1), \"abcd\");\n  \
                   subReadRootMousePos(l.pos);\n  \
                   print(`${v.x} ${v.y} ${subReadRootLabelLen(l)}`);\n  \
                   const model = new RrModel(new RrMatrix(0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0, \
                   0, 0, 0, 0), 0, null);\n  \
                   const matrix = new RrMatrix(0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);\n  \
                   print(`${subReadRootDrawMeshInstanced(new RrRMesh(3), model.transform, 1)} \
                   ${subReadRootDrawMeshInstanced(new RrRMesh(3), matrix, 1)}`);\n  \
                   const w = new RrWHolder(2, new RrWExt(new RrVec3(1, 2, 3), 77, null).pos);\n  \
                   const r = new RrWHolder(2, new RrWExt(new RrVec3(1, 2, 3), 77, null).pos);\n  \
                   subReadRootWTouch(w);\n  \
                   print(`${subReadRootWRead(w)} ${subReadRootWRead(r)}`);\n}\n";
    let expected = "1 -1\n3 4 4\n371 351\n1077 77\n".to_string();
    assert_eq!(run_both(&mirror, program), (expected.clone(), expected));
    // Across two mirrors: `RrEntity` extends the `RrVec3` of another header.
    // C AOT includes both headers from their own directory.
    let line = |name: &str| {
        READ_ROOT_HEADER
            .lines()
            .find(|line| line.ends_with(&format!(" {name};")))
            .unwrap_or_else(|| panic!("{name}"))
            .to_string()
    };
    let math_header = format!(
        "#ifndef READ_ROOT_MATH_H\n#define READ_ROOT_MATH_H\n#include <stdint.h>\n{}\n\
         void subReadRootCross(const RrVec3 *a, const RrVec3 *b, RrVec3 *out);\n#endif\n",
        line("RrVec3")
    );
    let entity_header = format!(
        "#include <stdint.h>\n#include \"read-root-math.h\"\n\
         /* @subscript-external RrVec3 */\n{}\n{}\n\
         int32_t subReadRootEntityCount(const RrEntity *e);\n",
        line("RrVMesh"),
        line("RrEntity")
    );
    let directory =
        std::env::temp_dir().join(format!("subscript-s187-entity-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create directory");
    std::fs::write(directory.join("read-root-math.h"), &math_header).expect("write");
    std::fs::write(directory.join("read-root-entity.h"), &entity_header).expect("write");
    let math = generate_for_header(&math_header, "read-root-math.h")
        .unwrap_or_else(|error| panic!("the math header binds: {}", error.0));
    let entity = generate_for_header(
        &entity_header.replace(
            "#include \"read-root-math.h\"",
            &format!(
                "#include \"{}\"",
                directory.join("read-root-math.h").display()
            ),
        ),
        "read-root-entity.h",
    )
    .unwrap_or_else(|error| panic!("the entity header binds: {}", error.0));
    let files = vec![
        SourceFile::ambient("read-root-math.generated.d.ts", math),
        SourceFile::ambient("read-root-entity.generated.d.ts", entity),
        SourceFile::new(
            "main.ts",
            "export function main(): void {\n  \
             const out = new RrVec3(0, 0, 0);\n  \
             const e = new RrEntity(new RrVec3(1, 0, 0), null);\n  \
             subReadRootCross(e.pos, new RrVec3(0, 1, 0), out);\n  \
             print(`${out.z}`);\n}\n"
                .to_string(),
        ),
    ];
    let libraries = [library_with(std::slice::from_ref(&directory))];
    let jit = run_jit_with_native_libraries(&files, &libraries).expect("dev JIT");
    let aot = run_c_aot_with_native_libraries(&files, &libraries).expect("C AOT");
    let _ = std::fs::remove_dir_all(&directory);
    assert_eq!(
        (
            String::from_utf8_lossy(&jit).into_owned(),
            String::from_utf8_lossy(&aot).into_owned()
        ),
        ("1\n".to_string(), "1\n".to_string())
    );
}

/// §187 rule 8: the copy-back of a scratch struct skips a userdata slot and
/// a callback field, so a fill of `RrNamed` is accepted and the script keeps
/// its slot; `subReadRootPSubmit` writes a scalar and the userdata of a
/// callback field, and only the scalar reaches the script. In a struct whose
/// bytes the call passes, the script reads C's bytes, so the slot has no
/// read lowering: embedded by value in a scratch struct (`RrNamedUd`), at a
/// root that copies its bytes (`RrUd`), and behind a pointer member
/// (`subReadRootPStep`).
fn a_userdata_slot_is_read_only_where_the_copy_back_skips_it() {
    let tail = "a userdata field that C can write";
    assert_rejected(
        &["subReadRootNamedUdFill", "RrNamedUd"],
        (
            "subReadRootNamedUdFill(RrNamedUd *out)",
            "subReadRootNamedUdFill(const RrNamedUd *out)",
            "// @subscript-c-parameter function=\"subReadRootNamedUdFill\" parameter=\"out\"",
        ),
        &format!(
            "foreign function `subReadRootNamedUdFill` parameter `out` reads `RrUd.ud`, {tail}"
        ),
    );
    assert_rejected(
        &["subReadRootUdFill"],
        (
            "subReadRootUdFill(RrUd *out)",
            "subReadRootUdFill(const RrUd *out)",
            "// @subscript-c-parameter function=\"subReadRootUdFill\" parameter=\"out\"",
        ),
        &format!("foreign function `subReadRootUdFill` parameter `out` reads `RrUd.ud`, {tail}"),
    );
    assert_eq!(
        bind(&subset(&["subReadRootPStep", "RrPBody", "RrPWorld"])),
        Err(format!(
            "foreign function `subReadRootPStep` parameter `w` reads `RrPBody.user`, {tail} \
             {TAIL}"
        ))
    );
    let mirror = bind(&subset(&["subReadRootNamedFill", "RrNamed"])).expect("the fill binds");
    let program = |call: &str| {
        format!(
            "export function main(): void {{\n  \
             const named = new RrNamed(\"x\", null);\n  {call}\n  \
             print(`${{named.name}} ${{named.ud === null}}`);\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootNamedFill(named);",
        ["aa true", "x true"],
    );
    let mirror =
        bind(&subset(&["subReadRootPSubmit", "RrJobCallback", "RrPJob"])).expect("the job binds");
    let program = |call: &str| {
        format!(
            "export function main(): void {{\n  \
             const job = new RrPJob((m, a, b) => {{}}, null, null, 1);\n  {call}\n  \
             print(`${{job.prio}} ${{job.userdata === null}}`);\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootPSubmit(job);",
        ["9 true", "1 true"],
    );
    let mirror = bind(&subset(&[
        "subReadRootDeviceNamedFill",
        "RrDeviceNamed",
        "subReadRootDevice",
        "subReadRootDeviceId",
    ]))
    .expect("control binds");
    let program = "export function main(): void {\n  \
                   const named = new RrDeviceNamed(\"x\", subReadRootDevice(7));\n  \
                   subReadRootDeviceNamedFill(named);\n  \
                   print(`${named.name} ${subReadRootDeviceId(named.device)}`);\n}\n";
    assert_eq!(
        run_both(&mirror, program),
        ("bb 42\n".into(), "bb 42\n".into())
    );
}

/// `subReadRootCmdRun(const RrCmd *cmd)` writes `cmd->counter->n`. The call
/// passes `RrCmd` as a scratch struct and its `RrCounter` target as the
/// script memory, so the write reaches the script. The control is the
/// same program with no call: the counter keeps its value.
fn a_target_passed_as_script_memory_is_read() {
    let mirror = bind(&subset(&["subReadRootCmdRun", "RrCmd"])).expect("the input binds");
    let program = |call: &str| {
        format!(
            "export function main(): void {{\n  \
             const counter = new RrCounter(1);\n  \
             const cmd = new RrCmd(\"abcd\", counter);\n  {call}\n  \
             const k = cmd.counter;\n  \
             print(`${{k === null ? -1 : k.n}}`);\n}}\n"
        )
    };
    assert_eq!(
        run_both(&mirror, &program("subReadRootCmdRun(cmd);")),
        ("5\n".into(), "5\n".into())
    );
    assert_eq!(
        run_both(&mirror, &program("")),
        ("1\n".into(), "1\n".into())
    );
}

/// The binder, the checked classes, and code generation give the same pass
/// for each struct and each pointer member, from one function. The header
/// covers each member shape.
fn the_binder_and_code_generation_take_one_pass_decision() {
    let header = format!(
        "{}\
         typedef struct RrChain {{ int32_t kind; struct RrChain *next; }} RrChain;\n\
         typedef struct RrChainExt {{ RrChain chain; int32_t extra; }} RrChainExt;\n\
         typedef struct RrList {{ size_t pointsCount; const RrPoint *points; }} RrList;\n\
         typedef struct RrNames {{ size_t namedCount; const RrDeviceNamed *named; }} RrNames;\n\
         typedef struct RrWrap {{ int32_t k; RrPoint point; }} RrWrap;\n\
         typedef struct RrWrapView {{ int32_t k; RrDeviceNamed named; }} RrWrapView;\n\
         typedef struct RrScratchLink {{ int32_t k; const RrCmd *cmd; }} RrScratchLink;\n\
         int32_t subReadRootChain(const RrChainExt *ext, const RrChain *chain);\n\
         int32_t subReadRootUse(const RrList *list, const RrNames *names, const RrWrap *wrap, \
         const RrWrapView *view, const RrScratchLink *link, RrOuterConst outer);\n",
        subset(&[
            "subReadRootHolderSum",
            "subReadRootCmdRun",
            "RrCmd",
            "subReadRootDeviceNamedFill",
            "RrDeviceNamed",
        ])
    );
    let binder = boundary_passes(&header).expect("the header parses");
    let mirror = bind(&header).expect("the header binds");
    let checked = check_program(&files(&mirror, "export function main(): void {}\n"))
        .expect("the mirror checks");
    let lowered = subscript_codegen::lir::lower_module(&checked).expect("the module lowers");
    let checked_view = Classes(&checked);
    let lowered_view = Classes(&lowered);
    let mut pointers = 0;
    for expected in &binder {
        let class = lowered
            .classes
            .iter()
            .find(|class| class.source_name == expected.name)
            .unwrap_or_else(|| panic!("`{}` lowers to a class", expected.name));
        let lowered_pass = if class.copies_boundary_bytes {
            StructPass::Bytes
        } else {
            StructPass::Scratch
        };
        assert_eq!(
            lowered_pass, expected.pass,
            "{} in code generation",
            expected.name
        );
        assert_eq!(
            struct_pass(&checked_view, class.id),
            expected.pass,
            "{} in the checked classes",
            expected.name
        );
        let fields = lowered_view.fields(class.id).expect("a boundary class");
        let lowered_pointers: Vec<_> = class
            .fields
            .iter()
            .zip(&fields)
            .filter_map(|(field, shape)| match shape {
                FieldShape::Pointer { target, .. } => Some((
                    field.source_name.clone(),
                    member_pass(
                        &lowered_view,
                        lowered_pass,
                        *target,
                        field.foreign_provenance != Some(ForeignTypeProvenance::ConstPointer),
                    ),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(lowered_pointers, expected.pointers, "{}", expected.name);
        pointers += lowered_pointers.len();
    }
    // Each pass and each pointer pass occurs, so no comparison is vacuous.
    for pass in [StructPass::Bytes, StructPass::Scratch] {
        assert!(binder.iter().any(|entry| entry.pass == pass), "{pass:?}");
    }
    let pointer_passes: Vec<_> = binder
        .iter()
        .flat_map(|entry| entry.pointers.iter().map(|(_, pass)| *pass))
        .collect();
    for pass in [
        subscript_boundary::PointerPass::ScriptMemory,
        subscript_boundary::PointerPass::ScratchWrittenBack,
        subscript_boundary::PointerPass::ScratchReadOnly,
    ] {
        assert!(pointer_passes.contains(&pass), "{pass:?}");
    }
    assert!(pointers >= 4, "{pointers}");
}

/// §187 rule 7: the copy-back writes a member back only when C changed it.
/// `subReadRootJobRun` calls the callback, which sets `count.n` to 99 in
/// the script, and C leaves the member: the script value stays. The
/// control `subReadRootJobSet` calls the same callback and then writes
/// `count->n = 5`: C's value is written back.
fn a_script_write_during_the_call_stays_when_c_leaves_the_member() {
    let mirror = bind(&subset(&[
        "subReadRootJobRun",
        "subReadRootJobSet",
        "RrJobCallback",
        "RrCount",
        "RrJob",
    ]))
    .expect("the job binds");
    let program = "let job: RrJob | null = null;\n\
                   function count(): string {\n  const j = job;\n  \
                   if (j === null) { return \"none\"; }\n  const c = j.count;\n  \
                   return c === null ? \"null\" : `${c.n}`;\n}\n\
                   function make(): RrJob {\n  return new RrJob((m, a, b) => {\n    \
                   const j = job;\n    if (j !== null) {\n      const c = j.count;\n      \
                   if (c !== null) { c.n = 99; }\n    }\n  }, null, null, new RrCount(1, null));\n}\n\
                   export function main(): void {\n  const run = make();\n  job = run;\n  \
                   subReadRootJobRun(run);\n  print(`run ${count()}`);\n  const set = make();\n  \
                   job = set;\n  subReadRootJobSet(set);\n  print(`set ${count()}`);\n}\n";
    assert_eq!(
        run_both(&mirror, program),
        ("run 99\nset 5\n".into(), "run 99\nset 5\n".into())
    );
}

/// §187 rule 7: a target behind a `const` pointer is not written back.
/// `subReadRootRenderConst` writes `id` through the `const` pointer and
/// adds 1 to `mesh->count` through the non-`const` member: the script keeps
/// `id` and reads the count. The control has no call.
fn a_target_behind_a_const_pointer_is_not_written_back() {
    let mirror = bind(&subset(&["subReadRootRenderConst"])).expect("the scene binds");
    let program = |call: &str| {
        let result = if call.is_empty() {
            "scene.id".to_string()
        } else {
            call.to_string()
        };
        format!(
            "export function main(): void {{\n  \
             const scene = new RrScene(7, new RrMesh(3, new RrMaterial(1.5)));\n  \
             const r = {result};\n  const mesh = scene.mesh;\n  \
             print(`${{r}} ${{scene.id}} ${{mesh === null ? -1 : mesh.count}}`);\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootRenderConst(scene)",
        ["50 7 4", "7 7 3"],
    );
}

/// §187 rule 9: a struct that reaches itself through pointer members, by a
/// self link (`RrNode`) or a mutual pair (`RrCycleA`, `RrCycleB`), cannot
/// pass as scratch copies. The binder rejects each position; the checker
/// rejects each position of the mirror that declares them, a by-value one
/// included, with the same text. The control passes a struct whose target
/// copies its bytes.
fn a_struct_cycle_is_rejected_at_each_position() {
    let tail = "a member through which a struct reaches itself; a call cannot build a \
                struct cycle as scratch copies (compiler.md §187)";
    let node = format!(
        "foreign function `subReadRootNodeBump` parameter `node` passes `RrNode.next`, {tail}"
    );
    let cycle =
        format!("foreign function `subReadRootCycleUse` parameter `a` passes `RrCycleB.a`, {tail}");
    let by_value = format!(
        "foreign function `subReadRootNodeByValue` parameter `node` passes `RrNode.next`, \
         {tail}"
    );
    assert_eq!(
        bind(&subset(&["subReadRootNodeBump", "RrNode"])),
        Err(node.clone())
    );
    assert_eq!(
        bind(&subset(&["subReadRootCycleUse", "RrCycleA", "RrCycleB"])),
        Err(cycle.clone())
    );
    let structs = bind(&subset(&[
        "subReadRootHolderSum",
        "RrNode",
        "RrCycleA",
        "RrCycleB",
    ]))
    .expect("the structs bind with no position");
    let mirror = format!(
        "{structs}\
         // @subscript-c-parameter function=\"subReadRootNodeBump\" parameter=\"node\" const=false\n\
         // @subscript-c-parameter function=\"subReadRootCycleUse\" parameter=\"a\" const=true\n\
         declare function subReadRootNodeBump(node: RrNode | null): void;\n\
         declare function subReadRootCycleUse(a: RrCycleA | null): i32;\n\
         declare function subReadRootNodeByValue(node: RrNode): void;\n"
    );
    let program = "export function main(): void {}\n";
    assert_eq!(check(&mirror, program), vec![node, cycle, by_value]);
    let control = "export function main(): void {\n  \
                   print(`${subReadRootHolderSum(new RrHolder(2, new RrPoint(3)))}`);\n}\n";
    assert_eq!(run_both(&structs, control), ("5\n".into(), "5\n".into()));
}

/// §187 rule 10: a by-value struct whose bytes the call copies passes its
/// bytes, a fixed array of scalars included. The control passes the same
/// struct through a `const` pointer, as the script memory.
fn a_struct_whose_bytes_the_call_copies_passes_them_by_value() {
    let mirror = bind(&subset(&["subReadRootFtSum", "subReadRootFtSumP", "RrFt"]))
        .expect("the fixed-array struct binds");
    for call in ["subReadRootFtSum(t)", "subReadRootFtSumP(t)"] {
        let program = format!(
            "export function main(): void {{\n  \
             const m: FixedArray<f32, 4> = [1, 2, 3, 4];\n  const t = new RrFt(m, 5);\n  \
             print(`${{{call}}}`);\n}}\n"
        );
        assert_eq!(
            run_both(&mirror, &program),
            ("10\n".into(), "10\n".into()),
            "{call}"
        );
    }
}

/// §187 rule 11: a mutable scalar pair behind a non-`const` pointer member
/// of a `const` root keeps its script elements, and C writes them in place.
/// The control has no call.
fn a_scalar_pair_behind_a_pointer_is_written_in_place() {
    let mirror = bind(&subset(&["subReadRootSceneDeform", "RrDMesh", "RrDScene"]))
        .expect("the deform scene binds");
    let program = |call: &str| {
        format!(
            "export function main(): void {{\n  \
             const scene = new RrDScene(1, new RrDMesh(0, [1.5, 2.5]));\n  {call}\n  \
             const mesh = scene.mesh;\n  \
             if (mesh !== null) {{ print(`${{mesh.verts[0]}} ${{mesh.verts[1]}}`); }}\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "subReadRootSceneDeform(scene);",
        ["3 5", "1.5 2.5"],
    );
}

/// §187 rule 12: a result lowers by the read facts of its members, so a
/// result with a fixed array and a pointer member reads in both tiers. The
/// control is a result with the fixed array only.
fn a_result_reads_by_its_read_facts() {
    let mirror = bind(&subset(&[
        "subReadRootFrGet",
        "subReadRootFbGet",
        "RrFp",
        "RrFr",
        "RrFb",
    ]))
    .expect("the results bind");
    let program = "export function main(): void {\n  \
                   const b = subReadRootFbGet();\n  print(`${b.m[2]}`);\n  \
                   const r = subReadRootFrGet();\n  const p = r.p;\n  \
                   print(`${r.m[2]} ${p === null ? -1 : p.x}`);\n}\n";
    assert_eq!(
        run_both(&mirror, program),
        ("3\n3 42\n".into(), "3\n3 42\n".into())
    );
}

/// §187 rule 13: a scalar fixed array in a struct that the call builds as a
/// scratch struct copies as bytes in the build and in the copy-back.
/// `subReadRootBoundsRender` reads `bounds` of a scratch `RrBMesh` behind a
/// `const` root and writes `bounds[1]`; `RrTr` crosses through a `const`
/// pointer, by value, and as a fill. The control runs with no call.
fn a_scalar_fixed_array_in_a_scratch_struct_copies_as_bytes() {
    let mirror = bind(&subset(&[
        "subReadRootBoundsRender",
        "subReadRootTrSum",
        "subReadRootTrVal",
        "subReadRootTrFill",
        "RrBMesh",
        "RrBScene",
        "RrFp",
        "RrTr",
    ]))
    .expect("the fixed-array structs bind");
    let program = |call: &str| {
        let (render, sum, value, fill) = if call.is_empty() {
            ("-1", "-1", "-1", "")
        } else {
            (
                "subReadRootBoundsRender(scene)",
                "subReadRootTrSum(t)",
                "subReadRootTrVal(t)",
                "subReadRootTrFill(t);",
            )
        };
        format!(
            "export function main(): void {{\n  \
             const bounds: FixedArray<f32, 4> = [1, 2, 3, 4];\n  \
             const scene = new RrBScene(7, new RrBMesh(bounds, new RrMaterial(1.5)));\n  \
             const r = {render};\n  const mesh = scene.mesh;\n  \
             if (mesh !== null) {{\n    const mat = mesh.mat;\n    \
             print(`${{r}} ${{mesh.bounds[1]}} ${{mat === null ? -1 : mat.r}}`);\n  }}\n  \
             const m: FixedArray<f32, 4> = [1, 2, 3, 4];\n  const t = new RrTr(m, new RrFp(5));\n  \
             print(`${{{sum}}} ${{{value}}}`);\n  {fill}\n  print(`${{t.m[2]}}`);\n}}\n"
        )
    };
    assert_runs(
        &mirror,
        program,
        "call",
        ["5 20 3\n10 10\n30", "-1 2 1.5\n-1 -1\n3"],
    );
}

/// The `CEnum` alias of the fixture (§52).
fn mode_alias() -> SourceFile {
    SourceFile::ambient(
        "read-root-mode.d.ts",
        "type RrMode = CEnum<{\n  \"a\": 1;\n  \"b\": 2;\n}>;\n".to_string(),
    )
}

/// §187 rule 11: C writes the elements of a writable pair of `CEnum`
/// elements in place, with no validation, so the pair has no read lowering
/// at a fill and at a by-value input. The binder and the checker reject
/// each position with one text. The `const` twin `RrRecC` is accepted and
/// runs in both tiers.
fn a_writable_pair_of_validated_elements_is_not_read() {
    let tail = "a pair of validated elements that C can write";
    let message = |function: &str, parameter: &str| {
        format!(
            "foreign function `{function}` parameter `{parameter}` reads `RrRec.modes`, {tail} \
             {TAIL}"
        )
    };
    assert_eq!(
        bind(&subset(&["subReadRootRecFill", "RrModeC", "RrRec"])),
        Err(message("subReadRootRecFill", "r"))
    );
    assert_eq!(
        bind(&subset(&["subReadRootRecTouch", "RrModeC", "RrRec"])),
        Err(message("subReadRootRecTouch", "r"))
    );
    let mirror = rejected_mirror(
        &[
            "subReadRootRecFill",
            "subReadRootRecTouch",
            "RrModeC",
            "RrRec",
        ],
        "size_t modesCount; RrModeC *modes; } RrRec;",
        "size_t modesCount; const RrModeC *modes; } RrRec;",
        "// @subscript-c-member aggregate=\"RrRec\" member=\"modes\"",
    );
    let files = [
        SourceFile::ambient("read-root.generated.d.ts", mirror),
        mode_alias(),
        SourceFile::new("main.ts", "export function main(): void {}\n"),
    ];
    let diagnostics: Vec<String> = check_program(&files)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert_eq!(
        diagnostics,
        vec![
            message("subReadRootRecFill", "r"),
            message("subReadRootRecTouch", "r")
        ]
    );
    let twin = bind(&subset(&["subReadRootRecCGet", "RrModeC", "RrRecC"])).expect("the twin binds");
    let program = "export function main(): void {\n  \
                   const r = new RrRecC(0, \"a\", [\"a\", \"b\"]);\n  \
                   print(`${subReadRootRecCGet(r)} ${r.modes[1]}`);\n}\n";
    let files = vec![
        SourceFile::ambient("read-root.generated.d.ts", twin),
        mode_alias(),
        SourceFile::new("main.ts", program.to_string()),
    ];
    let libraries = [library()];
    let jit = run_jit_with_native_libraries(&files, &libraries).expect("dev JIT");
    let aot = run_c_aot_with_native_libraries(&files, &libraries).expect("C AOT");
    assert_eq!(
        (
            String::from_utf8_lossy(&jit).into_owned(),
            String::from_utf8_lossy(&aot).into_owned()
        ),
        ("2 b\n".to_string(), "2 b\n".to_string())
    );
}

/// §187 rule 15: the binder's write validation does not reject a member
/// that the pass decision lowers: a scalar fixed array beside a pair
/// (`RrPMesh`, through a `const` pointer and by value) and beside a string
/// view (`RrPNamed`).
fn a_scalar_fixed_array_beside_a_pair_or_a_view_binds() {
    let mirror = bind(&subset(&[
        "subReadRootPDraw",
        "subReadRootPDrawV",
        "subReadRootPNamedUse",
        "RrPMesh",
        "RrPNamed",
    ]))
    .expect("the structs bind");
    let program = "export function main(): void {\n  \
                   const b: FixedArray<f32, 4> = [1, 2, 3, 4];\n  \
                   const m = new RrPMesh(b, [7, 8]);\n  \
                   const n = new RrPNamed(\"abc\", b);\n  \
                   print(`${subReadRootPDraw(m)} ${subReadRootPDrawV(m)} \
                   ${subReadRootPNamedUse(n)}`);\n}\n";
    assert_eq!(
        run_both(&mirror, program),
        ("528 528 304\n".into(), "528 528 304\n".into())
    );
}

/// §187 rule 9: a cycle of structs that copy their bytes, through `const`
/// pointers only, passes as script memory; `subReadRootCSum` sums the list.
/// The control is a list of one node.
fn a_const_cycle_of_byte_structs_passes_as_script_memory() {
    let mirror = bind(&subset(&["subReadRootCSum", "RrCNode"])).expect("the list binds");
    let program = |head: &str| {
        format!(
            "export function main(): void {{\n  \
             const n3 = new RrCNode(3, null);\n  const n2 = new RrCNode(2, n3);\n  \
             const n1 = new RrCNode(1, n2);\n  print(`${{subReadRootCSum({head})}}`);\n}}\n"
        )
    };
    for (head, expected) in [("n1", "6\n"), ("n3", "3\n")] {
        assert_eq!(
            run_both(&mirror, &program(head)),
            (expected.to_string(), expected.to_string()),
            "{head}"
        );
    }
}

/// §187 rule 14: a script reference can name C memory, which has no box
/// header. `subReadRootIvAt` returns a pointer into a static C array; the
/// bytes before `grid[1]` are `grid[0]`, which the loop sets to every small
/// class id. The link passes as `RrIV2`, which copies its bytes, and the
/// call reads no class id, so `subReadRootIvSum` reads `707` every time and
/// `subReadRootIvBump` writes C memory. The control passes script boxes of
/// the header and of the extension `RrTile`.
fn c_memory_passes_as_it_is() {
    let mirror = bind(&subset(&[
        "subReadRootIvAt",
        "subReadRootIvSet",
        "subReadRootIvSum",
        "subReadRootIvBump",
        "subReadRootLabLen",
        "RrIV2",
        "RrSpr",
        "RrLab",
        "RrTile",
    ]))
    .expect("the grid binds");
    let program = "export function main(): void {\n  \
                   for (let k: i32 = 0; k < 64; k++) {\n    \
                   subReadRootIvSet(0, k, 0);\n    subReadRootIvSet(1, 7, 7);\n    \
                   subReadRootIvSet(2, 4660, 22136);\n    \
                   const sum = subReadRootIvSum(subReadRootIvAt(1));\n    \
                   if (sum !== 707) { print(`${k} ${sum}`); }\n  }\n  \
                   subReadRootIvBump(subReadRootIvAt(1));\n  \
                   const b: RrIV2 | null = new RrIV2(7, 7);\n  subReadRootIvBump(b);\n  \
                   const t = new RrTile(new RrIV2(1, 2), new RrIV2(3, 4), null);\n  \
                   print(`${subReadRootIvSum(subReadRootIvAt(1))} ${subReadRootIvSum(b)} \
                   ${subReadRootIvSum(t.pos)}`);\n}\n";
    assert_eq!(
        run_both(&mirror, program),
        ("807 807 102\n".into(), "807 807 102\n".into())
    );
}

/// §187 rule 14: a family member that is a rule 9 cycle
/// (`RrNodeV { RrVec3 pos; struct RrNodeV *parent; }`) does not reject the
/// header: `subReadRootCross` binds and checks, in one mirror and across
/// two. The link passes as `RrVec3`, so the `pos` of an `RrNodeV` box runs
/// as a plain `RrVec3` box does.
fn a_cycle_class_in_a_family_passes_as_the_header() {
    let mirror = bind(&subset(&["subReadRootCross", "RrVec3", "RrNodeV"]))
        .expect("the header with a cycle extension binds");
    let program = |a: &str| {
        format!(
            "export function main(): void {{\n  \
             const b: RrVec3 | null = new RrVec3(0, 1, 0);\n  \
             const out: RrVec3 | null = new RrVec3(0, 0, 0);\n  \
             subReadRootCross({a}, b, out);\n  \
             print(`${{out === null ? -1 : out.z}}`);\n}}\n"
        )
    };
    let plain: &str = "new RrVec3(1, 0, 0)";
    for a in [plain, "new RrNodeV(new RrVec3(1, 0, 0), null).pos"] {
        assert_eq!(
            run_both(&mirror, &program(a)),
            ("1\n".into(), "1\n".into()),
            "{a}"
        );
    }
    // Across two mirrors: the cycle extension declares `RrVec3` external.
    let line = |name: &str| {
        READ_ROOT_HEADER
            .lines()
            .find(|line| line.ends_with(&format!(" {name};")))
            .unwrap_or_else(|| panic!("{name}"))
            .to_string()
    };
    let node = generate_for_header(
        &format!(
            "#include <stdint.h>\n/* @subscript-external RrVec3 */\n{}\n\
             int32_t subReadRootNodeDepth(const RrNodeV *n);\n",
            line("RrNodeV")
        ),
        "read-root-node.h",
    );
    assert!(node.is_err(), "the node header alone passes its cycle");
    let math = bind(&subset(&["subReadRootCross", "RrVec3"])).expect("the math header binds");
    let node = generate_for_header(
        &format!(
            "#include <stdint.h>\n/* @subscript-external RrVec3 */\n{}\n",
            line("RrNodeV")
        ),
        "read-root-node.h",
    )
    .unwrap_or_else(|error| panic!("the node header binds: {}", error.0));
    let checked = check_program(&[
        SourceFile::ambient("read-root.generated.d.ts", math),
        SourceFile::ambient("read-root-node.generated.d.ts", node),
        SourceFile::new("main.ts", program(plain)),
    ]);
    assert!(checked.is_ok(), "{:?}", checked.err());
}

// compiler.md §190.1 rule 3: phase 1 runs in parallel; each phase 2 test
// starts a dev run with a native library or a file provider and runs in
// its own process, on that process's main thread.
pub(super) fn main() -> std::process::ExitCode {
    crate::main_thread::run(&crate::main_thread_tests![
        parallel: [
            a_pair_whose_elements_are_written_back_is_rejected,
            the_binder_and_code_generation_take_one_pass_decision,
        ],
        main_thread: [
            a_fill_keeps_the_script_link_of_a_pointer_member,
            a_pointer_target_is_written_back,
            a_scene_graph_is_written_back_at_every_depth,
            a_const_scene_twin_binds_and_runs,
            a_const_pair_of_struct_elements_is_read,
            a_link_to_an_embedded_header_passes_its_declared_type,
            a_header_of_a_math_or_graphics_header_binds_and_runs,
            a_userdata_slot_is_read_only_where_the_copy_back_skips_it,
            a_target_passed_as_script_memory_is_read,
            a_script_write_during_the_call_stays_when_c_leaves_the_member,
            a_target_behind_a_const_pointer_is_not_written_back,
            a_struct_cycle_is_rejected_at_each_position,
            a_struct_whose_bytes_the_call_copies_passes_them_by_value,
            a_scalar_pair_behind_a_pointer_is_written_in_place,
            a_result_reads_by_its_read_facts,
            a_scalar_fixed_array_in_a_scratch_struct_copies_as_bytes,
            a_writable_pair_of_validated_elements_is_not_read,
            a_scalar_fixed_array_beside_a_pair_or_a_view_binds,
            a_const_cycle_of_byte_structs_passes_as_script_memory,
            c_memory_passes_as_it_is,
            a_cycle_class_in_a_family_passes_as_the_header,
        ],
    ])
}
