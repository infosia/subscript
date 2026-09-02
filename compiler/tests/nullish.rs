use subscript_compiler::{
    check_program, hir, render_diagnostics, Diagnostic, RuleCode, SourceFile,
};

fn function_body(source: &str, name: &str) -> Vec<hir::Stmt> {
    let module = check_program(&[SourceFile::new("identity.ts", source)])
        .expect("the identity program must check");
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("function `{name}` must exist"))
        .body
        .clone()
}

fn expr_shape(expr: &hir::Expr) -> String {
    let kind = match &expr.kind {
        hir::ExprKind::Local(name) => format!("local({name})"),
        hir::ExprKind::Null => "null".to_string(),
        hir::ExprKind::Binary { op, left, right } => {
            format!("binary({op:?},{},{})", expr_shape(left), expr_shape(right))
        }
        hir::ExprKind::Cond { cond, then, els } => format!(
            "cond({},{},{})",
            expr_shape(cond),
            expr_shape(then),
            expr_shape(els)
        ),
        hir::ExprKind::Call { callee, args } => {
            let callee = match callee {
                hir::Callee::Method { recv, name } => {
                    format!("method({}, {name})", expr_shape(recv))
                }
                other => format!("{other:?}"),
            };
            let args = args.iter().map(expr_shape).collect::<Vec<_>>().join(",");
            format!("call({callee};{args})")
        }
        other => format!("{other:?}"),
    };
    format!("{kind}:{:?}", expr.ty)
}

fn stmt_shape(statement: &hir::Stmt) -> String {
    match statement {
        hir::Stmt::Expr(expr) => format!("expr({})", expr_shape(expr)),
        hir::Stmt::Return { value, .. } => format!(
            "return({})",
            value.as_ref().map_or_else(String::new, expr_shape)
        ),
        hir::Stmt::If {
            cond, then, els, ..
        } => format!(
            "if({};{};{})",
            expr_shape(cond),
            body_shape(then),
            els.as_ref()
                .map_or_else(String::new, |body| body_shape(body))
        ),
        other => format!("{other:?}"),
    }
}

fn body_shape(body: &[hir::Stmt]) -> String {
    body.iter().map(stmt_shape).collect::<Vec<_>>().join(";")
}

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("test.ts", source)]).expect_err("the nullish program must fail")
}

#[test]
fn nullish_place_has_the_conditional_hir() {
    let prefix = "class Box {}\n";
    let sugar =
        format!("{prefix}function choose(x: Box | null, y: Box): Box {{ return x ?? y; }}\n");
    let spelled = format!(
        "{prefix}function choose(x: Box | null, y: Box): Box {{ return x !== null ? x : y; }}\n"
    );
    assert_eq!(
        body_shape(&function_body(&sugar, "choose")),
        body_shape(&function_body(&spelled, "choose"))
    );
}

#[test]
fn optional_call_place_has_the_if_hir() {
    let prefix = "class Box { m(): void {} }\n";
    let sugar = format!("{prefix}function run(x: Box | null): void {{ x?.m(); }}\n");
    let spelled =
        format!("{prefix}function run(x: Box | null): void {{ if (x !== null) {{ x.m(); }} }}\n");
    assert_eq!(
        body_shape(&function_body(&sugar, "run")),
        body_shape(&function_body(&spelled, "run"))
    );
}

#[test]
fn optional_call_is_legal_in_a_for_update_clause() {
    let source = "class Box { m(): void {} }\n\
                  function run(x: Box | null): void {\n\
                  \x20 for (let i: i32 = 0; i < 2; x?.m()) { i++; }\n\
                  }\n";
    check_program(&[SourceFile::new("test.ts", source)])
        .expect("the optional call update must check");
}

#[test]
fn nullish_assignment_stays_rejected() {
    let source = "class Box {}\nfunction run(x: Box | null): void { x ??= new Box(); }\n";
    let files = [SourceFile::new("test.ts", source)];
    let result = check_program(&files).expect_err("nullish assignment must fail");
    assert_eq!(result[0].code, RuleCode::S100);
    assert_eq!(
        result[0].message,
        "assignment operator outside the decided surface"
    );
    let rendered = render_diagnostics(&files, &result);
    assert!(rendered.contains("= TypeScript accepts:"), "{rendered}");
    assert!(rendered.contains("a ??= new Box();"), "{rendered}");
}

