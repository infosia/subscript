//! Negation, loop exits, loop heads, and const captures (§162).
use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

fn check(body: &str, accepted: bool) {
    let source = format!("class A {{ x: i32 = 1; f: A | null = null; }}\nfunction mk(): A | null {{ return new A(); }}\nfunction cond(): boolean {{ return true; }}\n{body}");
    let result = check_program(&[SourceFile::new("test.ts", source)]);
    if accepted {
        assert!(result.is_ok(), "{body}: {result:?}");
    } else {
        let errors = result.expect_err(body);
        assert!(!errors.is_empty());
        assert!(
            errors.iter().any(|d| d.code == RuleCode::S011),
            "{body}: {errors:?}"
        );
    }
}

#[test]
fn negation_exchanges_facts_at_each_depth() {
    for (condition, accepted) in [
        ("!(a === null)", true),
        ("!(a !== null)", false),
        ("!!(a !== null)", true),
        ("!!(a === null)", false),
        ("!(a === null || b === null)", true),
        ("!(a === null && b === null)", false),
    ] {
        check(&format!("function read(a: A | null, b: A | null): void {{ if ({condition}) {{ a.x; b.x; }} }}").replace("a.x; b.x;", if condition.contains('b') { "a.x; b.x;" } else { "a.x;" }), accepted);
    }
    check(
        "function read(a: A | null): i32 { if (!(a !== null)) { return 0; } return a.x; }",
        true,
    );
    check(
        "function read(a: A | null): i32 { if (!(a === null)) { return 0; } return a.x; }",
        false,
    );
    for (op, accepted) in [("===", true), ("!==", false)] {
        check(&format!("function read(): void {{ let a: A | null = null; if (!((a = mk()) {op} null)) {{ a.x; }} }}"), accepted);
    }
}

#[test]
fn loop_exits_intersect_condition_and_break_facts() {
    for (body, accepted) in [
        ("a = new A(); if (cond()) { break; }", true),
        ("if (cond()) { break; } a = new A();", false),
        ("while (true) { break; } a = new A(); break;", true),
        (
            "while (true) { a = new A(); break; } if (cond()) { a = null; break; } break;",
            false,
        ),
        ("if (a !== null) { break; } a = mk();", true),
        ("if (cond()) { break; } a = mk();", false),
    ] {
        check(
            &format!("function read(a: A | null): void {{ for (;;) {{ {body} }} a.x; }}"),
            accepted,
        );
    }
    for (condition, accepted) in [("a === null", true), ("cond()", false)] {
        for loop_kind in ["while", "for"] {
            let head = if loop_kind == "while" {
                format!("while ({condition})")
            } else {
                format!("for (; {condition};)")
            };
            check(
                &format!("function read(a: A | null): void {{ {head} {{ a = mk(); }} a.x; }}"),
                accepted,
            );
        }
    }
    check("function read(a: A | null): void { while (a === null) { if (cond()) { break; } a = mk(); } a.x; }", false);
    check(
        "function read(a: A | null): void { while (true) { break; a = new A(); break; } a.x; }",
        false,
    );
    check("function read(a: A | null): void { while (true) { const a: A | null = new A(); break; } a.x; }", false);
    check("function read(a: A | null): void { if (a === null) { return; } while (true) { const a: A | null = null; break; } a.x; }", true);
    check("function read(a: A | null): void { while (true) { switch (1) { case 1: break; } a = new A(); break; } a.x; }", true);
}

#[test]
fn whole_local_non_null_stores_preserve_loop_head_facts() {
    for (store, accepted) in [("new A()", true), ("mk()", false)] {
        check(&format!("function read(): void {{ let e: A | null = new A(); for (let i: i32 = 0; i < 2; i++) {{ e.x; e = {store}; }} }}"), accepted);
        check(&format!("function read(): void {{ let e: A | null = new A(); while (cond()) {{ e.x; while (cond()) {{ e = {store}; continue; }} continue; }} }}"), accepted);
        check(&format!("function read(): void {{ let e: A | null = new A(); for (; cond(); e = {store}) {{ e.x; continue; }} }}"), accepted);
    }
}

