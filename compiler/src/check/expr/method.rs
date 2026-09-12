//! Checks the built-in instance methods on numbers, strings, arrays, maps, and sets.

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx, ParamSig};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, ArrFn, Callee, ExprKind, MapFn, NumFn, SetFn, StrFn};
use crate::types::{FuncType, Type};

use super::CallbackSpec;

impl<'p> Checker<'p> {
    /// A Q25/Q26 numeric receiver method. The accepted formatting
    /// methods operate on `f32`/`f64`; methods with a shared `f64`
    /// runtime entry widen an `f32` receiver exactly in HIR.
    pub(super) fn check_number_method(
        &mut self,
        recv: hir::Expr,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        if !matches!(
            name,
            "toFixed" | "toString" | "toExponential" | "toPrecision"
        ) {
            if name == "toLocaleString" {
                self.reject_api_form("f32 / f64", "toLocaleString", "toLocaleString", prop_pos);
                return self.err_expr(pos);
            }
            let type_name = self.type_name(&recv.ty);
            self.error(
                RuleCode::S018,
                format!("`{type_name}` has no method `{name}`"),
                prop_pos,
            );
            return self.err_expr(pos);
        }
        if !matches!(&recv.ty, Type::F32 | Type::F64) {
            self.reject_api_form(
                "sized integers",
                "toFixed/toString/toExponential/toPrecision",
                name,
                prop_pos,
            );
            return self.err_expr(pos);
        }

        let (f, optional, arity_message) = match name {
            "toFixed" => (
                NumFn::ToFixed,
                false,
                "`toFixed` takes exactly 1 i32 digit count",
            ),
            "toString" => (
                if recv.ty == Type::F32 {
                    NumFn::ToStringF32
                } else {
                    NumFn::ToStringF64
                },
                false,
                "`toString` requires an explicit radix (2–36)",
            ),
            "toExponential" => (
                NumFn::ToExponential,
                true,
                "`toExponential` takes zero or one i32 digit count",
            ),
            "toPrecision" => (
                NumFn::ToPrecision,
                false,
                "`toPrecision` requires an explicit i32 digit count",
            ),
            _ => return self.err_expr(pos),
        };
        let arity_ok = if optional {
            c.args.len() <= 1
        } else {
            c.args.len() == 1
        };
        if !arity_ok {
            let documented = match (name, c.args.len()) {
                ("toString", 0) => Some("toString()"),
                ("toPrecision", 0) => Some("toPrecision()"),
                _ => None,
            };
            if documented.is_some_and(|surface| {
                self.reject_api_form("f32 / f64", surface, surface, pos.clone())
            }) {
                return self.err_expr(pos);
            }
            self.error(
                RuleCode::S014,
                format!("{arity_message}, got {} argument(s) (Q26)", c.args.len()),
                pos.clone(),
            );
            return self.err_expr(pos);
        }

        let params = [ParamSig {
            name: String::new(),
            ty: Type::I32,
            has_default: optional,
        }];
        let mut checked = self.check_args(&params, &c.args, fx, &pos, name);
        if optional && checked.is_empty() {
            checked.push(hir::Expr {
                kind: ExprKind::Int(-1),
                ty: Type::I32,
                pos: pos.clone(),
            });
        }

        let recv_pos = recv.pos.clone();
        let recv = if recv.ty == Type::F32 && f != NumFn::ToStringF32 {
            hir::Expr {
                kind: ExprKind::Cast(Box::new(recv)),
                ty: Type::F64,
                pos: recv_pos,
            }
        } else {
            recv
        };
        let mut args = Vec::with_capacity(2);
        args.push(recv);
        args.extend(checked);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Num(f),
                args,
            },
            ty: Type::Str,
            pos,
        }
    }

    /// A `String` method intrinsic call on a string receiver
    /// (stdlib.md §8, Q21/Q27). Optional arguments are normalized here:
    /// starting positions default to `0`, ending positions and lengths
    /// use `i32::MAX` as the runtime's "to the end" sentinel, and `pad`
    /// defaults to `" "`. Every runtime symbol therefore has a fixed
    /// arity and both tiers lower the identical call (the Date.UTC
    /// technique, §3). The receiver becomes the call's first argument.
    pub(super) fn check_str_method(
        &mut self,
        recv: hir::Expr,
        f: StrFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let optional_slice = f == StrFn::Slice;
        let optional_zero_position =
            matches!(f, StrFn::IndexOf | StrFn::Includes | StrFn::StartsWith);
        let optional_end_position = matches!(f, StrFn::EndsWith | StrFn::Substring | StrFn::Substr);
        let optional_pad = matches!(f, StrFn::PadStart | StrFn::PadEnd);
        let params: Vec<ParamSig> = f
            .params()
            .iter()
            .enumerate()
            .map(|(i, p)| ParamSig {
                name: String::new(),
                ty: match p {
                    hir::StrParam::Str => Type::Str,
                    hir::StrParam::I32 => Type::I32,
                },
                has_default: optional_slice
                    || (i == 1
                        && (optional_zero_position || optional_end_position || optional_pad)),
            })
            .collect();
        let mut args = self.check_args(&params, &c.args, fx, &pos, f.name());
        if optional_slice && args.is_empty() {
            args.push(hir::Expr {
                kind: ExprKind::Int(0),
                ty: Type::I32,
                pos: pos.clone(),
            });
        }
        if args.len() + 1 == params.len() {
            if optional_slice || optional_end_position {
                args.push(hir::Expr {
                    kind: ExprKind::Int(i64::from(i32::MAX)),
                    ty: Type::I32,
                    pos: pos.clone(),
                });
            } else if optional_zero_position {
                args.push(hir::Expr {
                    kind: ExprKind::Int(0),
                    ty: Type::I32,
                    pos: pos.clone(),
                });
            } else if optional_pad {
                args.push(hir::Expr {
                    kind: ExprKind::Str(" ".to_string()),
                    ty: Type::Str,
                    pos: pos.clone(),
                });
            }
        }
        let ty = match f.ret() {
            hir::StrRet::I32 => Type::I32,
            hir::StrRet::Bool => Type::Bool,
            hir::StrRet::Str => Type::Str,
            hir::StrRet::StrArray => Type::Array(Box::new(Type::Str)),
        };
        let mut all = Vec::with_capacity(1 + args.len());
        all.push(recv);
        all.extend(args);
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Str(f),
                args: all,
            },
            ty,
            pos,
        }
    }

    /// Emits the Q21 rejection for a known out-of-subset `String`
    /// member, naming the member and pointing at the accepted spelling;
    /// returns `false` when `name` is not in the rejected set (the
    /// caller then falls back to the generic surface diagnostic).
    pub(super) fn str_subset_rejection(&mut self, name: &str, pos: Pos) -> bool {
        let Some(rejection) = crate::ambient::string_rejection(name) else {
            return false;
        };
        self.emit_api_rejection(rejection, name, pos);
        true
    }

    /// The generic out-of-surface diagnostic for a string member that is
    /// neither accepted nor in the named Q21 rejected set.
    pub(super) fn str_surface_error(&mut self, name: &str, pos: Pos) {
        self.error(
            RuleCode::S100,
            format!(
                "`{}` is outside the string surface (length, slice, `+` \
                 concatenation, `===`/`!==`, and the Q21 String methods)",
                name
            ),
            pos,
        );
    }

    /// The [`hir::ArrElemKind`] of an array element type under this
    /// program's class table (value classes excluded, stdlib.md §9).
    fn arr_elem_kind(&self, ty: &Type) -> Option<hir::ArrElemKind> {
        let classes = &self.classes;
        hir::ArrElemKind::of(ty, &|id| classes.get(id.0).is_some_and(|c| c.is_value))
    }

    /// Checks an accepted `Array` method call (stdlib.md §9, Q22):
    /// validates the arguments against the method's fixed shape,
    /// normalizes the optional ones (`join` separator, `slice`/`fill`
    /// range), types the callbacks under C5, and emits the
    /// [`Callee::Arr`] intrinsic with the receiver first.
    pub(super) fn check_array_method(
        &mut self,
        recv: hir::Expr,
        elem: Type,
        f: ArrFn,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        use ArrFn as A;
        let arr_ty = Type::Array(Box::new(elem.clone()));
        let mk = |args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Arr(f),
                args,
            },
            ty,
            pos,
        };
        let int_default = |value: i64, pos: &Pos| hir::Expr {
            kind: ExprKind::Int(value),
            ty: Type::I32,
            pos: pos.clone(),
        };
        // The callback-taking methods (and the equality searches) move
        // element values across the runtime↔script boundary; the
        // checker gates the element kinds that can (Q22).
        let needs_elem_kind =
            f.takes_callback() || matches!(f, A::IndexOf | A::LastIndexOf | A::Includes);
        if needs_elem_kind && self.arr_elem_kind(&elem).is_none() {
            let elem_n = self.type_name(&elem);
            self.error(
                RuleCode::S014,
                format!(
                    "`{}` is defined per element kind (scalars, strings, `Date`, \
                     reference classes); `{}` elements are outside that set (Q22)",
                    f.name(),
                    elem_n
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        match f {
            A::IndexOf | A::LastIndexOf | A::Includes => {
                let params = [ParamSig::positional(elem.clone())];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, f.name()));
                let ty = if f == A::Includes {
                    Type::Bool
                } else {
                    Type::I32
                };
                mk(args, ty, pos)
            }
            A::Join => {
                if hir::ArrFmtKind::of(&elem).is_none() {
                    let elem_n = self.type_name(&elem);
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`join` formats elements by the Q14 interpolation rules; \
                             `{}` elements are not interpolatable (Q22)",
                            elem_n
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [ParamSig {
                    name: String::new(),
                    ty: Type::Str,
                    has_default: true,
                }];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "join");
                if checked.is_empty() {
                    checked.push(hir::Expr {
                        kind: ExprKind::Str(",".to_string()),
                        ty: Type::Str,
                        pos: pos.clone(),
                    });
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, Type::Str, pos)
            }
            A::Slice => {
                let params = [
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                ];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "slice");
                if checked.is_empty() {
                    checked.push(int_default(0, &pos));
                }
                if checked.len() == 1 {
                    checked.push(int_default(ArrFn::END_SENTINEL, &pos));
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, arr_ty, pos)
            }
            A::Fill => {
                let params = [
                    ParamSig::positional(elem.clone()),
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                ];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "fill");
                // C5: `fill` stores its argument in the array.
                if let Some(value) = checked.first() {
                    if self.is_capturing_value(value, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not escape: `fill` stores its \
                             argument in the array",
                            value.pos.clone(),
                        );
                    }
                }
                if checked.len() == 1 {
                    checked.push(int_default(0, &pos));
                }
                if checked.len() == 2 {
                    checked.push(int_default(ArrFn::END_SENTINEL, &pos));
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, arr_ty, pos)
            }
            A::Reverse => {
                let args_checked = self.check_args(&[], &c.args, fx, &pos, "reverse");
                let mut args = vec![recv];
                args.extend(args_checked);
                mk(args, arr_ty, pos)
            }
            A::Concat => {
                let params = [ParamSig::positional(arr_ty.clone())];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "concat"));
                mk(args, arr_ty, pos)
            }
            A::Splice => {
                if c.args.len() > 2 {
                    self.reject_api_form(
                        "T[]",
                        "splice(start, deleteCount, ...items)",
                        "splice with inserted elements",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if c.args.len() != 2 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`splice` expects 2 arguments (start, deleteCount), got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [
                    ParamSig::positional(Type::I32),
                    ParamSig::positional(Type::I32),
                ];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "splice"));
                mk(args, arr_ty, pos)
            }
            A::Shift => {
                let mut args = vec![recv];
                args.extend(self.check_args(&[], &c.args, fx, &pos, "shift"));
                mk(args, elem, pos)
            }
            A::Unshift => {
                if c.args.len() > 1 {
                    self.reject_api_form(
                        "T[]",
                        "unshift(value, ...values)",
                        "unshift with multiple elements",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!("`unshift` expects 1 argument (value), got {}", c.args.len()),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [ParamSig::positional(elem)];
                let checked = self.check_args(&params, &c.args, fx, &pos, "unshift");
                // C5: `unshift` stores its argument in the array.
                if let Some(value) = checked.first() {
                    if self.is_capturing_value(value, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not escape: `unshift` stores its \
                             argument in the array",
                            value.pos.clone(),
                        );
                    }
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, Type::I32, pos)
            }
            A::CopyWithin => {
                if !(2..=3).contains(&c.args.len()) {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`copyWithin` expects 2 or 3 arguments (target, start, end?), got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let params = [
                    ParamSig::positional(Type::I32),
                    ParamSig::positional(Type::I32),
                    ParamSig {
                        name: String::new(),
                        ty: Type::I32,
                        has_default: true,
                    },
                ];
                let mut checked = self.check_args(&params, &c.args, fx, &pos, "copyWithin");
                if checked.len() == 2 {
                    checked.push(int_default(ArrFn::END_SENTINEL, &pos));
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(args, arr_ty, pos)
            }
            A::Sort => {
                if c.args.is_empty() {
                    self.reject_api_form("T[]", "sort()", "sort()", pos.clone());
                    return self.err_expr(pos);
                }
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`sort` expects 1 argument (the comparator), got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let cb = self.check_arr_callback(
                    &c.args[0],
                    vec![elem.clone(), elem],
                    Some(Type::I32),
                    fx,
                    CallbackSpec::new("sort", "Q22", false),
                );
                mk(vec![recv, cb], arr_ty, pos)
            }
            A::Reduce | A::ReduceRight => {
                if c.args.len() < 2 {
                    let surface = if f == A::Reduce {
                        "reduce(callback)"
                    } else {
                        "reduceRight(callback)"
                    };
                    self.reject_api_form("T[]", surface, surface, pos.clone());
                    return self.err_expr(pos);
                }
                if c.args.len() != 2 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` expects 2 arguments (callback, init), got {}",
                            f.name(),
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                if let Some(spread) = c.args[1].spread {
                    let p = self.pos(spread);
                    self.error(
                        RuleCode::S014,
                        "spread arguments require variadic parameters, which the language \
                         does not have",
                        p.clone(),
                    );
                    return self.err_expr(p);
                }
                // C4: the accumulator type `U` is the callback's when the
                // callback spells it (an annotated `acc` parameter, or a
                // function value's declared type) — that is `init`'s
                // natural contextual type, so a plain literal init does
                // not default to `i32` and poison `U`. Only an
                // un-annotated arrow leaves `U` to `init` itself.
                let (acc_ctx, checked_cb, resolved_acc) = self.reduce_acc_context(&c.args[0], fx);
                let init = self.check_expr(&c.args[1].expr, acc_ctx.as_ref(), fx);
                if matches!(init.ty, Type::Error) {
                    return self.err_expr(pos);
                }
                let acc_ty = match &acc_ctx {
                    // The callback fixes `U`; a non-conforming init is
                    // reported against the init, not the callback.
                    Some(u) => {
                        self.require_assignable(
                            &init.ty,
                            u,
                            init.pos.clone(),
                            &format!("the `{}` init", f.name()),
                        );
                        u.clone()
                    }
                    None => init.ty.clone(),
                };
                if matches!(acc_ty, Type::Error) {
                    return self.err_expr(pos);
                }
                if self.arr_elem_kind(&acc_ty).is_none() {
                    let acc_n = self.type_name(&acc_ty);
                    self.error(
                        RuleCode::S014,
                        format!(
                            "the `{}` accumulator crosses the runtime↔script \
                             boundary; `{}` is outside the supported kinds (Q22)",
                            f.name(),
                            acc_n
                        ),
                        init.pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let cb = match checked_cb {
                    // Already checked (a function value): validate its
                    // shape, never check it twice.
                    Some(v) => self.expect_callback_shape(
                        v,
                        &[acc_ty.clone(), elem],
                        Some(&acc_ty),
                        CallbackSpec::new(f.name(), "Q27", true),
                    ),
                    None => self.check_arr_callback_with_resolved(
                        &c.args[0],
                        vec![acc_ty.clone(), elem],
                        Some(acc_ty.clone()),
                        fx,
                        CallbackSpec::new(f.name(), "Q27", true),
                        resolved_acc.as_ref(),
                    ),
                };
                mk(vec![recv, cb, init], acc_ty, pos)
            }
            A::ForEach | A::Map | A::Filter | A::Some | A::Every | A::FindIndex => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`{}` expects 1 argument (the callback), got {}",
                            f.name(),
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let ret_ctx = match f {
                    A::ForEach => Some(Type::Void),
                    A::Map => None, // `U` inferred from the callback
                    _ => Some(Type::Bool),
                };
                let cb = self.check_arr_callback(
                    &c.args[0],
                    vec![elem],
                    ret_ctx,
                    fx,
                    CallbackSpec::new(f.name(), "Q27", true),
                );
                let ty = match f {
                    A::ForEach => Type::Void,
                    A::Filter => arr_ty,
                    A::Some | A::Every => Type::Bool,
                    A::FindIndex => Type::I32,
                    A::Map => {
                        let u = match &cb.ty {
                            Type::Func(ft) => ft.ret.clone(),
                            _ => Type::Error,
                        };
                        if matches!(u, Type::Error) {
                            return self.err_expr(pos);
                        }
                        if matches!(u, Type::Void) {
                            self.error(
                                RuleCode::S100,
                                "the `map` callback must return a value",
                                cb.pos.clone(),
                            );
                            return self.err_expr(pos);
                        }
                        if self.arr_elem_kind(&u).is_none() {
                            let u_n = self.type_name(&u);
                            self.error(
                                RuleCode::S014,
                                format!(
                                    "`map` produces a `{}[]`; `{}` is outside the \
                                     supported element kinds (Q22)",
                                    u_n, u_n
                                ),
                                cb.pos.clone(),
                            );
                            return self.err_expr(pos);
                        }
                        Type::Array(Box::new(u))
                    }
                    _ => Type::Error,
                };
                mk(vec![recv, cb], ty, pos)
            }
        }
    }

    /// True when `V` can carry `get`'s null miss: a reference class or
    /// opaque handle (including the built-in reference containers), or
    /// an already-nullable form of one.
    fn map_get_value_ok(&self, value: &Type) -> bool {
        self.is_reference_class(value)
            || matches!(value, Type::Array(_))
            || matches!(value, Type::Nullable(inner) if self.is_reference_class(inner))
    }

    /// Checks the Q27 static `Map.groupBy` intrinsic. Both generic
    /// arguments are inferred from the array and callback return, as for
    /// `Array.map`; the key result must be in the Q24 whitelist.
    pub(super) fn check_map_group_by(
        &mut self,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if c.args.len() != 2 {
            self.error(
                RuleCode::S100,
                format!(
                    "`Map.groupBy` expects an array and a callback, got {} argument(s)",
                    c.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let items = self.check_expr(&c.args[0].expr, None, fx);
        let elem = match &items.ty {
            Type::Array(elem) => (**elem).clone(),
            Type::Error => return self.err_expr(pos),
            other => {
                let actual = self.type_name(other);
                self.error(
                    RuleCode::S100,
                    format!("`Map.groupBy` items must be a `T[]`, got `{actual}`"),
                    items.pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        let callback = self.check_arr_callback(
            &c.args[1],
            vec![elem.clone()],
            None,
            fx,
            CallbackSpec::new("Map.groupBy", "Q27", false),
        );
        let key = match &callback.ty {
            Type::Func(ft) if ft.ret != Type::Void => ft.ret.clone(),
            Type::Func(_) => {
                self.error(
                    RuleCode::S100,
                    "`Map.groupBy` callback must return a key",
                    callback.pos.clone(),
                );
                return self.err_expr(pos);
            }
            Type::Error => return self.err_expr(pos),
            other => {
                let actual = self.type_name(other);
                self.error(
                    RuleCode::S100,
                    format!("`Map.groupBy` callback is not a function, got `{actual}`"),
                    callback.pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        if self.assoc_key_kind(&key).is_none() {
            let key_name = self.type_name(&key);
            self.error_diverging(
                RuleCode::S014,
                format!(
                    "`Map.groupBy` callback returns `{key_name}`, which is not a \
                     §10.2 Map/Set key kind (Q24)"
                ),
                callback.pos.clone(),
                Divergence::MapKeyKind,
            );
            return self.err_expr(pos);
        }
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Map(MapFn::GroupBy),
                args: vec![items, callback],
            },
            ty: Type::Map(Box::new(key), Box::new(Type::Array(Box::new(elem)))),
            pos,
        }
    }

    /// Checks a call on the `Array` builtin namespace (compiler.md §105).
    ///
    /// `from` is the accepted member (§105.2). Every other member has a
    /// recorded rejection of its own (§105.3), so none of them answers
    /// the general unknown-name diagnostic any more.
    pub(super) fn check_array_static_call(
        &mut self,
        name: &str,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        match name {
            "from" => self.check_array_from(call, fx, pos, prop_pos),
            "isArray" => {
                self.reject_api_form("Array", "isArray(value)", "Array.isArray", prop_pos.clone());
                self.check_poisoned_arguments(&call.args, fx);
                self.err_expr(pos)
            }
            "of" => {
                self.reject_api_form("Array", "of(value, …)", "Array.of", prop_pos.clone());
                self.check_poisoned_arguments(&call.args, fx);
                self.err_expr(pos)
            }
            other => {
                self.error(
                    RuleCode::S014,
                    format!("`Array.{other}` is outside the accepted Array namespace (Q22)"),
                    prop_pos,
                );
                self.check_poisoned_arguments(&call.args, fx);
                self.err_expr(pos)
            }
        }
    }

    /// Checks `Array.from(source)` (compiler.md §105.2).
    ///
    /// The result is a fresh `T[]` over the traversal that array-literal
    /// spread uses (§105.2 rules 2, 3 and 4), so the source rules and the
    /// element rules are the ones stdlib.md §14.3 and §14.4 already own.
    fn check_array_from(
        &mut self,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        // §105.2 rule 5: an explicit type argument supplies the element
        // type; with none the source is checked with no contextual type.
        let declared = match &call.type_args {
            Some(type_args) if type_args.params.len() == 1 => {
                let resolved = self.resolve_type(&type_args.params[0]);
                if resolved == Type::Error {
                    self.check_poisoned_arguments(&call.args, fx);
                    return self.err_expr(pos);
                }
                Some(resolved)
            }
            Some(_) => {
                self.error(
                    RuleCode::S014,
                    "`Array.from<T>` takes exactly one type argument",
                    prop_pos,
                );
                self.check_poisoned_arguments(&call.args, fx);
                return self.err_expr(pos);
            }
            None => None,
        };
        // §105.2 rule 7: the mapper overload is rejected for the work it
        // needs. TypeScript spells it `from(source, mapfn, thisArg?)`,
        // so two arguments select it and three do too.
        if (2..=3).contains(&call.args.len()) {
            self.reject_api_form(
                "Array",
                "Array.from(source, mapFn)",
                "Array.from(source, mapFn)",
                prop_pos,
            );
            self.check_poisoned_arguments(&call.args, fx);
            return self.err_expr(pos);
        }
        let [argument] = &call.args[..] else {
            self.error(
                RuleCode::S014,
                format!(
                    "`Array.from(source)` takes one source argument, got {}",
                    call.args.len()
                ),
                prop_pos,
            );
            self.check_poisoned_arguments(&call.args, fx);
            return self.err_expr(pos);
        };
        if let Some(spread) = argument.spread {
            let spread_pos = self.pos(spread);
            self.error_diverging(
                RuleCode::S014,
                "spread arguments require variadic parameters, which the language does not have",
                spread_pos,
                Divergence::VariadicArguments,
            );
            self.check_poisoned_arguments(&call.args, fx);
            return self.err_expr(pos);
        }
        let context = declared.clone().map(|elem| Type::Array(Box::new(elem)));
        let source = self.check_expr(&argument.expr, context.as_ref(), fx);
        let selected = match &source.ty {
            Type::Error => None,
            // compiler.md §104.1: the source is rejected on its resolved
            // type, in this position as in the other two.
            Type::Map(..) => {
                self.reject_api_form(
                    "Array",
                    "Array.from(Map)",
                    "Array.from(map)",
                    source.pos.clone(),
                );
                None
            }
            Type::Generator(_) => {
                self.reject_api_form(
                    "Array",
                    "Array.from(Generator<T>)",
                    "Array.from(generator)",
                    source.pos.clone(),
                );
                None
            }
            other => match other.iteration_element() {
                Some((kind, element)) => Some((hir::SpreadKind::from(kind), element)),
                None => {
                    let actual = self.type_name(other);
                    self.error(
                        RuleCode::S014,
                        format!(
                            "`Array.from(source)` accepts T[], FixedArray<T, N>, Set<T>, \
                             or string; got `{actual}`"
                        ),
                        source.pos.clone(),
                    );
                    None
                }
            },
        };
        let Some((spread, element)) = selected else {
            return self.err_expr(pos);
        };
        if let Some(declared) = &declared {
            self.require_assignable(
                &element,
                declared,
                source.pos.clone(),
                "the Array.from element",
            );
        }
        let element = declared.unwrap_or(element);
        if Self::is_context_affine_type(&element) {
            self.error(
                RuleCode::S100,
                "Worker, Inbox, and Outbox values may not be array elements",
                pos.clone(),
            );
        }
        hir::Expr {
            kind: ExprKind::ArraySpreadLit(vec![hir::ArrayLitElem {
                expr: source,
                spread: Some(spread),
            }]),
            ty: Type::Array(Box::new(element)),
            pos,
        }
    }

    /// Checks a `Map<K, V>` intrinsic method (stdlib.md §10, Q24).
    pub(super) fn check_map_method(
        &mut self,
        recv: hir::Expr,
        key: Type,
        value: Type,
        name: &str,
        c: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let Some(operation) = crate::ambient::map_method(name) else {
            if let Some(rejection) = crate::ambient::map_rejection(name) {
                if name == "keys" && ctx.is_some() {
                    self.error(
                        rejection.code,
                        crate::ambient::rejection_message(rejection, name),
                        prop_pos,
                    );
                } else {
                    self.emit_api_rejection(rejection, name, prop_pos);
                }
            } else {
                self.error(
                    RuleCode::S100,
                    format!("`Map` has no accepted method `{name}` (Q24)"),
                    prop_pos,
                );
            }
            return self.err_expr(pos);
        };
        use crate::ambient::MapMethod as M;
        let map_ty = Type::Map(Box::new(key.clone()), Box::new(value.clone()));
        let mk = |f: MapFn, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Map(f),
                args,
            },
            ty,
            pos,
        };
        match operation {
            M::Get => {
                if !self.map_get_value_ok(&value) {
                    self.reject_api_form("Map<K, scalar V>", "get(key)", "get(key)", prop_pos);
                    return self.err_expr(pos);
                }
                let params = [ParamSig::positional(key)];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "Map.get"));
                let ty = if matches!(value, Type::Nullable(_)) {
                    value
                } else {
                    Type::Nullable(Box::new(value))
                };
                mk(MapFn::Get, args, ty, pos)
            }
            M::GetOr => {
                let params = [
                    ParamSig::positional(key),
                    ParamSig::positional(value.clone()),
                ];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, "Map.getOr"));
                mk(MapFn::GetOr, args, value, pos)
            }
            M::Set => {
                let params = [ParamSig::positional(key), ParamSig::positional(value)];
                let checked = self.check_args(&params, &c.args, fx, &pos, "Map.set");
                if checked
                    .get(1)
                    .is_some_and(|argument| self.is_capturing_value(argument, fx))
                {
                    let value_pos = checked[1].pos.clone();
                    self.error(
                        RuleCode::S009,
                        "capturing lambdas may not escape: `Map.set` stores its value",
                        value_pos,
                    );
                }
                let mut args = vec![recv];
                args.extend(checked);
                mk(MapFn::Set, args, map_ty, pos)
            }
            M::Has | M::Delete => {
                let params = [ParamSig::positional(key)];
                let mut args = vec![recv];
                args.extend(self.check_args(
                    &params,
                    &c.args,
                    fx,
                    &pos,
                    if operation == M::Has {
                        "Map.has"
                    } else {
                        "Map.delete"
                    },
                ));
                mk(operation.operation(), args, Type::Bool, pos)
            }
            M::Clear => {
                let checked = self.check_args(&[], &c.args, fx, &pos, "Map.clear");
                let mut args = vec![recv];
                args.extend(checked);
                mk(MapFn::Clear, args, Type::Void, pos)
            }
            M::ForEach => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`Map.forEach` expects exactly 1 callback, got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let callback = self.check_arr_callback(
                    &c.args[0],
                    vec![value, key],
                    Some(Type::Void),
                    fx,
                    CallbackSpec::new("Map.forEach", "Q24", false),
                );
                mk(MapFn::ForEach, vec![recv, callback], Type::Void, pos)
            }
        }
    }

    /// Checks a `Set<K>` intrinsic method (stdlib.md §10, Q24).
    pub(super) fn check_set_method(
        &mut self,
        recv: hir::Expr,
        key: Type,
        name: &str,
        c: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        prop_pos: Pos,
    ) -> hir::Expr {
        let Some(operation) = crate::ambient::set_method(name) else {
            if let Some(rejection) = crate::ambient::set_rejection(name) {
                self.emit_api_rejection(rejection, name, prop_pos);
            } else {
                self.error(
                    RuleCode::S100,
                    format!("`Set` has no accepted method `{name}` (Q24)"),
                    prop_pos,
                );
            }
            return self.err_expr(pos);
        };
        use crate::ambient::SetMethod as S;
        let set_ty = Type::Set(Box::new(key.clone()));
        let mk = |f: SetFn, args: Vec<hir::Expr>, ty: Type, pos: Pos| hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Set(f),
                args,
            },
            ty,
            pos,
        };
        match operation {
            S::Add | S::Has | S::Delete => {
                let params = [ParamSig::positional(key)];
                let mut args = vec![recv];
                args.extend(self.check_args(&params, &c.args, fx, &pos, &format!("Set.{name}")));
                let ty = match operation {
                    S::Add => set_ty,
                    S::Has | S::Delete => Type::Bool,
                    _ => Type::Error,
                };
                mk(operation.operation(), args, ty, pos)
            }
            S::Clear => {
                let checked = self.check_args(&[], &c.args, fx, &pos, "Set.clear");
                let mut args = vec![recv];
                args.extend(checked);
                mk(SetFn::Clear, args, Type::Void, pos)
            }
            S::ForEach => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`Set.forEach` expects exactly 1 callback, got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let callback = self.check_arr_callback(
                    &c.args[0],
                    vec![key],
                    Some(Type::Void),
                    fx,
                    CallbackSpec::new("Set.forEach", "Q24", false),
                );
                mk(SetFn::ForEach, vec![recv, callback], Type::Void, pos)
            }
            S::Union
            | S::Intersection
            | S::Difference
            | S::SymmetricDifference
            | S::IsSubsetOf
            | S::IsSupersetOf
            | S::IsDisjointFrom => {
                if c.args.len() != 1 {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`Set.{name}` expects exactly 1 Set argument, got {}",
                            c.args.len()
                        ),
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let mut checked = self.check_args(
                    &[ParamSig::positional(Type::Error)],
                    &c.args,
                    fx,
                    &pos,
                    &format!("Set.{name}"),
                );
                let Some(other) = checked.pop() else {
                    return self.err_expr(pos);
                };
                match &other.ty {
                    Type::Set(other_key) => {
                        self.require_assignable(
                            other_key,
                            &key,
                            other.pos.clone(),
                            "the Set argument key",
                        );
                    }
                    Type::Error => return self.err_expr(pos),
                    _ => {
                        self.reject_api_form(
                            "Set<K>",
                            "algebra(non-Set)",
                            &format!("Set.{name} non-Set argument"),
                            other.pos.clone(),
                        );
                        return self.err_expr(pos);
                    }
                }
                let ty = if matches!(
                    operation,
                    S::IsSubsetOf | S::IsSupersetOf | S::IsDisjointFrom
                ) {
                    Type::Bool
                } else {
                    set_ty
                };
                mk(operation.operation(), vec![recv, other], ty, pos)
            }
        }
    }

    /// The accumulator type `U` a `reduce` callback spells, if any — the
    /// contextual type for `init` (C4).
    ///
    /// An arrow callback spells it as its first parameter's annotation:
    /// resolved here without reporting, because the callback check
    /// resolves the same annotation afterwards. Any other callback
    /// expression is a function value whose declared type already gives
    /// `U`; it is checked once, here, and returned so the caller
    /// shape-validates it. `(None, None)` when the callback does not
    /// spell `U` (an un-annotated arrow, or an expression that is not a
    /// function), which leaves `init` context-free.
    fn reduce_acc_context(
        &mut self,
        arg: &ast::ExprOrSpread,
        fx: &mut FnCtx,
    ) -> (Option<Type>, Option<hir::Expr>, Option<Type>) {
        if arg.spread.is_some() {
            return (None, None, None); // reported by `check_arr_callback`
        }
        if let ast::Expr::Arrow(a) = &*arg.expr {
            let Some(ast::Pat::Ident(binding)) = a.params.first() else {
                return (None, None, None);
            };
            let Some(ann) = binding.type_ann.as_ref() else {
                return (None, None, None);
            };
            let ty = self.resolve_type(&ann.type_ann);
            return (
                (!matches!(ty, Type::Error)).then_some(ty.clone()),
                None,
                Some(ty),
            );
        }
        let checked = self.check_expr(&arg.expr, None, fx);
        let acc = match &checked.ty {
            Type::Func(ft) => ft.params.first().cloned(),
            _ => None,
        };
        (acc, Some(checked), None)
    }

    /// Checks one callback argument of an `Array`, `Map`, or `Set`
    /// operation. Q27 Array callbacks accept the base parameter list and
    /// the same list with one trailing `i32` index; the fixed Q24
    /// callbacks and `sort` accept exactly the supplied list. `ret` is
    /// `None` when the return type is inferred from the callback (`map`
    /// and `Map.groupBy`).
    fn check_arr_callback(
        &mut self,
        arg: &ast::ExprOrSpread,
        params: Vec<Type>,
        ret: Option<Type>,
        fx: &mut FnCtx,
        spec: CallbackSpec<'_>,
    ) -> hir::Expr {
        self.check_arr_callback_with_resolved(arg, params, ret, fx, spec, None)
    }

    fn check_arr_callback_with_resolved(
        &mut self,
        arg: &ast::ExprOrSpread,
        params: Vec<Type>,
        ret: Option<Type>,
        fx: &mut FnCtx,
        spec: CallbackSpec<'_>,
        resolved_first: Option<&Type>,
    ) -> hir::Expr {
        if arg.spread.is_some() {
            let checked = self.check_args(
                &[ParamSig::positional(Type::Error)],
                std::slice::from_ref(arg),
                fx,
                &self.pos(arg.expr.span()),
                spec.method,
            );
            let p = self.pos(arg.spread.unwrap_or_default());
            debug_assert!(checked.is_empty());
            return self.err_expr(p);
        }
        let expr = &*arg.expr;
        if let ast::Expr::Arrow(a) = expr {
            let a_pos = self.pos(a.span);
            let Some(expected) =
                self.callback_params_for_arity(&params, a.params.len(), spec, a_pos.clone())
            else {
                return self.err_expr(a_pos);
            };
            let checked =
                self.check_lambda_with(a, Some(&expected), ret.as_ref(), resolved_first, fx, a_pos);
            // An annotation may override the context; the resulting
            // function type must still match one accepted shape (the
            // return stays free when it is inferred).
            return self.expect_callback_shape(checked, &params, ret.as_ref(), spec);
        }
        // A function value (named reference or function-typed local).
        // A dual-arity Array callback has no single contextual function
        // type; named and local function values already carry their full
        // declared type and are validated below.
        let ctx_ty = (!spec.allow_index)
            .then_some(ret.as_ref())
            .flatten()
            .map(|r| {
                Type::Func(Box::new(FuncType {
                    params: params.clone(),
                    ret: r.clone(),
                }))
            });
        let checked = self.check_expr(expr, ctx_ty.as_ref(), fx);
        self.expect_callback_shape(checked, &params, ret.as_ref(), spec)
    }

    /// Selects the contextual parameter list for a callback's source
    /// arity and emits the subset diagnostic when no accepted list has
    /// that length.
    fn callback_params_for_arity(
        &mut self,
        base: &[Type],
        actual: usize,
        spec: CallbackSpec<'_>,
        pos: Pos,
    ) -> Option<Vec<Type>> {
        let CallbackSpec {
            method,
            q_rule,
            allow_index,
        } = spec;
        if actual == base.len() {
            return Some(base.to_vec());
        }
        if allow_index && actual == base.len() + 1 {
            let mut indexed = base.to_vec();
            indexed.push(Type::I32);
            return Some(indexed);
        }
        if allow_index && actual == base.len() + 2 {
            let actual = format!("{method} callback with the container parameter");
            let _ = self.reject_api_form("T[]", "callback(value, index, array)", &actual, pos);
            return None;
        }
        self.error(
            RuleCode::S014,
            if allow_index {
                format!(
                    "`{method}` callbacks take {} parameter(s), or {} with a trailing \
                     `i32` index; got {actual} (Q27)",
                    base.len(),
                    base.len() + 1,
                )
            } else if q_rule == "Q24" || q_rule == "Q27" {
                format!(
                    "`{method}` callbacks take exactly {} parameter(s); \
                     extra lib callback parameters are not accepted ({q_rule})",
                    base.len(),
                )
            } else {
                format!(
                    "`{method}` callbacks take exactly {} parameter(s); got {actual} ({q_rule})",
                    base.len(),
                )
            },
            pos,
        );
        None
    }

    /// Validates a checked callback value against the method's accepted
    /// parameter list or lists (and return type, when it is not inferred);
    /// returns the value unchanged on success and a poisoned expression
    /// after the mismatch diagnostic otherwise.
    fn expect_callback_shape(
        &mut self,
        checked: hir::Expr,
        params: &[Type],
        ret: Option<&Type>,
        spec: CallbackSpec<'_>,
    ) -> hir::Expr {
        let CallbackSpec {
            method,
            allow_index,
            ..
        } = spec;
        let ok = match &checked.ty {
            Type::Error => true,
            Type::Func(ft) => {
                let indexed = allow_index
                    && ft.params.len() == params.len() + 1
                    && ft.params[..params.len()] == *params
                    && ft.params.last() == Some(&Type::I32);
                (ft.params == params || indexed) && ret.is_none_or(|r| ft.ret == *r)
            }
            _ => false,
        };
        if ok {
            return checked;
        }
        if let Type::Func(ft) = &checked.ty {
            let accepted_arity = ft.params.len() == params.len()
                || (allow_index && ft.params.len() == params.len() + 1);
            if !accepted_arity {
                let pos = checked.pos.clone();
                let _ = self.callback_params_for_arity(params, ft.params.len(), spec, pos.clone());
                return self.err_expr(pos);
            }
        }
        let got = self.type_name(&checked.ty);
        let wanted: Vec<String> = params.iter().map(|t| self.type_name(t)).collect();
        let wanted = if allow_index {
            format!("({}) or ({}, i32)", wanted.join(", "), wanted.join(", "))
        } else {
            format!("({})", wanted.join(", "))
        };
        let ret_n = match ret {
            Some(r) => self.type_name(r),
            None => "…".to_string(),
        };
        self.error(
            RuleCode::S100,
            format!(
                "type mismatch: the `{}` callback expects `{}` => {}, got `{}`",
                method, wanted, ret_n, got
            ),
            checked.pos.clone(),
        );
        self.err_expr(checked.pos)
    }

    /// Emits the Q22 rejection for a known out-of-subset `Array`
    /// member, naming the member and pointing at the accepted spelling;
    /// returns `false` when `name` is not in the rejected set (the
    /// caller then falls back to the generic surface diagnostic).
    pub(super) fn arr_subset_rejection(&mut self, name: &str, pos: Pos) -> bool {
        let Some(rejection) = crate::ambient::array_rejection(name) else {
            return false;
        };
        self.emit_api_rejection(rejection, name, pos);
        true
    }

    /// The generic out-of-surface diagnostic for an array member that is
    /// neither accepted nor in the named Q22 rejected set.
    pub(super) fn arr_surface_error(&mut self, name: &str, pos: Pos) {
        self.error(
            RuleCode::S100,
            format!(
                "`{}` is outside the array surface (length, indexing, push, pop, \
                 and the Q22 Array methods)",
                name
            ),
            pos,
        );
    }
}
