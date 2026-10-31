//! A read of a `FixedArray` element keeps the array live (`compiler.md` §218.1
//! rule 12). A `for…of` reads a hidden local through an address of that local.
//! An address of a field or an element of a value names that value as its base.
//!
//! Measured cost: 0.48 s for the nine tests on arm64 debug. The form tests check and lower
//! one small program. Each tier test runs one program under the interpreter,
//! the dev JIT, and one C compilation, and the C step is most of the cost. A
//! suspension is in a tier test, because a corpus entry with a coroutine grows
//! the LIR snapshot. `Map.getOr` is in a tier test, because `node` has no such
//! method and `a403` is `js-comparable`.

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::lir as l;
use subscript_compiler::{check_program, SourceFile, Type};

const FORM: &str = "export function main(): void {\n\
                    \x20 const fixed: FixedArray<string, 2> = [\"a\", \"b\"];\n\
                    \x20 for (const x of fixed) { print(x); }\n\
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

fn cursor_kind(function: &l::Function, instruction: &l::Instruction) -> l::ForOfKind {
    let Some(l::Operand::Value(cursor)) = instruction.operands.first() else {
        panic!("a cursor operand");
    };
    let l::ValueType::Iterator(iterator) = &function.values[cursor.0 as usize].ty else {
        panic!("an iterator type");
    };
    iterator.kind
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
    // The dynamic array has a cursor read, and the fixed array has none.
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

#[test]
fn a_cursor_read_of_a_fixed_array_traversal_is_invalid() {
    let mut module = lowered();
    let function = main_function(&mut module);
    let snapshot = function.clone();
    let fixed_cursor = snapshot
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|i| {
            matches!(i.kind, l::InstructionKind::IteratorHasNext)
                && cursor_kind(&snapshot, i) == l::ForOfKind::FixedArrayValues
        })
        .expect("a fixed-array cursor test")
        .operands
        .clone();
    let element = snapshot
        .values
        .iter()
        .find(|value| value.ty == l::ValueType::Data(Type::Str))
        .expect("a string value")
        .id;
    let read = instructions(function)
        .find(|i| i.kind == l::InstructionKind::LoadAddress && i.result.is_some())
        .expect("one load");
    read.kind = l::InstructionKind::IteratorValue;
    read.operands = fixed_cursor;
    read.result = Some(element);
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("iterator value signature is invalid")),
        "{:?}",
        messages(&module)
    );
}

#[test]
fn a_fixed_array_cursor_over_a_data_subject_is_invalid() {
    let mut module = lowered();
    let function = main_function(&mut module);
    let store = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|i| matches!(i.kind, l::InstructionKind::StoreLocal(_)))
        .expect("the hidden local store")
        .operands[0]
        .clone();
    for instruction in instructions(function) {
        if matches!(
            instruction.kind,
            l::InstructionKind::IteratorCreate {
                kind: l::ForOfKind::FixedArrayValues,
                ..
            }
        ) {
            instruction.operands = vec![store.clone()];
        }
    }
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
