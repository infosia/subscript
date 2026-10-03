//! Shared-location effects preserve local narrowing (compiler.md §124).

use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn narrowed_local_survives_the_call_that_ends_field_narrowing() {
    for value in ["c", "h.c"] {
        let source = format!(
            "class Cell {{ v: i32 = 7; }}
             class Holder {{ c: Cell | null = new Cell(); }}
             function clear(h: Holder): void {{ h.c = null; }}
             function read(h: Holder, c: Cell | null): i32 {{
               if ({value} !== null) {{ clear(h); return {value}.v; }}
               return 0;
             }}"
        );
        let result = check_program(&[SourceFile::new("test.ts", source)]);
        if value == "c" {
            result.expect("a call preserves local narrowing");
        } else {
            let diagnostics = result.expect_err("the same call ends field narrowing");
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].code, RuleCode::S011);
        }
    }
}

#[test]
fn argument_order_observes_the_call_effect() {
    let source = |arguments: &str| {
        [SourceFile::new(
            "test.ts",
            format!(
                "class Cell {{ v: i32 = 7; }}
             class Holder {{ c: Cell | null = new Cell(); }}
             function clear(h: Holder): i32 {{ h.c = null; return 0; }}
             function pair(a: i32, b: i32): void {{ }}
             function read(h: Holder): void {{
               if (h.c !== null) {{ pair({arguments}); }}
             }}"
            ),
        )]
    };
    check_program(&source("h.c.v, clear(h)")).expect("read before call");
    let diagnostics = check_program(&source("clear(h), h.c.v")).expect_err("read after call");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
}

#[test]
fn a_terminating_branch_does_not_restore_a_call_killed_fact() {
    let source = |effect: &str| {
        [SourceFile::new(
            "test.ts",
            format!(
                "class Cell {{ v: i32 = 7; }}
             class Holder {{ c: Cell | null = new Cell(); }}
             function clear(h: Holder): void {{ h.c = null; }}
             function read(h: Holder): i32 {{
               if (h.c === null) {{ return 0; }} else {{ {effect} }}
               return h.c.v;
             }}"
            ),
        )]
    };
    check_program(&source("")).expect("the remaining branch narrows");
    let diagnostics = check_program(&source("clear(h);")).expect_err("the remaining branch calls");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
}

#[test]
fn creating_an_async_handle_ends_shared_narrowing() {
    let source = |value: &str| {
        [SourceFile::new(
            "test.ts",
            format!(
                "class Cell {{ v: i32 = 7; }}
             class Holder {{ c: Cell | null = new Cell(); }}
             async function clear(h: Holder): Promise<void> {{ h.c = null; }}
             async function read(h: Holder, c: Cell | null): Promise<void> {{
               if ({value} !== null) {{
                 const pending = clear(h);
                 print(`${{{value}.v}}`);
                 await pending;
               }}
             }}"
            ),
        )]
    };
    check_program(&source("c")).expect("the local remains narrowed");
    let diagnostics = check_program(&source("h.c")).expect_err("handle creation calls other code");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
}

#[test]
fn a_condition_cannot_restore_a_fact_before_its_last_call() {
    let source = |condition: &str| {
        [SourceFile::new(
            "test.ts",
            format!(
                "class Cell {{ v: i32 = 7; }}
             class Holder {{ c: Cell | null = new Cell(); }}
             function clear(h: Holder): boolean {{ h.c = null; return true; }}
             function read(h: Holder): i32 {{
               if ({condition}) {{ return h.c.v; }}
               return 0;
             }}"
            ),
        )]
    };
    check_program(&source("clear(h) && h.c !== null")).expect("null check after call");
    let diagnostics =
        check_program(&source("h.c !== null && clear(h)")).expect_err("call after null check");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
}

fn checked_body(
    body: &str,
) -> Result<subscript_compiler::hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[SourceFile::new(
        "test.ts",
        format!(
            "class Cell {{ v: i32 = 7; }}
         class Holder {{ c: Cell | null = new Cell(); }}
         let global: Cell | null = null;
         function clear(h: Holder): void {{ h.c = null; }}
         function tick(): void {{ global = null; }}
         function read(h: Holder, local: Cell | null): void {{ {body} }}"
        ),
    )])
}

fn shared_error(body: &str) {
    let diagnostics = checked_body(body).expect_err("a shared narrowing must end");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(
        diagnostics[0].divergence,
        Some(subscript_compiler::divergence::Divergence::SharedLocationNarrowing)
    );
}

