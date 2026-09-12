//! Checks the receiver and the Math, Number, Date, RegExp, and Worker namespaces.

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{static_member_symbol, Checker, FnCtx, ParamSig, ScopeItem};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, Callee, DateFn, ExprKind, MathFn, NumFn, RegexFn, StrFn, WorkerFn};
use crate::types::{ClassId, Type};

use super::regex_literal;

impl<'p> Checker<'p> {
    /// Checks a receiver expression and enforces C7: member access on a
    /// nullable value requires prior narrowing.
    pub(super) fn check_receiver(&mut self, obj: &ast::Expr, fx: &mut FnCtx) -> hir::Expr {
        let mut checked = self.check_expr_with_header_receiver(obj, None, fx, true);
        self.apply_narrowing(&mut checked, fx);
        if let Type::Nullable(_) = checked.ty {
            let name = self.type_name(&checked.ty);
            self.error(
                RuleCode::S011,
                format!(
                    "`{}` may be null here; narrow with a null check first",
                    name
                ),
                checked.pos.clone(),
            );
            checked.ty = Type::Error;
        }
        checked
    }

    /// Resolves `obj` in `obj.prop` when `obj` is a type name used as a
    /// namespace (enums, class statics, `Object.setPrototypeOf`, the
    /// ambient `Math` of stdlib.md §1). Returns `Some` when handled.
    /// `for_write` marks an assignment-target position.
    pub(super) fn check_namespace_member(
        &mut self,
        obj: &ast::Expr,
        prop: &str,
        prop_pos: Pos,
        fx: &mut FnCtx,
        for_write: bool,
    ) -> Option<hir::Expr> {
        let ast::Expr::Ident(id) = obj else {
            return None;
        };
        let name = id.sym.to_string();
        // A local scope owns the name against each type name.
        if fx.owns_local_name(&name) {
            return None;
        }
        // `Context.<member>` (Q6/Q7/Q34): function members are intercepted
        // in call position; the namespace and its members are not values.
        if name == "Context" && self.ambient_visible(&name, fx) {
            let detail = if prop == "suspend" {
                "`Context.suspend` may only appear as the direct call in `await Context.suspend()` (Q34)".to_string()
            } else if crate::ambient::context_fn(prop).is_some()
                || crate::ambient::context_bytes_fn(prop).is_some()
            {
                format!("`Context.{prop}` may only be called, not read as a value (Q6/Q7/Q34)")
            } else {
                format!("`Context.{prop}` is outside the accepted Context subset (Q6/Q7/Q34)")
            };
            self.error(RuleCode::S014, detail, prop_pos.clone());
            return Some(self.err_expr(prop_pos));
        }
        // `Math.<member>` (stdlib.md §1): the ambient namespace applies
        // only when no program declaration shadows the name. Function
        // members are intercepted by `check_method_call` before this
        // point; here a member is a constant fold, a rejected
        // un-called function, or an out-of-subset rejection.
        if name == "Math" && self.ambient_visible(&name, fx) {
            return Some(self.check_math_member(prop, prop_pos, for_write));
        }
        // `Number.<member>` (stdlib.md §11, Q25): predicate calls are
        // intercepted by `check_method_call`; here constants fold and
        // every other read/write receives the subset diagnostic.
        if name == "Number" && self.ambient_visible(&name, fx) {
            return Some(self.check_number_member(prop, prop_pos, for_write));
        }
        // `JSON.<member>` (stdlib.md §13, Q28): accepted functions are
        // intercepted in call position; namespace members are not values.
        if name == "JSON" && self.ambient_visible(&name, fx) {
            let detail = if matches!(prop, "stringify" | "parse") {
                format!("`JSON.{prop}` may only be called, not read as a value (Q28)")
            } else {
                format!("`JSON.{prop}` is outside the accepted JSON subset (Q28)")
            };
            self.error(RuleCode::S014, detail, prop_pos.clone());
            return Some(self.err_expr(prop_pos));
        }
        // `Date.<member>` (stdlib.md §3): the static function members
        // (`UTC`, `now`) are intercepted by `check_method_call` before
        // this point; here every member read is a rejection.
        if name == "Date" && self.ambient_visible(&name, fx) {
            return Some(self.check_date_member(prop, prop_pos, for_write));
        }
        // `Array.<member>` (compiler.md §105): the accepted `from` is
        // intercepted in call position, so a read here is a rejection.
        if name == "Array" && self.ambient_visible(&name, fx) {
            if matches!(prop, "from" | "isArray" | "of") {
                self.error(
                    RuleCode::S014,
                    format!("`Array.{prop}` may only be called, not read as a value (Q22)"),
                    prop_pos.clone(),
                );
            } else {
                self.error(
                    RuleCode::S014,
                    format!("`Array.{prop}` is outside the accepted Array namespace (Q22)"),
                    prop_pos.clone(),
                );
            }
            return Some(self.err_expr(prop_pos));
        }
        if (name == "Map" || name == "Set") && self.ambient_visible(&name, fx) {
            if name == "Map" && prop == "groupBy" {
                self.error(
                    RuleCode::S014,
                    "`Map.groupBy` may only be called, not read as a value (Q27)",
                    prop_pos.clone(),
                );
                return Some(self.err_expr(prop_pos));
            }
            self.error(
                RuleCode::S014,
                format!(
                    "`{name}.{prop}` is outside the accepted Map/Set subset; \
                     iterator-based APIs are rejected (Q24)"
                ),
                prop_pos.clone(),
            );
            return Some(self.err_expr(prop_pos));
        }
        match self.scope_item(&name) {
            Some(ScopeItem::Poisoned) => Some(self.err_expr(prop_pos)),
            Some(ScopeItem::Class(id)) => {
                if prop == "prototype" {
                    self.error_diverging(
                        RuleCode::S003,
                        "no prototype mutation",
                        prop_pos.clone(),
                        Divergence::DynamicObjectModel,
                    );
                    return Some(self.err_expr(prop_pos));
                }
                let class_name = self.classes[id.0].name.clone();
                if let Some(signature) = self.class_sigs[id.0].static_fields.get(prop).cloned() {
                    let symbol = static_member_symbol(&class_name, prop);
                    if for_write && !signature.mutable {
                        self.error(
                            RuleCode::S100,
                            format!("cannot rebind `const` binding `{symbol}`"),
                            prop_pos.clone(),
                        );
                    }
                    return Some(hir::Expr {
                        kind: ExprKind::Global(symbol),
                        ty: signature.ty,
                        pos: prop_pos,
                    });
                }
                if self.class_sigs[id.0].has_static_accessor(prop) {
                    let signature = self.class_sigs[id.0].static_methods.get(prop).cloned();
                    let Some(signature) = signature else {
                        self.error(
                            RuleCode::S018,
                            format!("static read accessor `{class_name}.{prop}` is missing"),
                            prop_pos.clone(),
                        );
                        return Some(self.err_expr(prop_pos));
                    };
                    return Some(hir::Expr {
                        kind: ExprKind::Call {
                            callee: Callee::Func(static_member_symbol(&class_name, prop)),
                            args: Vec::new(),
                        },
                        ty: signature.ret,
                        pos: prop_pos,
                    });
                }
                if self.class_sigs[id.0].static_methods.contains_key(prop) {
                    self.error(
                        RuleCode::S100,
                        format!("static method `{class_name}.{prop}` may only be called"),
                        prop_pos.clone(),
                    );
                    return Some(self.err_expr(prop_pos));
                }
                self.error(
                    RuleCode::S018,
                    format!("class `{class_name}` has no static member `{prop}`"),
                    prop_pos.clone(),
                );
                Some(self.err_expr(prop_pos))
            }
            Some(ScopeItem::GenericClass(_)) => {
                if prop == "prototype" {
                    self.error_diverging(
                        RuleCode::S003,
                        "no prototype mutation",
                        prop_pos.clone(),
                        Divergence::DynamicObjectModel,
                    );
                } else {
                    self.error(
                        RuleCode::S018,
                        format!("generic class `{name}` has no static member `{prop}`"),
                        prop_pos.clone(),
                    );
                }
                Some(self.err_expr(prop_pos))
            }
            Some(ScopeItem::Enum(id)) => {
                let member = self.enums[id.0]
                    .members
                    .iter()
                    .find(|(n, _)| n == prop)
                    .cloned();
                match member {
                    Some((member, value)) => Some(hir::Expr {
                        kind: ExprKind::EnumMember { id, member, value },
                        ty: Type::Enum(id),
                        pos: prop_pos,
                    }),
                    None => {
                        self.error(
                            RuleCode::S018,
                            format!("enum `{}` has no member `{}`", name, prop),
                            prop_pos.clone(),
                        );
                        Some(self.err_expr(prop_pos))
                    }
                }
            }
            Some(ScopeItem::StringAlias(_)) => {
                self.error(
                    RuleCode::S100,
                    format!("string-literal union alias `{name}` has no static values"),
                    prop_pos.clone(),
                );
                Some(self.err_expr(prop_pos))
            }
            Some(ScopeItem::Global(_))
            | Some(ScopeItem::Func(_))
            | Some(ScopeItem::GenericFunc(_))
            | Some(ScopeItem::Foreign(_)) => None,
            None => {
                if name == "Object" {
                    if prop == "setPrototypeOf" {
                        self.error_diverging(
                            RuleCode::S003,
                            "no prototype mutation",
                            prop_pos.clone(),
                            Divergence::DynamicObjectModel,
                        );
                        return Some(self.err_expr(prop_pos));
                    }
                    if prop == "groupBy" {
                        self.reject_api_form(
                            "Object",
                            "groupBy",
                            "Object.groupBy",
                            prop_pos.clone(),
                        );
                        return Some(self.err_expr(prop_pos));
                    }
                }
                None
            }
        }
    }