#[test]
fn loop_heads_follow_the_store_order() {
    for (stores, accepted) in [
        ("e = mk(); e = new A();", true),
        ("e = new A(); e = mk();", false),
    ] {
        check(&format!("function read(): void {{ let e: A | null = new A(); while (cond()) {{ e.x; {stores} }} }}"), accepted);
    }
    check("function read(): void { let e = new A(); e.f = new A(); while (true) { e.f.x; e = new A(); break; } }", false);
    check(
        "function read(): void { let e = new A(); e.f = new A(); while (true) { e.f.x; break; } }",
        true,
    );
}

#[test]
fn only_nullable_same_name_field_stores_end_the_loop_head_fact() {
    // h is the recorded store-order gap.
    for (receiver, divergence) in [
        ("h", None),
        ("alias", Some(Divergence::SharedLocationNarrowing)),
    ] {
        let source = format!("class A {{ x: i32 = 1; f: A | null = null; }} function read(): void {{ const h = new A(); const alias = h; h.f = new A(); while (true) {{ h.f.x; {receiver}.f = null; break; }} }}");
        let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(errors[0].divergence, divergence);
        check(&format!("function read(): void {{ const h = new A(); const alias = h; h.f = new A(); while (true) {{ h.f.x; {receiver}.f = new A(); break; }} }}"), true);
    }
    check("function read(): void { let h: A | null = new A(); while (true) { h.x; h = new A(); break; } }", true);
}

#[test]
fn lambdas_keep_only_whole_const_local_facts() {
    for (binding, accepted) in [("const", true), ("let", false)] {
        for body in ["(): i32 => a.x", "(): i32 => { return a.x; }"] {
            check(&format!("function read(): void {{ {binding} a = mk(); if (a === null) {{ return; }} const g = {body}; }}"), accepted);
        }
    }
    check(
        "function read(): void { const a: A | null = new A(); const g = (): i32 => a.x; }",
        true,
    );
    check(
        "function read(): void { const a = mk(); const g = (): i32 => a.x; }",
        false,
    );
    check(
        "function read(): void { const h = new A(); h.f = new A(); const g = (): i32 => h.f.x; }",
        false,
    );
    check("function read(): void { const h = new A(); const a = h.f; if (a === null) { return; } const g = (): i32 => a.x; }", true);
    check("function read(): void { const a = mk(); if (a === null) { return; } const g = (a: A | null): i32 => a.x; }", false);
}

#[test]
fn a_for_update_sees_every_continue_edge() {
    for (assignment, accepted) in [("a = new A();", true), ("", false)] {
        check(&format!("function read(a: A | null): void {{ for (; cond(); a.x) {{ {assignment} if (cond()) {{ continue; }} a = new A(); }} }}"), accepted);
    }
}

#[test]
fn catch_breaks_join_the_loop_exits() {
    for (assignment, accepted) in [("a = new A();", true), ("", false)] {
        check(&format!("function read(a: A | null): void {{ while (true) {{ try {{ if (cond()) {{ throw new Error(\"x\"); }} a = new A(); break; }} catch (e) {{ {assignment} break; }} }} a.x; }}"), accepted);
    }
}

#[test]
fn loop_exits_keep_the_shared_end_diagnostic() {
    let source = "class A { x: i32 = 1; f: A | null = null; } function touch(): void {} function read(): void { const h = new A(); while (true) { if (h.f === null) { return; } touch(); break; } h.f.x; }";
    let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S011);
    assert_eq!(
        errors[0].divergence,
        Some(Divergence::SharedLocationNarrowing)
    );
    check("function read(): void { const h = new A(); while (true) { if (h.f === null) { return; } break; } h.f.x; }", true);
}

