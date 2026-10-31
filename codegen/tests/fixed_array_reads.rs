//! A read of a `FixedArray` element keeps the array live (`compiler.md` §218.1
//! rule 12). A traversal of a place reads the place (§220.1). A traversal of
//! another subject reads a hidden local through an address of that local. An
//! address of a field or an element of a value names that value as its base.
//!
//! Measured cost: 0.96 s for the eleven tests on arm64 debug. The form tests check and lower
//! one small program. Each tier test runs one program under the interpreter,
//! the dev JIT, and one C compilation, and the C step is most of the cost. A
//! suspension is in a tier test, because a corpus entry with a coroutine grows
//! the LIR snapshot. `Map.getOr` is in a tier test, because `node` has no such
//! method and `a403` is `js-comparable`.

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::lir as l;
use subscript_compiler::{check_program, SourceFile, Type};

const FORM: &str = "function pair(): FixedArray<string, 2> { return [\"a\", \"b\"]; }\n\
                    export function main(): void {\n\
                    \x20 for (const x of pair()) { print(x); }\n\
                    \x20 const dynamic: string[] = [\"c\"];\n\
                    \x20 for (const x of dynamic) { print(x); }\n\
                    }\n";

fn lowered() -> l::Module {
    let hir = check_program(&[SourceFile::new("for-of.ts", FORM)]).expect("checks clean");
    lower_module(&hir).expect("lowers")
}

fn main_function(module: &mut l::Module) -> &mut l::Function {
    module
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .expect("function main")
}

fn messages(module: &l::Module) -> Vec<String> {
    verify_module(module)
        .expect_err("the form is invalid")
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

fn instructions(function: &mut l::Function) -> impl Iterator<Item = &mut l::Instruction> {
    function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
}

#[test]
fn a_fixed_array_traversal_reads_a_hidden_local_with_no_data_copy() {
    let mut module = lowered();
    assert!(verify_module(&module).is_ok());
    let function = main_function(&mut module);
    let locals: Vec<_> = function
        .locals
        .iter()
        .filter(|local| local.source_name == "<for-of subject>")
        .collect();
    assert_eq!(locals.len(), 1);
    assert_eq!(
        locals[0].ty,
        l::ValueType::Data(Type::FixedArray(Box::new(Type::Str), 2))
    );
    let local = locals[0].id;
    let snapshot = function.clone();
    let reads: Vec<_> = instructions(function)
        .filter(|instruction| instruction.kind == l::InstructionKind::IteratorValue)
        .map(|instruction| instruction.operands.len())
        .collect();
    // The dynamic array has a cursor read, and the fixed array has no cursor.
    assert_eq!(reads, vec![3]);
    let block = snapshot
        .blocks
        .iter()
        .find(|block| {
            block
                .instructions
                .iter()
                .any(|i| i.kind == l::InstructionKind::AddressOfLocal(local))
                && block.source_name.as_deref() == Some("for-of.body")
        })
        .expect("the loop body");
    let kinds: Vec<_> = block.instructions.iter().map(|i| &i.kind).collect();
    assert!(matches!(
        kinds[..3],
        [
            l::InstructionKind::AddressOfLocal(_),
            l::InstructionKind::AddressOfIndex { checked: false },
            l::InstructionKind::LoadAddress
        ]
    ));
}

/// Each address that an instruction defines and another block reads, and each
/// address that a block parameter carries.
fn addresses_across_blocks(function: &l::Function) -> Vec<l::ValueId> {
    let is_address = |value: l::ValueId| {
        matches!(
            function.values[value.0 as usize].ty,
            l::ValueType::Address(_)
        )
    };
    let mut defined = std::collections::HashMap::new();
    let mut found: Vec<l::ValueId> = Vec::new();
    for block in &function.blocks {
        found.extend(block.parameters.iter().copied().filter(|v| is_address(*v)));
        for instruction in &block.instructions {
            if let Some(result) = instruction.result {
                defined.insert(result, block.id);
            }
        }
    }
    for block in &function.blocks {
        for instruction in &block.instructions {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    if is_address(*value) && defined.get(value).is_some_and(|b| *b != block.id) {
                        found.push(*value);
                    }
                }
            }
        }
    }
    found
}

