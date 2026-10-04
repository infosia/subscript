//! Local assignment paths and initialized controls (§158).
//! Measured cost: one checker table and one tsc batch, 0.32 s; no native compile.
//! The storage and function-owner checks add less than 0.01 s.

use std::{collections::BTreeMap, fs, path::PathBuf, process::Command, time::Instant};
use subscript_compiler::{check_program, hir, SourceFile};

#[path = "support/tsc.rs"]
mod tsc;

/// One batch checks each path against stock tsc and the initialized control.
#[test]
fn assignment_paths_and_controls() {
    let started = Instant::now();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let temporary = std::env::temp_dir().join(format!("subscript-s158-{}", std::process::id()));
    fs::create_dir_all(&temporary).unwrap();
    let rows = [
        (
            "case-test-write",
            "switch(n()) { case (x=1):break; case x:break; } x=2;",
            false,
        ),
        (
            "case-test-default-first",
            "switch(n()) { default:print(`${x}`);break; case (x=1):break; } x=2;",
            false,
        ),
        (
            "case-test",
            "if(n()>5) { x=1; } switch(n()-1) { case x:break; } x=2;",
            false,
        ),
        (
            "case-test-assigned",
            "x=1; switch(n()) { case x:break; }",
            true,
        ),
        (
            "case-test-after-body",
            "switch(n()) { case 0:x=1;break; case x:break; } x=2;",
            false,
        ),
        ("for-infinite", "for(;;) { x=1;break; }", true),
        ("for-true", "for(;true;) { x=1;break; }", true),
        (
            "for-infinite-break",
            "for(;;) { if(c()) break; x=1;break; }",
            false,
        ),
        (
            "for-true-break",
            "for(;true;) { if(c()) break; x=1;break; }",
            false,
        ),
        (
            "switch-alias",
            "const a:A=c()?\"a\":\"b\"; switch(a) { case \"a\":x=1;break; case \"b\":x=2;break; }",
            true,
        ),
        (
            "switch-alias-break",
            "const a:A=c()?\"a\":\"b\"; switch(a) { case \"a\":break; case \"b\":x=2;break; }",
            false,
        ),
        ("for-shadow", "for(let x:i32=0;x<2;x++) {}", false),
        ("for-of-shadow", "for(const x of [1,2]) {}", false),
        (
            "switch-shadow",
            "switch(n()) { default: let x:i32=1; print(`${x}`); }",
            false,
        ),
        (
            "catch-shadow",
            "try {} catch(x) { if(x instanceof Error) { print(x.message); } }",
            false,
        ),
        ("chain", "let y:i32; x=y=1; print(`${y}`);", true),
        ("if-both", "if (c()) { x=1; } else { x=2; }", true),
        ("if-one", "if (c()) { x=1; }", false),
        (
            "throw",
            "if (c()) { x=1; } else { throw new Error(\"stop\"); }",
            true,
        ),
        ("return", "if (c()) { x=1; } else { return; }", true),
        ("never", "if (c()) { x=1; } else { unreachable(); }", true),
        (
            "switch",
            "switch(n()) { case 0:x=1;break; default:x=2;break; }",
            true,
        ),
        ("switch-missing", "switch(n()) { case 0:x=1;break; }", false),
        (
            "switch-fallthrough",
            "switch(n()) { case 0: case 1:x=1;break; default:x=2; }",
            true,
        ),
        (
            "switch-break",
            "switch(n()) { case 0:break; default:x=2; }",
            false,
        ),
        (
            "switch-return",
            "switch(n()) { case 0:return; default:x=2; }",
            true,
        ),
        ("try-catch", "try { x=1; } catch { x=2; }", true),
        ("try-only", "try { x=1; } catch {}", false),
        ("while-true", "while(true) { x=1;break; }", true),
        (
            "while-break",
            "while(true) { if(c()) { break; } x=1;break; }",
            false,
        ),
        (
            "while-continue",
            "while(true) { if(c()) { continue; } x=1;break; }",
            true,
        ),
        (
            "while-return",
            "while(true) { if(c()) { return; } x=1;break; }",
            true,
        ),
        ("while-zero", "while(c()) { x=1; }", false),
        ("while-false", "while(false) { print(`${x}`); } x=1;", true),
        ("for", "for(let i:i32=0;i<2;i++) { x=1; }", false),
        ("for-of", "for(const i of [1,2]) { x=i; }", false),
        ("conditional", "x=c()?1:2; x+=1;", true),
        ("compound", "x+=1;", false),
        ("prefix-increment", "++x;", false),
        ("postfix-increment", "x++;", false),
        ("prefix-decrement", "--x;", false),
        ("postfix-decrement", "x--;", false),
        ("self-assignment", "x=x;", false),
        ("rhs-read", "x=x+1;", false),
        ("short-circuit", "c() && ((x=1)===1);", false),
        ("conditional-one", "c() ? (x=1) : 2;", false),
        ("conditional-both", "c() ? (x=1) : (x=2);", true),
        ("block", "{ x=1; }", true),
        ("shadow", "{ let x:i32=1; x=2; }", false),
        (
            "shadow-after-assignment",
            "x=1; { let x:i32=2; print(`${x}`); }",
            true,
        ),
        (
            "nested-switch-break",
            "while(true) { switch(n()) { default: break; } x=1;break; }",
            true,
        ),
        (
            "read-in-condition",
            "if(x===0) { x=1; } else { x=2; }",
            false,
        ),
        (
            "read-in-body",
            "while(c()) { print(`${x}`); x=1; } x=2;",
            false,
        ),
        (
            "read-in-step",
            "for(let i:i32=0;i<2;x++) { i++; } x=2;",
            false,
        ),
    ];
    let mut files = vec![root.join("prelude/lang.d.ts")];
    let mut expected = BTreeMap::new();
    for (name, statements, accepts) in rows {
        for initialized in [false, true] {
            let key = format!(
                "{name}-{}.ts",
                if initialized { "control" } else { "subject" }
            );
            let initializer = if initialized { "=0" } else { "" };
            let source = format!("type A=\"a\"|\"b\"; function c():boolean {{ return true; }} function n():i32 {{ return 0; }}\nexport function main():void {{ let x:i32{initializer}; {statements} print(`${{x}}`); }}");
            let result = check_program(&[SourceFile::entry(&key, &source)]);
            let accepted = initialized || accepts;
            if accepted {
                result.unwrap_or_else(|d| panic!("{key}: {d:?}"));
            } else {
                let diagnostics = result.unwrap_err();
                assert!(
                    diagnostics
                        .iter()
                        .any(|d| d.message.starts_with("local `x` is read before assignment")),
                    "{key}: {diagnostics:?}"
                );
                assert!(
                    diagnostics.iter().all(|d| d.divergence.is_none()),
                    "{key}: {diagnostics:?}"
                );
            }
            let path = temporary.join(&key);
            fs::write(&path, source).unwrap();
            files.push(path);
            expected.insert(key, accepted);
        }
    }
    let field_rows = [
        (
            "field-case-test-write",
            "switch(n()) { case (this.x=1):break; case this.x:break; } this.x=2;",
            false,
        ),
        (
            "field-case-test",
            "if(n()>5) { this.x=1; } switch(n()-1) { case this.x:break; } this.x=2;",
            false,
        ),
        (
            "field-case-test-assigned",
            "this.x=1; switch(n()) { case this.x:break; }",
            true,
        ),
        (
            "field-case-test-after-body",
            "switch(n()) { case 0:this.x=1;break; case this.x:break; } this.x=2;",
            false,
        ),
    ];
    for (name, statements, accepts) in field_rows {
        for initialized in [false, true] {
            let key = format!(
                "{name}-{}.ts",
                if initialized { "control" } else { "subject" }
            );
            let initializer = if initialized { "=0" } else { "" };
            let source = format!("export {{}}; function n():i32 {{ return 0; }} class C {{ x:i32{initializer}; constructor() {{ {statements} }} }}");
            let result = check_program(&[SourceFile::entry(&key, &source)]);
            if initialized || accepts {
                result.unwrap_or_else(|d| panic!("{key}: {d:?}"));
            } else {
                let diagnostics = result.unwrap_err();
                assert!(
                    diagnostics
                        .iter()
                        .any(|d| d.message.contains("field `x`") && d.divergence.is_none()),
                    "{key}: {diagnostics:?}"
                );
            }
            let path = temporary.join(&key);
            fs::write(&path, source).unwrap();
            files.push(path);
            expected.insert(key, initialized || accepts);
        }
    }
    let form_count = expected.len();
    let config = temporary.join("tsconfig.json");
    fs::write(&config, tsc::tsconfig(&files)).unwrap();
    let output = Command::new(tsc::tsc_binary(&root))
        .args(["--pretty", "false", "--project"])
        .arg(&config)
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let mut rejected = BTreeMap::new();
    for line in text.lines() {
        let (file, _) = line
            .split_once('(')
            .unwrap_or_else(|| panic!("unexpected tsc output: {line}"));
        let key = PathBuf::from(file)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            line.contains("error TS2454:") || line.contains("error TS2565:"),
            "{line}"
        );
        rejected.insert(key, true);
    }
    assert_eq!(output.status.code(), Some(2), "{text}");
    for (key, accepted) in expected {
        assert_eq!(!rejected.contains_key(&key), accepted, "{key}: {text}");
    }
    fs::remove_dir_all(temporary).unwrap();
    eprintln!(
        "s158: {form_count} checker forms and one tsc batch: {:.3}s",
        started.elapsed().as_secs_f64()
    );
}

