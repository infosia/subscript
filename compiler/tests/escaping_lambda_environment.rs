//! Ownership selection follows compiler.md §274 rules 1, 2, and 6.
use subscript_compiler::{check_program, hir, SourceFile};
fn flags(child: hir::HirChild<'_>, result: &mut Vec<bool>) {
    match child {
        hir::HirChild::Expr(e) => {
            if let hir::ExprKind::Lambda {
                captures,
                owns_environment,
                ..
            } = &e.kind
            {
                if !captures.is_empty() {
                    result.push(*owns_environment);
                }
            }
            for child in e.children() {
                flags(child, result);
            }
        }
        hir::HirChild::Stmt(s) => {
            for child in s.children() {
                flags(child, result);
            }
        }
    }
}
#[test]
fn managed_destinations_own_captures() {
    for source in [
        "function make():()=>i32 { const k:i32=3; return ():i32=>k; }",
        "function make():void { const k:i32=3; const p=new P(named); p.f=():i32=>k; }",
        "function make():void { const k:i32=3; const a=[():i32=>k]; }",
        "function make():void { const k:i32=3; const m=new Map<i32,()=>i32>(); m.set(0,():i32=>k); }",
        "function make():void { const k:i32=3; const p=new P(():i32=>k); }",
        "function make():void { const k:i32=3; const p=new P(():i32=>k); const s=new Set<P>(); s.add(p); }",
        "function make():()=>i32 { const k:i32=3; const f=():i32=>k; return ():i32=>f(); }",
    ] {
        let module=check_program(&[SourceFile::new("owned.ts",format!("function named():i32 {{ return 0; }} class P {{ constructor(public f:()=>i32) {{}} }} {source} export function main():void {{}}"))]).unwrap();
        let mut result=Vec::new();
        for owner in module.expression_owners() {
            if let hir::ExpressionOwner::Body {statements,..}=owner { for s in statements { flags(hir::HirChild::Stmt(s), &mut result); } }
        }
        assert!(!result.is_empty(), "{source}");
        assert!(result.iter().all(|v| *v), "{source}: {result:?}");
    }
}
#[test]
fn nonescaping_parameter_keeps_stack_environment() {
    let module=check_program(&[SourceFile::new("stack.ts","function call(f:()=>i32):i32 { return f(); } export function main():void { const k:i32=3; call(():i32=>k); }")]).unwrap();
    let call = module.functions.iter().find(|f| f.name == "call").unwrap();
    assert_eq!(call.params[0].escape_kind, hir::EscapeKind::None);
    let mut result = Vec::new();
    for f in &module.functions {
        for s in &f.body {
            flags(hir::HirChild::Stmt(s), &mut result);
        }
    }
    assert_eq!(result, vec![false]);
}