const PLACE_FORM: &str = "class R {\n\
                          \x20 a: FixedArray<string, 2> = [\"a\", \"b\"];\n\
                          \x20 m: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];\n\
                          }\n\
                          function* walk(r: R, d: FixedArray<string, 2>[]): Generator<string> {\n\
                          \x20 for (const x of r.a) { yield x; }\n\
                          \x20 for (const x of d[0]) { yield x; }\n\
                          }\n\
                          async function go(r: R, i: i32): Promise<void> {\n\
                          \x20 for (const x of r.m[i]) { await Context.suspend(); print(`${x}`); }\n\
                          }\n\
                          export async function main(): Promise<void> {\n\
                          \x20 const r = new R();\n\
                          \x20 const b: FixedArray<i32, 2> = [1, 2];\n\
                          \x20 for (const x of b) { print(`${x}`); }\n\
                          \x20 r.a.forEach((x: string): void => { print(x); });\n\
                          \x20 print(`${b.reduceRight((n: i32, x: i32): i32 => n + x, 0)}`);\n\
                          \x20 for (const x of walk(r, [[\"p\", \"q\"]])) { print(x); }\n\
                          \x20 await go(r, 1);\n\
                          }\n";

fn place_form() -> l::Module {
    let hir = check_program(&[SourceFile::entry("place.ts", PLACE_FORM)]).expect("checks clean");
    lower_module(&hir).expect("lowers")
}

#[test]
fn a_traversal_of_a_place_has_no_hidden_local_and_no_address_across_a_step() {
    let module = place_form();
    assert!(verify_module(&module).is_ok());
    let mut element_reads = 0;
    for function in &module.functions {
        assert!(
            !function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(
                    |i| matches!(i.kind, l::InstructionKind::IteratorCreate { .. })
                        && function.source_name != "main"
                ),
            "{} has a cursor",
            function.source_name
        );
        assert!(
            function
                .locals
                .iter()
                .all(|local| local.source_name != "<for-of subject>"),
            "{} has a hidden local",
            function.source_name
        );
        assert_eq!(
            addresses_across_blocks(function),
            Vec::new(),
            "{}",
            function.source_name
        );
        element_reads += function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|i| i.kind == l::InstructionKind::AddressOfIndex { checked: false })
            .count();
    }
    // Each of the six traversals has one element read.
    assert_eq!(element_reads, 6);
}

#[test]
fn an_address_kept_across_a_step_is_found() {
    let mut module = place_form();
    let function = main_function(&mut module);
    assert_eq!(addresses_across_blocks(function), Vec::new());
    // One step takes an address that another step computed.
    let (block, address) = function
        .blocks
        .iter()
        .find_map(|block| {
            let i = block
                .instructions
                .iter()
                .find(|i| matches!(i.kind, l::InstructionKind::AddressOfLocal(_)))?;
            Some((block.id, i.result?))
        })
        .expect("a local address");
    let element = function
        .blocks
        .iter_mut()
        .filter(|candidate| candidate.id != block)
        .flat_map(|block| &mut block.instructions)
        .find(|i| i.kind == l::InstructionKind::AddressOfIndex { checked: false })
        .expect("an element address in another block");
    element.operands[0] = l::Operand::Value(address);
    assert_eq!(addresses_across_blocks(function).len(), 1);
}

#[test]
fn the_hidden_local_of_a_call_result_is_released_at_the_loop_exit() {
    const PREFIX: &str = "class Holder { n: i32 = 7; task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }\n\
                          async function fail(): Promise<void> { throw new Error(\"lost\"); }\n\
                          function one(h: Holder): FixedArray<Holder, 1> { return [h]; }\n";
    let run = |name: &str, body: &str| {
        let source = format!("{PREFIX}export function main(): void {{\n{body}\n}}\n");
        let files = [SourceFile::entry(name, &source)];
        let module = check_program(&files).expect("checks clean");
        let lir = lower_module(&module).expect("lowers");
        let interpreted = subscript_codegen::interpreter::interpret(&lir);
        let trapped = |error: String| error.contains("lost");
        [
            interpreted.map_err(|error| trapped(format!("{error:?}"))),
            subscript_codegen::run_jit(&files).map_err(|error| trapped(format!("{error:?}"))),
            subscript_codegen::run_c_aot(&files).map_err(|error| trapped(format!("{error:?}"))),
        ]
    };
    let released = "for (const h of one(new Holder(fail()))) { print(\"step\"); }\n\
                    Context.collect();\n\
                    print(\"end\");";
    for outcome in run("released.ts", released) {
        assert_eq!(outcome, Err(true));
    }
    let held = "const kept = new Holder(fail());\n\
                for (const h of one(kept)) { print(\"step\"); }\n\
                Context.collect();\n\
                print(`end ${kept.n}`);";
    for outcome in run("held.ts", held) {
        assert_eq!(outcome, Ok(b"step\nend 7\n".to_vec()));
    }
}

