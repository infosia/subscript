//! The exception surface of `compiler.md` §115.1–§115.3: the Error
//! classes, `throw`, `try`/`catch`, the catch binding, and `instanceof`.
//!
//! The Error classes share one layout, so the checker
//! builds one reference class for the seven names. A hidden `u32` kind tag
//! precedes the `name` and `message` fields, and `instanceof` reads the tag
//! (§115.1 rule 3). The seven names are one static type: TypeScript reads
//! the seven `lib.es5.d.ts` interfaces as one shape, so an assignment
//! between two of them is `tsc`-clean in both directions.

use super::FactSet;
use crate::check::rejection::RejectionSite;
use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::Pos;
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

/// The value that an `instanceof` test narrows in its true branch, when
/// `condition` is one (§115.3 rule 5).
pub(crate) fn instanceof_narrowed_value(condition: &hir::Expr) -> Option<&hir::Expr> {
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
    Some(obj)
}

fn local_expr(name: &str, ty: Type, pos: Pos) -> hir::Expr {
    hir::Expr {
        pending_work: None,
        kind: ExprKind::Local(name.to_string(), ty.clone(), false),
        ty,
        pos,
    }
}

fn this_field_store(class: ClassId, field: &str, ty: Type, pos: &Pos) -> hir::Stmt {
    hir::Stmt::Expr(hir::Expr {
        pending_work: None,
        kind: ExprKind::Assign {
            update: None,
            op: None,
            target: Box::new(hir::Expr {
                pending_work: None,
                kind: ExprKind::Field {
                    obj: Box::new(hir::Expr {
                        pending_work: None,
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
    /// Declares the shared Error-family layout (compiler.md §119.1 rule 6).
    pub(crate) fn declare_error_class(&mut self) -> ClassId {
        let pos = &Pos::new("", 0, 0);
        let id = self.new_class("Error", false, false, None, pos.clone());
        let field = |name: &str, ty: Type| hir::Field {
            written_non_null: false,
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
            default_can_raise: false,
            name: name.to_string(),
            ty,
            default: None,
            foreign_provenance: None,
            pos: pos.clone(),
        };
        self.classes[id.0].ctor = Some(hir::Function {
            synthesized_helper: false,
            symbol: hir::Symbol::from_full_text("constructor"),
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
        id
    }

    /// Whether an ambient Error-family name is visible here: no local and
    /// no program declaration shadows it.
    pub(crate) fn error_name_is_ambient(&self, name: &str, fx: &FnCtx) -> bool {
        ErrorKind::from_name(name).is_some() && self.ambient_visible(name, fx)
    }

    /// Whether `ty` is the Error class.
    pub(crate) fn is_error_type(&self, ty: &Type) -> bool {
        matches!(self.apparent_type(ty), Type::Class(id) if id == self.error_class)
    }

    /// Whether `ty` is the Error class or holds it at any depth: as an
    /// array element, a nullable, or a field of a class that it reaches.
    /// The Error classes are not JSON types (§115.1 rule 6).
    pub(crate) fn type_holds_error(&self, ty: &Type) -> bool {
        fn visit(checker: &Checker<'_>, ty: &Type, seen: &mut HashSet<ClassId>) -> bool {
            if checker.is_error_type(ty) {
                return true;
            }
            match &checker.apparent_type(ty) {
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
        if n.type_args.is_some() {
            self.reject_subset(
                RejectionSite::ErrorConstructorTypeArguments,
                format!("`{name}` is not generic"),
                pos.clone(),
            );
        }
        let arguments: &[ast::ExprOrSpread] = n.args.as_deref().unwrap_or(&[]);
        let message = match arguments {
            [] => hir::Expr {
                pending_work: None,
                kind: ExprKind::Str(String::new()),
                ty: Type::Str,
                pos: pos.clone(),
            },
            [argument] if argument.spread.is_none() => {
                let message = self.check_expr(&argument.expr, Some(&Type::Str), fx);
                if !matches!(self.apparent_type(&message.ty), Type::Str | Type::Error) {
                    let found = self.type_name(&message.ty);
                    self.reject_subset(
                        if matches!(self.apparent_type(&message.ty), Type::StringAlias(_)) {
                            RejectionSite::ErrorMessageType
                        } else {
                            RejectionSite::ErrorMessageNonString
                        },
                        format!("the `{name}` message must be a `string`, got `{found}`"),
                        message.pos.clone(),
                    );
                }
                message
            }
            _ => {
                self.reject_subset(
                    RejectionSite::ErrorConstructorArguments,
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
        let class = self.error_class;
        hir::Expr {
            pending_work: None,
            kind: ExprKind::New {
                class,
                args: vec![
                    hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Int(kind.tag()),
                        ty: Type::U32,
                        pos: pos.clone(),
                    },
                    hir::Expr {
                        pending_work: None,
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
        self.reject_subset(RejectionSite::ErrorCallWithoutNew, format!("`{name}` is constructed with `new {name}(message)`; a call without `new` is rejected"), pos.clone());
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
        if !local.caught
            || fx
                .narrowed
                .get(name)
                .is_some_and(|fact| fact.narrows_type())
        {
            return false;
        }
        self.reject_subset(
            RejectionSite::CatchBindingUnnarrowedUse,
            format!(
                "the catch binding `{name}` is used outside `instanceof` and `throw`; \
                 narrow it first with `{name} instanceof Error`"
            ),
            pos.clone(),
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
                if !matches!(self.apparent_type(&value.ty), Type::Error)
                    && !self.is_error_type(&value.ty)
                {
                    let found = self.type_name(&value.ty);
                    self.reject_subset(
                        RejectionSite::ThrowOperandNotErrorFamily,
                        format!(
                            "`throw` requires an Error-family object; \
                             this operand has type `{found}`"
                        ),
                        pos.clone(),
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
            self.reject_subset(
                RejectionSite::FinallyClause,
                "`finally` is not in the decided exception surface; \
                 repeat the cleanup after the `try` statement and in its `catch` block",
                self.pos(finalizer.span),
            );
        }
        let Some(handler) = &t.handler else {
            return false;
        };
        let mut note_paths = fx.narrowing_note_paths();
        let base = fx.narrowed.clone();
        let initial_notes = fx.ended_shared_narrowing.clone();
        let (body, _) = self.check_block(&t.block.stmts, fx);
        let body_facts = fx.narrowed.clone();
        let mut body_notes = fx.ended_shared_narrowing.clone();
        let effects = self.body_narrowing_effects(&body, fx, &base, &initial_notes);
        fx.ended_shared_narrowing = initial_notes;
        fx.narrowed = base.clone();
        if let Some(effects) = &effects {
            self.apply_narrowing_effects(effects, fx);
        }
        let binding = self.catch_binding(handler);
        let class = self.error_class;
        fx.scopes.push(Default::default());
        if let Some((name, binding_pos)) = &binding {
            self.declare_local(
                name,
                Local {
                    annotated: false,
                    ty: Type::Class(class),
                    mutable: false,
                    async_origins: HashSet::new(),
                    caught: true,
                    function_value_required: None,
                },
                binding_pos.clone(),
                fx,
            );
        }
        let (handler_body, _) = self.check_block(&handler.body.stmts, fx);
        fx.pop_scope();
        let handler_facts = fx.narrowed.clone();
        let body_terminates = !super::fallthrough::sequence_can_fall_through(&body);
        let handler_terminates = !super::fallthrough::sequence_can_fall_through(&handler_body);
        let body_possible: HashSet<_> = body_facts.union(&body_notes).cloned().collect();
        let handler_possible: HashSet<_> = handler_facts
            .union(&fx.ended_shared_narrowing)
            .cloned()
            .collect();
        if body_terminates {
            note_paths.extend_facts(handler_possible);
        } else if handler_terminates {
            note_paths.extend_facts(body_possible);
        } else {
            note_paths.extend_facts(body_possible.intersection(&handler_possible).cloned());
        }
        if !handler_terminates {
            // A nullable store can reach the handler before a later restoration or call.
            body_notes.retain(|key| {
                handler_facts.contains(key)
                    || effects
                        .as_ref()
                        .is_none_or(|effects| !effects.nullable_store_ends(key))
            });
        }
        fx.narrowed = if body_terminates {
            handler_facts
        } else if handler_terminates {
            body_facts
        } else {
            body_facts.intersection(&handler_facts).cloned().collect()
        };
        fx.ended_shared_narrowing.extend_facts(body_notes);
        fx.finish_narrowing_join(&note_paths);
        out.push(hir::Stmt::Try {
            body,
            binding: binding.map(|(name, _)| (name, Type::Class(class))),
            handler: handler_body,
            pos,
        });
        !super::fallthrough::sequence_can_fall_through(&out[out.len() - 1..])
    }

    /// Checks one block in its own scope.
    fn check_block(&mut self, statements: &[ast::Stmt], fx: &mut FnCtx) -> (Vec<hir::Stmt>, bool) {
        let reachable = fx.flow_reachable;
        fx.scopes.push(Default::default());
        self.reserve_block_declarations(statements, fx);
        let mut body = Vec::new();
        for statement in statements {
            self.check_stmt(statement, fx, &mut body);
        }
        let terminates = !super::fallthrough::sequence_can_fall_through(&body);
        self.end_scope_narrowing(fx);
        fx.pop_scope();
        fx.flow_reachable = reachable;
        (body, terminates)
    }

    /// The catch binding name and position (§115.3 rule 1).
    fn catch_binding(&mut self, handler: &ast::CatchClause) -> Option<(String, Pos)> {
        let param = handler.param.as_ref()?;
        let ast::Pat::Ident(binding) = param else {
            let any_annotation = match param {
                ast::Pat::Array(p) => p.type_ann.as_ref(),
                ast::Pat::Object(p) => p.type_ann.as_ref(),
                _ => None,
            }.is_some_and(|a| matches!(a.type_ann.as_ref(), ast::TsType::TsKeywordType(k) if k.kind == ast::TsKeywordTypeKind::TsAnyKeyword));
            self.reject_subset(if any_annotation { RejectionSite::CatchBindingPattern } else { RejectionSite::CatchBindingPatternWithoutAny }, "a catch binding is one name; a binding pattern is not in the decided exception surface", self.pos(param.span()));
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
                self.reject_subset(
                    if matches!(annotation.type_ann.as_ref(), ast::TsType::TsKeywordType(k) if k.kind == ast::TsKeywordTypeKind::TsAnyKeyword) { RejectionSite::CatchBindingAnnotation } else { RejectionSite::CatchBindingInvalidAnnotation },
                    "a catch binding annotation other than `unknown` is rejected; \
                     write `catch (e)` or `catch (e: unknown)`",
                    self.pos(binding.span()),
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
        // A type-only import binding as the right operand is a value use
        // (compiler.md §134 rule 2): its S100 is the only diagnostic.
        if let ast::Expr::Ident(id) = right {
            let name = id.sym.as_ref();
            let type_only = !fx.owns_local_name(name)
                && self
                    .scope_binding(name)
                    .is_some_and(|binding| binding.type_only);
            if type_only {
                let right_pos = self.pos(id.span);
                self.scope_item(name, &right_pos);
                if self.caught_operand(&b.left, fx).is_none() {
                    self.check_expr(&b.left, None, fx);
                }
                return self.err_expr(pos);
            }
        }
        let kind = match right {
            ast::Expr::Ident(id) if self.error_name_is_ambient(id.sym.as_ref(), fx) => {
                ErrorKind::from_name(id.sym.as_ref())
            }
            _ => None,
        };
        let Some(kind) = kind else {
            self.reject_subset(
                if matches!(right, ast::Expr::Ident(id) if matches!(self.peek_scope_item(id.sym.as_ref()), Some(super::ScopeItem::Class(_) | super::ScopeItem::GenericClass(_) | super::ScopeItem::Func(_)))) { RejectionSite::InstanceofRightNotErrorFamily } else { RejectionSite::InstanceofRightNotClass },
                "`instanceof` requires an Error-family class as its right operand",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        let class = self.error_class;
        let value = match self.caught_operand(&b.left, fx) {
            Some(caught) => caught,
            None => self.check_expr(&b.left, None, fx),
        };
        if !matches!(self.apparent_type(&value.ty), Type::Error) && !self.is_error_type(&value.ty) {
            let found = self.type_name(&value.ty);
            self.reject_subset(
                if matches!(
                    self.apparent_type(&value.ty),
                    Type::Class(_)
                        | Type::Func(_)
                        | Type::Array(_)
                        | Type::FixedArray(..)
                        | Type::Map(..)
                        | Type::Set(_)
                ) {
                    RejectionSite::InstanceofLeftNotErrorFamily
                } else {
                    RejectionSite::InstanceofLeftPrimitive
                },
                format!(
                    "`instanceof` tests an Error-family object or a catch binding; \
                     this operand has type `{found}`"
                ),
                value.pos.clone(),
            );
            return self.err_expr(pos);
        }
        let tag = hir::Expr {
            pending_work: None,
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
            pending_work: None,
            kind: ExprKind::Binary {
                op,
                left: Box::new(tag),
                right: Box::new(hir::Expr {
                    pending_work: None,
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
