//! Non-null stores and immutable globals (compiler.md §304).

use crate::{check_program, SourceFile};

fn check(source: &str, accepted: bool) {
    let prefix = "class A { x:i32=1; f:A|null=null; } class H { f:A|null=null; g:A|null=null; } function mk():A|null{return new A();} function other():void{} ";
    let result = check_program(&[SourceFile::entry("stores.ts", format!("{prefix}{source}"))]);
    if accepted {
        result.unwrap_or_else(|errors| panic!("{source}: {errors:?}"));
    } else {
        let errors = result.expect_err(source);
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code.as_str(), "S011", "{source}: {errors:?}");
    }
}

/// Cost: 0.01 seconds for 10 checker programs; no engine subprocess.
#[test]
fn const_initializer_proofs_have_lexical_scope() {
    for (binding, accepted) in [
        ("const n=new A();", true),
        ("const b=new A(); const n=b;", true),
        ("const n=mk();", false),
        ("let n=new A();", true),
        ("let n:A|null=new A(); n=mk();", false),
        ("const n=otherFlag()?new A():new A();", true),
        ("const n=otherFlag()?new A():null;", false),
    ] {
        check(&format!("function otherFlag():boolean{{return true;}} export function main():void{{let a:A|null=new A(); for(let i:i32=0;i<2;i++){{a.x; {binding} a=n;}}}}"), accepted);
    }
    for (inner, accepted) in [("new A()", true), ("mk()", false)] {
        check(&format!("export function main():void{{const n=new A(); let a:A|null=new A(); for(let i:i32=0;i<2;i++){{a.x; {{const n={inner}; a=n;}}}}}}"), accepted);
    }
    // A branch-local proof cannot reach the outer binding in its sibling branch.
    check("export function main():void{let n:A|null=mk(); let a:A|null=new A(); for(let i:i32=0;i<2;i++){a.x; if(i===0){const n=new A(); a=n;}else{a=n;}}}", false);
}

/// Cost: 0.01 seconds for 9 checker programs; no engine subprocess.
#[test]
fn loop_sources_form_the_largest_stable_set() {
    for (last, accepted) in [("new A()", true), ("mk()", false)] {
        for nested in [false, true] {
            let stores = format!("a=b; b=c; c={last};");
            let stores = if nested {
                format!("for(let j:i32=0;j<2;j++){{{stores}}}")
            } else {
                stores
            };
            check(&format!("export function main():void{{let a:A|null=new A(); let b:A|null=new A(); let c:A|null=new A(); for(let i:i32=0;i<2;i++){{a.x; {stores}}}}}"), accepted);
        }
    }
    check("export function main():void{let a:A|null=new A(); let b:A|null=new A(); for(let i:i32=0;i<2;i++){a.x; a=b; b=a;}}", true);
    check("export function main():void{let a:A|null=new A(); let b:A|null=null; for(let i:i32=0;i<2;i++){a.x; a=b; b=a;}}", false);
    for (source, accepted) in [("new A()", true), ("mk()", false)] {
        check(&format!("export function main():void{{let b:A|null=new A(); let a:A|null=new A(); for(let i:i32=0;i<2;i++){{a.x; {{let b={source}; a=b;}}}}}}"), accepted);
    }
    // The annotation supplies the proof independently of the retained set.
    check("export function main():void{let a:A|null=new A(); for(let i:i32=0;i<2;i++){a.x; let b:A=new A(); a=b;}}", true);
}

/// Cost: four checker programs; no engine subprocess.
#[test]
fn loop_body_local_store_does_not_end_outer_fact() {
    // A store to the outer binding must still end its non-null fact, including
    // when a different local's name starts with the outer binding's name.
    for body in ["x=c;", "let xx:A|null=null; x=c;"] {
        check(&format!("export function main():void{{let x:A|null=new A(); let c:A|null=mk(); for(let i:i32=0;i<2;i++){{x.x; {{{body}}}}}}}"), false);
    }
    for body in ["let x:A|null=null; x=c;", "let x=new H(); x.f=c;"] {
        check(&format!("export function main():void{{let x:A|null=new A(); let c:A|null=mk(); for(let i:i32=0;i<2;i++){{x.x; {{{body}}}}}}}"), true);
    }
}

/// Cost: 16 checker programs; no engine subprocess.
#[test]
fn field_stores_keep_only_non_null_terminal_facts() {
    for (value, accepted) in [("new A()", true), ("mk()", false), ("null", false)] {
        for receiver in ["h", "k"] {
            for looped in [false, true] {
                let body = if looped {
                    format!("for(let i:i32=0;i<2;i++){{h.f.x; {receiver}.f={value};}}")
                } else {
                    format!("if(h.f!==null){{{receiver}.f={value}; h.f.x;}}")
                };
                check(&format!("export function main():void{{const h=new H(); const k=new H(); h.f=new A(); {body}}}"), accepted);
            }
        }
    }
    for (value, accepted) in [("new A()", true), ("mk()", false)] {
        check(&format!("export function main():void{{const h=new H(); const k=new H(); h.f=new A(); let a:A|null=new A(); for(let i:i32=0;i<2;i++){{h.f.x; k.f=a; a={value};}}}}"), accepted);
    }
    // A non-null replacement can still end a fact through the replaced field.
    check("export function main():void{const h=new H(); const k=h; h.f=new A(); h.f.f=new A(); k.f=new A(); h.f.f.x;}", false);
    check("export function main():void{const h=new H(); const k=h; h.f=new A(); h.f.f=new A(); k.g=new A(); h.f.f.x;}", true);
}

/// Cost: 0.01 seconds for 12 checker programs; no engine subprocess.
#[test]
fn only_module_const_values_keep_facts_across_other_code() {
    for (binding, accepted) in [("const", true), ("let", false)] {
        for body in [
            "other(); G.x;",
            "const g=():i32=>G.x; g();",
            "const h=new H(); h.f=mk(); G.x;",
        ] {
            check(&format!("{binding} G:A|null=mk(); export function main():void{{if(G===null){{return;}} {body}}}"), accepted);
        }
        check(&format!("{binding} G:A|null=mk(); async function tick():Promise<void>{{}} async function read():Promise<i32>{{if(G===null){{return 0;}} await tick(); return G.x;}} export function main():void{{}}"), accepted);
        check(
            &format!(
                "{binding} G:A|null=mk(); export function main():void{{const g=():i32=>G.x;}}"
            ),
            false,
        );
    }
    check(
        "const G=new H(); export function main():void{if(G.f===null){return;} other(); G.f.x;}",
        false,
    );
    check("class S{static f:A|null=null;} export function main():void{if(S.f===null){return;} other(); S.f.x;}", false);
}

/// Cost: less than 0.01 seconds for two checker programs; no engine subprocess.
#[test]
fn generic_field_store_uses_the_apparent_value_type() {
    for (constraint, accepted) in [("A", true), ("A|null", false)] {
        check(&format!("function store<T extends {constraint}>(h:H,k:H,p:T):void{{if(h.f===null){{return;}} k.f=p; h.f.x;}} export function main():void{{}}"), accepted);
    }
}