#[test]
fn builtin_operations_preserve_shared_narrowing_without_callbacks() {
    for operation in [
        "a.slice(0);",
        "a.push(2);",
        "const m = new Map<string, i32>();",
    ] {
        let body = format!(
            "const a: i32[] = [1]; if (h.c !== null) {{ {operation} print(`${{h.c.v}}`); }}"
        );
        checked_body(&body).expect("the built-in cannot run script code");
        shared_error(&body.replace(operation, "clear(h);"));
    }
    shared_error("const a: i32[] = [1]; if (h.c !== null) { a.forEach((v: i32): void => { }); print(`${h.c.v}`); }");
}

#[test]
fn a_shadowing_local_is_not_the_module_global() {
    checked_body("const global = local; if (global !== null) { tick(); print(`${global.v}`); }")
        .expect("the const local is a separate binding");
    shared_error("if (global !== null) { tick(); print(`${global.v}`); }");
}

#[test]
fn all_loop_heads_apply_body_condition_and_step_effects() {
    for loop_body in [
        "while (true) { print(`${VALUE.v}`); clear(h); break; }",
        "for (let i: i32 = 0; i < 2; i++) { print(`${VALUE.v}`); clear(h); }",
        "for (const i of [1, 2]) { print(`${VALUE.v}`); clear(h); }",
        "for (let i: i32 = 0; i < 2; clear(h)) { print(`${VALUE.v}`); i++; }",
        "while (clearAndContinue(h)) { print(`${VALUE.v}`); }",
    ] {
        let source = |value: &str| {
            [SourceFile::new(
                "test.ts",
                format!(
                    "class Cell {{ v: i32 = 7; }}
             class Holder {{ c: Cell | null = new Cell(); }}
             function clear(h: Holder): void {{ h.c = null; }}
             function clearAndContinue(h: Holder): boolean {{ h.c = null; return true; }}
             function read(h: Holder, local: Cell | null): void {{
               if ({value} !== null) {{ {} }}
             }}",
                    loop_body.replace("VALUE", value)
                ),
            )]
        };
        check_program(&source("local")).expect("loop effects preserve the local");
        let diagnostics =
            check_program(&source("h.c")).expect_err("loop effects end field narrowing");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S011);
    }
}

#[test]
fn try_handler_and_exit_apply_the_subtree_effects() {
    for body in [
        "try { clear(h); } catch { print(`${VALUE.v}`); }",
        "try { clear(h); } catch { } print(`${VALUE.v}`);",
        "try { } catch { clear(h); } print(`${VALUE.v}`);",
        "try { const alias = h; alias.c = null; } catch { } print(`${VALUE.v}`);",
    ] {
        checked_body(&format!(
            "if (local !== null) {{ {} }}",
            body.replace("VALUE", "local")
        ))
        .expect("try effects preserve local narrowing");
        shared_error(&format!(
            "if (h.c !== null) {{ {} }}",
            body.replace("VALUE", "h.c")
        ));
    }
}

#[test]
fn no_script_effect_runs_when_a_lambda_is_only_created() {
    let body = "if (h.c !== null) { const f = (): void => { }; print(`${h.c.v}`); }";
    checked_body(body).expect("a lambda body does not run at creation");
    shared_error(&body.replace("print", "f(); print"));
}

#[test]
fn diagnostic_notes_do_not_escape_the_narrowing_scope() {
    let ordinary = [
        "if (h.c !== null) { clear(h); } print(`${h.c.v}`);",
        "if (h.c !== null) { clear(h); } h.c = null; print(`${h.c.v}`);",
        "if (h.c !== null) { clear(h); } if (h.c !== null) { } print(`${h.c.v}`);",
    ];
    for body in ordinary {
        let diagnostics = checked_body(body).expect_err("ordinary unnarrowed use");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S011);
        assert_eq!(diagnostics[0].divergence, None, "{diagnostics:?}");
    }
    shared_error("if (h.c !== null) { clear(h); print(`${h.c.v}`); }");
}

#[test]
fn a_redeclared_local_does_not_inherit_a_field_kill_note() {
    let diagnostics = checked_body(
        "{ const item = h; if (item.c !== null) { clear(item); } }
         { const item = h; print(`${item.c.v}`); }",
    )
    .expect_err("the second binding was never narrowed");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(diagnostics[0].divergence, None);
    shared_error("if (h.c !== null) { clear(h); print(`${h.c.v}`); }");
}