#[test]
fn a_cursor_over_an_address_is_invalid() {
    let mut module = lowered();
    let function = main_function(&mut module);
    let address = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|i| matches!(i.kind, l::InstructionKind::AddressOfLocal(_)))
        .and_then(|i| i.result)
        .expect("the address of the hidden local");
    let create = instructions(function)
        .find(|i| matches!(i.kind, l::InstructionKind::IteratorCreate { .. }))
        .expect("the cursor of the dynamic array");
    create.operands = vec![l::Operand::Value(address)];
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("iterator creation signature is invalid")),
        "{:?}",
        messages(&module)
    );
}

#[test]
fn a_suspension_in_the_body_keeps_the_elements_on_each_tier() {
    let source = "function built(a: string, b: string): string { return a + b; }\n\
                  function* walk(): Generator<string> {\n\
                  \x20 const names: FixedArray<string, 2> = [built(\"a\", \"1\"), built(\"b\", \"2\")];\n\
                  \x20 const counts: FixedArray<i32, 2> = [4, 5];\n\
                  \x20 for (const x of names) { yield x; Context.collect(); yield built(x, \"!\"); }\n\
                  \x20 for (const y of counts) { yield `${y}`; }\n\
                  }\n\
                  async function go(): Promise<i32> {\n\
                  \x20 const names: FixedArray<string | null, 2> = [built(\"c\", \"3\"), null];\n\
                  \x20 const counts: FixedArray<i32, 2> = [7, 8];\n\
                  \x20 let total = 0;\n\
                  \x20 for (const x of names) { await Context.suspend(); Context.collect(); print(x ?? \"none\"); }\n\
                  \x20 for (const y of counts) { await Context.suspend(); total += y; }\n\
                  \x20 return total;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 for (const v of walk()) { print(v); }\n\
                  \x20 print(`${await go()}`);\n\
                  }\n";
    let files = [SourceFile::entry("for-of-suspend.ts", source)];
    let expected = b"a1\na1!\nb2\nb2!\n4\n5\nc3\nnone\n15\n".to_vec();
    let module = check_program(&files).expect("checks clean");
    let lir = lower_module(&module).expect("lowers");
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).unwrap(),
        expected
    );
    assert_eq!(subscript_codegen::run_jit(&files).unwrap(), expected);
    assert_eq!(subscript_codegen::run_c_aot(&files).unwrap(), expected);
}

#[test]
fn an_index_on_a_value_with_no_binding_reads_the_element_on_each_tier() {
    let source = "function built(a: string, b: string): string { return a + b; }\n\
                  @ValueType\n\
                  class S { a: FixedArray<i32, 2> = [1, 2]; }\n\
                  function make(): S { return new S(); }\n\
                  export function main(): void {\n\
                  \x20 const names = new Map<string, FixedArray<string, 2>>();\n\
                  \x20 names.set(\"k\", [built(\"a\", \"b\"), \"z\"]);\n\
                  \x20 print(names.getOr(\"k\", [\"p\", \"q\"])[0]);\n\
                  \x20 print(names.getOr(\"x\", [\"p\", \"q\"])[1]);\n\
                  \x20 const kept = names.getOr(\"k\", [\"p\", \"q\"]);\n\
                  \x20 print(kept[1]);\n\
                  \x20 const counts = new Map<string, FixedArray<i32, 2>>();\n\
                  \x20 counts.set(\"k\", [1, 2]);\n\
                  \x20 print(`${counts.getOr(\"k\", [3, 4])[1]} ${counts.getOr(\"x\", [3, 4])[0]}`);\n\
                  \x20 print(`${make().a[1]}`);\n\
                  }\n";
    let files = [SourceFile::entry("fixed-array-index.ts", source)];
    let expected = b"ab\nq\nz\n2 3\n2\n".to_vec();
    let module = check_program(&files).expect("checks clean");
    let lir = lower_module(&module).expect("lowers");
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).unwrap(),
        expected
    );
    assert_eq!(subscript_codegen::run_jit(&files).unwrap(), expected);
    assert_eq!(subscript_codegen::run_c_aot(&files).unwrap(), expected);
}

