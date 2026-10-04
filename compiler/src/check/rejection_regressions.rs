//! Independent controls for declaration poisoning and builtin-name resolution.

use super::{rejection::RejectionSite, rejection_programs, rejection_total};
use crate::{check_program, SourceFile};

#[test]
fn source_declarations_shadow_builtin_type_names() {
    let programs = rejection_programs::programs();
    for key in [
        "promise-source",
        "promise-source-generic",
        "fixed-source",
        "fixed-source-generic",
        "generator-source",
        "sized-source-shadow",
        "source-enum-bytes",
    ] {
        let program = programs.iter().find(|p| p.key == key).unwrap();
        assert!(program.codes.is_empty(), "{key}");
        let diagnostics = rejection_programs::check(program);
        assert!(diagnostics.is_empty(), "{key}: {diagnostics:?}");
    }
}

#[test]
fn source_builtin_shadows_reject_missing_members() {
    let programs = rejection_programs::programs();
    for (key, site) in [
        (
            "shadow-fixed-member",
            RejectionSite::ClassUndeclaredMemberRead,
        ),
        (
            "shadow-generator-next",
            RejectionSite::ClassUndeclaredMethodCall,
        ),
    ] {
        let program = programs.iter().find(|p| p.key == key).unwrap();
        rejection_total::clear_reached();
        let diagnostics = rejection_programs::check(program);
        let reached = rejection_total::take_reached();
        assert_eq!(diagnostics.len(), 1, "{key}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code.as_str(), "S018", "{key}");
        assert!(
            reached.iter().any(|(actual, message, pos)| {
                *actual == site && *message == diagnostics[0].message && *pos == diagnostics[0].pos
            }),
            "{key}: {reached:?}"
        );
    }
}

#[test]
fn type_parameter_defaults_reject_declarations_without_use_cascades() {
    let programs = rejection_programs::programs();
    for key in ["default-function", "default-method", "default-class"] {
        let program = programs.iter().find(|p| p.key == key).unwrap();
        let diagnostics = rejection_programs::check(program);
        assert_eq!(diagnostics.len(), 1, "{key}: {diagnostics:?}");
        assert_eq!(
            diagnostics[0].divergence,
            Some(crate::divergence::Divergence::TypeParameterDefault),
            "{key}"
        );
    }
}

#[test]
fn rejected_local_declarations_bind_poisoned_names() {
    for source in [
        "function f(): void { class C {} const x: C = new C(); }",
        "function f(): void { function g(): void {} g(); }",
        "function f(): void { g(); function g(): void {} }",
        "function f(): i32 { return g(2); function g(x:i32):i32 {return x;} }",
        "class A { f():void { g(); function g():void {} } }",
        "function f():void { { g(); function g():void {} } }",
        "type Fn = (x:i32)=>i32; const f:Fn=(x)=>x*2;",
        "function f(): void { type Map = i32; const x: Map = 1; }",
        "function f(): void { var x: i32 = 1; x; }",
        "function f(): void { let x; x = 1; print(`${x}`); }",
    ] {
        let files = [SourceFile::entry("main.ts", source)];
        let diagnostics = check_program(&files).expect_err("the local declaration is rejected");
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert!(diagnostics[0].divergence.is_some(), "{diagnostics:?}");
    }
}

#[test]
fn rejected_mirror_modules_poison_imported_exports() {
    let files = [
        SourceFile::ambient(
            "mirror.d.ts",
            "declare module \"external\" { export function f(): void; }",
        ),
        SourceFile::entry(
            "main.ts",
            "import { f } from \"external\"; export function main(): void { f(); }",
        ),
    ];
    let diagnostics = check_program(&files).expect_err("mirror module scope is rejected");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].divergence,
        Some(crate::divergence::Divergence::MirrorModuleDeclarationForm)
    );
}

#[test]
fn nullable_flow_does_not_cross_a_conditional_write_or_function_boundary() {
    let prefix = "class A { x:i32=1; } ";
    for body in [
        "function f(a:A|null,flag:boolean):void {while(flag){a=new A();} a.x;}",
        "function f(a:A|null,flag:boolean):void {for(;flag;){a=new A();} a.x;}",
        "function f(a:A|null):void {const values:i32[]=[1]; for(const n of values){a=new A();} a.x;}",
        "function f(a:A|null):void {a=new A(); const cb:()=>i32=():i32=>a.x;}",
        "function f(a:A|null):void {try{a=new A();}catch(e){} a.x;}",
        "function f(a:A|null):void {try{}catch(e){a=new A();} a.x;}",
    ] {
        let source = format!("{prefix}{body}");
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).expect_err("nullable read");
        let nullable = diagnostics.iter().find(|d| d.code.as_str()=="S011").unwrap();
        assert_eq!(nullable.divergence, None, "{body}: {diagnostics:?}");
    }
}