#[test]
fn storage_marker_preserves_the_annotated_type() {
    let checked = check_program(&[SourceFile::entry("main.ts", "class C { n:i32=1; } export function main():void { let n:i32; let f:f64; let s:string; let c:C; let q:C|null; }")]).unwrap();
    let main = checked.functions.iter().find(|f| f.name == "main").unwrap();
    assert_eq!(main.body.len(), 5);
    for statement in &main.body {
        let hir::Stmt::Let { ty, init, .. } = statement else {
            panic!("{statement:?}");
        };
        assert!(matches!(init.kind, hir::ExprKind::Unassigned));
        assert_eq!(ty, &init.ty);
        assert!(init.children().is_empty());
    }
}

#[test]
fn all_function_owners_check_their_local_reads() {
    for source in [
        "{ let x:i32; print(`${x}`); }",
        "function id<T>(v:T):T { let x:T; return x; }",
        "class G<T> { id(v:T):T { let x:T; return x; } }",
        "class C { id<T>(v:T):T { let x:T; return x; } }",
        "class C { constructor() { let x:i32; print(`${x}`); } }",
        "class C { m():void { let x:i32; print(`${x}`); } }",
        "const f=():void => { let x:i32; print(`${x}`); };",
        "class C { f:()=>void=():void => { let x:i32; print(`${x}`); }; }",
        "function f(cb:()=>void=():void => { let x:i32; print(`${x}`); }):void {}",
        "async function f():Promise<void> { let x:i32; print(`${x}`); }",
        "function *f():Generator<i32> { let x:i32; yield x; }",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.starts_with("local `x` is read before assignment")),
            "{source}: {diagnostics:?}"
        );
        let control = source
            .replace("let x:i32;", "let x:i32=0;")
            .replace("let x:T;", "let x:T=v;");
        check_program(&[SourceFile::entry("main.ts", control)]).unwrap();
    }
}

