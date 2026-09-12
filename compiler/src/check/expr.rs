//! Expression checking: contextual literal typing (C4), sized-numeric
//! arithmetic (C3/Q18), nominal member access (C1/Q4/Q5), calls, `as`
//! conversions, lambdas (C5), and null narrowing at use sites (C7).

mod aggregate;
mod assign;
mod call;
mod entry;
mod lambda;
mod literal;
mod member;
mod method;
mod namespace;
mod operator;

use swc_ecma_ast as ast;

use crate::diag::Pos;
use crate::hir::{self, BinOp, Callee, ExprKind};
use crate::types::{ClassId, Type};

use super::{static_member_symbol, Checker};

enum PlaceSource<'a> {
    Ident(&'a ast::Ident),
    Member(&'a ast::MemberExpr),
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
        member: &'a ast::MemberExpr,
        tested: bool,
    },
    Call {
        call: &'a ast::OptCall,
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
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(receiver),
                        name: "get".to_string(),
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
                kind: ExprKind::Call {
                    callee: if let Some(receiver) = receiver {
                        Callee::Method {
                            recv: Box::new(receiver),
                            name,
                        }
                    } else {
                        Callee::Func(static_member_symbol(&checker.classes[class.0].name, &name))
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

/// Dotted path key for narrowing (`node`, `node.next`, `this.x`).
pub(crate) fn path_key(e: &hir::Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Local(n) | ExprKind::Global(n) => Some(n.clone()),
        ExprKind::This => Some("this".to_string()),
        ExprKind::Field { obj, name } => path_key(obj).map(|p| format!("{}.{}", p, name)),
        ExprKind::JsonResultValue(obj) => path_key(obj).map(|p| format!("{p}.value")),
        _ => None,
    }
}

fn is_place_expr(expr: &hir::Expr) -> bool {
    match &expr.kind {
        ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::This => true,
        ExprKind::Field { obj, .. } => is_place_expr(obj),
        ExprKind::Index { obj, index, .. } => is_place_expr(obj) && is_place_expr(index),
        _ => false,
    }
}

/// True for literals that can adopt a contextual type: numeric literals
/// (C4) and string literals in a Q32 alias context.
fn literalish(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Lit(ast::Lit::Num(_) | ast::Lit::Str(_)) => true,
        ast::Expr::Paren(p) => literalish(&p.expr),
        ast::Expr::Unary(u) if u.op == ast::UnaryOp::Minus => literalish(&u.arg),
        _ => false,
    }
}

/// Recognizes the one token whose ordinary identifier path is banned by C7.
/// The checker admits it before general expression checking only in a
/// strict comparison against an absence-capable descriptor member (§43).
fn is_undefined_ident(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Ident(id) => id.sym.as_ref() == "undefined",
        ast::Expr::Paren(paren) => is_undefined_ident(&paren.expr),
        _ => false,
    }
}

fn unparen_expr(mut e: &ast::Expr) -> &ast::Expr {
    while let ast::Expr::Paren(paren) = e {
        e = &paren.expr;
    }
    e
}

fn flatten_optional_chain<'a>(
    chain: &'a ast::OptChainExpr,
    steps: &mut Vec<OptionalStep<'a>>,
) -> &'a ast::Expr {
    match chain.base.as_ref() {
        ast::OptChainBase::Member(member) => {
            let root = flatten_optional_operand(&member.obj, steps);
            steps.push(OptionalStep::Member {
                member,
                tested: chain.optional,
            });
            root
        }
        ast::OptChainBase::Call(call) => {
            let root = flatten_optional_operand(&call.callee, steps);
            steps.push(OptionalStep::Call {
                call,
                tested: chain.optional,
            });
            root
        }
    }
}

fn flatten_optional_operand<'a>(
    expression: &'a ast::Expr,
    steps: &mut Vec<OptionalStep<'a>>,
) -> &'a ast::Expr {
    match unparen_expr(expression) {
        ast::Expr::OptChain(chain) => flatten_optional_chain(chain, steps),
        other => other,
    }
}

fn opt_call_as_call(call: &ast::OptCall) -> ast::CallExpr {
    ast::CallExpr {
        span: call.span,
        ctxt: call.ctxt,
        callee: ast::Callee::Expr(call.callee.clone()),
        args: call.args.clone(),
        type_args: call.type_args.clone(),
    }
}

