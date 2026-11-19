//! Constraint method calls in instance bodies (compiler.md §335).

use super::*;

/// One generic method call with its actual result substitutions (compiler.md §335 rule 1).
pub(super) struct MethodInstance {
    pub(super) name: String,
    pub(super) arguments: Option<Vec<Option<hir::Expr>>>,
    pub(super) result: Type,
}

impl<'p> Checker<'p> {
    /// Reads actual method arguments before runtime instance caching (compiler.md §335 rule 1).
    pub(super) fn method_result_arguments(
        &self,
        class: ClassId,
        name: &str,
        is_static: bool,
        arguments: &[Type],
    ) -> Vec<(u32, Type)> {
        let methods = if is_static {
            &self.class_sigs[class.0].static_generic_methods
        } else {
            &self.class_sigs[class.0].generic_methods
        };
        let Some(parameters) = methods
            .get(name)
            .and_then(|method| method.function.type_parameters.as_deref())
        else {
            return Vec::new();
        };
        parameters
            .params
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| (parameter.name.span.start, argument.clone()))
            .collect()
    }

    /// Keeps constraints inside their class body; external members use plain types (compiler.md §335 rule 1a).
    pub(super) fn member_value_type(&self, class: ClassId, ty: &Type, fx: &FnCtx) -> Type {
        let inside = fx.lexical_class == Some(class)
            || fx
                .frames
                .iter()
                .any(|frame| frame.static_this_class == Some(class));
        if self.instance_arguments.contains_key(&class) && !inside {
            ty.instance_type()
        } else {
            ty.clone()
        }
    }

    /// Projects one complete method type through the shared member rule (compiler.md §335 rule 1a).
    pub(super) fn member_signature(
        &self,
        class: ClassId,
        mut signature: FnSig<'p>,
        fx: &FnCtx<'p>,
    ) -> FnSig<'p> {
        let member = Type::Func(Box::new(crate::types::FuncType {
            callback_typedef: None,
            params: signature
                .params
                .iter()
                .map(|parameter| parameter.ty().clone())
                .collect(),
            ret: signature.ret.clone(),
        }));
        let Type::Func(member) = self.member_value_type(class, &member, fx) else {
            return signature;
        };
        for (parameter, ty) in signature.params.iter_mut().zip(member.params) {
            if matches!(parameter.state, TypeState::Decided(_)) {
                parameter.state = TypeState::decided(ty);
            }
        }
        signature.ret = member.ret;
        signature
    }

    /// Builds a cached result pattern with callee parameter identities (compiler.md §335 rule 1b).
    pub(super) fn instance_result_template(
        &mut self,
        function: &ast::Function,
        pos: &Pos,
    ) -> Option<Type> {
        let parameters = function.type_parameters.as_deref()?;
        function.return_type.as_ref()?;
        let saved = self.subst.clone();
        for parameter in &parameters.params {
            if let Some(instance) = saved.get(parameter.name.name.as_ref()) {
                let constraint = match instance {
                    Type::ConstrainedInstance(instance) => instance.constraint.clone(),
                    _ => None,
                };
                self.subst.insert(
                    parameter.name.name.to_string(),
                    Type::ConstrainedInstance(Box::new(crate::types::ConstrainedInstance {
                        identity: parameter.name.span.start,
                        name: parameter.name.name.to_string(),
                        constraint,
                        instance: instance.instance_type(),
                    })),
                );
            }
        }
        let result = self.resolve_fn_result(function, pos.clone()).ret;
        self.subst = saved;
        Some(result)
    }

    /// Projects each caller's arguments onto a cached result pattern (compiler.md §335 rule 1b).
    pub(super) fn call_instance_result(
        &self,
        key: &str,
        instance: &str,
        arguments: &[Type],
        handle: bool,
    ) -> Type {
        let Some(signature) = self.fn_sigs.get(instance) else {
            return Type::Error;
        };
        let Some(parameters) = self
            .generic_fns
            .get(key)
            .and_then(|template| template.function.type_parameters.as_deref())
        else {
            return signature.ret.clone();
        };
        let substitutions: Vec<_> = parameters
            .params
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| (parameter.name.span.start, argument.clone()))
            .collect();
        let result = signature
            .result_template
            .as_ref()
            .unwrap_or(&signature.ret)
            .substitute_instances(&substitutions);
        if handle && signature.is_async {
            Type::async_handle(result)
        } else {
            result
        }
    }

    /// Checks with the interface signature and calls the instance method (compiler.md §335 rule 2).
    pub(super) fn check_constrained_method_call(
        &mut self,
        receiver: &hir::Expr,
        property: &ast::IdentifierName,
        call: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: &Pos,
        awaited: bool,
    ) -> Option<hir::Expr> {
        let Type::ConstrainedInstance(parameter) = &receiver.ty else {
            return None;
        };
        let Type::Interface(interface) = self.apparent_type(parameter.constraint.as_ref()?) else {
            return None;
        };
        let Type::Class(class) = self.apparent_type(&receiver.ty) else {
            return None;
        };
        let name = property.name.as_ref();
        let member = self.interface_member(&interface, name)?;
        if member.accessor {
            return None;
        }
        self.class_sigs[class.0].methods.get(name)?;
        if awaited && !matches!(self.apparent_type(&member.ret), Type::AsyncHandle(_)) {
            return None;
        }
        self.mark_user_call(call, true);
        let prop_pos = self.pos(property.span);
        if self.reject_member_access(class, name, false, false, fx, prop_pos) {
            return Some(self.err_expr(pos.clone()));
        }
        self.decide_method_parameters(class, name, false);
        let Some(signature) = self.class_sigs[class.0].methods.get(name).cloned() else {
            return Some(self.err_expr(pos.clone()));
        };
        let params: Vec<_> = member
            .params
            .into_iter()
            .map(ParamSig::positional)
            .collect();
        let mut args = self.check_args(
            RejectionSite::InstanceMethodArgumentCount,
            &params,
            &call.arguments,
            fx,
            pos,
            name,
        );
        for (argument, parameter) in args.iter_mut().zip(&signature.params) {
            self.require_expr_assignable(argument, parameter.ty(), fx, "the argument");
        }
        let receiver = Box::new(self.apparent_expr(receiver.clone()));
        let (kind, actual, target) = if signature.is_async {
            let callee = hir::AsyncCallee::Method {
                class,
                receiver,
                name: hir::Symbol::from_full_text(name),
            };
            if awaited {
                let Type::AsyncHandle(target) = self.apparent_type(&member.ret) else {
                    return Some(self.err_expr(pos.clone()));
                };
                (
                    hir::ExprKind::AsyncCall { callee, args },
                    signature.ret,
                    *target,
                )
            } else {
                let origin = fx.register_async_origin(pos.clone());
                (
                    hir::ExprKind::AsyncHandleCreate {
                        callee,
                        args,
                        origin,
                    },
                    Type::async_handle(signature.ret),
                    member.ret,
                )
            }
        } else {
            (
                hir::ExprKind::Call {
                    callee: hir::Callee::Method {
                        recv: receiver,
                        name: hir::Symbol::from_full_text(name),
                    },
                    args,
                },
                signature.ret,
                member.ret,
            )
        };
        let value = hir::Expr {
            pending_work: None,
            kind,
            ty: actual,
            pos: pos.clone(),
        };
        let value = if value.ty != target {
            super::type_rules::cast_value(value, target)
        } else {
            value
        };
        let value = if signature.is_async {
            value
        } else {
            self.track_async_call_result(value, fx)
        };
        if awaited && !signature.is_async {
            let Type::AsyncHandle(result) = self.apparent_type(&value.ty) else {
                return Some(self.err_expr(pos.clone()));
            };
            let origins = self.expr_async_origins(&value, fx);
            fx.handle_async_origins(&origins);
            return Some(hir::Expr {
                pending_work: None,
                kind: hir::ExprKind::AsyncHandleAwait(Box::new(value)),
                ty: *result,
                pos: pos.clone(),
            });
        }
        Some(value)
    }
}
