use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

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
fn nullish_assignment_stays_rejected() {
    let result =
        diagnostics("class Box {}\nfunction run(x: Box | null): void { x ??= new Box(); }\n");
    assert_eq!(result[0].code, RuleCode::S100);
    assert_eq!(
        result[0].message,
        "assignment operator outside the decided surface"
    );
}

#[test]
fn optional_chain_rejects_a_value_class_receiver() {
    let result = diagnostics(
        "@CStruct\nclass Value { v: i32 = 1; }\nfunction run(value: Value): i32 { return value?.v ?? 0; }\n",
    );
    assert_eq!(result[0].code, RuleCode::S100);
    assert_eq!(
        result[0].message,
        "the tested receiver has type `Value`, which is not nullable"
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
