//! An async return adopts a handle (compiler.md §236).
//!
//! `return e`, where `e` is a `Promise<S>` in an async function with the
//! result `Promise<T>`, has the meaning of `node` adoption (rule 2). The
//! function holds one completion state and one epilogue (rule 2a):
//!
//! - An adopting return stores the handle, sets the state to the index of
//!   its handle local, and returns a placeholder. So it leaves each scope
//!   of the function as a `return` does.
//! - A value return sets the state to 0 first.
//! - An exception that leaves the body sets the state to 0: the body is
//!   the body of a `catch` that clears the state and throws again.
//! - The epilogue is the `finally` of the function body. If the state is
//!   not 0, it waits one turn of the queue and returns the `await` of the
//!   selected handle. That return replaces the placeholder.
//!
//! So the last completion of the function wins. A replaced or abandoned
//! handle is released with its local, with no `await`. The turn is an
//! `await` of the async function with an empty body that §186 uses for
//! one turn.

use super::*;

/// One handle local of the adopted returns of a function, for one handle
/// type, and the completion state that each adopted return writes
/// (compiler.md §236 rule 2a).
#[derive(Debug, Clone)]
pub(crate) struct Adoption {
    handle: String,
    /// The completion state local; each adoption of a function has the same.
    state: String,
    ty: Type,
    /// The value type of the handle.
    value: Type,
}

