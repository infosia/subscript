//! The decision of an in-place `filter` (`compiler.md` §359 rules 1 to 4).
//!
//! One HIR walk over each body finds the static `filter` calls whose
//! receiver is a new array with one reader. The lowering reads the result
//! by the key of each call; it does not walk the HIR again.

use super::*;

/// Names one static `filter` call: its position and the position of its
/// callback. A chain gives each of its calls the position of the chain
/// start, so the callback tells the calls of one chain apart.
pub(super) type FilterKey = (String, u32, u32, String, u32, u32);

/// The key of a static `filter` call.
pub(super) fn filter_key(call: &Pos, callback: &Pos) -> FilterKey {
    (
        call.file.clone(),
        call.line,
        call.col,
        callback.file.clone(),
        callback.line,
        callback.col,
    )
}

/// The decision mode of a lowering. Only tests select a mode other than
/// `Normal` (`compiler.md` §359 rule 14 and acceptance 3).
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InPlaceMode {
    /// Rules 1 to 4 decide each call.
    Normal,
    /// Each `filter` allocates: the reference of rule 14.
    Off,
    /// The decision ignores rule 3: the control of acceptance 3.
    IgnoreRule3,
}

/// Lowers `module` as [`lower_module`] does, with the decision mode
/// `in_place`. This entry is for tests only (`compiler.md` §359 rule 14).
///
/// # Errors
///
/// Returns the errors of [`lower_module`].
#[doc(hidden)]
pub fn lower_module_with_in_place_mode(
    module: &hir::Module,
    in_place: InPlaceMode,
) -> Result<l::Module, LowerError> {
    lower_module_in_mode(module, false, in_place)
}

/// Returns the key of each static `filter` call that is in place.
///
/// A key that names two calls with different decisions (for example two
/// instances of one generic function) is not in the set.
pub(super) fn in_place_filters(module: &hir::Module, mode: InPlaceMode) -> HashSet<FilterKey> {
    if mode == InPlaceMode::Off {
        return HashSet::new();
    }
    let mut accepted = HashSet::new();
    let mut declined = HashSet::new();
    for owner in module.expression_owners() {
        let hir::ExpressionOwner::Body {
            statements,
            function,
        } = owner
        else {
            continue;
        };
        // Rule 4: a generator or an async function has no in-place form.
        let coroutine = function.is_some_and(|function| function.is_generator || function.is_async);
        let mut walk = Walk {
            ignore_rule_3: mode == InPlaceMode::IgnoreRule3,
            ..Walk::default()
        };
        for parameter in function.iter().flat_map(|function| &function.params) {
            walk.declare(&parameter.name);
        }
        walk.statements(statements);
        for call in &walk.calls {
            if !coroutine && walk.in_place(call) {
                accepted.insert(call.key.clone());
            } else {
                declined.insert(call.key.clone());
            }
        }
    }
    accepted.retain(|key| !declined.contains(key));
    accepted
}

/// True if `args` are those of a static callback call (§8.1d B): a
/// dynamic array receiver, and a named function or a lambda literal.
pub(super) fn is_static_callback(args: &[hir::Expr]) -> bool {
    let dynamic_receiver = matches!(
        args.first().map(|argument| &argument.ty),
        Some(Type::Array(_))
    );
    let known_callback = args.get(1).is_some_and(|callback| {
        let callback = match &callback.kind {
            hir::ExprKind::Cast(inner) => inner,
            _ => callback,
        };
        matches!(
            callback.kind,
            hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. }
        )
    });
    dynamic_receiver && known_callback
}

/// Rule 2a: a static `map` or `filter` call on a dynamic array, or a
/// dynamic array literal.
fn is_new_array(expr: &hir::Expr) -> bool {
    match &expr.kind {
        hir::ExprKind::Call {
            callee: hir::Callee::Arr(hir::ArrFn::Map | hir::ArrFn::Filter),
            args,
        } => is_static_callback(args),
        hir::ExprKind::ArrayLit(_) => matches!(expr.ty, Type::Array(_)),
        _ => false,
    }
}

enum Receiver {
    New,
    Local(String),
    Other,
}

struct FilterCall {
    key: FilterKey,
    receiver: Receiver,
    loops: Vec<u32>,
    in_async_lambda: bool,
}

#[derive(Default)]
struct Walk {
    /// The loops that contain the current node, outermost first. A lambda
    /// body counts as a loop: its code can run more than once (rule 3).
    loops: Vec<u32>,
    next_loop: u32,
    async_lambdas: usize,
    declarations: HashMap<String, usize>,
    reads: HashMap<String, usize>,
    /// Each `const` local whose initializer is a form of rule 2a, with the
    /// loops that contain its declaration.
    new_constants: HashMap<String, Vec<u32>>,
    calls: Vec<FilterCall>,
    ignore_rule_3: bool,
}