#[test]
fn a_field_of_a_value_temporary_reads_the_element_on_each_tier() {
    let source = "class C { n: i32; constructor(n: i32) { this.n = n; } }\n\
                  function built(a: string, b: string): string { return a + b; }\n\
                  function pair(): FixedArray<string, 2> { return [built(\"a\", \"b\"), built(\"c\", \"d\")]; }\n\
                  function* gp(): Generator<FixedArray<string, 2>> { yield pair(); yield pair(); }\n\
                  function* gc(): Generator<FixedArray<C, 2>> { yield [new C(5), new C(6)]; }\n\
                  function* gf(): Generator<FixedArray<FixedArray<string, 2>, 2>> { yield [pair(), pair()]; }\n\
                  @ValueType\n\
                  class V { x: i32 = 4; a: FixedArray<i32, 2> = [1, 2]; }\n\
                  function* gv(): Generator<V> { yield new V(); }\n\
                  export function main(): void {\n\
                  \x20 const it = gp();\n\
                  \x20 print(it.next().value[1]);\n\
                  \x20 print(`${gc().next().value[1].n}`);\n\
                  \x20 print(gf().next().value[0][0]);\n\
                  \x20 print(gf().next().value[1][1]);\n\
                  \x20 print(`${gv().next().value.x} ${gv().next().value.a[1]}`);\n\
                  }\n";
    let files = [SourceFile::entry("value-field.ts", source)];
    let expected = b"cd\n6\nab\ncd\n4 2\n".to_vec();
    let module = check_program(&files).expect("checks clean");
    let lir = lower_module(&module).expect("lowers");
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).unwrap(),
        expected
    );
    assert_eq!(subscript_codegen::run_jit(&files).unwrap(), expected);
    assert_eq!(subscript_codegen::run_c_aot(&files).unwrap(), expected);
}

#[test]
fn a_field_address_of_a_value_temporary_without_a_base_is_invalid() {
    let source = "function* gp(): Generator<FixedArray<string, 2>> { yield [\"a\", \"b\"]; }\n\
                  export function main(): void { print(gp().next().value[0]); }\n";
    let hir = check_program(&[SourceFile::new("field-base.ts", source)]).expect("checks clean");
    let mut module = lower_module(&hir).expect("lowers");
    assert!(verify_module(&module).is_ok());
    let function = main_function(&mut module);
    let address = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| matches!(instruction.kind, l::InstructionKind::AddressOfField(_)))
        .and_then(|instruction| instruction.result)
        .expect("one field address");
    let l::ValueType::Address(ty) = &mut function.values[address.0 as usize].ty else {
        panic!("an address type");
    };
    assert!(ty.array_base.is_some());
    ty.array_base = None;
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("field address signature is invalid")),
        "{:?}",
        messages(&module)
    );
}

#[test]
fn a_field_address_of_a_reference_class_has_no_base() {
    let source = "class C { a: FixedArray<i32, 2> = [1, 2]; }\n\
                  function make(): C { return new C(); }\n\
                  export function main(): void { make().a[0] = 3; }\n";
    let hir = check_program(&[SourceFile::new("handle-base.ts", source)]).expect("checks clean");
    let module = lower_module(&hir).expect("lowers");
    assert!(verify_module(&module).is_ok());
}

#[test]
fn an_element_address_of_an_array_value_names_the_value_as_its_base() {
    let source = "function pair(): FixedArray<string, 2> { return [\"a\", \"b\"]; }\n\
                  export function main(): void { print(pair()[0]); }\n";
    let hir = check_program(&[SourceFile::new("base.ts", source)]).expect("checks clean");
    let mut module = lower_module(&hir).expect("lowers");
    assert!(verify_module(&module).is_ok());
    let function = main_function(&mut module);
    let address = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| matches!(instruction.kind, l::InstructionKind::AddressOfIndex { .. }))
        .and_then(|instruction| instruction.result)
        .expect("one element address");
    let l::ValueType::Address(ty) = &mut function.values[address.0 as usize].ty else {
        panic!("an address type");
    };
    assert!(ty.array_base.is_some());
    // The form with no base lets the storage plan clear the array before the load.
    ty.array_base = None;
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("index address signature is invalid")),
        "{:?}",
        messages(&module)
    );
}
