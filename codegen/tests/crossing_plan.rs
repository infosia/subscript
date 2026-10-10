//! `specs/blocks/compiler.md` §189 rules 2 to 5 on a lowered module: the
//! plan of each node, one plan per callee, and the order of the report.
//! The test of acceptance 2 compares the plan with the emitted C
//! (`cli/tests/boundary.rs`).
//!
//! Cost: each test checks and lowers one small program; under 0.02 s each
//! on Apple arm64, debug.

use std::sync::Arc;

use subscript_compiler::crossing::{call_sites, ParameterPlan};
use subscript_compiler::lir;

/// A mirror with a written-back chain, an embedded struct, a pair of
/// scratch elements, and a by-value struct.
const FILL_MIRROR: &str = "// @subscript-c-header include=\"fill.h\"\n\
// @subscript-c-member aggregate=\"FInner\" member=\"q\" const=false\n\
// @subscript-c-member aggregate=\"FOuter\" member=\"inner\" const=false\n\
// @subscript-c-member aggregate=\"FItem\" member=\"box\" const=true\n\
// @subscript-c-member aggregate=\"FList\" member=\"items\" const=true\n\
// @subscript-c-parameter function=\"fOuterFill\" parameter=\"out\" const=false\n\
// @subscript-c-parameter function=\"fBoxSet\" parameter=\"box\" const=false\n\
// @subscript-c-parameter function=\"fList\" parameter=\"list\" const=true\n\
declare class FQ { y: i32; constructor(y: i32); }\n\
declare class FInner { x: i32; q: FQ | null; constructor(x: i32, q: FQ | null); }\n\
declare class FOuter { k: i32; inner: FInner | null; constructor(k: i32, inner: FInner | null); }\n\
declare class FVec { x: f32; y: f32; constructor(x: f32, y: f32); }\n\
declare class FBox { v: FVec; name: string; constructor(v: FVec, name: string); }\n\
declare class FItem { n: i32; box: FBox | null; constructor(n: i32, box: FBox | null); }\n\
declare class FList { items: FItem[]; constructor(items: FItem[]); }\n\
declare function fOuterFill(out: FOuter | null): void;\n\
declare function fBoxSet(box: FBox | null): void;\n\
declare function fList(list: FList | null): i32;\n\
declare function fAdd(a: FVec, b: FVec): FVec;\n";

const FILL_PROGRAM: &str = "function twice<T>(a: T): void {\n\
  print(`${fAdd(new FVec(1, 2), new FVec(3, 4)).x}`);\n\
}\n\
export function main(): void {\n\
  const outer = new FOuter(1, new FInner(2, new FQ(3)));\n\
  fOuterFill(outer);\n\
  fBoxSet(new FBox(new FVec(1, 2), \"box\"));\n\
  const items: FItem[] = [new FItem(1, new FBox(new FVec(1, 2), \"a\"))];\n\
  print(`${fList(new FList(items))}`);\n\
  twice<i32>(1);\n\
  twice<f64>(1);\n\
}\n";

fn fill_module() -> Result<lir::Module, String> {
    let files = [
        subscript_compiler::SourceFile::ambient("fill.d.ts", FILL_MIRROR),
        subscript_compiler::SourceFile::entry("main.ts", FILL_PROGRAM),
    ];
    let hir = subscript_compiler::check_program(&files).map_err(|d| format!("{d:?}"))?;
    subscript_codegen::lir::lower_module(&hir).map_err(|e| e.to_string())
}

fn callee(module: &lir::Module, name: &str) -> Result<lir::ForeignFunctionId, String> {
    module
        .foreign_functions
        .iter()
        .find(|function| function.source_name == name)
        .map(|function| function.id)
        .ok_or_else(|| format!("no foreign function `{name}`"))
}