impl Walk {
    fn declare(&mut self, name: &str) {
        *self.declarations.entry(name.to_string()).or_default() += 1;
    }

    fn read(&mut self, name: &str) {
        *self.reads.entry(name.to_string()).or_default() += 1;
    }

    fn in_place(&self, call: &FilterCall) -> bool {
        if call.in_async_lambda {
            return false;
        }
        match &call.receiver {
            Receiver::New => true,
            Receiver::Local(name) => {
                self.declarations.get(name) == Some(&1)
                    && self.reads.get(name) == Some(&1)
                    && self
                        .new_constants
                        .get(name)
                        .is_some_and(|loops| self.ignore_rule_3 || rule_3_holds(loops, &call.loops))
            }
            Receiver::Other => false,
        }
    }

    fn in_loop(&mut self, visit: impl FnOnce(&mut Self)) {
        self.loops.push(self.next_loop);
        self.next_loop += 1;
        visit(self);
        self.loops.pop();
    }

    fn statements(&mut self, statements: &[hir::Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &hir::Stmt) {
        match statement {
            hir::Stmt::Let {
                name,
                ty,
                mutable,
                init,
                ..
            } => {
                self.expr(init);
                self.declare(name);
                if !*mutable && *ty == init.ty && is_new_array(init) {
                    self.new_constants.insert(name.clone(), self.loops.clone());
                }
            }
            hir::Stmt::While { cond, body, .. } | hir::Stmt::DoWhile { body, cond, .. } => {
                self.in_loop(|walk| {
                    walk.expr(cond);
                    walk.statements(body);
                });
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(init) = init {
                    self.statement(init);
                }
                self.in_loop(|walk| {
                    if let Some(cond) = cond {
                        walk.expr(cond);
                    }
                    walk.statements(step);
                    walk.statements(body);
                });
            }
            hir::Stmt::ForOf {
                name,
                subject,
                body,
                ..
            }
            | hir::Stmt::GeneratorForOf {
                name,
                subject,
                body,
                ..
            } => {
                self.expr(subject);
                self.in_loop(|walk| {
                    walk.declare(name);
                    walk.statements(body);
                });
            }
            hir::Stmt::Try {
                body,
                binding,
                handler,
                ..
            } => {
                self.statements(body);
                if let Some((name, _)) = binding {
                    self.declare(name);
                }
                self.statements(handler);
            }
            hir::Stmt::Using { bindings, .. } => {
                for binding in bindings {
                    self.declare(&binding.name);
                }
                self.children(statement.children());
            }
            _ => self.children(statement.children()),
        }
    }

    fn children(&mut self, children: Vec<hir::HirChild<'_>>) {
        for child in children {
            match child {
                hir::HirChild::Expr(expr) => self.expr(expr),
                hir::HirChild::Stmt(statement) => self.statement(statement),
            }
        }
    }

    fn expr(&mut self, expr: &hir::Expr) {
        match &expr.kind {
            hir::ExprKind::Local(name, ..) => self.read(name),
            hir::ExprKind::Lambda {
                params,
                captures,
                is_async,
                ..
            } => {
                for capture in captures {
                    self.read(&capture.name);
                }
                self.async_lambdas += usize::from(*is_async);
                self.in_loop(|walk| {
                    for parameter in params {
                        walk.declare(&parameter.name);
                    }
                    walk.children(expr.children());
                });
                self.async_lambdas -= usize::from(*is_async);
            }
            hir::ExprKind::Call {
                callee: hir::Callee::Arr(hir::ArrFn::Filter),
                args,
            } if is_static_callback(args) => {
                let receiver = &args[0];
                let receiver = if is_new_array(receiver) {
                    Receiver::New
                } else if let hir::ExprKind::Local(name, ..) = &receiver.kind {
                    Receiver::Local(name.clone())
                } else {
                    Receiver::Other
                };
                self.calls.push(FilterCall {
                    key: filter_key(&expr.pos, &args[1].pos),
                    receiver,
                    loops: self.loops.clone(),
                    in_async_lambda: self.async_lambdas > 0,
                });
                self.children(expr.children());
            }
            _ => self.children(expr.children()),
        }
    }
}

/// Rule 3: each loop that contains the call also contains the declaration.
fn rule_3_holds(declaration: &[u32], call: &[u32]) -> bool {
    declaration == call
}

#[cfg(test)]
mod tests;