#[test]
fn an_assigned_let_keeps_the_c5_capture_rejection() {
    for declaration in ["let x:i32; x=1;", "let x:i32=1;"] {
        let source = format!("export function main():void {{ {declaration} const read=():i32=>x; print(`${{read()}}`); }}");
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].divergence,
            Some(subscript_compiler::divergence::Divergence::MutableLocalCapture)
        );
    }
}

#[test]
fn a_rejected_annotation_poisons_the_uninitialized_local() {
    for ty in ["Missing", "number", "any"] {
        let source = format!("export function main():void {{ let x:{ty}; print(`${{x}}`); }}");
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{ty}: {diagnostics:?}");
        assert!(!diagnostics[0]
            .message
            .starts_with("local `x` is read before assignment"));
    }
}

#[test]
fn generic_local_reads_report_once_with_instances() {
    for source in [
        "function id<T>(v:T):T { let x:T; return x; } export function main():void { id<i32>(1); id<f64>(2.0); }",
        "class G<T> { id(v:T):T { let x:T; return x; } } export function main():void { new G<i32>().id(1); new G<f64>().id(2.0); }",
        "class C { id<T>(v:T):T { let x:T; return x; } } export function main():void { const c=new C(); c.id<i32>(1); c.id<f64>(2.0); }",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert!(diagnostics[0].message.starts_with("local `x` is read before assignment"));
        assert!(diagnostics[0].divergence.is_none());
        let control = source.replace("let x:T;", "let x:T=v;");
        check_program(&[SourceFile::entry("main.ts", control)]).unwrap();
    }
}
