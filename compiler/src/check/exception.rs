//! The exception surface of `compiler.md` §115.1–§115.3: the Error
//! classes, `throw`, `try`/`catch`, the catch binding, and `instanceof`.
//!
//! The Error classes share one layout, so the checker
//! builds one reference class for the seven names. A hidden `u32` kind tag
//! precedes the `name` and `message` fields, and `instanceof` reads the tag
//! (§115.1 rule 3). The seven names are one static type: TypeScript reads
//! the seven `lib.es5.d.ts` interfaces as one shape, so an assignment
//! between two of them is `tsc`-clean in both directions.

use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, BinOp, ExprKind, ERROR_KIND_FIELD};
use crate::types::{ClassId, Type};

use super::{Checker, FnCtx, Local};

/// The class names and stable tags of the Error family (stdlib.md §19.1).
pub(crate) const ERROR_CLASSES: &[&str] = &[
    "Error",
    "SyntaxError",
    "TypeError",
    "RangeError",
    "ReferenceError",
    "EvalError",
    "URIError",
];

/// One index into the Error class table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ErrorKind(usize);

impl ErrorKind {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        ERROR_CLASSES
            .iter()
            .position(|candidate| *candidate == name)
            .map(Self)
    }

    fn tag(self) -> i64 {
        self.0 as i64
    }
    fn name(self) -> &'static str {
        ERROR_CLASSES[self.0]
    }
}

/// The path that an `instanceof` test narrows in its true branch, when
/// `condition` is one (§115.3 rule 5).
pub(crate) fn instanceof_narrowed_path(condition: &hir::Expr) -> Option<String> {
    let ExprKind::Binary {
        op: BinOp::Eq | BinOp::Lt,
        left,
        ..
    } = &condition.kind
    else {
        return None;
    };
    let ExprKind::Field { obj, name } = &left.kind else {
        return None;
    };
    if name != ERROR_KIND_FIELD {
        return None;
    }
    super::expr::path_key(obj)
}

fn local_expr(name: &str, ty: Type, pos: Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Local(name.to_string()),
        ty,
        pos,
    }
}

fn this_field_store(class: ClassId, field: &str, ty: Type, pos: &Pos) -> hir::Stmt {
    hir::Stmt::Expr(hir::Expr {
        kind: ExprKind::Assign {
            op: None,
            target: Box::new(hir::Expr {
                kind: ExprKind::Field {
                    obj: Box::new(hir::Expr {
                        kind: ExprKind::This,
                        ty: Type::Class(class),
                        pos: pos.clone(),
                    }),
                    name: field.to_string(),
                },
                ty: ty.clone(),
                pos: pos.clone(),
            }),
            value: Box::new(local_expr(field_parameter(field), ty.clone(), pos.clone())),
        },
        ty,
        pos: pos.clone(),
    })
}

fn field_parameter(field: &str) -> &str {
    if field == ERROR_KIND_FIELD {
        "kind"
    } else {
        field
    }
}

