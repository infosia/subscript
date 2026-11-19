//! Expression checking: contextual literal typing (C4), sized-numeric
//! arithmetic (C3/Q18), nominal member access (C1/Q4/Q5), calls, `as`
//! conversions, lambdas (C5), and null narrowing at use sites (C7).

mod aggregate;
mod array_of_and_map_copy;
mod assign;
mod await_handle;
mod call;
mod callback_shape;
mod conditional;
mod construction;
mod container_new;
mod entry;
mod function_list;
pub(crate) use entry::ContextKind;
pub(super) mod lambda;
pub(super) mod literal;
mod logical;
mod logical_assign;
mod member;
mod member_view;
use member_view::{member_view, MemberKey};
mod method;
mod multi_argument;
mod namespace;
mod operator;
mod self_call;
mod unary;

use oxc_ast::ast;

use crate::diag::Pos;
use crate::hir::{self, BinOp, Callee, ExprKind};
use crate::types::{ClassId, Type};

use super::{static_member_symbol, Checker};

enum PlaceSource<'a> {
    Ident(&'a ast::IdentifierReference<'a>),
    Member(&'a ast::MemberExpression<'a>),
    Unsupported,
}

enum Place {
    Local(hir::Expr),
    Global(hir::Expr),
    Field(hir::Expr),
    Index(hir::Expr),
    IndexSignature {
        receiver: hir::Expr,
        index: hir::Expr,
        signature: hir::IndexSignature,
        pos: Pos,
    },
    Accessor {
        class: ClassId,
        receiver: Option<hir::Expr>,
        name: String,
        ty: Type,
        pos: Pos,
    },
    StaticField(hir::Expr),
}

enum OptionalStep<'a> {
    Member {
        member: &'a ast::MemberExpression<'a>,
        tested: bool,
    },
    Call {
        call: &'a ast::CallExpression<'a>,
        tested: bool,
    },
}

struct OptionalPlan {
    tests: Vec<hir::Expr>,
    value: hir::Expr,
    ends_in_call: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaceKind {
    Local,
    Global,
    Field,
    Index,
    IndexSignature,
    Accessor,
    StaticField,
}

#[cfg(test)]
std::thread_local! {
    static CLASSIFIED_PLACES: std::cell::RefCell<Vec<PlaceKind>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

#[cfg(test)]
pub(crate) fn take_classified_places() -> Vec<PlaceKind> {
    CLASSIFIED_PLACES.with(|places| std::mem::take(&mut *places.borrow_mut()))
}

impl Place {
    fn ty(&self) -> &Type {
        match self {
            Self::Local(expr)
            | Self::Global(expr)
            | Self::Field(expr)
            | Self::Index(expr)
            | Self::StaticField(expr) => &expr.ty,
            Self::IndexSignature { signature, .. } => &signature.element_ty,
            Self::Accessor { ty, .. } => ty,
        }
    }

    fn into_read(self, checker: &Checker<'_>) -> hir::Expr {
        match self {
            Self::Local(expr)
            | Self::Global(expr)
            | Self::Field(expr)
            | Self::Index(expr)
            | Self::StaticField(expr) => expr,
            Self::IndexSignature {
                receiver,
                index,
                signature,
                pos,
            } => hir::Expr {
                pending_work: None,
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(receiver),
                        name: hir::Symbol::from_full_text("get"),
                    },
                    args: vec![index],
                },
                ty: signature.element_ty,
                pos,
            },
            Self::Accessor {
                class,
                receiver,
                name,
                ty,
                pos,
            } => hir::Expr {
                pending_work: None,
                kind: ExprKind::Call {
                    callee: if let Some(receiver) = receiver {
                        Callee::Method {
                            recv: Box::new(receiver),
                            name: hir::Symbol::from_full_text(name.clone()),
                        }
                    } else {
                        Callee::Func(hir::Symbol::from_full_text(static_member_symbol(
                            class,
                            &checker.classes[class.0].name,
                            &name,
                        )))
                    },
                    args: Vec::new(),
                },
                ty,
                pos,
            },
        }
    }

    #[cfg(test)]
    fn record_kind(&self) {
        let kind = match self {
            Self::Local(_) => PlaceKind::Local,
            Self::Global(_) => PlaceKind::Global,
            Self::Field(_) => PlaceKind::Field,
            Self::Index(_) => PlaceKind::Index,
            Self::IndexSignature { .. } => PlaceKind::IndexSignature,
            Self::Accessor { .. } => PlaceKind::Accessor,
            Self::StaticField(_) => PlaceKind::StaticField,
        };
        CLASSIFIED_PLACES.with(|places| places.borrow_mut().push(kind));
    }
}

