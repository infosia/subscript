//! `new Map` and `new Set` with explicit or inferred type arguments
//! (stdlib.md §10.1, compiler.md §197).
use super::*;
use crate::check::rejection::RejectionSite;
use swc_common::Spanned;

use crate::check::{ContainerSlot, FnCtx};
use crate::hir::{Callee, MapFn, SetFn};

/// The value type argument of a `Map` construction.
enum ValueArgument<'a> {
    Written(&'a ast::TsType),
    Inferred(Type),
}

impl<'p> Checker<'p> {
    /// Checks `new Map<K, V>()` and `new Set<K>(source)`. With no type
    /// arguments, inference gives them (§197 rule 1). `name` is `Map` or
    /// `Set`; `new Map(source)` is the Map copy form.
    pub(super) fn check_container_new(
        &mut self,
        name: &str,
        n: &ast::NewExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
        ident_pos: Pos,
    ) -> hir::Expr {
        if name == "Map" && n.args.as_ref().is_some_and(|args| !args.is_empty()) {
            return self.check_map_copy(n, ctx, fx, pos, ident_pos);
        }
        let arguments: &[ast::ExprOrSpread] = n.args.as_deref().unwrap_or(&[]);
        let mut source = None;
        let (key, key_pos, value) = match &n.type_args {
            None => {
                let Some((types, checked)) =
                    self.infer_container_arguments(name, arguments, ctx, fx, &ident_pos)
                else {
                    return self.err_expr(pos);
                };
                source = checked;
                let mut types = types.into_iter();
                let key = types.next().unwrap_or(Type::Error);
                (
                    key,
                    ident_pos.clone(),
                    types.next().map(ValueArgument::Inferred),
                )
            }
            Some(type_args) => {
                let expected = if name == "Map" { 2 } else { 1 };
                if type_args.params.len() != expected {
                    self.reject_subset(
                        RejectionSite::ContainerConstructorTypeArgumentCount,
                        format!("`new {name}` takes exactly {expected} type argument(s)"),
                        ident_pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                (
                    Type::Error,
                    self.pos(type_args.params[0].span()),
                    type_args
                        .params
                        .get(1)
                        .map(|value| ValueArgument::Written(value)),
                )
            }
        };
        let saved_context = self.enter_container_context(ctx);
        let key = match &n.type_args {
            Some(type_args) => {
                let saved = self.in_assoc_key;
                self.in_assoc_key = true;
                let key = self.resolve_type(&type_args.params[0]);
                self.in_assoc_key = saved;
                key
            }
            None => key,
        };
        let key_slot = if name == "Map" {
            ContainerSlot::MapKey
        } else {
            ContainerSlot::SetElement
        };
        let key = self.container_argument(key_slot, key, key_pos.clone());
        if !self.instance_restriction(
            crate::check::opaque::InstanceRestriction::AssociativeKey,
            &key,
        ) && !matches!(self.apparent_type(&key), Type::Error)
            && self.assoc_key_kind(&key).is_none()
        {
            let key_name = self.type_name(&key);
            self.reject_subset(
                RejectionSite::NewMapSetKey,
                format!("`{key_name}` is not a permitted Map/Set key kind (Q24)"),
                key_pos,
            );
        }
        if name == "Map" {
            let value = match value {
                Some(ValueArgument::Written(value)) => {
                    let resolved = self.resolve_type(value);
                    let value_pos = self.pos(value.span());
                    self.container_argument(ContainerSlot::MapValue, resolved, value_pos)
                }
                Some(ValueArgument::Inferred(value)) => {
                    self.container_argument(ContainerSlot::MapValue, value, ident_pos.clone())
                }
                None => Type::Error,
            };
            self.leave_container_context(saved_context);
            let ty = Type::map(key, value);
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::Call {
                    callee: Callee::Map(MapFn::New),
                    args: Vec::new(),
                },
                ty,
                pos,
            };
        }
        self.leave_container_context(saved_context);
        let ty = Type::set(key.clone());
        // The arity guard and the source index are one match, so no
        // caller holds an emptiness precondition for the other.
        let args = match arguments {
            [] => Vec::new(),
            [argument] => match self.check_set_source(argument, source, &key, fx) {
                Some(source) => vec![source],
                None => return self.err_expr(pos),
            },
            _ => {
                self.reject_subset(
                    RejectionSite::SetConstructorSourceCount,
                    "`new Set` takes at most one source argument",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Set(SetFn::New),
                args,
            },
            ty,
            pos,
        }
    }
}