impl Checker<'_> {
    /// compiler.md §236 rule 3: a return whose leaves mix a handle and a
    /// value has the meaning of one return per leaf. `return b ? x : h` is
    /// `if (b) return x; else return h;`, so the condition runs once. A
    /// nested conditional splits again when its own leaves mix. Returns
    /// whether the return split.
    pub(super) fn split_mixed_return(
        &mut self,
        operand: &ast::Expr,
        ret: &Type,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) -> bool {
        let in_async = fx.frames.last().is_some_and(|frame| frame.is_async);
        if !in_async || matches!(self.apparent_type(ret), Type::Void | Type::Error) {
            return false;
        }
        let ast::Expr::Cond(conditional) = crate::check::expr::unparen_expr(operand) else {
            return false;
        };
        let mut leaves = Vec::new();
        leaves_of(operand, &mut leaves);
        let handles = leaves
            .iter()
            .filter(|leaf| self.leaf_gives_handle(leaf, fx))
            .count();
        if handles == 0 || handles == leaves.len() {
            return false;
        }
        let leaf_return = |leaf: &ast::Expr| {
            Box::new(ast::Stmt::Return(ast::ReturnStmt {
                span: leaf.span(),
                arg: Some(Box::new(leaf.clone())),
            }))
        };
        let split = ast::Stmt::If(ast::IfStmt {
            span: conditional.span,
            test: conditional.test.clone(),
            cons: leaf_return(&conditional.cons),
            alt: Some(leaf_return(&conditional.alt)),
        });
        self.check_stmt(&split, fx, out);
        true
    }

    /// The context of the operand of `return` in a function with the result
    /// `ret` (compiler.md §236 rule 3). If each leaf of the operand gives a
    /// handle without a context, the operand takes `Promise<T>`. Otherwise
    /// it takes `T`. A return whose leaves mix splits before this context
    /// (`split_mixed_return`).
    pub(super) fn return_context(&self, operand: &ast::Expr, ret: &Type, fx: &FnCtx) -> Type {
        let in_async = fx.frames.last().is_some_and(|frame| frame.is_async);
        if !in_async || matches!(self.apparent_type(ret), Type::Void) {
            return ret.clone();
        }
        let mut leaves = Vec::new();
        leaves_of(operand, &mut leaves);
        if leaves.iter().all(|leaf| self.leaf_gives_handle(leaf, fx)) {
            return Type::async_handle(ret.clone());
        }
        ret.clone()
    }

    /// Whether `leaf`, read without a context, has the type `Promise<…>`:
    /// the declared result of its callee, or the declared type of a local,
    /// a parameter, or a field.
    fn leaf_gives_handle(&self, leaf: &ast::Expr, fx: &FnCtx) -> bool {
        self.leaf_handle(leaf, fx).is_some()
    }

    /// The declared `Promise<…>` type of `leaf`, read without a context.
    /// A leaf whose declaration has a type parameter, or a reaction, gives
    /// `Type::Error`: it gives a handle, but its value type depends on the
    /// call.
    fn leaf_handle(&self, leaf: &ast::Expr, fx: &FnCtx) -> Option<Type> {
        let handle =
            |ty: Type| matches!(self.apparent_type(&ty), Type::AsyncHandle(_)).then_some(ty);
        match leaf {
            ast::Expr::Ident(name) => self.local_type(name.sym.as_ref(), fx).and_then(handle),
            ast::Expr::Member(member) => {
                let ast::MemberProp::Ident(field) = &member.prop else {
                    return None;
                };
                let class = self.receiver_class(&member.obj, fx)?;
                self.classes[class.0]
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field.sym.as_ref())
                    .and_then(|candidate| handle(candidate.ty.clone()))
            }
            ast::Expr::Call(call) => {
                let ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                self.callee_handle(crate::check::expr::unparen_expr(callee), fx)
            }
            _ => None,
        }
    }

    fn callee_handle(&self, callee: &ast::Expr, fx: &FnCtx) -> Option<Type> {
        let template = |function: &ast::Function| declares_handle(function).then_some(Type::Error);
        match callee {
            ast::Expr::Ident(name) if !fx.owns_local_name(name.sym.as_ref()) => {
                match self.peek_scope_item(name.sym.as_ref()) {
                    Some(ScopeItem::Func(function)) => self
                        .fn_sigs
                        .get(&function)
                        .and_then(|sig| self.signature_handle(sig)),
                    Some(ScopeItem::GenericFunc(key)) => self
                        .generic_fns
                        .get(&key)
                        .and_then(|generic| template(&generic.function)),
                    _ => None,
                }
            }
            ast::Expr::Ident(name) => self
                .local_type(name.sym.as_ref(), fx)
                .and_then(|ty| self.apparent_type(&ty).function_type().cloned())
                .and_then(|function| {
                    matches!(self.apparent_type(&function.ret), Type::AsyncHandle(_))
                        .then(|| function.ret.clone())
                }),
            ast::Expr::Member(member) => {
                let ast::MemberProp::Ident(name) = &member.prop else {
                    return None;
                };
                if matches!(name.sym.as_ref(), "then" | "catch" | "finally") {
                    return Some(Type::Error);
                }
                let class = self.receiver_class(&member.obj, fx)?;
                let signatures = &self.class_sigs[class.0];
                let name = name.sym.as_ref();
                match signatures
                    .methods
                    .get(name)
                    .or_else(|| signatures.static_methods.get(name))
                {
                    Some(sig) => self.signature_handle(sig),
                    None => signatures
                        .generic_methods
                        .get(name)
                        .or_else(|| signatures.static_generic_methods.get(name))
                        .and_then(|generic| template(&generic.function)),
                }
            }
            _ => None,
        }
    }

    /// The handle type that a call of `sig` gives: the declared result of
    /// a function that returns a `Promise`, or the handle of an async
    /// function.
    fn signature_handle(&self, sig: &FnSig) -> Option<Type> {
        if matches!(self.apparent_type(&sig.ret), Type::AsyncHandle(_)) {
            Some(sig.ret.clone())
        } else if sig.is_async {
            Some(Type::async_handle(sig.ret.clone()))
        } else {
            None
        }
    }

    /// The value type `T` that an unannotated async arrow body `operand`
    /// gives when its leaves mix a handle and a value (compiler.md §236
    /// rule 3): the value type of the first handle leaf whose declaration
    /// states it. Absent when the leaves do not mix, or when no handle leaf
    /// states its value type.
    pub(in crate::check) fn mixed_body_result(
        &self,
        operand: &ast::Expr,
        fx: &FnCtx,
    ) -> Option<Type> {
        let mut leaves = Vec::new();
        leaves_of(operand, &mut leaves);
        let handles: Vec<Type> = leaves
            .iter()
            .filter_map(|leaf| self.leaf_handle(leaf, fx))
            .collect();
        if handles.is_empty() || handles.len() == leaves.len() {
            return None;
        }
        handles.iter().find_map(|ty| match self.apparent_type(ty) {
            Type::AsyncHandle(value) => Some(*value),
            _ => None,
        })
    }

    /// The declared type of the local or parameter `name`.
    fn local_type(&self, name: &str, fx: &FnCtx) -> Option<Type> {
        fx.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name))
            .map(|local| local.ty.clone())
    }

    /// The class of a receiver that is a local, a parameter, `this`, a
    /// construction, or a class name. A `C | null` receiver gives `C`.
    fn receiver_class(&self, receiver: &ast::Expr, fx: &FnCtx) -> Option<ClassId> {
        let ty = match crate::check::expr::unparen_expr(receiver) {
            ast::Expr::Ident(name) => match self.local_type(name.sym.as_ref(), fx) {
                Some(ty) => ty,
                None => match self.peek_scope_item(name.sym.as_ref()) {
                    Some(ScopeItem::Class(class)) => return Some(class),
                    _ => return None,
                },
            },
            ast::Expr::This(_) => fx.frames.last()?.this_ty.clone()?,
            ast::Expr::New(new) => match &*new.callee {
                ast::Expr::Ident(name) => match self.peek_scope_item(name.sym.as_ref()) {
                    Some(ScopeItem::Class(class)) => return Some(class),
                    _ => return None,
                },
                _ => return None,
            },
            _ => return None,
        };
        // A nullable receiver reads its class: the checker of the member
        // access decides whether the access is legal.
        match self.apparent_type(&ty) {
            Type::Class(class) => Some(class),
            Type::Nullable(inner, _) => match self.apparent_type(&inner) {
                Type::Class(class) => Some(class),
                _ => None,
            },
            _ => None,
        }
    }

    /// Lowers `return e` of compiler.md §236 rule 1 into `out`: hold the
    /// handle, write the state, and return a placeholder, so that the function
    /// leaves each scope as a `return` does. The adoption epilogue of the
    /// function reads the handle (rule 2). The return reads the handle, so
    /// S013 does not fire for it (rule 5). Returns `checked` unchanged when
    /// the return is not such a return.
    pub(super) fn adopt_returned_handle(
        &mut self,
        checked: hir::Expr,
        ret: &Type,
        fx: &mut FnCtx,
        pos: &Pos,
        out: &mut Vec<hir::Stmt>,
    ) -> Option<hir::Expr> {
        let in_async = fx.frames.last().is_some_and(|frame| frame.is_async);
        // Rule 1 reads the type `Promise<S>` itself: a type parameter that a
        // `Promise` constrains keeps its diagnostic.
        let Type::AsyncHandle(value) = checked.ty.clone() else {
            return Some(checked);
        };
        if !in_async
            || matches!(
                self.apparent_type(ret),
                Type::AsyncHandle(_) | Type::Error | Type::Void
            )
            || !self.assignable(&value, ret)
        {
            return Some(checked);
        }
        let origins = self.expr_async_origins(&checked, fx);
        fx.handle_async_origins(&origins);
        let ty = checked.ty.clone();
        let index = match fx.adoptions.iter().position(|adoption| adoption.ty == ty) {
            Some(index) => index,
            None => {
                let id = self.next_compound_local_id;
                self.next_compound_local_id += 1;
                let state = fx.adoptions.first().map_or_else(
                    || format!("[[adopt#{id}.state]]"),
                    |first| first.state.clone(),
                );
                fx.adoptions.push(Adoption {
                    handle: format!("[[adopt#{id}]]"),
                    state,
                    ty: ty.clone(),
                    value: *value,
                });
                fx.adoptions.len() - 1
            }
        };
        let adoption = &fx.adoptions[index];
        out.push(hir::Stmt::Expr(assign(&adoption.handle, &ty, checked)));
        out.push(hir::Stmt::Expr(assign(
            &adoption.state,
            &Type::I32,
            state_value(index + 1, pos),
        )));
        out.push(hir::Stmt::Return {
            value: Some(expr(hir::ExprKind::Zero, ret.clone(), pos)),
            pos: pos.clone(),
        });
        None
    }

    /// The completion state and the epilogue of compiler.md §236 rules 2
    /// and 2a. `body` becomes the body of a `catch` that clears the state
    /// and throws again, inside a `try` whose `finally` is the epilogue. A
    /// value return of `body` clears the state first. If the state selects
    /// a handle, the epilogue waits one turn and returns the `await` of
    /// that handle. A body with no adopted return is unchanged (rule 5).
    /// Each node of the epilogue carries the position `pos` of the
    /// function, because it runs as the function completes.
    pub(super) fn adoption_epilogue(
        &mut self,
        mut body: Vec<hir::Stmt>,
        adoptions: Vec<Adoption>,
        pos: &Pos,
    ) -> Vec<hir::Stmt> {
        let Some(first) = adoptions.first() else {
            return body;
        };
        let state = first.state.clone();
        clear_state_at_value_returns(&mut body, &state, pos);
        let mut out = vec![hir::Stmt::Let {
            name: state.clone(),
            ty: Type::I32,
            mutable: true,
            dispose: false,
            init: state_value(0, pos),
            pos: pos.clone(),
        }];
        let turn = hir::Symbol::from_full_text(self.promise_turn(pos));
        let mut selected = vec![hir::Stmt::Expr(expr(
            hir::ExprKind::AsyncCall {
                callee: hir::AsyncCallee::Function(turn),
                args: Vec::new(),
            },
            Type::Void,
            pos,
        ))];
        let last = adoptions.len() - 1;
        for (index, adoption) in adoptions.iter().enumerate() {
            out.push(hir::Stmt::Let {
                name: adoption.handle.clone(),
                ty: adoption.ty.clone(),
                mutable: true,
                dispose: false,
                init: expr(hir::ExprKind::Zero, adoption.ty.clone(), pos),
                pos: pos.clone(),
            });
            let ret = hir::Stmt::Return {
                value: Some(expr(
                    hir::ExprKind::AsyncHandleAwait(Box::new(local(
                        &adoption.handle,
                        &adoption.ty,
                        pos,
                    ))),
                    adoption.value.clone(),
                    pos,
                )),
                pos: pos.clone(),
            };
            selected.push(if index == last {
                ret
            } else {
                hir::Stmt::If {
                    cond: compare(&state, hir::BinOp::Eq, index + 1, pos),
                    then: vec![ret],
                    els: None,
                    pos: pos.clone(),
                }
            });
        }
        let error = Type::Class(self.error_class);
        let caught = format!("{state}.error");
        let guarded = hir::Stmt::Try {
            body,
            binding: Some((caught.clone(), error.clone())),
            handler: vec![
                hir::Stmt::Expr(assign(&state, &Type::I32, state_value(0, pos))),
                hir::Stmt::Throw {
                    value: local(&caught, &error, pos),
                    pos: pos.clone(),
                },
            ],
            pos: pos.clone(),
        };
        out.push(hir::Stmt::Using {
            bindings: Vec::new(),
            body: vec![guarded],
            finalizer: Some(vec![hir::Stmt::If {
                cond: compare(&state, hir::BinOp::Ne, 0, pos),
                then: selected,
                els: None,
                pos: pos.clone(),
            }]),
            pos: pos.clone(),
        });
        out
    }
}

