//! A read of a `FixedArray` element keeps the array live (`compiler.md` §218.1
//! rule 12). A `for…of` gives each element read the array as an operand. An
//! address of an element of an array value names that value as its base.
//!
//! Measured cost: 0.51 s for the six tests on arm64 debug. The form tests check and lower
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

/// The element reads of `main`, with the traversal kind of each cursor.
fn reads(function: &mut l::Function) -> Vec<(l::ForOfKind, &mut l::Instruction)> {
    let values = function.values.clone();
    function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .filter(|instruction| instruction.kind == l::InstructionKind::IteratorValue)
        .map(|instruction| {
            let Some(l::Operand::Value(cursor)) = instruction.operands.first() else {
                panic!("a cursor operand");
            };
            let l::ValueType::Iterator(iterator) = &values[cursor.0 as usize].ty else {
                panic!("an iterator type");
            };
            (iterator.kind, instruction)
        })
        .collect()
}

fn messages(module: &l::Module) -> Vec<String> {
    verify_module(module)
        .expect_err("the form is invalid")
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

#[test]
fn each_fixed_array_read_has_the_array_as_an_operand() {
    let mut module = lowered();
    assert!(verify_module(&module).is_ok());
    let function = main_function(&mut module);
    let values = function.values.clone();
    let reads = reads(function);
    assert_eq!(reads.len(), 2);
    for (kind, read) in reads {
        if kind == l::ForOfKind::FixedArrayValues {
            let Some(l::Operand::Value(array)) = read.operands.get(3) else {
                panic!("{:?}", read.operands);
            };
            assert_eq!(
                values[array.0 as usize].ty,
                l::ValueType::Data(Type::FixedArray(Box::new(Type::Str), 2))
            );
        } else {
            assert_eq!(read.operands.len(), 3);
        }
    }
}

#[test]
fn a_fixed_array_read_without_the_array_is_invalid() {
    let mut module = lowered();
    for (kind, read) in reads(main_function(&mut module)) {
        if kind == l::ForOfKind::FixedArrayValues {
            read.operands.truncate(3);
        }
    }
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("iterator value signature is invalid")),
        "{:?}",
        messages(&module)
    );
}

#[test]
fn a_dynamic_array_read_with_an_array_operand_is_invalid() {
    let mut module = lowered();
    let mut reads = reads(main_function(&mut module));
    let array = reads
        .iter()
        .find(|(kind, _)| *kind == l::ForOfKind::FixedArrayValues)
        .and_then(|(_, read)| read.operands.get(3).cloned())
        .expect("the fixed-array operand");
    for (kind, read) in &mut reads {
        if *kind != l::ForOfKind::FixedArrayValues {
            read.operands.push(array.clone());
        }
    }
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("iterator value signature is invalid")),
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