/// Rule 2: each node carries its pass, the members that make a struct a
/// scratch struct, and the members that the call writes back; the result
/// carries its read facts (rule 4).
#[test]
fn the_plan_gives_each_node_its_pass_its_causes_and_its_write_back() -> Result<(), String> {
    use subscript_boundary::{PointerPass, StructPass};
    use subscript_compiler::crossing::{call_plan, MemberCrossing, ResultPlan, WriteBack};
    let module = fill_module()?;

    let fill = call_plan(&module, callee(&module, "fOuterFill")?)?;
    assert!(fill.scratch_scope, "the nested target allocates");
    let ParameterPlan::Pointer(out) = &fill.parameters[0] else {
        return Err(format!("{:?}", fill.parameters[0]));
    };
    assert_eq!(out.pass, PointerPass::ScratchWrittenBack);
    let target = out.target.as_ref().ok_or("a scratch target")?;
    assert_eq!(
        (target.pass, target.causes.clone()),
        (StructPass::Scratch, vec![1])
    );
    let write_back = out.write_back.as_ref().ok_or("a write-back")?;
    assert_eq!(write_back.members.len(), 1);
    assert_eq!(
        (write_back.members[0].field, &write_back.members[0].write),
        (0, &WriteBack::Bytes)
    );
    let MemberCrossing::Pointer(inner) = &target.members[1].crossing else {
        return Err(format!("{:?}", target.members[1]));
    };
    assert_eq!(inner.pass, PointerPass::ScratchWrittenBack);
    let MemberCrossing::Pointer(q) = &inner.target.as_ref().ok_or("inner")?.members[1].crossing
    else {
        return Err("q".into());
    };
    assert_eq!(
        (q.pass, q.target.is_none()),
        (PointerPass::ScriptMemory, true)
    );
    assert_eq!(fill.result, ResultPlan::Void);

    let boxed = call_plan(&module, callee(&module, "fBoxSet")?)?;
    assert!(!boxed.scratch_scope, "the target is a local of the call");
    let ParameterPlan::Pointer(pointer) = &boxed.parameters[0] else {
        return Err(format!("{:?}", boxed.parameters[0]));
    };
    let writes = pointer
        .write_back
        .as_ref()
        .ok_or("a write-back")?
        .members
        .iter()
        .map(|member| member.write.clone())
        .collect::<Vec<_>>();
    assert!(matches!(
        writes.as_slice(),
        [WriteBack::EmbeddedBytes(_), WriteBack::StringView]
    ));

    let list = call_plan(&module, callee(&module, "fList")?)?;
    assert!(list.scratch_scope, "the scratch array allocates");
    let ParameterPlan::Pointer(pointer) = &list.parameters[0] else {
        return Err(format!("{:?}", list.parameters[0]));
    };
    assert_eq!(pointer.pass, PointerPass::ScratchReadOnly);
    assert!(pointer.write_back.is_none());
    let MemberCrossing::Pair(pair) = &pointer.target.as_ref().ok_or("list")?.members[0].crossing
    else {
        return Err("items".into());
    };
    let elements = pair.elements.as_ref().ok_or("scratch elements")?;
    assert_eq!(elements.pass, PointerPass::ScratchReadOnly);
    assert_eq!(elements.element.as_ref().ok_or("element")?.causes, vec![1]);
    assert_eq!(list.result, ResultPlan::Value);

    let add = call_plan(&module, callee(&module, "fAdd")?)?;
    assert!(!add.scratch_scope);
    for parameter in &add.parameters {
        let ParameterPlan::ByValue(structure) = parameter else {
            return Err(format!("{parameter:?}"));
        };
        assert_eq!(structure.pass, StructPass::Bytes);
        assert!(structure.members.is_empty() && structure.causes.is_empty());
    }
    let ResultPlan::Struct(read) = &add.result else {
        return Err(format!("{:?}", add.result));
    };
    assert!(read.by_members.is_empty() && read.unreadable.is_none());
    Ok(())
}

/// The plans of a module are built once per callee.
#[test]
fn a_callee_plan_is_built_once() -> Result<(), String> {
    let module = fill_module()?;
    let mut plans = subscript_compiler::crossing::Plans::default();
    let id = callee(&module, "fList")?;
    let first = plans.get(&module, id)?;
    let second = plans.get(&module, id)?;
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(
        *first,
        subscript_compiler::crossing::call_plan(&module, id)?
    );
    Ok(())
}

/// Rules 3 and 5: the lines sort by position, and a generic function gives
/// one line set for each instantiation that the program calls.
#[test]
fn the_report_sorts_by_position_and_repeats_each_instantiation() -> Result<(), String> {
    let module = fill_module()?;
    let sites = call_sites(&module)?;
    assert_eq!(sites.len(), 5);
    let lines = subscript_compiler::crossing::report(&module)?;
    let add = lines
        .iter()
        .filter(|line| line.starts_with("main.ts:2:") && line.contains(" fAdd "))
        .count();
    assert_eq!(add, 6, "two instantiations of three lines");
    let positions = lines
        .iter()
        .map(|line| {
            let position = line.split(' ').next().unwrap_or_default();
            let mut parts = position
                .split(':')
                .skip(1)
                .map(|part| part.parse::<u32>().unwrap_or(u32::MAX));
            (parts.next(), parts.next())
        })
        .collect::<Vec<_>>();
    let mut sorted = positions.clone();
    sorted.sort();
    assert_eq!(positions, sorted);
    let boxed = lines
        .iter()
        .find(|line| line.contains(" fBoxSet box:"))
        .and_then(|line| line.split_once(' '))
        .map(|(_, rest)| rest);
    assert_eq!(
        boxed,
        Some(
            "fBoxSet box: scratch written-back if changed, none for null \
             [FBox.name string view] writes-back v name"
        )
    );
    Ok(())
}
