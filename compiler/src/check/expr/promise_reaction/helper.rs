//! The compiler-supplied async helpers of `then`, `catch`, and `finally`
//! (compiler.md §186 rules 2 and 3).
//!
//! The checker builds one HIR async function for each reaction form,
//! callback shape, and type pair, and reuses it at every call with the
//! same key. Each helper awaits the receiver and calls the callbacks with
//! the meaning of these TypeScript bodies (`turn` is an async function
//! with an empty body, so `await turn()` is one ready-queue turn):
//!
//! | Form | Body |
//! |---|---|
//! | `then(f)`, `f: (v: T) => U` | `return f(await h);` |
//! | `then(f)`, `f: (v: T) => Promise<U>` | `const p = f(await h); await turn(); return await p;` |
//! | `then(f)`, `f: (v: T) => void` | `f(await h);` |
//! | `then(f)`, `f: () => …` | `await h;` and then the same call with no argument |
//! | `then(f, r)` | `let v: T; try { v = await h; } catch (e) { return r(e); } return f(v);` |
//! | `catch(r)` | `try { return await h; } catch (e) { return r(e); }` |
//! | `finally(f)` | `try { return await h; } finally { f(); await turn(); await turn(); }` |
//!
//! Each callback call delivers its result as the `then(f)` rows show. The
//! extra turns give the `node` order: an adoption takes two turns in
//! `node` and `return await p` takes one, and a `finally` reaction takes
//! two more turns than a `try`/`finally` block (§186 rule 3).
//!
//! The positions come from the first call with the key. The receiver
//! reads and its `await` take the receiver position, a callback call and
//! its delivery take the callback argument position, and the statements
//! take the method name position.

use super::*;

fn local(name: &str, ty: &Type, annotated: bool, pos: &Pos) -> hir::Expr {
    hir::Expr {
        pending_work: None,
        kind: ExprKind::Local(name.to_string(), ty.clone(), annotated),
        ty: ty.clone(),
        pos: pos.clone(),
    }
}

fn await_handle(handle: hir::Expr, value: &Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        pending_work: None,
        kind: ExprKind::AsyncHandleAwait(Box::new(handle)),
        ty: value.clone(),
        pos: pos.clone(),
    }
}

/// A value that carries a handle crosses the call boundary with its
/// obligation, as a source call result does.
fn transfer(value: hir::Expr, transfers: bool) -> hir::Expr {
    if !transfers {
        return value;
    }
    let ty = value.ty.clone();
    let pos = value.pos.clone();
    hir::Expr {
        pending_work: None,
        kind: ExprKind::AsyncHandleTransfer {
            value: Box::new(value),
            origin: 0,
        },
        ty,
        pos,
    }
}

fn parameter(name: &str, ty: Type, pos: &Pos) -> hir::Param {
    hir::Param {
        escapes: false,
        default_can_raise: false,
        name: name.to_string(),
        ty,
        default: None,
        foreign_provenance: None,
        pos: pos.clone(),
    }
}

/// The helper parts that each body shape reads.
struct Body<'a> {
    value: &'a Type,
    result: &'a Type,
    /// Whether the receiver value type is `void`.
    value_void: bool,
    /// Whether the helper result type is `void`.
    result_void: bool,
    /// The turn function; a helper with no extra turn names none.
    turn: Option<hir::Symbol>,
    receiver: &'a Pos,
    pos: &'a Pos,
}