#[test]
fn return_and_throw_guards_keep_the_existing_narrowing() {
    for guard in ["return;", "throw new Error(\"null\");", "{ return; }"] {
        let source = format!(
            "class A{{x:i32=1;}} function f(a:A|null):void{{if(a===null){{{guard}}} a.x;}}"
        );
        assert!(
            check_program(&[SourceFile::entry("main.ts", source)]).is_ok(),
            "{guard}"
        );
    }
}

#[test]
fn a_shadowed_write_keeps_the_outer_nonnull_fact() {
    let source = "class A{x:i32=1;} function f():void{const a:A|null=new A(); {let a:A|null=null; a=null;} a.x;}";
    check_program(&[SourceFile::entry("main.ts", source)])
        .expect("outer initializer fact survives");
}

#[test]
fn first_diagnostics_and_switch_facts() {
    let programs = rejection_programs::programs();
    for program in programs.iter().filter(|p| {
        [
            "distinct-nominal-class-assignment",
            "incompatible-nominal-assignment",
            "distinct-nominal-class-assignment-witness-2",
            "incompatible-nominal-assignment-witness-2",
            "distinct-nominal-container-assignment",
            "incompatible-nominal-assignment-witness-3",
            "distinct-nominal-container-assignment-witness-2",
            "incompatible-nominal-assignment-witness-4",
            "erased-nominal-type-arguments",
            "incompatible-nominal-assignment-witness-5",
            "abstract-method-body-missing",
            "function-implementation-missing-witness-2",
            "this-static-method-member",
            "instance-member-in-static-method",
            "reference-class-inheritance-witness-2",
            "dynamic-import-call",
            "field-assignment-nested-every-normal-exit-witness-2",
            "field-assignment-nested-unassigned-exit-witness-2",
            "nullable-member",
            "enum-string-value",
            "constructor-parameter-property-witness-2",
            "optional-parameter-witness-2",
            "default-export-declaration-witness-2",
            "default-export-declaration-witness-3",
            "distinct-nominal-class-assignment-witness-3",
            "incompatible-nominal-assignment-witness-6",
            "distinct-nominal-class-assignment-witness-4",
            "incompatible-nominal-assignment-witness-7",
            "distinct-nominal-container-assignment-witness-3",
            "incompatible-nominal-assignment-witness-8",
            "distinct-nominal-container-assignment-witness-4",
            "incompatible-nominal-assignment-witness-9",
            "erased-nominal-type-arguments-witness-2",
            "incompatible-nominal-assignment-witness-10",
            "abstract-method-body-missing-witness-2",
            "function-implementation-missing-witness-3",
            "this-static-method-member-witness-2",
            "instance-member-in-static-method-witness-2",
            "reference-class-inheritance-witness-3",
            "dynamic-import-call-witness-2",
            "field-assignment-nested-every-normal-exit-witness-3",
            "field-assignment-nested-unassigned-exit-witness-3",
            "nullable-member-witness-2",
            "enum-string-value-witness-2",
            "constructor-parameter-property-witness-3",
            "optional-parameter-witness-3",
            "default-export-declaration-witness-4",
            "default-export-declaration-witness-5",
            "distinct-nominal-class-assignment-witness-5",
            "incompatible-nominal-assignment-witness-11",
            "distinct-nominal-class-assignment-witness-6",
            "incompatible-nominal-assignment-witness-12",
            "distinct-nominal-container-assignment-witness-5",
            "incompatible-nominal-assignment-witness-13",
            "distinct-nominal-container-assignment-witness-6",
            "incompatible-nominal-assignment-witness-14",
            "erased-nominal-type-arguments-witness-3",
            "incompatible-nominal-assignment-witness-15",
            "abstract-method-body-missing-witness-3",
            "function-implementation-missing-witness-4",
            "this-static-method-member-witness-3",
            "instance-member-in-static-method-witness-3",
            "reference-class-inheritance-witness-4",
            "dynamic-import-call-witness-3",
            "field-assignment-nested-every-normal-exit-witness-4",
            "field-assignment-nested-unassigned-exit-witness-4",
            "nullable-member-witness-3",
            "enum-string-value-witness-3",
            "constructor-parameter-property-witness-4",
            "optional-parameter-witness-4",
            "default-export-declaration-witness-6",
            "default-export-declaration-witness-7",
            "distinct-nominal-class-assignment-witness-7",
            "incompatible-nominal-assignment-witness-16",
            "distinct-nominal-class-assignment-witness-8",
            "incompatible-nominal-assignment-witness-17",
            "distinct-nominal-container-assignment-witness-7",
            "incompatible-nominal-assignment-witness-18",
            "distinct-nominal-container-assignment-witness-8",
            "incompatible-nominal-assignment-witness-19",
            "erased-nominal-type-arguments-witness-4",
            "incompatible-nominal-assignment-witness-20",
            "abstract-method-body-missing-witness-4",
            "function-implementation-missing-witness-5",
            "this-static-method-member-witness-4",
            "instance-member-in-static-method-witness-4",
            "reference-class-inheritance-witness-5",
            "dynamic-import-call-witness-4",
            "field-assignment-nested-every-normal-exit-witness-5",
            "field-assignment-nested-unassigned-exit-witness-5",
            "nullable-member-witness-4",
            "enum-string-value-witness-4",
            "constructor-parameter-property-witness-5",
            "optional-parameter-witness-5",
            "default-export-declaration-witness-8",
            "default-export-declaration-witness-9",
            "optional-private-member",
            "unknown-class-constructor",
            "non-expression-callee-unavailable",
            "non-expression-callee-unavailable-witness-2",
            "erased-assignable-type-mismatch",
            "non-expression-callee-unavailable-witness-3",
            "non-expression-callee-unavailable-witness-4",
            "erased-assignable-type-mismatch-witness-2",
            "non-expression-callee-unavailable-witness-5",
            "non-expression-callee-unavailable-witness-6",
            "erased-assignable-type-mismatch-witness-3",
            "non-expression-callee-unavailable-witness-7",
            "non-expression-callee-unavailable-witness-8",
            "erased-assignable-type-mismatch-witness-4",
            "duplicate-numeric-index-signature-witness-2",
            "non-expression-callee-unavailable-witness-9",
            "non-expression-callee-unavailable-witness-10",
            "non-expression-callee-unavailable-witness-11",
            "non-expression-callee-unavailable-witness-12",
            "non-expression-callee-unavailable-witness-13",
            "this-static-method-member-witness-5",
            "instance-member-in-static-method-witness-5",
            "this-static-method-member-witness-6",
            "instance-member-in-static-method-witness-6",
            "this-static-method-member-witness-7",
            "instance-member-in-static-method-witness-7",
            "this-static-method-member-witness-8",
            "instance-member-in-static-method-witness-8",
            "local-read-unassigned-print",
            "assignment-type-mismatch",
            "local-read-unassigned-initializer",
            "assignment-type-mismatch-witness-2",
            "local-read-unassigned-assignment-rhs",
            "assignment-type-mismatch-witness-3",
            "local-read-unassigned-return",
            "assignment-type-mismatch-witness-4",
        ]
        .contains(&p.key.as_str())
    }) {
        let diagnostics = rejection_programs::check(program);
        assert!(!diagnostics.is_empty(), "{}", program.key);
        if program.codes.is_empty() {
            assert!(
                diagnostics[0].divergence.is_some(),
                "{}: {diagnostics:?}",
                program.key
            );
        } else {
            assert!(
                diagnostics[0].divergence.is_none(),
                "{}: {diagnostics:?}",
                program.key
            );
        }
        if program.key.starts_with("local-initializer-missing")
            || program.key.starts_with("enum-string-value")
        {
            assert_eq!(diagnostics.len(), 1, "{}: {diagnostics:?}", program.key);
        }
    }
}

#[test]
fn rejected_parameter_properties_bind_poisoned_parameter_names() {
    for source in [
        "class C { constructor(public x:f64) { print(`${x}`); } }",
        "class C { constructor(public x:f64, y:i32) { print(`${x}${y}`); } }",
        "class C { constructor(public x:f64=1.0) { print(`${x}`); } }",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert_eq!(
            diagnostics[0].divergence,
            Some(crate::divergence::Divergence::ConstructorParameterPropertyForm)
        );
    }
}