#[test]
fn a_loop_condition_note_does_not_escape_the_loop() {
    let diagnostics = checked_body("while (h.c !== null) { clear(h); } print(`${h.c.v}`);")
        .expect_err("the condition does not prove the field non-null after the loop");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].divergence, None);
    shared_error("while (h.c !== null) { clear(h); print(`${h.c.v}`); }");
}

#[test]
fn direct_global_stores_end_only_that_global_binding() {
    let diagnostics = checked_body("if (global !== null) { global = null; print(`${global.v}`); }")
        .expect_err("direct store ends narrowing");
    assert_eq!(diagnostics[0].divergence, None);
    checked_body("const global = local; if (global !== null) { tick(); print(`${global.v}`); }")
        .expect("the shadowing local survives a store to the module global");
}

#[test]
fn constructor_effects_use_the_complete_class_definitions() {
    let source = |effect: &str| {
        [SourceFile::new("test.ts", format!(
        "class Cell {{ v: i32 = 7; }}
         let cell: Cell | null = new Cell();
         function clear(): i32 {{ cell = null; return 0; }}
         function read(): void {{ if (cell !== null) {{ const t = new Trigger(); print(`${{cell.v}}`); }} }}
         class Trigger {{ value: i32 = {effect}; }}"
    ))]
    };
    check_program(&source("1")).expect("the initializer cannot call script code");
    let diagnostics =
        check_program(&source("clear()")).expect_err("the later class has a calling initializer");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
}

#[test]
fn generator_next_and_using_exits_end_shared_facts() {
    for effect in [
        "sequence.next();",
        "{ using active = cleaner; }",
        "try { using active = cleaner; } catch { }",
    ] {
        let source = |value: &str| {
            [SourceFile::new(
                "test.ts",
                format!(
                    "class Cell {{ v: i32 = 7; }}
             class Holder {{ c: Cell | null = new Cell(); }}
             class Cleaner {{ [Symbol.dispose](): void {{ }} }}
             function* values(): Generator<i32> {{ yield 1; }}
             function read(h: Holder, local: Cell | null): void {{
               const cleaner = new Cleaner(); const sequence = values();
               if ({value} !== null) {{ {effect} print(`${{{value}.v}}`); }}
             }}"
                ),
            )]
        };
        check_program(&source("local")).expect("a local survives the scope exit or resumption");
        let diagnostics = check_program(&source("h.c")).expect_err("script code can run");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S011);
    }
}

#[test]
fn map_and_set_callbacks_end_shared_narrowing() {
    for operation in [
        "m.forEach((value: i32, key: string): void => { });",
        "s.forEach((value: i32): void => { });",
        "Map.groupBy([1, 2], (value: i32): i32 => value);",
    ] {
        let body = format!("const m = new Map<string, i32>(); const s = new Set<i32>(); if (VALUE !== null) {{ {operation} print(`${{VALUE.v}}`); }}");
        checked_body(&body.replace("VALUE", "local")).expect("the callback preserves a local");
        shared_error(&body.replace("VALUE", "h.c"));
    }
}

#[test]
fn do_while_remains_outside_the_language() {
    let diagnostics =
        checked_body("do { continue; if (h.c === null) { return; } } while (h.c.v > 0);")
            .expect_err("do-while is unsupported");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].divergence,
        Some(subscript_compiler::divergence::Divergence::DoWhileStatement)
    );
}

#[test]
fn synthesized_json_helpers_preserve_shared_narrowing() {
    for operation in [
        "const s = JSON.stringify(h.c);",
        "const p = JSON.parse<Cell>(\"{\\\"v\\\":1}\");",
    ] {
        let body = format!("if (h.c !== null) {{ {operation} print(`${{h.c.v}}`); }}");
        checked_body(&body).expect("synthesized helper runs no script code");
        shared_error(&body.replace(operation, "clear(h);"));
    }
}

