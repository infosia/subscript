//! Section 157 controls. Cost: four in-process tests measured 0.06 seconds.

use crate::{check_program, SourceFile};

fn accept(source: &str) {
    check_program(&[SourceFile::entry("main.ts", source)]).expect("accepted control");
}

fn reject(source: &str, message: &str) {
    let errors = check_program(&[SourceFile::entry("main.ts", source)]).expect_err("rejected form");
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "{errors:?}"
    );
}

#[test]
fn receiver_capture_uses_const_environment_slots() {
    let module = check_program(&[SourceFile::entry(
        "main.ts",
        include_str!("../../../corpus/accept/a330-this-capture.ts"),
    )])
    .unwrap();
    fn captures(node: crate::hir::HirChild<'_>, count: &mut usize) {
        match node {
            crate::hir::HirChild::Expr(expression) => {
                if let crate::hir::ExprKind::Lambda { captures, .. } = &expression.kind {
                    assert_eq!(captures.len(), 1);
                    assert_eq!(captures[0].name, "this");
                    *count += 1;
                }
                for child in expression.children() {
                    captures(child, count);
                }
            }
            crate::hir::HirChild::Stmt(statement) => {
                for child in statement.children() {
                    captures(child, count);
                }
            }
        }
    }
    let mut count = 0;
    for class in &module.classes {
        for method in class.methods.iter().chain(class.ctor.iter()) {
            for statement in &method.body {
                captures(crate::hir::HirChild::Stmt(statement), &mut count);
            }
        }
    }
    assert!(count >= 9, "{count}");
    accept("class C { n:i32=1; f():i32 { const self=this; const read=():i32=>self.n; return read(); } } export function main():void{}");
}

#[test]
fn capture_escape_has_clean_controls() {
    for (body, control) in [
        ("return ():i32=>this.n;", "return ():i32=>1;"),
        (
            "this.cb=():i32=>this.n; return this.cb;",
            "this.cb=():i32=>1; return this.cb;",
        ),
        (
            "const xs:(()=>i32)[]=[():i32=>this.n]; return xs[0];",
            "const xs:(()=>i32)[]=[():i32=>1]; return xs[0];",
        ),
    ] {
        let source = format!("class C {{ n:i32=1; cb:()=>i32=():i32=>0; f():()=>i32 {{ {body} }} }} export function main():void{{}}");
        reject(&source, "captur");
        accept(&source.replace(body, control));
    }
    let source = "class C { n:i32=1; install():void { new SubCallbackInfo((message, userdata, userparam):void=>{print(`${this.n}`);}, null, null); } } export function main():void{}";
    let mirror = SourceFile::ambient(
        "interop.generated.d.ts",
        include_str!("../../../corpus/interop/interop.generated.d.ts"),
    );
    let diagnostics = check_program(&[mirror.clone(), SourceFile::entry("main.ts", source)])
        .expect_err("C callback capture");
    assert!(diagnostics
        .iter()
        .any(|error| error.message.contains("aggregate constructor")));
    check_program(&[
        mirror,
        SourceFile::entry("main.ts", source.replace("${this.n}", "clean")),
    ])
    .expect("clean C callback control");
    reject("class C { n:i32=1; f():()=>i32 { const outer=():()=>i32=>():i32=>this.n; return outer(); } } export function main():void{}", "captur");
}