#[test]
fn for_of_exits_restore_the_outer_binding() {
    for (value, accepted) in [("new A()", true), ("mk()", false)] {
        check(&format!("function read(): void {{ let a: A | null = {value}; const xs: i32[] = [1]; for (const a of xs) {{ break; }} a.x; }}"), accepted);
    }
}

#[test]
fn loop_store_values_use_declared_types_without_facts() {
    for (value, accepted) in [
        ("b", false),
        ("new A()", true),
        ("(b)", false),
        ("cond() ? b : new A()", false),
        ("cond() ? new A() : new A()", true),
        ("(c = b)", false),
        ("(c = new A())", true),
    ] {
        for head in [
            "for (let i: i32 = 0; i < 3; i++)",
            "while (cond())",
            "for (const i of xs)",
        ] {
            check(&format!("function read(): void {{ let a: A | null = new A(); let b: A | null = new A(); let c: A | null = new A(); const xs: i32[] = [1, 2, 3]; {head} {{ a.x; a = {value}; b = null; }} }}"), accepted);
        }
        check(&format!("function read(): void {{ let a: A | null = new A(); let b: A | null = new A(); let c: A | null = new A(); while (cond()) {{ a.x; while (cond()) {{ a = {value}; b = null; continue; }} }} }}"), accepted);
    }
    for (value, accepted) in [("b", false), ("new A()", true)] {
        check(&format!("let b: A | null = new A(); function read(): void {{ if (b === null) {{ return; }} let a: A | null = new A(); for (let i: i32 = 0; i < 3; i++) {{ a.x; a = {value}; b = null; }} }}"), accepted);
    }
    check("function read(): void { const b: A = new A(); let a: A | null = new A(); while (cond()) { a.x; a = b; } }", true);
    check("const b = new A(); function read(): void { let a: A | null = new A(); while (cond()) { a.x; a = b; } }", true);
    check("function read(): void { const h = new A(); h.f = new A(); let a: A | null = new A(); while (cond()) { a.x; a = h.f; h.f = null; } }", false);
}

#[test]
fn loop_store_forms_require_annotation_provenance() {
    for (store, accepted) in [
        ("const c = b; a = c;", false),
        ("let c = b; a = c;", false),
        ("a = id(b);", false),
        ("a = [b][0];", false),
        ("const arr = [b]; a = arr[0];", false),
        ("for (const c of [b]) { a = c; }", false),
        ("const c: A = new A(); a = c;", true),
        ("a = p;", true),
        ("const c = new A(); a = c;", true),
    ] {
        check(&format!("function id<T>(v: T): T {{ return v; }} function read(p: A): void {{ let a: A | null = new A(); let b: A | null = new A(); for (let i: i32 = 0; i < 3; i++) {{ a.x; {store} b = null; }} }}"), accepted);
    }
}

#[test]
fn loop_store_parameters_require_written_non_generic_types() {
    for (parameter, elements, accepted) in [
        ("v", "b", false),
        ("v: A", "new A()", true),
        ("v = b", "b", false),
        ("v: A = new A()", "new A()", true),
    ] {
        check(&format!("function read(): void {{ let b: A | null = new A(); for (let i = 0; i < 2; i++) {{ const xs = [{elements}]; xs.forEach(({parameter}): void => {{ let a: A | null = new A(); for (let j = 0; j < 2; j++) {{ a.x; a = v; }} }}); b = null; }} }}"), accepted);
    }
    for (parameter, accepted) in [("p: T", false), ("p: A", true)] {
        for argument in ["A", "A | null"] {
            check(&format!("function g<T extends A | null>({parameter}, q: A): void {{ let a: A | null = q; for (let j = 0; j < 2; j++) {{ a.x; a = p; }} }} function read(): void {{ g<{argument}>(new A(), new A()); }}"), accepted);
        }
    }
}

#[test]
fn back_edges_use_the_checked_generic_field_type() {
    for (field, ty, accepted) in [("v: T", "T", true), ("v: A", "A", true)] {
        check(&format!("class Box<T> {{ {field}; constructor(v: {ty}) {{ this.v = v; }} }} function read(): void {{ const box = new Box<A>(new A()); let a: A | null = new A(); for (let j = 0; j < 2; j++) {{ a.x; a = box.v; }} }}"), accepted);
    }
}

