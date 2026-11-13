//! A C callback parameter cannot receive captures (compiler.md §274 rule 5).
#[path = "corpus/interop.rs"]
#[allow(dead_code)]
mod interop;

use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn a_c_callback_parameter_rejects_a_capturing_lambda() {
    let source = "function install(cb: SubLogCallback): void { new SubCallbackInfo(cb, null, null); } export function main(): void { const value:i32=7; install((message, userdata, userparam):void=>{print(`${value}`);}); }";
    let diagnostics = check_program(&[
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("capture.ts", source),
    ])
    .expect_err("a C callback parameter cannot receive a capture");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == RuleCode::S100 && diagnostic.message.contains("SubLogCallback")
    }));
    let module = check_program(&[
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("capture.ts", source.replace("${value}", "clean")),
    ])
    .expect("a non-capturing C callback is accepted");
    let install = module
        .functions
        .iter()
        .find(|function| function.name == "install")
        .unwrap();
    assert_eq!(
        install.params[0].escape_kind,
        subscript_compiler::hir::EscapeKind::CCallback
    );
}

#[test]
fn c_callback_kind_dominates_managed_storage_and_propagates() {
    let source = "class Holder { constructor(public cb: SubLogCallback) {} } function install(cb: SubLogCallback): void { new Holder(cb); new SubCallbackInfo(cb,null,null); } function forward(cb:SubLogCallback):void { install(cb); } export function main():void { const k:i32=7; forward((message, userdata, userparam):void=>{print(`${k}`);}); }";
    let diagnostics = check_program(&[
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("capture.ts", source),
    ])
    .expect_err("a C callback dominates managed storage");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == RuleCode::S100 && d.message.contains("SubLogCallback")),
        "{diagnostics:?}"
    );
    let module = check_program(&[
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("capture.ts", source.replace("${k}", "clean")),
    ])
    .unwrap();
    for name in ["forward", "install"] {
        let f = module.functions.iter().find(|f| f.name == name).unwrap();
        assert_eq!(
            f.params[0].escape_kind,
            subscript_compiler::hir::EscapeKind::CCallback
        );
    }
}

fn callback_check(
    source: &str,
) -> Result<subscript_compiler::hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[
        SourceFile::ambient("simple.generated.d.ts", "// @subscript-c-header include=\"simple.h\"\n// @subscript-c-callback typedef=\"Callback\"\ntype Callback=(m:i32)=>void; type Other=(m:i32)=>void;"),
        SourceFile::new("identity.ts", source),
    ])
}

#[test]
fn callback_identity_survives_type_operations() {
    let prefix = "type Alias = Callback; type Plain = (m:i32)=>void; function named(m:i32):void {} class Box<T> { constructor(public value:T) {} } function id<T>(x:T):T { return x; }";
    for body in [
        "const cb:Alias=named; const plain:Plain=cb;",
        "const plain:Plain=named; const cb:Alias=plain;",
        "const cb:Alias|null=named; const plain:Plain|null=cb;",
        "const a:Alias[]=[named]; const b:Plain[]=a;",
        "const a:FixedArray<Alias,1>=[named]; const b:FixedArray<Plain,1>=a;",
        "const a=new Map<i32,Alias>(); const b:Map<i32,Plain>=a;",
        "const a=new Box<Alias>(named); const b:Plain=a.value;",
        "const a:Plain=named; const b=a as Alias;",
        "const a:Alias=named; const b=a as Plain;",
        "const a:Alias=named; const b:Other=a;",
        "const a:Plain=named; const b=id<Plain>(a) as Alias;",
        "const a:Alias=named; const b:Plain=id<Alias>(a);",
        "const a=new Box<Alias>(named); const b:Box<Plain>=a;",
    ] {
        let diagnostics = callback_check(&format!(
            "{prefix} export function main():void {{ {body} }}"
        ))
        .unwrap_err();
        assert!(
            diagnostics.iter().any(|d| {
                d.message.contains(if body.contains("b:Other") {
                    "Other"
                } else {
                    "Callback"
                })
            }),
            "{body}: {diagnostics:?}"
        );
    }
}

#[test]
fn callback_literals_convert_at_expected_places() {
    for body in [
        "const cb:Callback=named;",
        "const cb:Callback=(m):void=>{};",
        "const cb:Callback|null=named;",
        "const a:Callback[]=[named];",
        "const a:FixedArray<Callback,1>=[named];",
        "const a=new Map<i32,Callback>(); a.set(0,named);",
    ] {
        callback_check(&format!(
            "function named(m:i32):void {{}} export function main():void {{ {body} }}"
        ))
        .unwrap();
    }
}

#[test]
fn a_generic_cast_cannot_create_callback_identity() {
    let errors = callback_check("function convert<T>(x:T):Callback { return x as Callback; } export function main():void { const plain=(n:i32):void=>{}; convert(plain); }").unwrap_err();
    assert!(
        errors.iter().any(|d| d.message.contains("Callback")),
        "{errors:?}"
    );
}

#[test]
fn function_debug_text_preserves_plain_form_and_callback_name() {
    use subscript_compiler::types::{FuncType, Type};
    let mut function = FuncType {
        callback_typedef: None,
        params: vec![Type::I32],
        ret: Type::Void,
    };
    assert_eq!(
        format!("{function:?}"),
        "FuncType { params: [I32], ret: Void }"
    );
    function.callback_typedef = Some("Callback".into());
    assert_eq!(
        format!("{function:?}"),
        "FuncType { callback_typedef: \"Callback\", params: [I32], ret: Void }"
    );
}
