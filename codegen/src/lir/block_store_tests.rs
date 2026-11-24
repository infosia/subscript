//! Tests of the store fact (`compiler.md` §356 rule 13).

use super::*;
use subscript_compiler::{check_program, SourceFile};

fn lower(source: &str) -> l::Module {
    let hir = check_program(&[SourceFile::new("block_store.ts", source)]).expect("checks");
    super::super::lower_module(&hir).expect("lowers")
}

fn function<'m>(module: &'m l::Module, name: &str) -> &'m l::Function {
    module
        .functions
        .iter()
        .find(|function| function.source_name == name)
        .unwrap_or_else(|| panic!("function {name} is missing"))
}

/// The facts of the instructions of `name` whose kind `select` accepts.
fn facts(
    module: &l::Module,
    name: &str,
    select: fn(&l::InstructionKind) -> bool,
) -> Vec<BlockStore> {
    let function = function(module, name);
    let definitions = definitions(function);
    let facts: Vec<_> = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|instruction| select(&instruction.kind))
        .map(|instruction| instruction_block_store(module, function, &definitions, instruction))
        .collect();
    assert!(!facts.is_empty(), "{name} has no selected instruction");
    facts
}

fn store_address(kind: &l::InstructionKind) -> bool {
    *kind == l::InstructionKind::StoreAddress
}

const SAMPLE: &str = r#"
class Node { next: Node | null = null; value: i32 = 0; }
@ValueType
class Vec2 { x: f64 = 0; y: f64 = 0; }
let shared: Node | null = null;
function link(a: Node, b: Node): void { a.next = b; }
function number(a: Node): void { a.value = 3; }
function element(xs: Node[], b: Node): void { xs[0] = b; }
function global(b: Node): void { shared = b; }
function local(): i32 { let v: Vec2 = new Vec2(); v.x = 1; return v.x as i32; }
function grow(xs: Node[], b: Node): void { xs.push(b); }
function plain(a: i32): i32 { let x: i32 = a; return x + 1; }
function* walk(b: Node): Generator<i32> {
  let n: Node = b;
  let v: Vec2 = new Vec2();
  for (let i: i32 = 0; i < 3; i += 1) { yield 1; n = b; v.x = 2; }
  const m: Node = n;
  const f = (): i32 => m.value;
  yield f() + (v.x as i32);
}
function capture(b: Node): i32 { const c: Node = b; const f = (): i32 => c.value; return f(); }
export function main(): void {
  const a: Node = new Node();
  const b: Node = new Node();
  link(a, b); number(a); element([a], b); global(b); grow([a], b);
  print(`${local()} ${plain(1)} ${capture(b)}`);
  for (const v of walk(b)) { print(`${v}`); }
}
"#;

#[test]
fn a_field_and_an_element_of_a_reference_are_in_a_block() {
    let module = lower(SAMPLE);
    assert!(facts(&module, "link", store_address)
        .iter()
        .all(|fact| *fact == BlockStore::Operand(1)));
    assert!(facts(&module, "number", store_address)
        .iter()
        .all(|fact| *fact == BlockStore::Operand(1)));
    assert!(facts(&module, "element", store_address)
        .iter()
        .all(|fact| *fact == BlockStore::Operand(1)));
}

