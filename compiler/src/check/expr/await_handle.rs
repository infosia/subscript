//! `await` of a synchronous call that returns a handle (compiler.md §234).
//!
//! The call is an ordinary call, and `await` reads its handle, as an
//! `await` of a handle in a local does (§234 rule 1). The decision reads
//! the instantiated result type of the callee, so an alias or a type
//! argument that gives `Promise<T>` decides as a written `Promise<T>` does.
//! The handle has the rules of a held handle: S013 and §178 rule 1 treat
//! the call as an async origin (rule 3).

use super::*;
use crate::check::rejection::RejectionSite;
use crate::check::{FnCtx, FnSig, ScopeItem};

impl Checker<'_> {
    /// Checks `await K.m(...)` for a synchronous static method `m` of a
    /// class `K` as an ordinary call, and returns its handle. A result that
    /// is not a handle keeps the diagnostic of an awaited static call,
    /// "class `K` used as a value", and gives the poisoned expression.
    /// `None` for each other callee. The call takes no context (§215.1
    /// rule 3a).
    pub(super) fn await_static_call(
        &mut self,
        callee: &ast::Expr,
        operand: &ast::Expr,
        fx: &mut FnCtx,
    ) -> Option<hir::Expr> {
        let ast::Expr::Member(member) = callee else {
            return None;
        };
        let (ast::Expr::Ident(receiver), ast::MemberProp::Ident(name)) =
            (&*member.obj, &member.prop)
        else {
            return None;
        };
        if fx.owns_local_name(receiver.sym.as_ref()) {
            return None;
        }
        let Some(ScopeItem::Class(class)) = self.peek_scope_item(receiver.sym.as_ref()) else {
            return None;
        };
        let signatures = &self.class_sigs[class.0];
        let synchronous = signatures
            .static_methods
            .get(name.sym.as_ref())
            .is_some_and(|sig| !sig.is_async)
            || signatures
                .static_generic_methods
                .get(name.sym.as_ref())
                .is_some_and(|template| !template.function.is_async);
        if !synchronous {
            return None;
        }
        self.active_call_context = None;
        let handle = self.check_expr(operand, None, fx);
        if matches!(
            self.apparent_type(&handle.ty),
            Type::AsyncHandle(_) | Type::Error
        ) {
            return Some(handle);
        }
        // The receiver check gives the diagnostic of an awaited static call.
        self.check_receiver(&member.obj, fx);
        Some(self.err_expr(handle.pos))
    }

    /// The ordinary call of a synchronous instance method `name` on
    /// `receiver`, as the method call path builds it (§234 rule 1).
    pub(super) fn synchronous_method_call(
        &mut self,
        receiver: hir::Expr,
        name: String,
        sig: &FnSig,
        call: &ast::CallExpr,
        inferred: Option<Vec<Option<hir::Expr>>>,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        let pos = self.pos(call.span);
        let args = self.check_args_with_arguments(
            RejectionSite::InstanceMethodArgumentCount,
            &sig.params,
            &call.args,
            fx,
            &pos,
            &name,
            inferred,
            sig.generic,
        );
        let value = hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: hir::Callee::Method {
                    recv: Box::new(receiver),
                    name: hir::Symbol::from_full_text(name),
                },
                args,
            },
            ty: sig.ret.clone(),
            pos,
        };
        self.track_async_call_result(value, fx)
    }

    /// Awaits the handle that a checked call gives (§234 rule 1).
    pub(super) fn await_call_handle(
        &mut self,
        handle: hir::Expr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let Type::AsyncHandle(value) = self.apparent_type(&handle.ty) else {
            return self.err_expr(pos);
        };
        let origins = self.expr_async_origins(&handle, fx);
        fx.handle_async_origins(&origins);
        hir::Expr {
            pending_work: None,
            kind: ExprKind::AsyncHandleAwait(Box::new(handle)),
            ty: *value,
            pos,
        }
    }
}