/// Path key for type-narrowing and element-check facts (§163).
pub(crate) fn path_key(e: &hir::Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Local(n, _, _) => Some(n.clone()),
        ExprKind::Global(n, _) => Some(format!("[[global]]{}", n.full_text())),
        ExprKind::This => Some("this".to_string()),
        ExprKind::Field { obj, name } => path_key(obj)
            .filter(|p| !p.contains(".[[element:"))
            .map(|p| format!("{}.{}", p, name)),
        ExprKind::Index {
            obj,
            element_key: Some(key),
            ..
        } => path_key(obj).map(|receiver| format!("{receiver}.[[element:{key}]]")),
        _ => None,
    }
}

fn is_place_expr(expr: &hir::Expr) -> bool {
    match &expr.kind {
        ExprKind::Local(..) | ExprKind::Global(_, _) | ExprKind::This => true,
        ExprKind::Field { obj, .. } => is_place_expr(obj),
        ExprKind::Index { obj, index, .. } => is_place_expr(obj) && is_place_expr(index),
        _ => false,
    }
}

/// True for literals that can adopt a contextual type: numeric literals
/// (C4) and string literals in a Q32 alias context.
fn literalish(e: &ast::Expression) -> bool {
    match e {
        ast::Expression::NumericLiteral(_) | ast::Expression::StringLiteral(_) => true,
        ast::Expression::ParenthesizedExpression(p) => literalish(&p.expression),
        ast::Expression::UnaryExpression(u) if u.operator == ast::UnaryOperator::UnaryNegation => {
            literalish(&u.argument)
        }
        _ => false,
    }
}

/// Whether an expression is the identifier `undefined`, in parentheses or
/// not. The identifier is the token of C7 only with no binding of that
/// name in scope: `Checker::is_undefined_token` decides (compiler.md §253
/// rule 1).
pub(super) fn is_undefined_name(e: &ast::Expression) -> bool {
    match e {
        ast::Expression::Identifier(id) => id.name.as_ref() == "undefined",
        ast::Expression::ParenthesizedExpression(paren) => is_undefined_name(&paren.expression),
        _ => false,
    }
}

pub(super) fn unparen_expr<'s, 'a>(mut e: &'s ast::Expression<'a>) -> &'s ast::Expression<'a> {
    while let ast::Expression::ParenthesizedExpression(paren) = e {
        e = &paren.expression;
    }
    e
}

fn flatten_optional_chain<'a>(
    chain: &'a ast::ChainExpression<'a>,
    steps: &mut Vec<OptionalStep<'a>>,
) -> Result<&'a ast::Expression<'a>, oxc_span::Span> {
    match &chain.expression {
        ast::ChainElement::CallExpression(call) => {
            let root = flatten_optional_operand(&call.callee, steps)?;
            steps.push(OptionalStep::Call {
                call,
                tested: call.optional,
            });
            Ok(root)
        }
        ast::ChainElement::TSNonNullExpression(nonnull) => Err(nonnull.span),
        member => {
            let member = member.to_member_expression();
            let root = flatten_optional_operand(member.object(), steps)?;
            steps.push(OptionalStep::Member {
                member,
                tested: member.optional(),
            });
            Ok(root)
        }
    }
}

fn flatten_optional_operand<'a>(
    expression: &'a ast::Expression<'a>,
    steps: &mut Vec<OptionalStep<'a>>,
) -> Result<&'a ast::Expression<'a>, oxc_span::Span> {
    let expression = unparen_expr(expression);
    match expression {
        ast::Expression::ChainExpression(chain) => flatten_optional_chain(chain, steps),
        ast::Expression::CallExpression(call) if has_optional_step(expression) => {
            let root = flatten_optional_operand(&call.callee, steps)?;
            steps.push(OptionalStep::Call {
                call,
                tested: call.optional,
            });
            Ok(root)
        }
        ast::Expression::TSNonNullExpression(nonnull) if has_optional_step(expression) => {
            Err(nonnull.span)
        }
        expression
            if expression.as_member_expression().is_some() && has_optional_step(expression) =>
        {
            if let Some(member) = expression.as_member_expression() {
                let root = flatten_optional_operand(member.object(), steps)?;
                steps.push(OptionalStep::Member {
                    member,
                    tested: member.optional(),
                });
                Ok(root)
            } else {
                Ok(expression)
            }
        }
        other => Ok(other),
    }
}