#[test]
fn switch_dispatch_and_exit_do_not_inherit_case_local_facts() {
    let dispatch = "if (local !== null) { switch (1) { case 0: print(`${local.v}`); break; case 1: local = null; break; } }";
    checked_body(dispatch).expect("another case cannot end the dispatch fact");
    let invalid = dispatch.replace("case 0: print", "case 0: local = null; print");
    assert!(checked_body(&invalid)
        .expect_err("a store before the read ends its fact")
        .iter()
        .any(|diagnostic| diagnostic.code == RuleCode::S011));
    for body in [
        "switch (1) { case 0: if (local === null) { return; } break; case 1: print(`${local.v}`); }",
        "switch (1) { case 0: if (local === null) { return; } break; } print(`${local.v}`);",
        "switch (1) { case 0: if (local === null) { return; } case 1: print(`${local.v}`); }",
    ] {
        let diagnostics = checked_body(body).expect_err("a dispatch edge has no narrowing");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == RuleCode::S011));
        checked_body(&format!("if (local === null) {{ return; }} {}", body.replace("if (local === null) { return; }", "")))
            .expect("all dispatch edges carry the local fact");
    }
    shared_error("if (h.c !== null) { switch (1) { case 0: clear(h); break; case 1: break; } print(`${h.c.v}`); }");
}

#[test]
fn direct_field_store_has_no_shared_alias_note() {
    let diagnostics = checked_body("if (h.c !== null) { h.c = null; print(`${h.c.v}`); }")
        .expect_err("the direct store ends its fact");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(diagnostics[0].divergence, None);
    shared_error("const alias = h; if (h.c !== null) { alias.c = null; print(`${h.c.v}`); }");
}

#[test]
fn shared_reads_derive_sites_from_declared_storage_types() {
    use subscript_compiler::hir::{Expr, ExprKind, HirChild, Stmt, TrapSite};
    fn expression(expr: &Expr, module: &subscript_compiler::hir::Module, counts: &mut [usize; 3]) {
        let guarded = expr
            .trap_sites(module)
            .iter()
            .any(|site| matches!(site, TrapSite::NullNarrowing { .. }));
        match &expr.kind {
            ExprKind::Global(_) if guarded => counts[0] += 1,
            ExprKind::Field { .. } if guarded => counts[1] += 1,
            ExprKind::Local(..) => {
                assert!(!guarded);
                counts[2] += 1;
            }
            _ => {}
        }
        for child in expr.children() {
            visit(child, module, counts);
        }
    }
    fn statement(stmt: &Stmt, module: &subscript_compiler::hir::Module, counts: &mut [usize; 3]) {
        for child in stmt.children() {
            visit(child, module, counts);
        }
    }
    fn visit(
        child: HirChild<'_>,
        module: &subscript_compiler::hir::Module,
        counts: &mut [usize; 3],
    ) {
        match child {
            HirChild::Expr(expr) => expression(expr, module, counts),
            HirChild::Stmt(stmt) => statement(stmt, module, counts),
        }
    }
    let module = checked_body("if (global !== null) { const a = global; } if (h.c !== null) { const b = h.c; } if (local !== null) { const c = local; }").expect("narrowed reads");
    let mut counts = [0; 3];
    for function in &module.functions {
        for stmt in &function.body {
            statement(stmt, &module, &mut counts);
        }
    }
    assert_eq!(counts[0], 1);
    assert_eq!(counts[1], 1);
    assert!(counts[2] > 0);
}

#[test]
fn generator_result_fields_survive_calls_and_alias_stores() {
    for effect in ["tick();", "h.value = null;", "while (false) { tick(); }"] {
        let source = |value: &str, effect: &str| {
            SourceFile::new(
                "test.ts",
                format!(
                    "class Cell {{ v: i32 = 7; }}
             class Holder {{ value: Cell | null = new Cell(); }}
             function tick(): void {{ }}
             function* cells(): Generator<Cell | null> {{ yield new Cell(); }}
             function read(): void {{
               let r = cells().next(); const h = new Holder();
               if ({value} !== null) {{ {effect} print(`${{{value}.v}}`); }}
             }}"
                ),
            )
        };
        check_program(&[source("r.value", effect)]).expect("local value field survives");
        let errors = check_program(&[source("h.value", effect)]).expect_err("shared field ends");
        assert!(
            errors.iter().any(|error| error.code == RuleCode::S011),
            "{errors:?}"
        );
        for direct in [
            "r = cells().next();",
            "while (false) { r = cells().next(); }",
        ] {
            let errors = check_program(&[source("r.value", direct)])
                .expect_err("local assignment ends its fact");
            assert!(
                errors.iter().any(|error| error.code == RuleCode::S011),
                "{errors:?}"
            );
        }
    }
}
