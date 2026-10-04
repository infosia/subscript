//! Parameter defaults use shared lowered functions.
//! Measured cost: three checker/LIR tests, 0.01 s wall time; no native compile.
use subscript_codegen::lir::lower_module;
use subscript_compiler::{check_program, lir as l, SourceFile};

#[test]
fn recursive_defaults_have_one_function_and_receive_prior_arguments_and_this() {
    let hir = check_program(&[SourceFile::entry(
        "main.ts",
        r#"
function first(n: i32, acc = n > 0 ? second(n - 1) : 0): i32 { return n + acc; }
function second(n: i32, acc = n > 0 ? first(n - 1) : 0): i32 { return n + acc; }
class Counter { step = 2; take(n: i32, by = this.step + n): i32 { return by; } }
export function main(): void { first(2); first(3); second(2); const c = new Counter(); c.take(1); c.take(2); }
"#,
    )])
    .unwrap();
    let lowered = lower_module(&hir).unwrap();
    let defaults: Vec<_> = lowered
        .functions
        .iter()
        .filter(|f| f.source_name.starts_with("<default "))
        .collect();
    assert_eq!(defaults.len(), 3, "one function per defaulted parameter");
    let with_receiver = defaults
        .iter()
        .find(|f| {
            f.parameters
                .iter()
                .any(|p| p.kind == l::ParameterKind::Receiver)
        })
        .unwrap();
    assert_eq!(
        with_receiver.parameters.len(),
        2,
        "the receiver and the earlier argument"
    );
    for function in defaults {
        let calls = lowered
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .filter(|instruction| {
                matches!(&instruction.kind, l::InstructionKind::Call(target)
                if target.kind == l::CallTargetKind::Function(function.id))
            })
            .count();
        assert!(calls >= 2, "the call sites must share the lowered default");
    }
}

#[test]
fn async_receiver_defaults_and_generator_defaults_use_shared_functions() {
    let hir = check_program(&[SourceFile::entry(
        "main.ts",
        r#"
class S { base: i32 = 7;
    async m(n: i32 = this.base, k = n + 2): Promise<i32> { return n + k; }
    static *numbers(n = 7, k = n + 2): Generator<i32> { yield n + k; }
}
export async function main(): Promise<void> {
    await new S().m(); await new S().m();
    for (const n of S.numbers()) {} for (const n of S.numbers()) {}
}
"#,
    )])
    .unwrap();
    let lowered = lower_module(&hir).unwrap();
    let defaults: Vec<_> = lowered
        .functions
        .iter()
        .filter(|f| f.source_name.starts_with("<default "))
        .collect();
    assert_eq!(defaults.len(), 4);
    let receivers: Vec<_> = defaults
        .iter()
        .filter(|f| {
            f.parameters
                .iter()
                .any(|p| p.kind == l::ParameterKind::Receiver)
        })
        .collect();
    assert_eq!(receivers.len(), 2, "both async defaults receive this");
    assert_eq!(
        receivers
            .iter()
            .map(|f| f.parameters.len())
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn each_generic_instance_and_constructor_keys_its_own_default() {
    let hir = check_program(&[SourceFile::entry(
        "main.ts",
        r#"
function g<T>(n = 1): i32 { return n; }
class C { constructor(n = 1) {} }
export function main(): void { g<i32>(); g<f64>(); g<i32>(); g<f64>(); new C(); new C(); }
"#,
    )])
    .unwrap();
    let lowered = lower_module(&hir).unwrap();
    let defaults: Vec<_> = lowered
        .functions
        .iter()
        .filter(|f| f.source_name.starts_with("<default "))
        .collect();
    assert_eq!(
        defaults.len(),
        3,
        "each instantiated function and constructor owns one default"
    );
    for default in defaults {
        let count = lowered.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.instructions).filter(|i| matches!(&i.kind, l::InstructionKind::Call(t) if t.kind == l::CallTargetKind::Function(default.id))).count();
        assert_eq!(
            count, 2,
            "calls share only the same function parameter default"
        );
    }
}