fn has_optional_step(expression: &ast::Expression<'_>) -> bool {
    match unparen_expr(expression) {
        ast::Expression::ChainExpression(_) => true,
        ast::Expression::CallExpression(call) => call.optional || has_optional_step(&call.callee),
        ast::Expression::TSNonNullExpression(nonnull) => has_optional_step(&nonnull.expression),
        expression => expression
            .as_member_expression()
            .is_some_and(|member| member.optional() || has_optional_step(member.object())),
    }
}

fn opt_call_as_call<'s, 'a>(call: &'s ast::CallExpression<'a>) -> &'s ast::CallExpression<'a> {
    call
}

fn assign_op(op: ast::AssignmentOperator) -> Option<(BinOp, &'static str)> {
    use ast::AssignmentOperator as A;
    Some(match op {
        A::Addition => (BinOp::Add, "+="),
        A::Subtraction => (BinOp::Sub, "-="),
        A::Multiplication => (BinOp::Mul, "*="),
        A::Division => (BinOp::Div, "/="),
        A::Remainder => (BinOp::Rem, "%="),
        A::BitwiseAnd => (BinOp::BitAnd, "&="),
        A::BitwiseOR => (BinOp::BitOr, "|="),
        A::BitwiseXOR => (BinOp::BitXor, "^="),
        A::ShiftLeft => (BinOp::Shl, "<<="),
        A::ShiftRight => (BinOp::Shr, ">>="),
        A::ShiftRightZeroFill => (BinOp::UShr, ">>>="),
        _ => return None,
    })
}