#[test]
fn optional_chain_rejects_a_value_class_receiver() {
    let source = "@CStruct\nclass Value { v: i32 = 1; }\nfunction run(value: Value): i32 { return value?.v ?? 0; }\n";
    let files = [SourceFile::new("test.ts", source)];
    let result = check_program(&files).expect_err("the optional chain must fail");
    assert_eq!(result[0].code, RuleCode::S100);
    assert_eq!(
        result[0].message,
        "the tested receiver has type `Value`, which is not nullable"
    );
    let rendered = render_diagnostics(&files, &result);
    let block = rendered
        .split("= TypeScript accepts:")
        .nth(1)
        .expect("the divergence block must render");
    assert!(block.contains("?."), "{rendered}");
}

#[test]
fn optional_chain_on_an_unknown_name_reports_one_diagnostic() {
    let result = diagnostics("const n: i32 = zz?.v ?? 0;\n");
    assert_eq!(result.len(), 1, "diagnostics: {result:?}");
    assert_eq!(result[0].code, RuleCode::S016);
    assert_eq!(result[0].message, "unknown name `zz`");
}

#[test]
fn unknown_call_and_constructor_names_use_s016() {
    for (source, message) in [
        ("zz();\n", "unknown function `zz`"),
        ("new Nope();\n", "unknown class `Nope`"),
    ] {
        let result = diagnostics(source);
        assert_eq!(result.len(), 1, "diagnostics: {result:?}");
        assert_eq!(result[0].code, RuleCode::S016);
        assert_eq!(result[0].message, message);
    }
}

#[test]
fn non_place_nullish_receiver_in_a_field_initializer_fails() {
    let source = "class Box {}\nfunction maybe(): Box | null { return null; }\nconst fallback: Box = new Box();\nclass Holder { x: Box = maybe() ?? fallback; }\n";
    let files = [SourceFile::new("test.ts", source)];
    let result = check_program(&files).expect_err("the field initializer must fail");
    assert_eq!(result.len(), 1, "diagnostics: {result:?}");
    assert_eq!(result[0].code, RuleCode::S100);
    assert_eq!(
        result[0].message,
        "a non-place receiver of `??` or `?.` cannot be used in an initializer"
    );
    assert!(
        render_diagnostics(&files, &result).contains("= TypeScript accepts:"),
        "the rejection must render its divergence block"
    );
}

#[test]
fn place_nullish_receiver_in_a_field_initializer_checks() {
    let source = "class Box {}\nconst candidate: Box | null = null;\nconst fallback: Box = new Box();\nclass Holder { x: Box = candidate ?? fallback; }\n";
    check_program(&[SourceFile::new("test.ts", source)])
        .expect("a place receiver needs no synthetic local");
}

#[test]
fn module_initializer_lambda_call_keeps_the_indirect_call_order_error() {
    let source =
        "const n: i32 = ((): i32 => 3)();\nexport function main(): void { print(`${n}`); }\n";
    let result = diagnostics(source);
    assert_eq!(result.len(), 1, "diagnostics: {result:?}");
    assert_eq!(result[0].code, RuleCode::S100);
    assert_eq!(
        result[0].message,
        "`n` is accessed before its declaration, through an indirect call"
    );
}

#[test]
fn optional_chain_rejects_argument_position() {
    let result = diagnostics(
        "class Box { v: i32 = 1; }\nfunction use(value: i32): void {}\nfunction run(x: Box | null): void { use(x?.v); }\n",
    );
    assert_eq!(result[0].code, RuleCode::S012);
    assert!(result[0]
        .message
        .starts_with("an optional chain has type `i32 | undefined`"));
}

#[test]
fn nullish_rejects_a_wrong_right_type() {
    let result = diagnostics(
        "class Box {}\nclass Other {}\nfunction run(x: Box | null): Box { return x ?? new Other(); }\n",
    );
    assert_eq!(result[0].code, RuleCode::S005);
}

#[test]
fn optional_computed_step_stays_rejected() {
    let result = diagnostics(
        "class Values { [index: u32]: i32; get(index: u32): i32 { return 0; } set(index: u32, value: i32): void {} }\nfunction run(values: Values | null): i32 { return values?.[0] ?? 0; }\n",
    );
    assert_eq!(result[0].code, RuleCode::S100);
    assert_eq!(
        result[0].message,
        "an optional chain cannot use `?.[i]`; narrow the receiver and use `[i]`"
    );
}
