//! Prefix and postfix values in each execution form.

use crate::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn source() -> [SourceFile; 1] {
    [SourceFile::new(
        "update.ts",
        r#"class Box { x: i32 = 10; }
        const box: Box = new Box();
        const xs: i32[] = [20];
        let order: i32 = 0;
        function object(): Box { order = order * 10 + 1; return box; }
        function array(): i32[] { order = order * 10 + 2; return xs; }
        function index(): i32 { order = order * 10 + 3; return 0; }
        export function main(): void {
            let value: i32 = 5;
            const a = value++;
            const b = ++value;
            const c = value--;
            const d = --value;
            print(`${a} ${b} ${c} ${d} ${value}`);
            value++;
            ++value;
            value--;
            --value;
            print(`${value}`);
            const fieldOld = object().x++;
            const indexOld = array()[index()]++;
            print(`${fieldOld} ${box.x} ${indexOld} ${xs[0]} ${order}`);
            order = 0;
            const fieldNew = ++object().x;
            const indexNew = ++array()[index()];
            print(`${fieldNew} ${box.x} ${indexNew} ${xs[0]} ${order}`);
            let byte: u8 = 255;
            const byteOld = byte++;
            print(`${byteOld} ${byte}`);
        }"#,
    )]
}

const EXPECTED: &[u8] = b"5 7 7 5 5\n5\n10 11 20 21 123\n12 12 22 22 123\n255 0\n";

#[test]
fn dev_prefix_and_postfix_values_and_statements() {
    assert_eq!(run_jit(&source()).expect("dev updates"), EXPECTED);
}

// compiler.md §337 rule 3: debug best of three: 0.21 s; 1 C build.
#[test]
fn ship_prefix_and_postfix_values_and_statements() {
    assert_eq!(run_c_aot(&source()).expect("ship updates"), EXPECTED);
}

#[test]
fn interpreter_prefix_and_postfix_values_and_statements() {
    let hir = check_program(&source()).expect("updates check");
    let lir = lower_module(&hir).expect("updates lower");
    assert_eq!(interpret(&lir).expect("interpreter updates"), EXPECTED);
}