    /// True when `obj` is the ambient `Math` namespace (stdlib.md §1):
    /// the identifier `Math` with no local binding and no program
    /// declaration shadowing it.
    pub(super) fn is_math_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        self.ambient_namespace(obj, fx) == Some("Math")
    }

    /// True when `obj` is the ambient `Context` namespace (Q6/Q7):
    /// the identifier `Context` with no local binding and no program
    /// declaration shadowing it.
    pub(super) fn is_context_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        self.ambient_namespace(obj, fx) == Some("Context")
    }

    /// A `Math` member outside a call position (stdlib.md §1): a
    /// constant folds to its `f64` literal; anything else is rejected
    /// with the Q19 subset code.
    fn check_math_member(&mut self, prop: &str, prop_pos: Pos, for_write: bool) -> hir::Expr {
        if for_write {
            self.error(
                RuleCode::S014,
                format!("`Math.{}` is read-only (Q19)", prop),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        if let Some(value) = crate::ambient::math_const(prop) {
            return hir::Expr {
                kind: ExprKind::Float(value),
                ty: Type::F64,
                pos: prop_pos,
            };
        }
        if crate::ambient::math_fn(prop).is_some() {
            self.error(
                RuleCode::S014,
                format!("`Math.{}` may only be called, not read as a value", prop),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        self.error(
            RuleCode::S014,
            format!("`Math.{}` is outside the accepted Math subset (Q19)", prop),
            prop_pos.clone(),
        );
        self.err_expr(prop_pos)
    }

    /// A `Math.<fn>(…)` intrinsic call (stdlib.md §1): exact arity
    /// (Q19 — the lib's variadic `max`/`min`/`hypot` beyond two are out
    /// of subset). The binary32 bit-access members use their sized
    /// signatures from stdlib.md §17.1.
    pub(super) fn check_math_call(
        &mut self,
        f: MathFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let arity = f.arity();
        if c.args.len() != arity {
            if matches!(f, MathFn::Hypot | MathFn::Max | MathFn::Min)
                && c.args.len() > 2
                && self.reject_api_form(
                    "Math",
                    "max/min/hypot with more than two arguments",
                    f.name(),
                    pos.clone(),
                )
            {
                return self.err_expr(pos);
            }
            let argument_type = match f {
                MathFn::Clz32 | MathFn::F32FromBits => "u32",
                MathFn::Imul => "i32",
                _ => "f64",
            };
            self.error(
                RuleCode::S014,
                format!(
                    "`Math.{}` takes exactly {} {} argument(s), got {} \
                     (Q19: the lib's variadic forms are out of subset)",
                    f.name(),
                    arity,
                    argument_type,
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let param_ty = match f {
            MathFn::Clz32 | MathFn::F32FromBits => Type::U32,
            MathFn::Imul => Type::I32,
            _ => Type::F64,
        };
        let params: Vec<ParamSig> = (0..arity)
            .map(|_| ParamSig::positional(param_ty.clone()))
            .collect();
        let args = self.check_args(&params, &c.args, fx, &pos, &format!("Math.{}", f.name()));
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Math(f),
                args,
            },
            ty: match f {
                MathFn::Clz32 | MathFn::Imul => Type::I32,
                MathFn::F32ToBits => Type::U32,
                _ => Type::F64,
            },
            pos,
        }
    }

    /// True when `obj` is the unshadowed ambient `Number` namespace.
    pub(super) fn is_number_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        self.ambient_namespace(obj, fx) == Some("Number")
    }

    /// A `Number` namespace member outside a call position: constants
    /// fold to f64 literals; statics are call-only; all other members
    /// are rejected under Q25/Q27.
    fn check_number_member(&mut self, prop: &str, prop_pos: Pos, for_write: bool) -> hir::Expr {
        if for_write {
            self.error(
                RuleCode::S014,
                format!("`Number.{prop}` is read-only (Q25)"),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        if let Some(value) = crate::ambient::number_const(prop) {
            return hir::Expr {
                kind: ExprKind::Float(value),
                ty: Type::F64,
                pos: prop_pos,
            };
        }
        if crate::ambient::number_static(prop).is_some() {
            self.error(
                RuleCode::S014,
                format!("`Number.{prop}` may only be called, not read as a value (Q25)"),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        self.error(
            RuleCode::S014,
            format!("`Number.{prop}` is outside the accepted Number subset (Q25)"),
            prop_pos.clone(),
        );
        self.err_expr(prop_pos)
    }

    /// A `Number.is*` predicate call: exactly one `f64`, returning
    /// boolean through the shared Q25 runtime.
    pub(super) fn check_number_predicate_call(
        &mut self,
        f: NumFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if c.args.len() != 1 {
            self.error(
                RuleCode::S014,
                format!(
                    "`Number.{}` takes exactly 1 f64 argument, got {} (Q25)",
                    f.name(),
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let args = self.check_args(
            &[ParamSig::positional(Type::F64)],
            &c.args,
            fx,
            &pos,
            &format!("Number.{}", f.name()),
        );
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Num(f),
                args,
            },
            ty: Type::Bool,
            pos,
        }
    }

    /// Global or `Number`-namespace `parseInt` / `parseFloat` calls.
    /// Both spellings lower to the same [`NumFn`] identity and runtime
    /// symbol. Their arity is part of Q25/Q27: in particular,
    /// parseInt's radix is required.
    pub(super) fn check_number_global_call(
        &mut self,
        f: NumFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        call_name: &str,
    ) -> hir::Expr {
        let params: Vec<ParamSig> = match f {
            NumFn::ParseInt => vec![
                ParamSig::positional(Type::Str),
                ParamSig::positional(Type::I32),
            ],
            NumFn::ParseFloat => vec![ParamSig::positional(Type::Str)],
            _ => {
                self.error(
                    RuleCode::S100,
                    "internal Q25 parser identity mismatch",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        if c.args.len() != params.len() {
            if f == NumFn::ParseInt && c.args.len() == 1 && call_name == "parseInt" {
                self.reject_api_form("global", "parseInt(value)", "parseInt(value)", pos.clone());
            } else {
                self.error(
                    RuleCode::S014,
                    format!(
                        "`{}` takes exactly {} argument(s), got {} (Q25)",
                        call_name,
                        params.len(),
                        c.args.len()
                    ),
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        }
        let args = self.check_args(&params, &c.args, fx, &pos, call_name);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Num(f),
                args,
            },
            ty: Type::F64,
            pos,
        }
    }

    /// True when the name `Date` still resolves to the ambient
    /// namespace/constructor (stdlib.md §3): no function-local binding
    /// and no program declaration shadows it (same rule as `Math`).
    /// Consulted by both member access and `new Date(…)` so shadowing
    /// behaves identically in every position.
    pub(super) fn date_is_ambient(&self, fx: &FnCtx) -> bool {
        self.ambient_visible("Date", fx)
    }

    /// True when `Map` / `Set` resolves to the ambient generic reference
    /// class rather than a local or program declaration.
    pub(super) fn assoc_is_ambient(&self, name: &str, fx: &FnCtx) -> bool {
        self.ambient_visible(name, fx)
    }

    /// True when `RegExp` resolves to the ambient constructor rather
    /// than a local or program declaration.
    pub(super) fn regexp_is_ambient(&self, fx: &FnCtx) -> bool {
        self.ambient_visible("RegExp", fx)
    }

    /// True when `Worker` resolves to Q35's ambient static namespace rather
    /// than a local or program declaration.
    pub(super) fn worker_is_ambient(&self, fx: &FnCtx) -> bool {
        self.ambient_visible("Worker", fx)
    }

    /// Returns the innermost non-transferable field in one Q35 message
    /// payload. Value classes and FixedArray elements are descended
    /// recursively; a reference-class field is itself the offending leaf.
    fn non_transferable_message_field(
        &self,
        ty: &Type,
        path: &str,
        pos: &Pos,
        string_slot_allowed: bool,
        visiting: &mut std::collections::HashSet<ClassId>,
    ) -> Option<(Pos, String, Type)> {
        if self.plain_value_leaf(ty) {
            return None;
        }
        match ty {
            Type::Str if string_slot_allowed => None,
            Type::FixedArray(element, _) if string_slot_allowed && **element == Type::Str => None,
            Type::FixedArray(element, _) => {
                self.non_transferable_message_field(element, path, pos, false, visiting)
            }
            Type::Class(id) if self.classes.get(id.0).is_some_and(|class| class.is_value) => {
                if !visiting.insert(*id) {
                    return None;
                }
                let class = &self.classes[id.0];
                let result = class.fields.iter().find_map(|field| {
                    let nested = format!("{path}.{}", field.name);
                    self.non_transferable_message_field(
                        &field.ty, &nested, &field.pos, false, visiting,
                    )
                });
                visiting.remove(id);
                result
            }
            other => Some((pos.clone(), path.to_string(), other.clone())),
        }
    }

    fn validate_message_class(&mut self, id: ClassId) -> bool {
        let class = self.classes[id.0].clone();
        for field in &class.fields {
            let path = format!("{}.{}", class.name, field.name);
            if let Some((pos, path, ty)) = self.non_transferable_message_field(
                &field.ty,
                &path,
                &field.pos,
                true,
                &mut std::collections::HashSet::new(),
            ) {
                let type_name = self.type_name(&ty);
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "message class `{}` is not transferable: innermost field `{path}` has non-transferable type `{type_name}`",
                        class.name
                    ),
                    pos,
                    Divergence::WorkerContextAffinity,
                );
                return false;
            }
        }
        true
    }

    pub(super) fn check_worker_spawn(
        &mut self,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if c.args.len() != 1
            || c.args
                .first()
                .is_some_and(|argument| argument.spread.is_some())
        {
            self.error(
                RuleCode::S100,
                format!(
                    "`Worker.spawn` expects one directly named entry function, got {} argument(s)",
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let argument = &c.args[0].expr;
        let mut entry_expr: &ast::Expr = argument;
        while let ast::Expr::Paren(paren) = entry_expr {
            entry_expr = &paren.expr;
        }
        let ast::Expr::Ident(ident) = entry_expr else {
            self.error_diverging(
                RuleCode::S100,
                "`Worker.spawn` entry must be a directly named module-level function; lambdas and other values are not worker entries",
                self.pos(argument.span()),
                Divergence::WorkerEntryShape,
            );
            return self.err_expr(pos);
        };
        if fx.owns_local_name(ident.sym.as_ref()) {
            let ident_pos = self.pos(ident.span);
            if self
                .lookup_local(ident.sym.as_ref(), &ident_pos, fx)
                .is_some_and(|local| matches!(local.ty, Type::Error))
            {
                return self.err_expr(pos);
            }
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must name a module-level function directly, not a local function value",
                self.pos(ident.span),
            );
            return self.err_expr(pos);
        }
        let item = self.scope_item(ident.sym.as_ref());
        if matches!(item, Some(ScopeItem::Poisoned)) {
            self.check_poisoned_arguments(&c.args, fx);
            return self.err_expr(pos);
        }
        let Some(ScopeItem::Func(function)) = item else {
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must name a non-generic module-level function directly",
                self.pos(ident.span),
            );
            return self.err_expr(pos);
        };
        let Some(sig) = self.fn_sigs.get(&function).cloned() else {
            return self.err_expr(pos);
        };
        if sig.is_async {
            self.error_diverging(
                RuleCode::S100,
                "`Worker.spawn` entry must be synchronous; async worker entries are rejected",
                self.pos(ident.span),
                Divergence::WorkerEntryShape,
            );
            return self.err_expr(pos);
        }
        if sig.is_generator
            || sig.ret != Type::Void
            || sig.params.len() != 2
            || sig.params.iter().any(|parameter| parameter.has_default)
        {
            self.error(
                RuleCode::S100,
                "`Worker.spawn` entry must have the exact synchronous shape `(inbox: Inbox<In>, outbox: Outbox<Out>) => void`",
                self.pos(ident.span),
            );
            return self.err_expr(pos);
        }
        let (input, output) = match (&sig.params[0].ty, &sig.params[1].ty) {
            (Type::Inbox(input), Type::Outbox(output)) => ((**input).clone(), (**output).clone()),
            _ => {
                self.error(
                    RuleCode::S100,
                    "`Worker.spawn` entry must have the exact synchronous shape `(inbox: Inbox<In>, outbox: Outbox<Out>) => void`",
                    self.pos(ident.span),
                );
                return self.err_expr(pos);
            }
        };
        let (Type::Class(input_id), Type::Class(output_id)) = (&input, &output) else {
            return self.err_expr(pos);
        };

        if let Some(type_args) = &c.type_args {
            if type_args.params.len() != 2 {
                self.error(
                    RuleCode::S100,
                    "`Worker.spawn` takes exactly two explicit type arguments",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
            let explicit_input = self.resolve_type(&type_args.params[0]);
            let explicit_output = self.resolve_type(&type_args.params[1]);
            if explicit_input != input || explicit_output != output {
                self.error(
                    RuleCode::S100,
                    "`Worker.spawn` type arguments must exactly match its entry's Inbox/Outbox message classes",
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        }

        let mut valid = self.validate_message_class(*input_id);
        if input_id != output_id {
            valid &= self.validate_message_class(*output_id);
        }
        if !valid {
            return self.err_expr(pos);
        }
        let worker_entry = hir::WorkerEntry {
            function,
            input: *input_id,
            output: *output_id,
        };
        let entry_index = self
            .worker_entries
            .iter()
            .position(|entry| entry == &worker_entry)
            .unwrap_or_else(|| {
                let index = self.worker_entries.len();
                self.worker_entries.push(worker_entry);
                index
            });
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Worker(WorkerFn::Spawn(entry_index)),
                args: Vec::new(),
            },
            ty: Type::Worker(Box::new(input), Box::new(output)),
            pos,
        }
    }

    pub(super) fn check_regex_new(
        &mut self,
        n: &ast::NewExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if n.type_args.is_some() {
            self.error(RuleCode::S100, "`RegExp` is not generic", pos.clone());
        }
        let params = [
            ParamSig {
                name: "pattern".to_string(),
                ty: Type::Str,
                has_default: false,
            },
            ParamSig {
                name: "flags".to_string(),
                ty: Type::Str,
                has_default: true,
            },
        ];
        let empty: Vec<ast::ExprOrSpread> = Vec::new();
        let args_ast = n.args.as_deref().unwrap_or(&empty);
        let mut args = self.check_args(&params, args_ast, fx, &pos, "new RegExp");
        if args.len() == 1 {
            args.push(hir::Expr {
                kind: ExprKind::Str(String::new()),
                ty: Type::Str,
                pos: pos.clone(),
            });
        }
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Regex(RegexFn::New),
                args,
            },
            ty: Type::RegExp,
            pos,
        }
    }

    pub(super) fn check_regex_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let (function, param, result) = match name {
            "test" => (RegexFn::Test, Type::Str, Type::Bool),
            "matchStart" => (RegexFn::MatchStart, Type::I32, Type::I32),
            "matchEnd" => (RegexFn::MatchEnd, Type::I32, Type::I32),
            "exec" => {
                self.error_diverging(
                    RuleCode::S014,
                    "`RegExp.exec` is rejected: its result needs an array with extra fields and a tuple type, neither of which the language has (Q31)",
                    prop_pos,
                    Divergence::RegExpSubset,
                );
                return self.err_expr(pos);
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    format!("`RegExp` has no accepted method `{name}`"),
                    prop_pos,
                );
                return self.err_expr(pos);
            }
        };
        let params = [ParamSig::positional(param)];
        let checked = self.check_args(&params, &c.args, fx, &pos, name);
        let mut args = Vec::with_capacity(2);
        args.push(recv);
        args.extend(checked);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Regex(function),
                args,
            },
            ty: result,
            pos,
        }
    }

    /// Checks the String methods whose first argument is overloaded
    /// between a literal-string pattern and a RegExp handle (stdlib.md
    /// §15). The first argument is checked once so a regex literal
    /// cannot produce duplicate diagnostics.
    pub(super) fn check_string_pattern_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let literal_without_global = name == "replaceAll"
            && c.args.first().is_some_and(|arg| {
                regex_literal(&arg.expr).is_some_and(|regex| !regex.flags.as_ref().contains('g'))
            });
        if literal_without_global {
            let diagnostic_pos = c
                .args
                .first()
                .map_or_else(|| pos.clone(), |arg| self.pos(arg.expr.span()));
            self.error_diverging(
                RuleCode::S100,
                "`string.replaceAll` with a RegExp literal requires the `g` flag",
                diagnostic_pos,
                Divergence::ReplaceAllGlobalFlag,
            );
        }
        let arity = if matches!(name, "replace" | "replaceAll") {
            2
        } else {
            1
        };
        if c.args.len() != arity || c.args.iter().any(|arg| arg.spread.is_some()) {
            self.error(
                RuleCode::S100,
                format!("`{name}` expects {arity} argument(s), got {}", c.args.len()),
                pos.clone(),
            );
        }
        let mut checked = Vec::with_capacity(c.args.len());
        for (index, arg) in c.args.iter().enumerate() {
            if arg.spread.is_some() {
                let spread_pos = self.pos(arg.spread.unwrap_or_default());
                self.error(
                    RuleCode::S014,
                    "spread arguments require variadic parameters, which the language does not have",
                    spread_pos,
                );
                continue;
            }
            let context = (index == 1).then_some(&Type::Str);
            let value = self.check_expr(&arg.expr, context, fx);
            if index == 1 {
                self.require_assignable(
                    &value.ty.clone(),
                    &Type::Str,
                    value.pos.clone(),
                    "the replacement",
                );
            }
            checked.push(value);
        }
        let Some(pattern) = checked.first() else {
            return self.err_expr(pos);
        };
        if literal_without_global {
            return self.err_expr(pos);
        }

        if pattern.ty == Type::RegExp {
            let function = match name {
                "search" => RegexFn::Search,
                "replace" => RegexFn::Replace,
                "replaceAll" => RegexFn::ReplaceAll,
                "split" => RegexFn::Split,
                _ => return self.err_expr(pos),
            };
            let mut args = Vec::with_capacity(1 + checked.len());
            args.push(recv);
            args.extend(checked);
            let ty = if name == "search" {
                Type::I32
            } else if name == "split" {
                Type::Array(Box::new(Type::Str))
            } else {
                Type::Str
            };
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Regex(function),
                    args,
                },
                ty,
                pos,
            };
        }

        if name == "search" {
            if pattern.ty != Type::Error {
                let message = "`string.search` requires a `RegExp`; string-pattern search is not in the P23 surface (Q31)";
                self.error(RuleCode::S014, message, prop_pos);
            }
            return self.err_expr(pos);
        }

        self.require_assignable(
            &pattern.ty.clone(),
            &Type::Str,
            pattern.pos.clone(),
            "the pattern",
        );
        let function = match name {
            "replace" => StrFn::Replace,
            "replaceAll" => StrFn::ReplaceAll,
            "split" => StrFn::Split,
            _ => return self.err_expr(pos),
        };
        let mut args = Vec::with_capacity(1 + checked.len());
        args.push(recv);
        args.extend(checked);
        let ty = if name == "split" {
            Type::Array(Box::new(Type::Str))
        } else {
            Type::Str
        };
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Str(function),
                args,
            },
            ty,
            pos,
        }
    }

    /// True when `obj` is the ambient `Date` namespace (stdlib.md §3):
    /// the identifier `Date` with no local binding and no program
    /// declaration shadowing it (same rule as `Math`).
    pub(super) fn is_date_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        let ast::Expr::Ident(id) = obj else {
            return false;
        };
        id.sym.as_ref() == "Date" && self.date_is_ambient(fx)
    }

    /// A `Date` namespace member outside a call position (stdlib.md §3):
    /// there are no constant members, so every read is a Q20 rejection.
    fn check_date_member(&mut self, prop: &str, prop_pos: Pos, for_write: bool) -> hir::Expr {
        if for_write {
            self.error(
                RuleCode::S014,
                format!("`Date.{}` is read-only (Q20)", prop),
                prop_pos.clone(),
            );
            return self.err_expr(prop_pos);
        }
        if prop == "parse"
            && self.reject_api_form("Date", "Date.parse", "Date.parse", prop_pos.clone())
        {
            return self.err_expr(prop_pos);
        }
        let why = if matches!(prop, "UTC" | "now") {
            format!(
                "`Date.{}` may only be called, not read as a value (Q20)",
                prop
            )
        } else {
            format!("`Date.{}` is outside the accepted Date subset (Q20)", prop)
        };
        self.error(RuleCode::S014, why, prop_pos.clone());
        self.err_expr(prop_pos)
    }

    /// A `Date` static call (`Date.UTC(…)`, `Date.now()`, stdlib.md §3).
    /// Returns `None` when `name` is not a static function member; the
    /// caller then falls through to the member rejection path.
    pub(super) fn check_date_static_call(
        &mut self,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> Option<hir::Expr> {
        match name {
            "UTC" => {
                // year and month0 are required; day defaults to 1, the
                // time components to 0 (ECMA Date.UTC with the lib's
                // optional parameters).
                let params: Vec<ParamSig> = (0..7)
                    .map(|i| ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: i >= 2,
                    })
                    .collect();
                let mut args = self.check_args(&params, &c.args, fx, &pos, "Date.UTC");
                // Normalize to the fixed 7-argument runtime signature at
                // check time, so both tiers lower the identical call.
                while args.len() < 7 {
                    let default = if args.len() == 2 { 1 } else { 0 };
                    args.push(hir::Expr {
                        kind: ExprKind::Int(default),
                        ty: Type::I32,
                        pos: pos.clone(),
                    });
                }
                Some(hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Date(DateFn::Utc),
                        args,
                    },
                    ty: Type::I64,
                    pos,
                })
            }
            "now" => {
                let args = self.check_args(&[], &c.args, fx, &pos, "Date.now");
                Some(hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Date(DateFn::Now),
                        args,
                    },
                    ty: Type::I64,
                    pos,
                })
            }
            _ => None,
        }
    }

    /// `new Date(…)` when no program declaration shadows `Date`
    /// (stdlib.md §3): exactly one `i64` millisecond argument. The
    /// zero-argument and multi-argument lib constructors mean
    /// current/local time and are out of subset (Q20).
    pub(super) fn check_date_new(
        &mut self,
        n: &ast::NewExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let empty: Vec<ast::ExprOrSpread> = Vec::new();
        let args_ast = n.args.as_deref().unwrap_or(&empty);
        match args_ast.len() {
            1 => {
                let params = [ParamSig::positional(Type::I64)];
                let args = self.check_args(&params, args_ast, fx, &pos, "new Date");
                hir::Expr {
                    kind: ExprKind::Call {
                        callee: Callee::Date(DateFn::New),
                        args,
                    },
                    ty: Type::Date,
                    pos,
                }
            }
            0 => {
                self.reject_api_form("Date", "new Date()", "new Date()", pos.clone());
                self.err_expr(pos)
            }
            _ => {
                self.reject_api_form(
                    "Date",
                    "new Date(year, month, ...)",
                    "new Date(year, month, ...)",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    /// A method call on a `Date` receiver (stdlib.md §3): `getTime()`
    /// folds to the receiver value retyped `i64` (the identity on the
    /// representation — both tiers agree by construction), the UTC
    /// accessors and `toISOString` become intrinsics carrying the
    /// receiver as their first argument, and everything else is a Q20
    /// rejection.
    pub(super) fn check_date_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        if name == "getTime" {
            self.check_args(&[], &c.args, fx, &pos, "getTime");
            return hir::Expr {
                kind: recv.kind,
                ty: Type::I64,
                pos,
            };
        }
        if let Some(method) = crate::ambient::date_method(name) {
            self.check_args(&[], &c.args, fx, &pos, name);
            let ty = if method == crate::ambient::DateMethod::ToIso {
                Type::Str
            } else {
                Type::I32
            };
            return hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Date(method.operation()),
                    args: vec![recv],
                },
                ty,
                pos,
            };
        }
        self.date_subset_rejection(name, prop_pos.clone());
        self.err_expr(pos)
    }

    /// Emits the Q20 rejection for an out-of-subset `Date` instance
    /// member, naming the member and pointing at the accepted spelling.
    pub(super) fn date_subset_rejection(&mut self, name: &str, pos: Pos) {
        let (code, why) = if let Some(rejection) = crate::ambient::date_rejection(name) {
            (
                rejection.code,
                crate::ambient::rejection_message(rejection, name),
            )
        } else {
            (
                RuleCode::S014,
                format!("`{}` is outside the accepted Date subset (Q20)", name),
            )
        };
        if let Some(rejection) = crate::ambient::date_rejection(name) {
            self.emit_api_rejection(rejection, name, pos);
        } else {
            self.error_diverging(code, why, pos, Divergence::DateSubset);
        }
    }
}