/// Measured cost: 0.015 seconds for 10 checker programs; no engine subprocess.
#[test]
fn back_edges_include_only_continues_of_this_loop() {
    let started = std::time::Instant::now();
    for (restore, accepted) in [("e = new A();", true), ("", false)] {
        check(&format!("function read(e: A | null): void {{ if(e === null) {{ return; }} for(let i=0;i<3;i++) {{ e.x; e=mk(); {{ if(i===1) {{ {restore} continue; }} }} e=new A(); }} }}"), accepted);
        check(&format!("function read(e: A | null): void {{ if(e === null) {{ return; }} while(cond()) {{ e.x; e=mk(); {restore} }} }}"), accepted);
        check(&format!("function read(e: A | null): void {{ if(e === null) {{ return; }} for(const i of [1,2,3]) {{ e.x; e=mk(); if(i===1) {{ {restore} continue; }} {restore} }} }}"), accepted);
        check(&format!("function read(e: A | null): void {{ if(e === null) {{ return; }} for(;cond();e=mk()) {{ e.x; {restore} continue; }} }}"), false);
    }
    check("function read(e: A | null): void { if(e === null) { return; } for(;cond();e=new A()) { e.x; e=mk(); {continue;} } }", true);
    check("function read(e: A | null): void { if(e===null){return;} for(let i=0;i<3;i++){ e.x; e=mk(); for(let j=0;j<2;j++){ if(j===1){continue;} } e=new A(); } }", true);
    eprintln!("s315 edge forms: {:?}", started.elapsed());
}

/// Measured cost: 0.004 seconds for three checker programs; no engine subprocess.
#[test]
fn nested_loops_prove_their_own_back_edges() {
    let started = std::time::Instant::now();
    for (inner_restore, accepted) in [("e=new A();", true), ("", false)] {
        check(&format!("function read(e: A | null): void {{ if(e===null){{return;}} for(let i=0;i<3;i++){{ e.x; for(let j=0;j<2;j++){{ e.x; e=mk(); {inner_restore} }} e=new A(); }} }}"), accepted);
    }
    // The inner loop loses its fact. Its explicit test permits its read.
    check("function read(e: A | null): void { if(e===null){return;} for(let i=0;i<3;i++){ e.x; for(let j=0;j<2;j++){ if(e!==null){e.x;} e=mk(); } e=new A(); } }", true);
    eprintln!("s315 nested forms: {:?}", started.elapsed());
}

/// Measured cost: 0.001 seconds for one checker program; no engine subprocess.
#[test]
fn failed_induction_keeps_only_the_second_check_diagnostics() {
    let started = std::time::Instant::now();
    let source = "class A { x:i32=1; } function mk():A|null{return null;} function read(e:A|null):void{if(e===null){return;} while(true){ e.x; let copy=e; if(copy===null){} e=mk(); }}";
    let errors = check_program(&[SourceFile::new("retry.ts", source)]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S011);
    assert!(errors[0].note.is_some());
    eprintln!("s315 retry diagnostics: {:?}", started.elapsed());
}

/// Measured cost: 0.018 seconds for three checker programs; no engine subprocess.
#[test]
fn a_for_initializer_establishes_the_entry_fact() {
    let started = std::time::Instant::now();
    check(
        "function read():void { for(let e:A|null=new A();cond();){e.x; e=mk(); e=new A();} }",
        true,
    );
    check(
        "function read(e:A|null):void { for(e=new A();cond();){e.x; e=mk(); e=new A();} }",
        true,
    );
    check("function read(e:A|null):void { if(e===null){return;} for(e=mk();cond();){e.x; e=new A();} }", false);
    eprintln!("s315 initializer forms: {:?}", started.elapsed());
}