impl Body<'_> {
    fn handle(&self) -> hir::Expr {
        local(
            "h",
            &Type::async_handle(self.value.clone()),
            true,
            self.receiver,
        )
    }

    fn awaited(&self) -> hir::Expr {
        await_handle(self.handle(), self.value, self.receiver)
    }

    fn turn(&self, pos: &Pos) -> Option<hir::Stmt> {
        let turn = self.turn.clone()?;
        Some(hir::Stmt::Expr(hir::Expr {
            pending_work: None,
            kind: ExprKind::AsyncCall {
                callee: hir::AsyncCallee::Function(turn),
                args: Vec::new(),
            },
            ty: Type::Void,
            pos: pos.clone(),
        }))
    }

    fn ret(value: Option<hir::Expr>, pos: &Pos) -> hir::Stmt {
        hir::Stmt::Return {
            value,
            pos: pos.clone(),
        }
    }

    /// Calls the callback parameter `name`, with `argument` when it takes one.
    fn call(name: &str, callback: &Callback, argument: Option<hir::Expr>) -> hir::Expr {
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Value(Box::new(local(name, &callback.ty, true, &callback.pos))),
                args: argument.into_iter().take(callback.arity).collect(),
            },
            ty: callback.result.clone(),
            pos: callback.pos.clone(),
        }
    }

    /// Calls `name` with `argument` and returns its delivered result.
    fn deliver(
        &self,
        name: &str,
        callback: &Callback,
        argument: Option<hir::Expr>,
    ) -> Vec<hir::Stmt> {
        let pos = &callback.pos;
        let call = Self::call(name, callback, argument);
        match callback.delivery {
            Delivery::Value => vec![Self::ret(
                Some(transfer(call, callback.transfers)),
                self.pos,
            )],
            Delivery::Void => vec![hir::Stmt::Expr(call), Self::ret(None, self.pos)],
            Delivery::Adopt => {
                let handle_ty = Type::async_handle(self.result.clone());
                let awaited = await_handle(local("p", &handle_ty, false, pos), self.result, pos);
                let mut out = vec![hir::Stmt::Let {
                    name: "p".to_string(),
                    ty: handle_ty,
                    mutable: false,
                    dispose: false,
                    init: transfer(call, callback.transfers),
                    pos: pos.clone(),
                }];
                out.extend(self.turn(pos));
                if self.result_void {
                    out.extend([hir::Stmt::Expr(awaited), Self::ret(None, self.pos)]);
                } else {
                    out.push(Self::ret(Some(awaited), self.pos));
                }
                out
            }
        }
    }

    /// `try { <body> } catch (e) { <r(e)> }`.
    fn guarded(&self, body: Vec<hir::Stmt>, error: &Type, r: &Callback) -> hir::Stmt {
        hir::Stmt::Try {
            body,
            binding: Some(("e".to_string(), error.clone())),
            handler: self.deliver("r", r, Some(local("e", error, false, &r.pos))),
            pos: self.pos.clone(),
        }
    }

    /// The await of the receiver as the last statement of a value path.
    fn await_or_return(&self) -> hir::Stmt {
        if self.value_void {
            hir::Stmt::Expr(self.awaited())
        } else {
            Self::ret(Some(self.awaited()), self.pos)
        }
    }

    fn statements(&self, reaction: &Reaction, error: &Type) -> Vec<hir::Stmt> {
        match reaction {
            Reaction::Then(f) => {
                if f.arity == 1 {
                    self.deliver("f", f, Some(self.awaited()))
                } else {
                    let mut out = vec![hir::Stmt::Expr(self.awaited())];
                    out.extend(self.deliver("f", f, None));
                    out
                }
            }
            Reaction::ThenBoth(f, r) => {
                if self.value_void {
                    let mut out =
                        vec![self.guarded(vec![hir::Stmt::Expr(self.awaited())], error, r)];
                    out.extend(self.deliver("f", f, None));
                    return out;
                }
                let stored = local("v", self.value, true, self.receiver);
                let assign = hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Assign {
                        update: None,
                        op: None,
                        target: Box::new(stored.clone()),
                        value: Box::new(self.awaited()),
                    },
                    ty: self.value.clone(),
                    pos: self.receiver.clone(),
                };
                let mut out = vec![
                    hir::Stmt::Let {
                        name: "v".to_string(),
                        ty: self.value.clone(),
                        mutable: true,
                        dispose: false,
                        init: hir::Expr {
                            pending_work: None,
                            kind: ExprKind::Unassigned,
                            ty: self.value.clone(),
                            pos: self.pos.clone(),
                        },
                        pos: self.pos.clone(),
                    },
                    self.guarded(vec![hir::Stmt::Expr(assign)], error, r),
                ];
                out.extend(self.deliver("f", f, Some(stored)));
                out
            }
            Reaction::Catch(r) => vec![self.guarded(vec![self.await_or_return()], error, r)],
            Reaction::Finally(f) => {
                let mut finalizer = vec![hir::Stmt::Expr(Self::call("f", f, None))];
                finalizer.extend(self.turn(&f.pos));
                finalizer.extend(self.turn(&f.pos));
                vec![hir::Stmt::Using {
                    bindings: Vec::new(),
                    body: vec![self.await_or_return()],
                    finalizer: Some(finalizer),
                    pos: self.pos.clone(),
                }]
            }
        }
    }
}

