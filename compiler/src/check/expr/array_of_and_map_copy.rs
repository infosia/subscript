//! Fixed-arity array construction and shallow Map copies (stdlib.md §9.11 and §10.9).
use super::*;
use swc_common::Spanned;

use crate::check::{ContainerSlot, FnCtx};
use crate::diag::RuleCode;
use crate::hir::{Callee, MapFn};

impl<'p> Checker<'p> {
    pub(super) fn check_array_of(
        &mut self,
        call: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        if call.args.len() > 1 || call.args.iter().any(|arg| arg.spread.is_some()) {
            self.reject_api_form("Array", "of(value, …)", "Array.of", prop_pos);
            self.check_poisoned_arguments(&call.args, fx);
            return self.err_expr(pos);
        }
        let declared = match &call.type_args {
            Some(args) if args.params.len() == 1 => Some(self.resolve_type(
                &ast::TsType::TsArrayType(ast::TsArrayType {
                    span: args.span,
                    elem_type: args.params[0].clone(),
                }),
            )),
            Some(_) => {
                self.error(
                    RuleCode::S100,
                    "`Array.of<T>` takes exactly one type argument",
                    prop_pos,
                );
                self.check_poisoned_arguments(&call.args, fx);
                return self.err_expr(pos);
            }
            None => None,
        };
        let literal = ast::ArrayLit {
            span: call.span,
            elems: call.args.iter().cloned().map(Some).collect(),
        };
        let context = declared
            .as_ref()
            .or_else(|| ctx.filter(|ty| matches!(ty, Type::Array(_))));
        self.check_array_lit(&literal, context, fx, pos)
    }

    pub(super) fn check_map_copy(
        &mut self,
        call: &ast::NewExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
        ident_pos: Pos,
    ) -> hir::Expr {
        let declared = match &call.type_args {
            Some(args) if args.params.len() == 2 => {
                let saved_context = self.enter_container_context(ctx);
                let saved = self.in_assoc_key;
                self.in_assoc_key = true;
                let key = self.resolve_type(&args.params[0]);
                self.in_assoc_key = saved;
                let key_pos = self.pos(args.params[0].span());
                let key = self.container_argument(ContainerSlot::MapKey, key, key_pos);
                if key != Type::Error && self.assoc_key_kind(&key).is_none() {
                    self.error(
                        RuleCode::S014,
                        "type is not a permitted Map/Set key kind (Q24)",
                        pos.clone(),
                    );
                }
                let value = self.resolve_type(&args.params[1]);
                let value_pos = self.pos(args.params[1].span());
                let value = self.container_argument(ContainerSlot::MapValue, value, value_pos);
                self.leave_container_context(saved_context);
                Some(Type::map(key, value))
            }
            Some(_) => {
                self.error(
                    RuleCode::S100,
                    "`new Map` takes exactly 2 type argument(s)",
                    ident_pos,
                );
                return self.err_expr(pos);
            }
            None => None,
        };
        let arguments = call.args.as_deref().unwrap_or(&[]);
        if let [argument] = arguments {
            if argument.spread.is_none() {
                let diagnostics_before = self.diags.len();
                let source = self.check_expr(&argument.expr, declared.as_ref(), fx);
                if source.ty == Type::Error || self.diags.len() > diagnostics_before {
                    return self.err_expr(pos);
                }
                if matches!(source.ty, Type::Nullable(_)) {
                    self.error(
                        RuleCode::S011,
                        format!(
                            "`{}` may be null here; narrow with a null check first",
                            self.type_name(&source.ty)
                        ),
                        source.pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if matches!(source.ty, Type::Map(_, _)) {
                    let ty = declared.unwrap_or_else(|| source.ty.clone());
                    self.require_assignable(&source.ty, &ty, source.pos.clone(), "the Map source");
                    return hir::Expr {
                        kind: hir::ExprKind::Call {
                            callee: Callee::Map(MapFn::New),
                            args: vec![source],
                        },
                        ty,
                        pos,
                    };
                }
            }
        }
        self.reject_api_form("Map", "new Map(iterable)", "new Map(iterable)", pos.clone());
        self.err_expr(pos)
    }
}