/// Measured cost: 0.004 seconds for two checker programs; no engine subprocess.
#[test]
fn a_for_of_subject_establishes_the_entry_fact() {
    let started = std::time::Instant::now();
    for (restore, accepted) in [("e=new A();", true), ("", false)] {
        check(&format!("function read(e:A|null):void {{ for(const i of [e=new A()]) {{ e.x; e=mk(); {restore} }} }}"), accepted);
    }
    eprintln!("s315 subject forms: {:?}", started.elapsed());
}

/// Measured cost: 0.005 seconds for two checker programs; no engine subprocess.
#[test]
fn a_retry_restores_generated_records_and_diagnostic_state() {
    let started = std::time::Instant::now();
    let source = "class A{x:i32=1;} class Box<T>{v:T;constructor(v:T){this.v=v;}} function mk():A|null{return null;} function cond():boolean{return true;} export function main():void {let e:A|null=new A(); while(cond()){const b=new Box<i32>(1); const f=():i32=>b.v; const r=/x/; print(`${f()} ${r.test(\"x\")}`); e=mk();}}";
    let module = check_program(&[SourceFile::entry("retry.ts", source)]).unwrap();
    let mut symbols = std::collections::HashSet::new();
    assert!(module
        .classes
        .iter()
        .all(|class| symbols.insert(class.symbol.clone())));
    symbols.clear();
    assert!(module
        .functions
        .iter()
        .all(|function| symbols.insert(function.symbol.clone())));
    assert_eq!(module.regex_literal_globals.len(), 1);
    let errors = check_program(&[
        SourceFile::entry("main.ts", "import type { A } from './lib'; function mk():A|null{return null;} function cond():boolean{return true;} export function main():void{let e:A|null=null; e=mk(); if(e===null){return;} while(cond()){new A();e=mk();}}"),
        SourceFile::new("lib.ts", "export class A{x:i32=1;}"),
    ]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("import type"), "{errors:?}");
    eprintln!("s315 retry state: {:?}", started.elapsed());
}

/// §315 acceptance 6: jumps carry each finalizer's stores in run order.
/// Cost: 0.045 seconds for 20 programs; no engine subprocess.
#[test]
fn jump_edges_include_finalizer_flow() {
    let started = std::time::Instant::now();
    for (store, accepted) in [("e=mk();", false), ("e=new A();", true)] {
        for body in [
            format!("for(let i=0;i<3;i++){{e.x; try{{if(i===0){{continue;}}}}finally{{{store}}} e=new A();}}"),
            format!("for(let i=0;i<3;i++){{try{{break;}}finally{{{store}}}}} e.x;"),
            format!("switch(k){{case 1:try{{break;}}finally{{{store}}}}} e.x;"),
            format!("for(let i=0;i<3;i++){{e.x; try{{try{{continue;}}finally{{e=new A();}}}}finally{{{store}}}}}"),
            format!("for(let i=0;i<3;i++){{try{{throw new Error();}}catch(err){{break;}}finally{{{store}}}}} e.x;"),
        ] {
            check(&format!("function read(e:A|null,k:i32):void{{if(e===null){{return;}} {body}}}"), accepted);
        }
    }
    // An outer finalizer cannot restore a fact ended by an inner finalizer.
    check("function read(e:A|null):void{if(e===null){return;} for(let i=0;i<3;i++){e.x; try{try{continue;}finally{e=mk();}}finally{e=new A();}}}", false);
    check("function read(e:A|null):void{if(e===null){return;} for(let i=0;i<3;i++){try{try{break;}finally{e=new A();}}finally{e=mk();}} e.x;}", false);
    check("function read(e:A|null,k:i32):void{if(e===null){return;} switch(k){case 1:try{try{break;}finally{e=new A();}}finally{e=mk();}} e.x;}", false);
    // An edge into the finalizer must not borrow a fact from another incoming path.
    check("function read(e:A|null):void{if(e===null){return;} while(true){try{e=mk(); break;}finally{if(e!==null){e.x;}}} e.x;}", false);
    // A jump within a nested target does not leave its enclosing try.
    check("function read(e:A|null):void{if(e===null){return;} try{for(let i=0;i<3;i++){break;} e.x;}finally{e=mk();}}", true);
    // A finalizer's fact of an inner binding cannot become a fact of its namesake.
    check("function read(e:A|null):void{while(true){let e:A|null=mk(); try{break;}finally{e=new A();}} e.x;}", false);
    check("function read(e:A|null):void{if(e===null){return;} while(true){let e:A|null=mk(); try{break;}finally{e=new A();}} e.x;}", true);
    check("function read(e:A|null,k:i32):void{switch(k){default:{let e:A|null=new A(); try{break;}finally{}}} e.x;}", false);
    // The finalizer's break replaces the held continue.
    check(
        "function read(e:A|null):void{while(true){try{continue;}finally{e=new A(); break;}} e.x;}",
        true,
    );
    check("function read(e:A|null):void{if(e===null){return;} while(true){try{e=mk(); e=new A(); break;}finally{e=mk();}} e.x;}", false);
    eprintln!("s315 finalizer forms: {:?}", started.elapsed());
}

