//! Assignment facts use the null-check state (compiler.md §159).

use crate::{check_program, SourceFile};

/// Checks table rows and controls without engine execution.
/// Cost: 0.09 seconds for 56 checker programs; no compiler or engine subprocess.
#[test]
fn assignment_rows_and_controls() {
    let prefix = "class A { x:i32=1; } class H { a:A|null=null; } class S { static a:A|null=null; } let g:A|null=null; function cond():boolean{return true;} function touch():void{} function mk():A|null{return cond()?new A():null;} ";
    // tsc 5.9.2 results measured on each complete program.
    for (body, accepted, shared) in [
        // tsc: accepts
        ("let a:A|null=new A(); a.x;", true, false),
        // tsc: accepts
        ("const a:A|null=new A(); a.x;", true, false),
        // tsc: accepts
        ("let a:A|null=null; a=new A(); a.x;", true, false),
        // tsc: accepts
        (
            "let a:A|null=null; if(a===null){a=new A();} a.x;",
            true,
            false,
        ),
        // tsc: TS18047
        (
            "let a:A|null=null; if(cond()){a=new A();} a.x;",
            false,
            false,
        ),
        // tsc: TS18047
        ("let a:A|null=new A(); a=null; a.x;", false, false),
        // tsc: accepts
        ("const h=new H(); h.a=new A(); h.a.x;", true, false),
        // tsc: accepts
        ("const h=new H(); h.a=new A(); touch(); h.a.x;", false, true),
        // tsc: accepts
        ("g=new A(); g.x;", true, false),
        // tsc: TS18047
        (
            "let a:A|null=null; for(let i:i32=0;i<1;i++){a=new A();} a.x;",
            false,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=null; a=cond()?new A():new A(); a.x;",
            true,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=null; if(cond()){a=new A();}else{a=new A();} a.x;",
            true,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=null; if(cond()){a=new A();}else{return;} a.x;",
            true,
            false,
        ),
        // tsc: accepts
        ("let a:A|null=new A(); touch(); a.x;", true, false),
        // tsc: accepts
        (
            "const h=new H(); h.a=new A(); touch(); if(h.a!==null){h.a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "const h=new H(); const alias=h; h.a=new A(); alias.a=null; h.a.x;",
            false,
            true,
        ),
        // tsc: accepts
        (
            "const h=new H(); const alias=h; h.a=new A(); alias.a=null; if(h.a!==null){h.a.x;}",
            true,
            false,
        ),
        // tsc: TS18047
        (
            "let a:A|null=new A(); a=cond()?new A():null; a.x;",
            false,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=new A(); a=cond()?new A():null; if(a!==null){a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=null; for(let i:i32=0;i<1;i++){a=new A();} if(a!==null){a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=new A(); a=null; a=cond()?new A():null; if(a!==null){a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=null; if(cond()){a=new A();} if(a!==null){a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "const a:A|null=new A(); {let a:A|null=null; a=null;} a.x;",
            true,
            false,
        ),
        // tsc: accepts
        (
            "const h=new H(); h.a=new A(); {const h=new H(); touch();} h.a.x;",
            false,
            true,
        ),
        // tsc: accepts
        (
            "const h=new H(); if(cond()){h.a=new A(); touch();}else{h.a=new A();} h.a.x;",
            false,
            true,
        ),
        // tsc: accepts
        (
            "const h=new H(); if(cond()){h.a=new A();}else{h.a=new A();} h.a.x;",
            true,
            false,
        ),
        // tsc: TS2339
        ("let a:A|null=new A(); if(a===null){a.x;}", false, false),
        // tsc: accepts
        ("let a:A|null=new A(); if(a!==null){a.x;}", true, false),
        // tsc: accepts
        (
            "let a:A|null=null; while((a=mk())!==null){a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=null; if((a=mk())!==null && a.x>0){a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "let a:A|null=null; if((a=mk())===null){}else{a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        ("let a:A|null=null; if((a=mk())!=null){a.x;}", true, false),
        // tsc: accepts
        ("let a:A|null=null; if(null!==(a=mk())){a.x;}", true, false),
        // tsc: TS18047
        ("let a:A|null=null; if((a=mk())===null){a.x;}", false, false),
        // tsc: TS18047
        (
            "let a:A|null=null; while((a=mk())!==null){} a.x;",
            false,
            false,
        ),
        // tsc: accepts
        (
            "const h=new H(); if((h.a=mk())!==null){h.a.x;}",
            true,
            false,
        ),
        // tsc: accepts
        (
            "const h=new H(); if((h.a=mk())!==null){touch(); h.a.x;}",
            false,
            true,
        ),
        // tsc: accepts
        (
            "const h=new H(); if((h.a=mk())!==null && cond()){h.a.x;}",
            false,
            true,
        ),
        // tsc: TS18047
        (
            "const h=new H(); h.a=new A(); try{h.a=null;}catch(e){} h.a.x;",
            false,
            false,
        ),
        // tsc: accepts
        (
            "const h=new H(); h.a=new A(); try{touch();}catch(e){} h.a.x;",
            false,
            true,
        ),
        // tsc: accepts
        ("let a:A|null=null; const b=new A(); if((a=b)!==null){a.x;}", true, false),
        // tsc: accepts
        ("let cur:A|null=null; const items:A[]=[new A()]; if((cur=items[0])===null){}", true, false),
        // tsc: accepts
        ("let a:A|null=null; const ok=(a=new A())!==null;", true, false),
        // tsc: accepts
        ("let a:A|null=null; cond() || (a=new A())!==null;", true, false),
        // tsc: accepts
        ("let cur:A|null=null; const next:A|null=new A(); while((cur=next)!==null){cur.x; break;}", true, false),
        // tsc: TS18047
        ("const h=new H(); h.a=new A(); let i:i32=0; while(i<3){i++; if(h.a.x>0){h.a=null;}}", false, false),
        // tsc: TS18047
        ("const h=new H(); h.a=new A(); while(cond()){print(`${h.a.x}`); h.a=null;}", false, false),
        // tsc: TS18047
        ("g=new A(); while(cond()){print(`${g.x}`); g=mk();}", false, false),
        // tsc: TS18047
        ("const h=new H(); h.a=new A(); try{h.a=null; touch(); h.a=new A();}catch(e){} h.a.x;", false, false),
        // tsc: TS18047
        ("const h=new H(); h.a=new A(); try{h.a=null; touch(); h.a=new A(); touch();}catch(e){} h.a.x;", false, false),
        // tsc: accepts
        ("const h=new H(); const alias=h; h.a=new A(); while(cond()){h.a.x; alias.a=null;}", false, true),
        // tsc: accepts
        ("const h=new H(); h.a=new A(); try{h.a=null; touch(); h.a=new A(); touch();}catch(e){return;} h.a.x;", false, true),
        // tsc: accepts
        ("const h=new H(); h.a=new A(); try{h.a=null; touch(); h.a=new A(); touch();}catch(e){h.a=new A();} h.a.x;", false, true),
        // tsc: accepts
        ("const h=new H(); h.a=new A(); while(cond()){h.a.x; {const h=new H(); h.a=null;}}", false, true),
        // tsc: accepts
        ("const h=new H(); h.a=new A(); try{const h=new H(); h.a=null;}catch(e){} h.a.x;", false, true),
        // tsc: accepts
        ("S.a=new A(); touch(); S.a.x;", false, true),
    ] {
        let result = check_program(&[SourceFile::entry(
            "main.ts",
            format!("{prefix}export function main():void{{{body}}}"),
        )]);
        if accepted {
            result.unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
        } else {
            let errors = result.expect_err(body);
            assert_eq!(errors.len(), 1, "{body}: {errors:?}");
            assert_eq!(errors[0].code.as_str(), "S011", "{body}");
            assert_eq!(errors[0].divergence.is_some(), shared, "{body}");
        }
    }
}

/// Checks the declared field type in an uninstantiated generic body.
/// Cost: 0.01 seconds for one checker program; no compiler or engine subprocess.
#[test]
fn generic_assignment_null_comparison() {
    // tsc 5.9.2: accepts.
    check_program(&[SourceFile::entry("main.ts", "class A { x:i32; constructor(x:i32){this.x=x;} } class Slot<T> { v:T|null=null; } function f<T>(unused:T,s:Slot<A>):i32 { if((s.v=new A(4))!==null){return s.v.x;} return 0; }")]).expect("generic assignment null comparison");
}