#[test]
fn constructor_calls_share_the_member_use_fact() {
    for invocation in [
        "read();",
        "consume(read);",
        "consume(():i32=>this.n);",
        "[1].map((v:i32):i32=>this.n+v);",
        "const alias=read; alias();",
        "const wrapper=():i32=>read(); wrapper();",
    ] {
        let source = format!("function consume(f:()=>i32):void{{ f(); }} class C {{ n:i32; constructor(){{ const read=():i32=>this.n; {invocation} this.n=1; }} }} export function main():void{{new C();}}");
        reject(&source, "calls a member of `this`");
        accept(&source.replace(
            &format!("{invocation} this.n=1;"),
            &format!("this.n=1; {invocation}"),
        ));
    }
    accept("class C { n:i32; constructor(){ const read=():i32=>this.n; this.n=1; read(); } } export function main():void{new C();}");
    let prefix = r#"function fail():void { throw new Error("x"); } "#;
    for body in [
        "let cb:()=>i32=():i32=>this.n; try { fail(); cb=():i32=>1; } catch { cb(); }",
        "let cb:()=>i32=():i32=>1; try { cb=():i32=>this.n; fail(); cb=():i32=>1; } catch { cb(); }",
    ] {
        reject(&format!("{prefix} class C {{ n:i32; constructor(){{ {body} this.n=1; }} }} export function main():void{{}}"), "calls a member of `this`");
    }
    for body in [
        "let cb:()=>i32=():i32=>this.n; try { cb=():i32=>1; fail(); } catch { cb(); }",
        "let cb:()=>i32=():i32=>1; try { { const cb=():i32=>this.n; fail(); } } catch { cb(); }",
        r#"let cb:()=>i32=():i32=>this.n; try { throw new Error("first"); } catch {} cb=():i32=>1; try { throw new Error("second"); } catch { cb(); }"#,
        r#"let cb:()=>i32=():i32=>this.n; try { try { throw new Error("first"); } catch {} cb=():i32=>1; fail(); } catch { cb(); }"#,
    ] {
        accept(&format!("{prefix} class C {{ n:i32; constructor(){{ {body} this.n=1; }} }} export function main():void{{}}"));
    }
    let source="class C { n:i32; constructor(read:()=>i32=():i32=>this.n){ read(); this.n=1; } } export function main():void{}";
    reject(source, "parameter default cannot capture `this`");
    accept(&source.replace("=>this.n", "=>1"));
    for replacement in [
        "cb=():i32=>1;",
        "{ cb=():i32=>1; }",
        "if (true) { cb=():i32=>1; } else { cb=():i32=>2; }",
    ] {
        accept(&format!("class C {{ n:i32; constructor(){{ let cb:()=>i32=():i32=>this.n; {replacement} cb(); this.n=1; }} }} export function main():void{{}}"));
    }
    reject("class C { n:i32; constructor(){ let cb:()=>i32=():i32=>this.n; if (true) { cb=():i32=>1; } cb(); this.n=1; } } export function main():void{}", "calls a member of `this`");
    reject("class C { n:i32; constructor(){ let cb:()=>i32=():i32=>1; for (let i:i32=0; i<2; i++) { cb(); cb=():i32=>this.n; } this.n=1; } } export function main():void{}", "calls a member of `this`");
    accept("class C { n:i32; constructor(){ const read=():i32=>1; { const read=():i32=>this.n; } read(); this.n=1; } } export function main():void{new C();}");
    accept("class C { n:i32; constructor(){ const read=():i32=>1; if (true) { const read=():i32=>this.n; } read(); this.n=1; } } export function main():void{new C();}");
    accept("class C { n:i32; constructor(){ const read=():i32=>1; read(); this.n=1; } } export function main():void{new C();}");
}

#[test]
fn rejected_receiver_contexts_have_controls() {
    let source="@ValueType class C { n:i32=1; f():i32 { const read=():i32=>this.n; return read(); } } export function main():void{}";
    reject(source, "ValueType receiver");
    accept(&source.replace("@ValueType ", ""));
    let source="class C { static n:i32=1; static f():i32 { const read=():i32=>this.n; return read(); } } export function main():void{}";
    reject(source, "static method");
    accept(&source.replace("=>this.n", "=>C.n"));
    for modifier in ["", "static "] {
        let source=format!("class C {{ {modifier}n:i32=1; {modifier}value:i32=(():i32=>this.n)(); }} export function main():void{{}}");
        reject(&source, "this");
        accept(&source.replace("=>this.n", "=>1"));
    }
    for declaration in [
        "m(k=()=>this.n):i32 { return k(); }",
        "m(k:()=>i32=():i32=>this.n):i32 { return k(); }",
        "m(k=take(():i32=>this.n)):i32 { return k; }",
        "m():i32 { const f=(k:()=>i32=():i32=>this.n):i32=>k(); return f(():i32=>1); }",
        "constructor(k=()=>this.n) { k(); }",
    ] {
        let source = format!("function take(f:()=>i32):i32 {{ return f(); }} class C {{ n:i32=1; {declaration} }} export function main():void {{}}");
        let errors = check_program(&[SourceFile::entry("main.ts", &source)]).unwrap_err();
        assert_eq!(errors[0].code.as_str(), "S009");
        assert_eq!(
            errors[0].divergence,
            Some(crate::divergence::Divergence::ThisInParameterDefaultArrow)
        );
        accept(&source.replace("=>this.n", "=>1"));
    }
    accept("class C { n:i32=1; m(k:()=>i32=():i32=>1):i32 { const read=():i32=>this.n; return read()+k(); } } export function main():void {}");
    let source="class C { n:i32=1; f():void { const read=function():i32 { return 1; }; read(); } } export function main():void{}";
    reject(source, "function expressions");
    accept(&source.replace("function():i32", "():i32 =>"));
}