/// §315 rule 1b: provenance crosses target joins and ends at a store or test.
/// Cost: 0.294 seconds for 166 programs; no engine subprocess.
#[test]
fn finalizer_jump_cause_survives_joins_and_is_replaced() {
    let started = std::time::Instant::now();
    let targets = [
        "while(true){try{break;}finally{e=mk();}}",
        "for(let i=0;i<3;i++){try{break;}finally{e=mk();}}",
        "switch(k){case 1:try{break;}finally{e=mk();}}",
        "while(true){try{try{break;}finally{e=mk();}}finally{}}",
        "while(true){try{try{break;}finally{e=mk();}}finally{e=mk();}}",
        "try{while(true){try{break;}finally{e=mk();}}}catch(err){}",
        "try{while(true){try{break;}finally{e=mk();}}}finally{}",
        "try{while(true){try{break;}finally{e=mk();}}}catch(err){}finally{}",
    ];
    for target in targets {
        for join in [
            "",
            "if(k===0){e=new A();}",
            "if(k===0){}else{e=new A();}",
            "k===0 ? e=new A() : e;",
            "k===0 && (e=new A());",
        ] {
            let prefix = format!("class A{{x:i32=1;}} function mk():A|null{{return null;}} function f(k:i32):void{{let e:A|null=new A(); {target} {join}");
            let source = format!("{prefix} e.x;}}");
            let files = [SourceFile::new("test.ts", source.clone())];
            let errors = check_program(&files).expect_err(&source);
            assert_eq!(errors[0].code, RuleCode::S011, "{source}: {errors:?}");
            assert_eq!(
                errors[0].divergence,
                if target.contains("catch") {
                    None
                } else {
                    Some(Divergence::FinallyJumpNarrowing)
                },
                "{source}: {errors:?}"
            );
            let rendered = subscript_compiler::render_diagnostics(&files, &errors);
            assert_eq!(
                rendered.contains("= TypeScript accepts:"),
                !target.contains("catch"),
                "{rendered}"
            );
            if !target.contains("catch") {
                assert!(
                    rendered.contains("if (e === null) { return; }"),
                    "{rendered}"
                );
            }
            for replacement in ["if(e===null){return;}", "e=new A();"] {
                let source = format!("{prefix} {replacement} e.x;}}");
                assert!(
                    check_program(&[SourceFile::new("test.ts", source.clone())]).is_ok(),
                    "{source}"
                );
            }
            // A later nullable store replaces the cause with an ordinary nullable fact.
            let source = format!("{prefix} e=mk(); e.x;}}");
            let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
            assert_eq!(errors[0].divergence, None);
        }
    }
    for body in [
        "for(let i=0;i<3;i++){e.x; try{continue;}finally{e=mk();}}",
        "while(k===0){e.x; try{continue;}finally{e=mk();}}",
        "for(const i of [1,2]){e.x; try{continue;}finally{e=mk();}}",
        "for(let i=0;i<3;i++){e.x; try{try{continue;}finally{e=mk();}}finally{}}",
    ] {
        let source = format!("class A{{x:i32=1;}} function mk():A|null{{return null;}} function f(k:i32):void{{let e:A|null=new A(); {body}}}");
        let errors =
            check_program(&[SourceFile::new("test.ts", source.clone())]).expect_err(&source);
        assert_eq!(
            errors[0].divergence,
            Some(Divergence::FinallyJumpNarrowing),
            "{source}: {errors:?}"
        );
    }
    // A hidden binding does not inherit the cause. Its scope exit restores the outer cause.
    let prefix = "class A{x:i32=1;} function mk():A|null{return null;} function f():void{let e:A|null=new A(); while(true){try{break;}finally{e=mk();}}";
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        format!("{prefix} {{let e:A|null=mk(); e.x;}}}}"),
    )])
    .unwrap_err();
    assert_eq!(errors[0].divergence, None);
    let errors = check_program(&[SourceFile::new(
        "test.ts",
        format!("{prefix} {{let e:A|null=mk(); e=new A();}} e.x;}}"),
    )])
    .unwrap_err();
    assert_eq!(errors[0].divergence, Some(Divergence::FinallyJumpNarrowing));
    eprintln!("s315 finalizer cause forms: {:?}", started.elapsed());
}