fn assign_op(op: ast::AssignOp) -> Option<(BinOp, &'static str)> {
    use ast::AssignOp as A;
    Some(match op {
        A::AddAssign => (BinOp::Add, "+="),
        A::SubAssign => (BinOp::Sub, "-="),
        A::MulAssign => (BinOp::Mul, "*="),
        A::DivAssign => (BinOp::Div, "/="),
        A::ModAssign => (BinOp::Rem, "%="),
        A::BitAndAssign => (BinOp::BitAnd, "&="),
        A::BitOrAssign => (BinOp::BitOr, "|="),
        A::BitXorAssign => (BinOp::BitXor, "^="),
        A::LShiftAssign => (BinOp::Shl, "<<="),
        A::RShiftAssign => (BinOp::Shr, ">>="),
        A::ZeroFillRShiftAssign => (BinOp::UShr, ">>>="),
        _ => return None,
    })
}

fn assign_binary_op(op: BinOp) -> Option<ast::BinaryOp> {
    use ast::BinaryOp as B;
    Some(match op {
        BinOp::Add => B::Add,
        BinOp::Sub => B::Sub,
        BinOp::Mul => B::Mul,
        BinOp::Div => B::Div,
        BinOp::Rem => B::Mod,
        BinOp::BitAnd => B::BitAnd,
        BinOp::BitOr => B::BitOr,
        BinOp::BitXor => B::BitXor,
        BinOp::Shl => B::LShift,
        BinOp::Shr => B::RShift,
        BinOp::UShr => B::ZeroFillRShift,
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
    Assign(&'a ast::AssignExpr),
    Update(&'a ast::UpdateExpr),
}

fn write_spelling(write: WriteExpression<'_>, target: &str) -> String {
    match write {
        WriteExpression::Update(update) => {
            let operator = if update.op == ast::UpdateOp::PlusPlus {
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
            let (operator, value) = if assign.op == ast::AssignOp::Assign {
                ("=", "v".to_string())
            } else {
                (
                    assign_op(assign.op).map_or("=", |(_, operator)| operator),
                    diagnostic_expr_spelling(&assign.right),
                )
            };
            format!("`{target} {operator} {value}`")
        }
    }
}

fn diagnostic_expr_spelling(expression: &ast::Expr) -> String {
    match unparen_expr(expression) {
        ast::Expr::Ident(identifier) => identifier.sym.to_string(),
        ast::Expr::Lit(ast::Lit::Num(number)) => number
            .raw
            .as_deref()
            .map_or_else(|| number.value.to_string(), ToString::to_string),
        ast::Expr::Lit(ast::Lit::Bool(boolean)) => boolean.value.to_string(),
        ast::Expr::Lit(ast::Lit::Null(_)) => "null".to_string(),
        _ => "v".to_string(),
    }
}

/// Returns the nominal class supplied by an object literal's context.
/// Descriptor construction goes through either `D` or `D | null` (§25,
/// §25.3a); a plain class here keeps its specific S005 rejection.
fn contextual_object_class(ctx: Option<&Type>) -> Option<ClassId> {
    match ctx? {
        Type::Class(id) => Some(*id),
        Type::Nullable(inner) => match inner.as_ref() {
            Type::Class(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum DescriptorProp<'a> {
    Expr(&'a ast::Expr),
    Shorthand(&'a ast::Ident),
}

#[derive(Clone, Copy)]
struct CallbackSpec<'a> {
    method: &'a str,
    q_rule: &'static str,
    allow_index: bool,
}

impl<'a> CallbackSpec<'a> {
    fn new(method: &'a str, q_rule: &'static str, allow_index: bool) -> Self {
        Self {
            method,
            q_rule,
            allow_index,
        }
    }
}

fn regex_literal(e: &ast::Expr) -> Option<&ast::Regex> {
    match e {
        ast::Expr::Lit(ast::Lit::Regex(regex)) => Some(regex),
        ast::Expr::Paren(paren) => regex_literal(&paren.expr),
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
            "compound assignment is not defined for `boolean`"
        );
    }

    #[test]
    fn compound_shift_preserves_the_count_and_operand_type() {
        let source = "export function main(): void { let value: u8 = 1; value <<= 8; }";
        let expression = normalized_main_expression(source);
        assert_eq!(expression.ty, crate::Type::U8);
        let hir::ExprKind::Assign { op, target, value } = expression.kind else {
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