/// Puts `state = 0` before each value return of `body` (compiler.md §236
/// rule 2a). An adopted return writes the state just before its
/// placeholder return, so it keeps its state. A lambda body is a separate
/// function, so the walk does not enter expressions.
fn clear_state_at_value_returns(body: &mut Vec<hir::Stmt>, state: &str, pos: &Pos) {
    let mut index = 0;
    while index < body.len() {
        let adopted = index > 0
            && matches!(&body[index - 1], hir::Stmt::Expr(hir::Expr {
                kind: hir::ExprKind::Assign { target, .. },
                ..
            }) if matches!(&target.kind, hir::ExprKind::Local(name, ..) if name == state));
        match &mut body[index] {
            hir::Stmt::Return { .. } if !adopted => {
                body.insert(
                    index,
                    hir::Stmt::Expr(assign(state, &Type::I32, state_value(0, pos))),
                );
                index += 1;
            }
            hir::Stmt::If { then, els, .. } => {
                clear_state_at_value_returns(then, state, pos);
                if let Some(els) = els {
                    clear_state_at_value_returns(els, state, pos);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::DoWhile { body, .. }
            | hir::Stmt::ForOf { body, .. }
            | hir::Stmt::GeneratorForOf { body, .. }
            | hir::Stmt::Block(body) => clear_state_at_value_returns(body, state, pos),
            hir::Stmt::For { body, step, .. } => {
                clear_state_at_value_returns(body, state, pos);
                clear_state_at_value_returns(step, state, pos);
            }
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    clear_state_at_value_returns(&mut case.body, state, pos);
                }
            }
            hir::Stmt::Try { body, handler, .. } => {
                clear_state_at_value_returns(body, state, pos);
                clear_state_at_value_returns(handler, state, pos);
            }
            hir::Stmt::Using {
                body, finalizer, ..
            } => {
                clear_state_at_value_returns(body, state, pos);
                if let Some(finalizer) = finalizer {
                    clear_state_at_value_returns(finalizer, state, pos);
                }
            }
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Return { .. }
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_)
            | hir::Stmt::Throw { .. } => {}
        }
        index += 1;
    }
}