impl Checker<'_> {
    /// The one Error class, created on first use.
    pub(crate) fn error_class(&mut self, pos: &Pos) -> ClassId {
        if let Some(id) = self.error_class {
            return id;
        }
        let name = if self.class_ids.contains_key("Error") {
            "[[Error]]"
        } else {
            "Error"
        };
        let id = self.new_class(name, false, false, None, pos.clone());
        let field = |name: &str, ty: Type| hir::Field {
            name: name.to_string(),
            ty,
            is_defaulted: false,
            is_absence_capable: false,
            init: None,
            foreign_provenance: None,
            pos: pos.clone(),
        };
        self.classes[id.0].fields = vec![
            field(ERROR_KIND_FIELD, Type::U32),
            field("name", Type::Str),
            field("message", Type::Str),
        ];
        let parameter = |name: &str, ty: Type| hir::Param {
            escapes: false,
            name: name.to_string(),
            ty,
            default: None,
            foreign_provenance: None,
            pos: pos.clone(),
        };
        self.classes[id.0].ctor = Some(hir::Function {
            name: "constructor".to_string(),
            can_raise: false,
            exported: false,
            is_generator: false,
            is_async: false,
            params: vec![
                parameter("kind", Type::U32),
                parameter("name", Type::Str),
                parameter("message", Type::Str),
            ],
            ret: Type::Void,
            body: vec![
                this_field_store(id, ERROR_KIND_FIELD, Type::U32, pos),
                this_field_store(id, "name", Type::Str, pos),
                this_field_store(id, "message", Type::Str, pos),
            ],
            pos: pos.clone(),
        });
        self.error_class = Some(id);
        id
    }

    /// Whether an ambient Error-family name is visible here: no local and
    /// no program declaration shadows it.
    pub(crate) fn error_name_is_ambient(&self, name: &str, fx: &FnCtx) -> bool {
        ErrorKind::from_name(name).is_some() && self.ambient_visible(name, fx)
    }

    /// Whether `ty` is the Error class.
    pub(crate) fn is_error_type(&self, ty: &Type) -> bool {
        matches!((ty, self.error_class), (Type::Class(id), Some(error)) if *id == error)
    }

    /// Whether `ty` is the Error class or holds it at any depth: as an
    /// array element, a nullable, or a field of a class that it reaches.
    /// The Error classes are not JSON types (§115.1 rule 6).
    pub(crate) fn type_holds_error(&self, ty: &Type) -> bool {
        fn visit(checker: &Checker<'_>, ty: &Type, seen: &mut HashSet<ClassId>) -> bool {
            if checker.is_error_type(ty) {
                return true;
            }
            match ty {
                Type::Array(element) | Type::FixedArray(element, _) | Type::Nullable(element) => {
                    visit(checker, element, seen)
                }
                Type::Class(id) => {
                    if !seen.insert(*id) {
                        return false;
                    }
                    checker.classes.get(id.0).is_some_and(|class| {
                        class
                            .fields
                            .iter()
                            .any(|field| visit(checker, &field.ty, seen))
                    })
                }
                _ => false,
            }
        }
        visit(self, ty, &mut HashSet::new())
    }

    /// Constructs an Error-family object with an optional message.
    /// (§115.1 rule 2).
    pub(crate) fn check_error_new(
        &mut self,
        name: &str,
        n: &ast::NewExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let Some(kind) = ErrorKind::from_name(name) else {
            return self.err_expr(pos);
        };
        self.error_class(&pos);
        if n.type_args.is_some() {
            self.error(
                RuleCode::S100,
                format!("`{name}` is not generic"),
                pos.clone(),
            );
        }
        let arguments: &[ast::ExprOrSpread] = n.args.as_deref().unwrap_or(&[]);
        let message = match arguments {
            [] => hir::Expr {
                kind: ExprKind::Str(String::new()),
                ty: Type::Str,
                pos: pos.clone(),
            },
            [argument] if argument.spread.is_none() => {
                let message = self.check_expr(&argument.expr, Some(&Type::Str), fx);
                if !matches!(message.ty, Type::Str | Type::Error) {
                    let found = self.type_name(&message.ty);
                    self.error(
                        RuleCode::S100,
                        format!("the `{name}` message must be a `string`, got `{found}`"),
                        message.pos.clone(),
                    );
                }
                message
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    format!("`new {name}` takes one `string` message argument"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        self.error_new(kind, message, pos)
    }

    /// The construction of one Error-family object with `message`.
    pub(crate) fn error_new(&mut self, kind: ErrorKind, message: hir::Expr, pos: Pos) -> hir::Expr {
        let class = self.error_class(&pos);
        hir::Expr {
            kind: ExprKind::New {
                class,
                args: vec![
                    hir::Expr {
                        kind: ExprKind::Int(kind.tag()),
                        ty: Type::U32,
                        pos: pos.clone(),
                    },
                    hir::Expr {
                        kind: ExprKind::Str(kind.name().to_string()),
                        ty: Type::Str,
                        pos: pos.clone(),
                    },
                    message,
                ],
            },
            ty: Type::Class(class),
            pos,
        }
    }

    /// A call of an Error-family name without `new` (§115.1 rule 2).
    pub(crate) fn reject_error_call(&mut self, name: &str, pos: Pos) -> hir::Expr {
        self.error(
            RuleCode::S100,
            format!("`{name}` is constructed with `new {name}(message)`; a call without `new` is rejected"),
            pos.clone(),
        );
        self.err_expr(pos)
    }

    /// The caught binding that `expr` names, before a narrowing test.
    fn caught_operand(&mut self, expr: &ast::Expr, fx: &mut FnCtx) -> Option<hir::Expr> {
        let mut expr = expr;
        while let ast::Expr::Paren(paren) = expr {
            expr = &paren.expr;
        }
        let ast::Expr::Ident(id) = expr else {
            return None;
        };
        let name = id.sym.to_string();
        // Peek without diagnostics: the scope that owns the name decides.
        let caught = fx
            .scopes
            .iter()
            .rev()
            .find(|scope| scope.vars.contains_key(&name) || scope.pending.contains(&name))
            .and_then(|scope| scope.vars.get(&name))
            .is_some_and(|local| local.caught);
        if !caught {
            return None;
        }
        let pos = self.pos(id.span);
        let local = self.lookup_local(&name, &pos, fx)?;
        Some(local_expr(&name, local.ty, pos))
    }

    /// Reports a read of an unnarrowed catch binding (§115.3 rule 4).
    /// Answers true when the read is rejected.
    pub(crate) fn reject_caught_read(
        &mut self,
        name: &str,
        local: &Local,
        pos: &Pos,
        fx: &FnCtx,
    ) -> bool {
        if !local.caught || fx.narrowed.contains(name) {
            return false;
        }
        self.error_diverging(
            RuleCode::S010,
            format!(
                "the catch binding `{name}` is used outside `instanceof` and `throw`; \
                 narrow it first with `{name} instanceof Error`"
            ),
            pos.clone(),
            Divergence::Exceptions,
        );
        true
    }

    /// `throw expr` (§115.2).
    pub(super) fn check_throw(
        &mut self,
        t: &ast::ThrowStmt,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) -> bool {
        let pos = self.pos(t.span);
        let value = match self.caught_operand(&t.arg, fx) {
            Some(caught) => caught,
            None => {
                let value = self.check_expr(&t.arg, None, fx);
                if !matches!(value.ty, Type::Error) && !self.is_error_type(&value.ty) {
                    let found = self.type_name(&value.ty);
                    self.error_diverging(
                        RuleCode::S010,
                        format!(
                            "`throw` requires an Error-family object; \
                             this operand has type `{found}`"
                        ),
                        pos.clone(),
                        Divergence::Exceptions,
                    );
                }
                value
            }
        };
        out.push(hir::Stmt::Throw { value, pos });
        true
    }

    /// `try { … } catch (e) { … }` (§115.3).
    pub(super) fn check_try(
        &mut self,
        t: &ast::TryStmt,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) -> bool {
        let pos = self.pos(t.span);
        if let Some(finalizer) = &t.finalizer {
            self.error_diverging(
                RuleCode::S010,
                "`finally` is not in the decided exception surface; \
                 repeat the cleanup after the `try` statement and in its `catch` block",
                self.pos(finalizer.span),
                Divergence::Exceptions,
            );
        }
        let Some(handler) = &t.handler else {
            return false;
        };
        let base = fx.narrowed.clone();
        let (body, body_terminates) = self.check_block(&t.block.stmts, fx);
        let mut assigned = HashSet::new();
        for statement in &t.block.stmts {
            super::stmt::assigned_roots_stmt(statement, &mut assigned);
        }
        // The handler starts after any prefix of the `try` block, so a
        // fact that the block can change does not hold there.
        fx.narrowed = base
            .iter()
            .filter(|key| !assigned.contains(super::stmt::root_of(key)))
            .cloned()
            .collect();
        let binding = self.catch_binding(handler);
        let class = self.error_class(&pos);
        fx.scopes.push(Default::default());
        if let Some((name, binding_pos)) = &binding {
            self.declare_local(
                name,
                Local {
                    ty: Type::Class(class),
                    mutable: false,
                    async_origins: HashSet::new(),
                    caught: true,
                },
                binding_pos.clone(),
                fx,
            );
        }
        let (handler_body, handler_terminates) = self.check_block(&handler.body.stmts, fx);
        fx.scopes.pop();
        for statement in &handler.body.stmts {
            super::stmt::assigned_roots_stmt(statement, &mut assigned);
        }
        fx.narrowed = base
            .iter()
            .filter(|key| !assigned.contains(super::stmt::root_of(key)))
            .cloned()
            .collect();
        out.push(hir::Stmt::Try {
            body,
            binding: binding.map(|(name, _)| (name, Type::Class(class))),
            handler: handler_body,
            pos,
        });
        body_terminates && handler_terminates
    }

    /// Checks one block in its own scope.
    fn check_block(&mut self, statements: &[ast::Stmt], fx: &mut FnCtx) -> (Vec<hir::Stmt>, bool) {
        fx.scopes.push(Default::default());
        self.reserve_block_declarations(statements, fx);
        let mut body = Vec::new();
        let mut terminates = false;
        for statement in statements {
            terminates |= self.check_stmt(statement, fx, &mut body);
        }
        fx.scopes.pop();
        (body, terminates)
    }

    /// The catch binding name and position (§115.3 rule 1).
    fn catch_binding(&mut self, handler: &ast::CatchClause) -> Option<(String, Pos)> {
        let param = handler.param.as_ref()?;
        let ast::Pat::Ident(binding) = param else {
            self.error_diverging(
                RuleCode::S010,
                "a catch binding is one name; a binding pattern is not in the decided exception surface",
                self.pos(param.span()),
                Divergence::Exceptions,
            );
            return None;
        };
        let pos = self.pos(binding.id.span);
        if let Some(annotation) = &binding.type_ann {
            let unknown = matches!(
                annotation.type_ann.as_ref(),
                ast::TsType::TsKeywordType(keyword)
                    if keyword.kind == ast::TsKeywordTypeKind::TsUnknownKeyword
            );
            if !unknown {
                self.error_diverging(
                    RuleCode::S010,
                    "a catch binding annotation other than `unknown` is rejected; \
                     write `catch (e)` or `catch (e: unknown)`",
                    self.pos(binding.span()),
                    Divergence::Exceptions,
                );
            }
        }
        Some((binding.id.sym.to_string(), pos))
    }

    /// `value instanceof C` (§115.3 rule 5).
    pub(crate) fn check_instanceof(
        &mut self,
        b: &ast::BinExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let mut right: &ast::Expr = &b.right;
        while let ast::Expr::Paren(paren) = right {
            right = &paren.expr;
        }
        let kind = match right {
            ast::Expr::Ident(id) if self.error_name_is_ambient(id.sym.as_ref(), fx) => {
                ErrorKind::from_name(id.sym.as_ref())
            }
            _ => None,
        };
        let Some(kind) = kind else {
            self.error_diverging(
                RuleCode::S100,
                "`instanceof` requires an Error-family class as its right operand",
                pos.clone(),
                Divergence::InstanceofNonError,
            );
            return self.err_expr(pos);
        };
        let class = self.error_class(&pos);
        let value = match self.caught_operand(&b.left, fx) {
            Some(caught) => caught,
            None => self.check_expr(&b.left, None, fx),
        };
        if !matches!(value.ty, Type::Error) && !self.is_error_type(&value.ty) {
            let found = self.type_name(&value.ty);
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "`instanceof` tests an Error-family object or a catch binding; \
                     this operand has type `{found}`"
                ),
                value.pos.clone(),
                Divergence::InstanceofNonError,
            );
            return self.err_expr(pos);
        }
        let tag = hir::Expr {
            kind: ExprKind::Field {
                obj: Box::new(hir::Expr {
                    ty: Type::Class(class),
                    ..value
                }),
                name: ERROR_KIND_FIELD.to_string(),
            },
            ty: Type::U32,
            pos: pos.clone(),
        };
        let (op, constant) = match kind {
            // Every Error-family object is an `Error`.
            ErrorKind(0) => (BinOp::Lt, ERROR_CLASSES.len() as i64),
            other => (BinOp::Eq, other.tag()),
        };
        hir::Expr {
            kind: ExprKind::Binary {
                op,
                left: Box::new(tag),
                right: Box::new(hir::Expr {
                    kind: ExprKind::Int(constant),
                    ty: Type::U32,
                    pos: pos.clone(),
                }),
            },
            ty: Type::Bool,
            pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_table_preserves_tags_and_names() {
        let expected = [
            "Error",
            "SyntaxError",
            "TypeError",
            "RangeError",
            "ReferenceError",
            "EvalError",
            "URIError",
        ];
        assert_eq!(ERROR_CLASSES, expected);
        for (tag, name) in expected.iter().enumerate() {
            let kind = ErrorKind::from_name(name).unwrap();
            assert_eq!(kind.tag(), tag as i64);
            assert_eq!(kind.name(), *name);
        }
        assert_eq!(ErrorKind::from_name("UnknownError"), None);
    }
}