/// §315 rule 1b: jump facts, mixed joins, use sites, and null tests classify diagnostics.
/// Cost: 0.009 seconds for eight programs; no engine subprocess.
#[test]
fn finalizer_jump_facts_and_null_tests_classify_diagnostics() {
    let started = std::time::Instant::now();
    {
        let files = [SourceFile::new("restore_loop.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction f(e: A | null): void { if (e === null) { return; } for (let i = 0; i < 3; i++) { try { e = mk(); break; } finally { e = new A(); } } print(`${e.x}`); }\n")];
        let errors = check_program(&files).expect_err("restore_loop");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(errors[0].divergence, None, "{errors:?}");
        assert!(!subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    {
        let files = [SourceFile::new("restore_switch.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction f(e: A | null, k: i32): void { if (e === null) { return; } switch (k) { case 0: try { e = mk(); break; } finally { e = new A(); } } print(`${e.x}`); }\n")];
        let errors = check_program(&files).expect_err("restore_switch");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(errors[0].divergence, None, "{errors:?}");
        assert!(!subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    {
        let files = [SourceFile::new("restore_continue.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction f(e: A | null): void { if (e === null) { return; } for (let i = 0; i < 3; i++) { print(`${e.x}`); try { e = mk(); if (i === 0) { continue; } } finally { e = new A(); } } }\n")];
        let errors = check_program(&files).expect_err("restore_continue");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(errors[0].divergence, None, "{errors:?}");
        assert!(!subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    {
        let files = [SourceFile::new("join_head.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction f(e: A | null): void { if (e === null) { return; } let i: i32 = 0; while (true) { print(`${e.x}`); i++; if (i > 2) { break; } try { if (i === 1) { continue; } } finally { e = mk(); } } }\n")];
        let errors = check_program(&files).expect_err("join_head");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(errors[0].divergence, None, "{errors:?}");
        assert!(!subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    {
        let files = [SourceFile::new("join_exit.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction f(e: A | null, k: i32): void { if (e === null) { return; } if (k === 0) { while (true) { try { break; } finally { e = mk(); } } } else { e = mk(); } print(`${e.x}`); }\n")];
        let errors = check_program(&files).expect_err("join_exit");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(errors[0].divergence, None, "{errors:?}");
        assert!(!subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    {
        let files = [SourceFile::new("nominal.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction f(): void { let e: A | null = new A(); for (let i = 0; i < 3; i++) { try { break; } finally { e = mk(); } } take(e); }\n")];
        let errors = check_program(&files).expect_err("nominal");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S005);
        assert_eq!(
            errors[0].divergence,
            Some(Divergence::FinallyJumpNarrowing),
            "{errors:?}"
        );
        assert!(subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    {
        let files = [SourceFile::new("null_test.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction f(): void { let e: A | null = new A(); for (let i = 0; i < 3; i++) { try { break; } finally { e = mk(); } } print(`${e === null ? 0 : 1}`); print(`${e.x}`); }\n")];
        let errors = check_program(&files).expect_err("null_test");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(
            errors[0].divergence,
            Some(Divergence::FinallyJumpNarrowing),
            "{errors:?}"
        );
        assert!(subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    {
        let files = [SourceFile::new("call.ts", "export {};\nclass A { x: i32 = 1; }\nfunction mk(): A | null { return null; }\nfunction take(a: A): void {}\nfunction id(x: i32): i32 { return x; } function f(next: ((x: i32) => i32) | null): void { let cb: ((x: i32) => i32) | null = id; for (let i = 0; i < 3; i++) { try { break; } finally { cb = next; } } print(`${cb(1)}`); }\n")];
        let errors = check_program(&files).expect_err("call");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(
            errors[0].divergence,
            Some(Divergence::FinallyJumpNarrowing),
            "{errors:?}"
        );
        assert!(subscript_compiler::render_diagnostics(&files, &errors)
            .contains("= TypeScript accepts:"));
    }
    eprintln!(
        "§315 rule 1b diagnostic classification: {:?}",
        started.elapsed()
    );
}

/// All nullable-use sites join causes with facts on the other incoming edges.
/// Cost: 0.037 seconds for 18 programs; no engine subprocess.
#[test]
fn finalizer_cause_joins_cover_each_nullable_use_site() {
    let started = std::time::Instant::now();
    for (declarations, initializer, store, use_site, code) in [
        (
            "class A { x:i32=1; } function mk():A|null{return null;}",
            "let e:A|null=new A();",
            "e=mk();",
            "e.x;",
            RuleCode::S011,
        ),
        (
            "class A { x:i32=1; } function mk():A|null{return null;} function take(a:A):void{}",
            "let e:A|null=new A();",
            "e=mk();",
            "take(e);",
            RuleCode::S005,
        ),
        (
            "function id(x:i32):i32{return x;}",
            "let e:((x:i32)=>i32)|null=id;",
            "e=null;",
            "e(1);",
            RuleCode::S100,
        ),
    ] {
        let target = format!("while(true){{try{{break;}}finally{{{store}}}}}");
        let assignment = store.trim_end_matches(';');
        for (body, has_cause) in [
            (format!("if(k===0){{{target}}}else{{{store}}}"), false),
            (format!("{target} k===0 ? ({assignment}) : e;"), false),
            (format!("{target} k===0 && ({assignment});"), false),
            (
                format!("switch(k){{case 0:{{{target}}} break; default:{store}}}"),
                false,
            ),
            (format!("try{{{target}}}catch(err){{{store}}}"), false),
            (format!("{target} print(`${{e===null ? 0 : 1}}`);"), true),
        ] {
            let source = format!("export {{}}; {declarations} function f(k:i32):void{{{initializer} {body} {use_site}}}");
            let files = [SourceFile::new("joins.ts", source.clone())];
            let errors = check_program(&files).expect_err(&source);
            assert_eq!(errors.len(), 1, "{source}: {errors:?}");
            assert_eq!(errors[0].code, code, "{source}: {errors:?}");
            assert_eq!(
                errors[0].divergence,
                has_cause.then_some(Divergence::FinallyJumpNarrowing),
                "{source}: {errors:?}"
            );
            let rendered = subscript_compiler::render_diagnostics(&files, &errors);
            assert_eq!(
                rendered.contains("= TypeScript accepts:"),
                has_cause,
                "{rendered}"
            );
        }
    }
    eprintln!("§315 rule 1b use-site joins: {:?}", started.elapsed());
}