fn assign_binary_op(op: BinOp) -> Option<ast::BinaryOperator> {
    use ast::BinaryOperator as B;
    Some(match op {
        BinOp::Add => B::Addition,
        BinOp::Sub => B::Subtraction,
        BinOp::Mul => B::Multiplication,
        BinOp::Div => B::Division,
        BinOp::Rem => B::Remainder,
        BinOp::BitAnd => B::BitwiseAnd,
        BinOp::BitOr => B::BitwiseOR,
        BinOp::BitXor => B::BitwiseXOR,
        BinOp::Shl => B::ShiftLeft,
        BinOp::Shr => B::ShiftRight,
        BinOp::UShr => B::ShiftRightZeroFill,
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BinUse {
    Expression,
    CompoundAssignment,
}

struct BinResult {
    expr: hir::Expr,
    terminal: bool,
}

enum WriteExpression<'a> {
    Assign(&'a ast::AssignmentExpression<'a>),
    Update(&'a ast::UpdateExpression<'a>),
}

fn write_spelling(write: WriteExpression<'_>, target: &str) -> String {
    match write {
        WriteExpression::Update(update) => {
            let operator = if update.operator == ast::UpdateOperator::Increment {
                "++"
            } else {
                "--"
            };
            if update.prefix {
                format!("`{operator}{target}`")
            } else {
                format!("`{target}{operator}`")
            }
        }
        WriteExpression::Assign(assign) => {
            let (operator, value) = if assign.operator == ast::AssignmentOperator::Assign {
                ("=", "v".to_string())
            } else {
                (
                    assign_op(assign.operator)
                        .map(|(_, operator)| operator)
                        .or_else(|| {
                            logical_assign::LogicalAssign::of(assign.operator)
                                .map(logical_assign::LogicalAssign::spelling)
                        })
                        .unwrap_or("="),
                    diagnostic_expr_spelling(&assign.right),
                )
            };
            format!("`{target} {operator} {value}`")
        }
    }
}

fn diagnostic_expr_spelling(expression: &ast::Expression) -> String {
    match unparen_expr(expression) {
        ast::Expression::Identifier(identifier) => identifier.name.to_string(),
        ast::Expression::NumericLiteral(number) => number
            .raw
            .as_deref()
            .map_or_else(|| number.value.to_string(), ToString::to_string),
        ast::Expression::BooleanLiteral(boolean) => boolean.value.to_string(),
        ast::Expression::NullLiteral(_) => "null".to_string(),
        _ => "v".to_string(),
    }
}

/// Returns the nominal class supplied by an object literal's context.
/// Descriptor construction goes through either `D` or `D | null` (§25,
/// §25.3a); a plain class here keeps its specific S005 rejection.
fn contextual_object_class(
    ctx: Option<&Type>,
    apparent_type: impl Fn(&Type) -> Type,
) -> Option<ClassId> {
    match &apparent_type(ctx?) {
        Type::Class(id) => Some(*id),
        Type::Nullable(inner, _) => match &apparent_type(inner.as_ref()) {
            Type::Class(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum DescriptorProp<'a> {
    Expr(&'a ast::Expression<'a>),
    Shorthand(&'a ast::IdentifierReference<'a>),
}

#[derive(Clone, Copy)]
enum CallbackMethod {
    Array(hir::ArrFn),
    MapForEach,
    SetForEach,
    MapGroupBy,
}

#[derive(Clone, Copy)]
struct CallbackSpec<'a> {
    method: &'a str,
    q_rule: &'static str,
    allow_index: bool,
    /// The prelude declaration includes the array; this is not a run-time fact.
    array_container: bool,
    /// The argument count from the exhaustive built-in table below.
    runtime_arity: usize,
}

impl CallbackSpec<'static> {
    fn new(operation: CallbackMethod) -> Self {
        use hir::ArrFn as A;
        // Each row states the arguments that JavaScript passes at run time.
        // The exhaustive Array match requires a row for each new method.
        let (method, q_rule, allow_index, runtime_arity) = match operation {
            CallbackMethod::Array(operation) => {
                let (allow_index, arity) = match operation {
                    // value, index, array (also for FixedArray)
                    A::ForEach
                    | A::Map
                    | A::Filter
                    | A::Some
                    | A::Every
                    | A::FindIndex
                    | A::Find
                    | A::FindLast
                    | A::FindLastIndex
                    | A::FlatMap => (true, 3),
                    // accumulator, value, index, array
                    A::Reduce | A::ReduceRight => (true, 4),
                    // left, right
                    A::Sort => (false, 2),
                    // These methods have no callback.
                    A::IndexOf
                    | A::LastIndexOf
                    | A::Includes
                    | A::Join
                    | A::Slice
                    | A::Fill
                    | A::Reverse
                    | A::Concat
                    | A::Splice
                    | A::Shift
                    | A::Unshift
                    | A::CopyWithin
                    | A::At => (false, 0),
                };
                (
                    operation.name(),
                    if operation == A::Sort { "Q22" } else { "Q27" },
                    allow_index,
                    arity,
                )
            }
            // value, key, map
            CallbackMethod::MapForEach => ("Map.forEach", "Q24", false, 3),
            // value, repeated value, set
            CallbackMethod::SetForEach => ("Set.forEach", "Q24", false, 3),
            // value, index (the prelude declares only the value)
            CallbackMethod::MapGroupBy => ("Map.groupBy", "Q27", false, 2),
        };
        Self {
            method,
            q_rule,
            allow_index,
            array_container: matches!(operation, CallbackMethod::Array(_)),
            runtime_arity,
        }
    }
}

fn regex_literal<'a>(e: &'a ast::Expression<'a>) -> Option<&'a ast::RegExpLiteral<'a>> {
    match e {
        ast::Expression::RegExpLiteral(regex) => Some(regex),
        ast::Expression::ParenthesizedExpression(paren) => regex_literal(&paren.expression),
        _ => None,
    }
}

/// The `f64` range for a synthesized numeric node with no source spelling
/// (§56.1). Such a node carries an `f64` value, which is exact up to
/// 2^53 - 1.
fn synthesized_int_range(ty: &Type) -> Option<(i64, i64)> {
    const EXACT: i128 = 9_007_199_254_740_991;
    let (lo, hi) = ty.int_bounds()?;
    Some((lo.max(-EXACT) as i64, hi.min(EXACT) as i64))
}

/// The inherited Object names in the TypeScript view of these receivers.
fn is_object_member(name: &str) -> bool {
    matches!(
        name,
        "constructor"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
    )
}

/// The site of an assignment to the method `name` (§209.1 rule 3).
/// `tsc` accepts a plain or logical assignment to a method that is not a
/// `#x` method. It rejects an arithmetic write (TS2365, TS2356) and a
/// write to a `#x` method (TS2803).
pub(super) fn method_write_site(
    name: &str,
    is_static: bool,
    arithmetic: bool,
) -> super::rejection::RejectionSite {
    use super::rejection::RejectionSite;
    let tsc_rejects = arithmetic || name.starts_with('#');
    match (is_static, tsc_rejects) {
        (false, false) => RejectionSite::MethodAssignment,
        (false, true) => RejectionSite::MethodAssignmentTscRejects,
        (true, false) => RejectionSite::StaticMethodAssignment,
        (true, true) => RejectionSite::StaticMethodAssignmentTscRejects,
    }
}

/// The Function names in TypeScript's class-constructor view.
fn is_function_member(name: &str) -> bool {
    is_object_member(name)
        || matches!(
            name,
            "name" | "length" | "apply" | "bind" | "call" | "arguments" | "caller"
        )
}

#[cfg(test)]
mod tests {
    use crate::{check_program, hir, RuleCode, SourceFile};

    fn normalized_main_expression(source: &str) -> hir::Expr {
        fn normalize(expr: &mut hir::Expr) {
            expr.pos = crate::diag::Pos::new("identity.ts", 0, 0);
            for child in expr.children_mut() {
                match child {
                    hir::HirChildMut::Expr(child) => normalize(child),
                    hir::HirChildMut::Stmt(_) => {
                        panic!("the identity expression must hold no statement")
                    }
                }
            }
        }

        let module = check_program(&[SourceFile::new("identity.ts", source)])
            .expect("the compound identity program must check");
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main function");
        let hir::Stmt::Expr(expression) = main.body.last().expect("main expression") else {
            panic!("main must end in an expression statement");
        };
        let mut expression = expression.clone();
        normalize(&mut expression);
        expression
    }

    // Cost: less than 0.01 s in debug; no code generation.
    #[test]
    fn optional_chains_keep_non_null_assertions_rejected() {
        let prefix = "class Value { v: i32 = 1; } class Holder { x: Value | null = null; } ";
        for body in ["return a?.x! ?? b;", "a?.x!; return b;"] {
            let source =
                format!("{prefix}function read(a: Holder | null, b: Value): Value {{ {body} }}");
            let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
                .expect_err("the assertion must stay rejected");
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains("the `!` assertion")),
                "{diagnostics:?}"
            );
        }
        check_program(&[SourceFile::new(
            "test.ts",
            format!(
                "{prefix}function read(a: Holder | null, b: Value): Value {{ return a?.x ?? b; }}"
            ),
        )])
        .expect("the chain without an assertion must check");
    }

    // Cost: less than 0.01 s in debug; no code generation.
    #[test]
    fn arrow_parameter_initializers_keep_their_values_and_type_checks() {
        let module = check_program(&[SourceFile::new("test.ts", "export function main(): void { const f = (x: i32 = 7): i32 => x; const y: i32 = f(); }")]).expect("the default must check");
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main");
        let crate::hir::Stmt::Let { init, .. } = &main.body[0] else {
            panic!("lambda binding");
        };
        let crate::hir::ExprKind::Lambda { params, .. } = &init.kind else {
            panic!("lambda");
        };
        assert!(matches!(
            params[0].default.as_ref().map(|value| &value.kind),
            Some(crate::hir::ExprKind::Int(7))
        ));
        let diagnostics = check_program(&[SourceFile::new(
            "test.ts",
            "const f = (x: i32 = 'bad'): i32 => x;",
        )])
        .expect_err("the initializer must take its parameter type");
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("the default value expects `i32`, got `string`")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn accessor_compound_assignment_matches_the_read_then_write_hir() {
        let class = "class Counter {\n  value: i32 = 1;\n  get v(): i32 { return this.value; }\n  set v(value: i32) { this.value = value; }\n}\n";
        let sugar = format!(
            "{class}export function main(): void {{\n  const counter: Counter = new Counter();\n  counter.v += 2;\n}}\n"
        );
        let spelled = format!(
            "{class}export function main(): void {{\n  const counter: Counter = new Counter();\n  counter.v = counter.v + 2;\n}}\n"
        );
        assert_eq!(
            normalized_main_expression(&sugar),
            normalized_main_expression(&spelled)
        );
    }

    #[test]
    fn static_accessor_compound_assignment_matches_the_read_then_write_hir() {
        let class = "class Counter {\n  static value: i32 = 1;\n  static get v(): i32 { return Counter.value; }\n  static set v(value: i32) { Counter.value = value; }\n}\n";
        let sugar = format!("{class}export function main(): void {{\n  Counter.v += 2;\n}}\n");
        let spelled =
            format!("{class}export function main(): void {{\n  Counter.v = Counter.v + 2;\n}}\n");
        assert_eq!(
            normalized_main_expression(&sugar),
            normalized_main_expression(&spelled)
        );
    }

    #[test]
    fn index_compound_assignment_matches_the_read_then_write_hir() {
        let class = "class Values {\n  [index: u32]: i32;\n  get(index: u32): i32 { return 0; }\n  set(index: u32, value: i32): void {}\n}\n";
        let sugar = format!(
            "{class}export function main(): void {{\n  const values: Values = new Values();\n  const index: u32 = 0;\n  values[index] += 2;\n}}\n"
        );
        let spelled = format!(
            "{class}export function main(): void {{\n  const values: Values = new Values();\n  const index: u32 = 0;\n  values[index] = values[index] + 2;\n}}\n"
        );
        assert_eq!(
            normalized_main_expression(&sugar),
            normalized_main_expression(&spelled)
        );
    }

    #[test]
    fn f32_from_bits_rejects_an_f64_argument_with_s007() {
        let source = "export function main(): void {\n  const value: f64 = 1.0;\n  print(`${Math.f32FromBits(value)}`);\n}\n";
        let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
            .expect_err("f32FromBits rejects an f64 argument");
        assert_eq!(diagnostics[0].code, RuleCode::S007);
        assert_eq!(diagnostics[0].pos.line, 3);
    }

    #[test]
    fn compound_assignment_keeps_its_specific_type_diagnostic() {
        let source =
            "export function main(): void {\n  let value: boolean = true;\n  value += true;\n}\n";
        let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
            .expect_err("boolean compound assignment must fail");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "operator `+=` is not defined for `boolean` and `boolean`"
        );
    }

    #[test]
    fn updates_carry_prefix_and_postfix_in_value_and_statement_positions() {
        for (spelling, kind) in [
            ("++value", hir::UpdateKind::Prefix),
            ("value++", hir::UpdateKind::Postfix),
            ("--value", hir::UpdateKind::Prefix),
            ("value--", hir::UpdateKind::Postfix),
        ] {
            for statement in [false, true] {
                let body = if statement {
                    format!("{spelling};")
                } else {
                    format!("print(`${{{spelling}}}`);")
                };
                let source =
                    format!("export function main(): void {{ let value: i32 = 3; {body} }}");
                let expression = normalized_main_expression(&source);
                fn find(expression: &hir::Expr) -> Option<hir::UpdateKind> {
                    if let hir::ExprKind::Assign { update, .. } = expression.kind {
                        if update.is_some() {
                            return update;
                        }
                    }
                    expression
                        .children()
                        .into_iter()
                        .find_map(|child| match child {
                            hir::HirChild::Expr(expression) => find(expression),
                            hir::HirChild::Stmt(_) => None,
                        })
                }
                assert_eq!(find(&expression), Some(kind), "{source}");
            }
        }
    }

    #[test]
    fn compound_shift_preserves_the_count_and_operand_type() {
        let source = "export function main(): void { let value: u8 = 1; value <<= 8; }";
        let expression = normalized_main_expression(source);
        assert_eq!(expression.ty, crate::Type::U8);
        let hir::ExprKind::Assign {
            op, target, value, ..
        } = expression.kind
        else {
            panic!("compound assignment");
        };
        assert_eq!(op, Some(hir::BinOp::Shl));
        assert_eq!(target.ty, crate::Type::U8);
        assert_eq!(value.ty, crate::Type::U8);
        assert!(matches!(value.kind, hir::ExprKind::Int(8)));
    }

    #[test]
    fn shift_operand_rules_keep_positive_controls() {
        for (bad, good, code) in [
            (
                "const x: u8 = 1; const y: u8 = x << 300;",
                "const x: u8 = 1; const y: u8 = x << 8;",
                RuleCode::S008,
            ),
            (
                "const x: u8 = 1; const k: i32 = 8; const y: u8 = x << k;",
                "const x: u8 = 1; const k: i32 = 8; const y: u8 = x << (k as u8);",
                RuleCode::S007,
            ),
            (
                "const x: f64 = 1.0; const y: f64 = x << 1;",
                "const x: i32 = 1; const y: i32 = x << 32;",
                RuleCode::S100,
            ),
        ] {
            let diagnostics = check_program(&[SourceFile::new("test.ts", bad)])
                .expect_err("invalid shift operand");
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.code == code),
                "{diagnostics:?}"
            );
            check_program(&[SourceFile::new("test.ts", good)]).expect("valid shift operand");
        }
    }
}

mod promise_all;
mod promise_reaction;

mod task_group;
mod type_arguments;
mod void_leaves;
pub(crate) use void_leaves::VoidLeafPlace;