impl Checker<'_> {
    /// Returns the symbol of the helper for one reaction at `value` and
    /// `result`, and builds it on first use.
    pub(super) fn promise_helper(
        &mut self,
        reaction: &Reaction,
        value: &Type,
        result: &Type,
        receiver: &Pos,
        method: &Pos,
        pos: &Pos,
    ) -> String {
        let callback_types = reaction
            .callbacks()
            .into_iter()
            .map(|callback| &callback.ty)
            .collect::<Vec<_>>();
        let key = format!(
            "promise-reaction:{}:{value:?}:{callback_types:?}",
            reaction.describe()
        );
        if let Some(symbol) = self.instance_symbols.get(&key) {
            return symbol.clone();
        }
        let turns = matches!(reaction, Reaction::Finally(_))
            || reaction
                .callbacks()
                .iter()
                .any(|callback| callback.delivery == Delivery::Adopt);
        let turn = turns.then(|| hir::Symbol::from_full_text(self.promise_turn(pos)));
        let name = format!(
            "[[Promise.{}]]<{}, {}>",
            reaction.describe(),
            self.type_name(value),
            self.type_name(result)
        );
        let symbol = crate::check::identity::instance_symbol(&key, &name);
        let error = Type::Class(self.error_class);
        let body = Body {
            value,
            result,
            value_void: self.apparent_type(value) == Type::Void,
            result_void: self.apparent_type(result) == Type::Void,
            turn,
            receiver,
            pos: method,
        }
        .statements(reaction, &error);
        let names: &[&str] = if matches!(reaction, Reaction::Catch(_)) {
            &["r"]
        } else {
            &["f", "r"]
        };
        let mut params = vec![parameter("h", Type::async_handle(value.clone()), pos)];
        params.extend(
            names
                .iter()
                .zip(reaction.callbacks())
                .map(|(name, callback)| parameter(name, callback.ty.clone(), pos)),
        );
        self.functions.push(hir::Function {
            symbol: hir::Symbol::from_full_text(symbol.clone()),
            synthesized_helper: false,
            name,
            exported: false,
            is_generator: false,
            is_async: true,
            params,
            ret: result.clone(),
            body,
            can_raise: false,
            pos: pos.clone(),
        });
        self.instance_symbols.insert(key, symbol.clone());
        symbol
    }

    /// The async function with an empty body whose `await` is one turn.
    fn promise_turn(&mut self, pos: &Pos) -> String {
        let key = "promise-reaction:turn".to_string();
        if let Some(symbol) = self.instance_symbols.get(&key) {
            return symbol.clone();
        }
        let name = "[[Promise.turn]]".to_string();
        let symbol = crate::check::identity::instance_symbol(&key, &name);
        self.functions.push(hir::Function {
            symbol: hir::Symbol::from_full_text(symbol.clone()),
            synthesized_helper: false,
            name,
            exported: false,
            is_generator: false,
            is_async: true,
            params: Vec::new(),
            ret: Type::Void,
            body: Vec::new(),
            can_raise: false,
            pos: pos.clone(),
        });
        self.instance_symbols.insert(key, symbol.clone());
        symbol
    }
}
