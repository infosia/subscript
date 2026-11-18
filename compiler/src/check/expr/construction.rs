//! Checks class and container construction.

use crate::check::rejection::RejectionSite;
use crate::check::{Checker, FnCtx, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, ExprKind};
use crate::types::Type;
use swc_ecma_ast as ast;

impl Checker<'_> {
    /// Checks the one source operand of `new Set<K>(source)`
    /// (compiler.md §103.1). `source` is `K[]`, `FixedArray<K, N>`,
    /// `Set<K>`, or, for `Set<string>`, a `string` that yields one code
    /// point per element. `checked` is the source that inference checked
    /// (compiler.md §197 rule 6). Returns `None` when the form is rejected.
    pub(super) fn check_set_source(
        &mut self,
        argument: &ast::ExprOrSpread,
        checked: Option<hir::Expr>,
        key: &Type,
        fx: &mut FnCtx,
    ) -> Option<hir::Expr> {
        if let Some(spread) = argument.spread {
            let spread_pos = self.pos(spread);
            self.reject_subset(
                RejectionSite::SetSourceSpread,
                "spread arguments require variadic parameters, which the language does not have",
                spread_pos,
            );
            return None;
        }
        // compiler.md §336 rule 1: the Set element gives the source its array context.
        let source = checked.unwrap_or_else(|| {
            self.check_expr(&argument.expr, Some(&Type::array(key.clone())), fx)
        });
        let element = match &self.apparent_type(&source.ty) {
            Type::Error => return None,
            // A Map source yields pairs, which need a tuple type.
            Type::Map(..) => {
                self.reject_api_form("Set", "new Set(Map)", "new Set(map)", source.pos.clone());
                return None;
            }
            Type::Generator(_) => {
                self.reject_api_form(
                    "Set",
                    "new Set(Generator<T>)",
                    "new Set(generator)",
                    source.pos.clone(),
                );
                return None;
            }
            other => match self.apparent_type(other).iteration_element() {
                Some((_, element)) => element,
                None => {
                    let actual = self.type_name(other);
                    self.reject_subset(
                        RejectionSite::SetSourceDomain,
                        format!(
                            "`new Set(source)` accepts T[], FixedArray<T, N>, Set<T>, or \
                             string; got `{actual}`"
                        ),
                        source.pos.clone(),
                    );
                    return None;
                }
            },
        };
        self.require_assignable(&element, key, source.pos.clone(), "the Set element");
        Some(source)
    }

    /// Checks a `new` expression. `ctx` is the contextual type; a
    /// container construction reads the poisoned arguments of it
    /// (compiler.md §132 rule 2).
    pub(super) fn check_new(
        &mut self,
        n: &ast::NewExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let mut callee: &ast::Expr = &n.callee;
        while let ast::Expr::Paren(p) = callee {
            callee = &p.expr;
        }
        let ast::Expr::Ident(id) = callee else {
            self.reject_subset(
                RejectionSite::ConstructorNotNamedClass,
                "`new` requires a class name",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        let name = id.sym.to_string();
        let ident_pos = self.pos(id.span);
        if name == "TaskGroup" && self.ambient_visible(&name, fx) {
            return self.check_task_group_new(n, fx, pos);
        }
        if fx.owns_local_name(&name) {
            if self
                .lookup_local(&name, &ident_pos, fx)
                .is_some_and(|local| !matches!(self.apparent_type(&local.ty), Type::Error))
            {
                self.reject_subset(
                    RejectionSite::LocalValueConstructed,
                    format!("`{name}` names a local value here, not a class"),
                    ident_pos.clone(),
                );
            }
            return self.err_expr(pos);
        }
        if name == "Function" {
            self.reject_subset(
                RejectionSite::DynamicFunctionConstructed,
                "no dynamic code evaluation (`new Function`)",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "Promise" {
            self.reject_subset(RejectionSite::PromiseConstructed, "Promise objects cannot be constructed; async functions expose no Promise object surface", pos.clone());
            return self.err_expr(pos);
        }
        if matches!(name.as_str(), "Worker" | "Inbox" | "Outbox")
            && self.peek_scope_item(&name).is_none()
        {
            self.reject_subset(
                RejectionSite::WorkerEndpointConstructed,
                format!(
                    "`new {name}` is rejected; Q35 worker handles and endpoints are runtime-created"
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        // compiler.md §105.3: the length constructor needs an array hole
        // and a missing-element value, and the language has neither.
        if name == "Array" && self.ambient_visible(&name, fx) {
            self.reject_api_form(
                "Array",
                "new Array(length)",
                "new Array(length)",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "Number" && self.is_number_namespace(callee, fx) {
            self.reject_api_form(
                "Number",
                "new Number(value)",
                "new Number(value)",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if name == "RegExp" && self.regexp_is_ambient(fx) {
            return self.check_regex_new(n, fx, pos);
        }
        if self.error_name_is_ambient(&name, fx) {
            return self.check_error_new(&name, n, fx, pos);
        }
        // `new Date(ms)` (stdlib.md §3): the ambient constructor applies
        // only when neither a program declaration nor a function-local
        // binding shadows the name (same resolution as member access).
        if name == "Date" && self.date_is_ambient(fx) {
            return self.check_date_new(n, fx, pos);
        }
        if (name == "Map" || name == "Set") && self.assoc_is_ambient(&name, fx) {
            return self.check_container_new(&name, n, ctx, fx, pos, ident_pos);
        }
        let mut checked = None;
        let class_id = match self.scope_item(&name, &ident_pos, fx) {
            Some(ScopeItem::Poisoned | ScopeItem::Namespace { .. } | ScopeItem::Interface(_)) => {
                if let Some(arguments) = &n.args {
                    self.check_poisoned_arguments(arguments, fx);
                }
                return self.err_expr(pos);
            }
            Some(ScopeItem::Class(class_id)) => {
                if n.type_args.is_some() {
                    self.reject_subset(
                        RejectionSite::NonGenericConstructorTypeArguments,
                        format!("`{}` is not generic", name),
                        ident_pos.clone(),
                    );
                }
                Some(class_id)
            }
            Some(ScopeItem::TypeAlias(_) | ScopeItem::SourceAlias(_)) => {
                // §202.1 rule 3a: an alias names a type, not a value.
                self.reject_subset(
                    RejectionSite::MirrorTypeAliasValue,
                    format!("type alias `{name}` used as a value"),
                    ident_pos.clone(),
                );
                if let Some(arguments) = &n.args {
                    self.check_poisoned_arguments(arguments, fx);
                }
                return self.err_expr(pos);
            }
            Some(ScopeItem::GenericClass(key)) => match &n.type_args {
                Some(type_args) => {
                    let arguments = self.resolve_instance_arguments(type_args);
                    if arguments.types.contains(&Type::Error) {
                        // compiler.md §132 rule 2a: an error type argument
                        // gives no instance. The type-argument count and
                        // the constructor argument count do not depend on
                        // the argument types, so each still reports.
                        let _ = self.instantiate_class(&key, &arguments, ident_pos.clone());
                        let constructor_arguments: &[ast::ExprOrSpread] =
                            n.args.as_deref().unwrap_or(&[]);
                        let type_argument_count = self
                            .generic_classes
                            .get(&key)
                            .map(|template| template.type_params.len());
                        if type_argument_count == Some(arguments.types.len()) {
                            if let Some(arity) = self.template_constructor_arity(&key) {
                                self.check_argument_count(
                                    RejectionSite::ValueConstructorArgumentCount,
                                    arity,
                                    constructor_arguments,
                                    &pos,
                                    &name,
                                );
                            }
                        }
                        self.check_poisoned_arguments(constructor_arguments, fx);
                        return self.err_expr(pos);
                    }
                    self.instantiate_class(&key, &arguments, ident_pos.clone())
                }
                None => {
                    let constructor_arguments: &[ast::ExprOrSpread] =
                        n.args.as_deref().unwrap_or(&[]);
                    let (arguments, values) = match self.infer_constructor_arguments(
                        &key,
                        constructor_arguments,
                        ctx,
                        fx,
                        &ident_pos,
                    ) {
                        Some(inferred) => inferred,
                        None => return self.err_expr(pos),
                    };
                    checked = Some(values);
                    self.instantiate_class(&key, &arguments, ident_pos.clone())
                }
            },
            _ => {
                let site = if self.ambient_namespace(callee, fx).is_some()
                    || (self.type_scope_item(&name).is_none()
                        && crate::ambient::lib_value_name(&name))
                {
                    if matches!(name.as_str(), "WeakMap" | "WeakSet" | "WeakRef") {
                        RejectionSite::WeakMapConstructor
                    } else {
                        RejectionSite::UnknownNamespaceConstructor
                    }
                } else {
                    RejectionSite::UnknownClassConstructor
                };
                self.reject_subset(site, format!("unknown class `{}`", name), ident_pos.clone());
                None
            }
        };
        let Some(class_id) = class_id else {
            return self.err_expr(pos);
        };
        if self.reject_construction_modifier(class_id, fx, pos.clone()) {
            return self.err_expr(pos);
        }
        if self.handle_classes.contains(&class_id) {
            self.reject_subset(
                RejectionSite::OpaqueHandleConstructed,
                format!(
                    "opaque handle `{}` is obtained from the host, not constructed",
                    name
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        // compiler.md §108.4 rule 5: a `declare class` in a program file
        // has no constructor body and no positional store, so no argument
        // reaches a field. A mirror class keeps `new`: `lower_new` stores
        // every argument into the field at the same position, which is the
        // mirror constructor's contract. An instance of an ambient generic
        // template reaches this site through the template's status.
        if self.declared_classes.contains(&class_id) && !self.classes[class_id.0].is_boundary {
            self.reject_subset(
                RejectionSite::AmbientClassConstructed,
                format!(
                    "ambient class `{name}` is obtained from the host, not constructed, because \
                     a `declare class` has no constructor body"
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if self.classes[class_id.0].is_descriptor {
            self.reject_subset(
                RejectionSite::DescriptorClassConstructed,
                format!(
                    "descriptor class `{name}` is constructed with an object literal, not `new`"
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        self.decide_constructor_parameters(class_id);
        let params = self.class_sigs[class_id.0].ctor.clone().unwrap_or_default();
        let empty: Vec<ast::ExprOrSpread> = Vec::new();
        let args_ast = n.args.as_deref().unwrap_or(&empty);
        let args = self.check_args_with_arguments(
            RejectionSite::ReferenceConstructorArgumentCount,
            &params,
            args_ast,
            fx,
            &pos,
            &name,
            checked,
            false,
        );
        hir::Expr {
            pending_work: None,
            kind: ExprKind::New {
                class: class_id,
                args,
            },
            ty: Type::Class(class_id),
            pos,
        }
    }
}