#[test]
fn a_module_variable_and_an_activation_local_are_roots() {
    let module = lower(SAMPLE);
    // A closure outside a coroutine keeps its captures in a root slot.
    let closure = |kind: &l::InstructionKind| matches!(kind, l::InstructionKind::MakeClosure(_));
    assert!(facts(&module, "capture", closure)
        .iter()
        .all(|fact| *fact == BlockStore::None));
    let global = |kind: &l::InstructionKind| matches!(kind, l::InstructionKind::StoreGlobal(_));
    assert_eq!(facts(&module, "global", global), vec![BlockStore::None]);
    // The same store into a local of activation storage is a store into a
    // root, and an address of that local names a root.
    let mut walk = function(&module, "walk").clone();
    for local in &mut walk.locals {
        local.storage = l::LocalStorageClass::Activation;
    }
    let definitions = definitions(&walk);
    let stores: Vec<_> = walk
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(instruction.kind, l::InstructionKind::StoreLocal(_))
                || (store_address(&instruction.kind)
                    && matches!(
                        instruction
                            .operands
                            .first()
                            .and_then(|operand| match operand {
                                l::Operand::Value(id) => definitions.get(id),
                                l::Operand::Constant(_) => None,
                            })
                            .and_then(|field| field.operands.first())
                            .and_then(|operand| match operand {
                                l::Operand::Value(id) => definitions.get(id),
                                l::Operand::Constant(_) => None,
                            })
                            .map(|base| &base.kind),
                        Some(l::InstructionKind::AddressOfLocal(_))
                    ))
        })
        .map(|instruction| instruction_block_store(&module, &walk, &definitions, instruction))
        .collect();
    assert!(stores.len() >= 2, "{stores:?}");
    assert!(
        stores.iter().all(|fact| *fact == BlockStore::None),
        "{stores:?}"
    );
}

#[test]
fn a_field_of_a_value_class_in_a_local_is_a_root() {
    let module = lower(SAMPLE);
    let local = function(&module, "local");
    let definitions = definitions(local);
    let stores: Vec<_> = local
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|instruction| store_address(&instruction.kind))
        .map(|instruction| instruction_block_store(&module, local, &definitions, instruction))
        .collect();
    // Each store of `local` goes to the activation storage of `v`.
    assert!(
        stores.iter().all(|fact| *fact == BlockStore::None),
        "{stores:?}"
    );
}

#[test]
fn an_inline_push_writes_into_the_array_storage() {
    let module = lower(SAMPLE);
    let call = |kind: &l::InstructionKind| {
        matches!(
            kind,
            l::InstructionKind::Call(l::CallTarget {
                kind: l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPush),
                ..
            })
        )
    };
    assert_eq!(facts(&module, "grow", call), vec![BlockStore::Operand(1)]);
}

#[test]
fn a_frame_local_of_a_coroutine_is_in_a_block() {
    let module = lower(SAMPLE);
    let walk = function(&module, "walk");
    assert!(is_coroutine(walk));
    assert!(entry_block_store(walk));
    let store_local = |kind: &l::InstructionKind| matches!(kind, l::InstructionKind::StoreLocal(_));
    let facts_of = facts;
    let facts = facts_of(&module, "walk", store_local);
    assert!(facts.contains(&BlockStore::Operand(0)), "{facts:?}");
    // A field of a value class in a frame local, and a value class that
    // the coroutine makes at an address, are in the frame.
    assert!(facts_of(&module, "walk", store_address)
        .iter()
        .all(|fact| *fact == BlockStore::Operand(1)));
    // The captures of a closure in a coroutine are in the frame.
    let closure = |kind: &l::InstructionKind| matches!(kind, l::InstructionKind::MakeClosure(_));
    assert_eq!(
        facts_of(&module, "walk", closure),
        vec![BlockStore::EachOperand]
    );
    assert!(walk
        .blocks
        .iter()
        .filter(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
        .all(|block| terminator_block_store(&block.terminator)));
    assert!(!entry_block_store(function(&module, "plain")));
}

#[test]
fn a_parameter_address_can_be_in_a_block() {
    let module = lower(SAMPLE);
    let plain = function(&module, "plain");
    let definitions = definitions(plain);
    // A value with no definition is a parameter or a block parameter.
    assert!(address_can_be_in_block(
        &module,
        plain,
        &definitions,
        &l::Operand::Value(l::ValueId(u32::MAX)),
    ));
}

#[test]
fn terminators_other_than_a_suspension_store_nothing() {
    let pos = Pos::new("t.ts", 1, 1);
    assert!(!terminator_block_store(&l::Terminator::Unreachable {
        pos: pos.clone()
    }));
    assert!(!terminator_block_store(&l::Terminator::Return {
        value: None,
        pos,
    }));
}
