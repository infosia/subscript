//! Finite generic instance chains execute on all engines (compiler.md §140).
//! Cost: 0.95 s in debug; eleven C compilations, eleven JIT executions, and eleven interpreter executions.

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn plain_recursion_and_finite_chains_agree_on_all_engines() {
    let files = [SourceFile::new(
        "main.ts",
        r#"
        function plain<T>(x: T, n: i32): i32 {
            if (n <= 0) { return 0; }
            return 1 + plain<T>(x, n - 1);
        }
        function finite<T>(x: T, n: i32): i32 {
            if (n <= 0) { return 0; }
            return 1 + finite<string>("s", n - 1);
        }
        function f<T>(x: T, n: i32): i32 {
            if (n <= 0) { return 0; }
            return 1 + g<T>(x, n - 1);
        }
        function g<U>(x: U, n: i32): i32 { return f<U>(x, n); }
        class H {
            plain<T>(x: T, n: i32): i32 {
                if (n <= 0) { return 0; }
                return 1 + this.plain<T>(x, n - 1);
            }
            finite<T>(x: T, n: i32): i32 {
                if (n <= 0) { return 0; }
                return 1 + this.finite<string>("s", n - 1);
            }
        }
        class N<T> { next(): N<T> | null { return null; } }
        export function main(): void {
            print(`${plain<i32>(1, 3)}`);
            print(`${finite<i32>(1, 3)}`);
            print(`${f<i32>(1, 3)}`);
            const h = new H();
            print(`${h.plain<i32>(1, 3)}`);
            print(`${h.finite<i32>(1, 3)}`);
            print(`${new N<i32>().next() === null}`);
        }
    "#,
    )];
    let expected = b"3\n3\n3\n3\n3\ntrue\n";
    let module =
        lower_module(&check_program(&files).expect("finite chains")).expect("finite chain LIR");
    assert_eq!(interpret(&module).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}

fn all_engines(source: &str, expected: &[u8]) {
    let files = [SourceFile::new("main.ts", source)];
    let module = lower_module(&check_program(&files).expect("a finite constant-argument chain"))
        .expect("finite chain LIR");
    assert_eq!(interpret(&module).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("dev JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("ship C"), expected);
}

#[test]
fn constant_class_arguments_form_a_finite_function_chain() {
    all_engines(
        include_str!("../../corpus/accept/a304-constant-instance-chain.ts"),
        b"3\n",
    );
}

#[test]
fn constant_array_arguments_form_a_finite_function_chain() {
    all_engines("function f<T>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<i32[]>(n - 1); } export function main(): void { print(`${f<i32>(3)}`); }", b"3\n");
}

#[test]
fn constant_arguments_form_a_finite_mutual_chain() {
    all_engines("class W<T> { v: T; constructor(v: T) { this.v = v; } } function f<T>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + g<T>(n - 1); } function g<U>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<W<i32>>(n - 1); } export function main(): void { print(`${f<i32>(4)}`); }", b"4\n");
}

#[test]
fn constant_nested_arguments_form_a_finite_class_chain() {
    all_engines("class Box<T> { v: T; constructor(v: T) { this.v = v; } deep(): Box<Box<i32>> | null { return null; } } export function main(): void { print(`${new Box<i32>(1).deep() === null}`); }", b"true\n");
}

#[test]
fn a_constant_request_breaks_parameter_forwarding_through_another_template() {
    all_engines("class W<T> { v: T; constructor(v: T) { this.v = v; } } function f<T>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + g<W<i32>>(n - 1); } function g<U>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<U>(n - 1); } export function main(): void { print(`${f<i32>(4)}`); }", b"4\n");
}

#[test]
fn f1_finite_position_dependencies() {
    all_engines("class W<T> {} function f<A, B>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<A, W<i32>>(n - 1); } export function main(): void { print(`${f<i32, i32>(3)}`); }", b"3\n");
}

#[test]
fn f4_finite_position_dependencies() {
    all_engines("class W<T> {} function f<A, B>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<B, W<i32>>(n - 1); } export function main(): void { print(`${f<i32, i32>(3)}`); }", b"3\n");
}

#[test]
fn f6_finite_position_dependencies() {
    all_engines("class P<A, B> {} function f<A, B>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<i32, P<A, i32>>(n - 1); } export function main(): void { print(`${f<i32, i32>(3)}`); }", b"3\n");
}

#[test]
fn f8_finite_field_dependencies() {
    all_engines("class W<T> {} class N<A, B> { next: N<A, W<i32>> | null = null; } export function main(): void { print(`${new N<i32, i32>().next === null}`); }", b"true\n");
}

#[test]
fn plain_swap_and_constant_nested_method_arguments_are_finite() {
    all_engines("function f<A, B>(n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<B, A>(n - 1); } class B<T> { next(): B<B<i32>> | null { return null; } } export function main(): void { print(`${f<i32, string>(3)}`); print(`${new B<i32>().next() === null}`); }", b"3\ntrue\n");
}