fn state_value(value: usize, pos: &Pos) -> hir::Expr {
    expr(
        hir::ExprKind::Int(i64::try_from(value).unwrap_or(i64::MAX)),
        Type::I32,
        pos,
    )
}

fn compare(state: &str, op: hir::BinOp, value: usize, pos: &Pos) -> hir::Expr {
    expr(
        hir::ExprKind::Binary {
            op,
            left: Box::new(local(state, &Type::I32, pos)),
            right: Box::new(state_value(value, pos)),
        },
        Type::Bool,
        pos,
    )
}

/// Each leaf of `operand`: the operand, or a branch of a conditional or of
/// parentheses in it (compiler.md §236 rule 3).
fn leaves_of<'a>(operand: &'a ast::Expr, out: &mut Vec<&'a ast::Expr>) {
    match crate::check::expr::unparen_expr(operand) {
        ast::Expr::Cond(conditional) => {
            leaves_of(&conditional.cons, out);
            leaves_of(&conditional.alt, out);
        }
        leaf => out.push(leaf),
    }
}

/// Whether a generic template is async, or declares a `Promise<T>` result.
fn declares_handle(function: &ast::Function) -> bool {
    function.is_async
        || matches!(
            function.return_type.as_deref().map(|annotation| &*annotation.type_ann),
            Some(ast::TsType::TsTypeRef(reference))
                if matches!(&reference.type_name, ast::TsEntityName::Ident(name) if name.sym.as_ref() == "Promise")
        )
}

fn expr(kind: hir::ExprKind, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        pending_work: None,
        kind,
        ty,
        pos: pos.clone(),
    }
}

fn local(name: &str, ty: &Type, pos: &Pos) -> hir::Expr {
    expr(
        hir::ExprKind::Local(name.to_string(), ty.clone(), false),
        ty.clone(),
        pos,
    )
}

fn assign(name: &str, ty: &Type, value: hir::Expr) -> hir::Expr {
    let pos = value.pos.clone();
    expr(
        hir::ExprKind::Assign {
            update: None,
            op: None,
            target: Box::new(local(name, ty, &pos)),
            value: Box::new(value),
        },
        ty.clone(),
        &pos,
    )
}
