//! Exception-exit ownership facts derived from lexical HIR scopes (§116.1 rule 4b).

use super::{all_declared_functions, sequence_exits, stops_statement_sequence, try_body_raises};
use std::collections::BTreeMap;
use subscript_compiler::{hir, lir as l, Pos, Type};

pub(super) fn compare(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut walk = Walk {
        hir,
        owners: 0,
        caught: 0,
        sites: Vec::new(),
        hooks: Vec::new(),
        controls: Vec::new(),
        names: Vec::new(),
    };
    for function in all_declared_functions(hir) {
        walk.owners = function.params.iter().filter(|p| owned(&p.ty)).count();
        // A parameter is not an immutable local (compiler.md §255 rule 1).
        walk.names = vec![function
            .params
            .iter()
            .map(|p| (p.name.clone(), false))
            .collect()];
        walk.sequence(&function.body);
        // compiler.md §255 rule 4: a parameter default is a body of its
        // own, with the parameters before it as its owners.
        for (index, param) in function.params.iter().enumerate() {
            let Some(default) = &param.default else {
                continue;
            };
            walk.owners = function.params[..index]
                .iter()
                .filter(|p| owned(&p.ty))
                .count();
            walk.caught = 0;
            walk.names = vec![function.params[..index]
                .iter()
                .map(|p| (p.name.clone(), false))
                .collect()];
            walk.expression(default);
        }
    }
    walk.owners = 0;
    walk.names = Vec::new();
    walk.sequence(&hir.top_level);
    let mut sites = BTreeMap::<(String, u32, u32), Vec<usize>>::new();
    for (pos, required) in walk.sites {
        sites
            .entry((pos.file, pos.line, pos.col))
            .or_default()
            .push(required);
    }
    for ((file, line, col), mut required) in sites {
        let pos = Pos::new(file, line, col);
        let mut observed = Vec::new();
        for function in &lir.functions {
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                if instruction.pos != pos
                    || matches!(instruction.kind, l::InstructionKind::ExceptionResume)
                {
                    continue;
                }
                let Some(edge) = instruction.raise_edge() else {
                    continue;
                };
                if let l::RaiseEdge::Handler(id) = edge {
                    if function.blocks.get(id.0 as usize).is_some_and(|block| {
                        matches!(&block.terminator, l::Terminator::Trap(trap) if trap.kind == l::TrapKind::DisposeRaisedDuringExit)
                    }) {
                        continue;
                    }
                }
                let (minimum, maximum) = releases(function, edge, &mut Vec::new());
                observed.push(minimum);
                if minimum != maximum {
                    findings.push(format!("{pos}: exception edge releases between {minimum} and {maximum} owned handles"));
                }
            }
        }
        required.sort_unstable();
        observed.sort_unstable();
        for (released, required) in observed.iter().zip(&required) {
            if released != required {
                findings.push(format!("{pos}: exception edge releases {released} owned handles; lexical scopes require {required}"));
            }
        }
        if observed.len() != required.len() {
            findings.push(format!(
                "{pos}: {} exception edges; lexical scopes require {}",
                observed.len(),
                required.len()
            ));
        }
    }
}

/// The owners that a `FixedArray` traversal keeps: a counted base of a place,
/// or the value of a subject that is not a place (compiler.md §220.1 rule 7).
fn traversal_owners(subject: &hir::Expr) -> usize {
    use hir::ExprKind as K;
    match &subject.kind {
        K::Local(..) | K::Global(_) | K::This => 0,
        K::Field { obj, .. } | K::Index { obj, .. } if matches!(obj.ty, Type::FixedArray(..)) => {
            traversal_owners(obj)
        }
        K::Field { obj, .. } | K::Index { obj, .. } => usize::from(owned(&obj.ty)),
        _ => usize::from(owned(&subject.ty)),
    }
}

fn owned(ty: &Type) -> bool {
    ty.counted_type().is_some()
}

fn releases(
    function: &l::Function,
    edge: &l::RaiseEdge,
    path: &mut Vec<l::BlockId>,
) -> (usize, usize) {
    match edge {
        l::RaiseEdge::Propagate => (0, 0),
        l::RaiseEdge::Handler(block) => block_releases(function, *block, path),
    }
}

fn block_releases(
    function: &l::Function,
    id: l::BlockId,
    path: &mut Vec<l::BlockId>,
) -> (usize, usize) {
    if path.contains(&id) {
        return (0, 0);
    }
    let Some(block) = function.blocks.get(id.0 as usize) else {
        return (0, 0);
    };
    path.push(id);
    let mut count = 0;
    for instruction in &block.instructions {
        match instruction.kind {
            l::InstructionKind::CatchEntry
                if !block.instructions.iter().any(|instruction| {
                    instruction.kind == l::InstructionKind::ExceptionMessage
                }) =>
            {
                path.pop();
                return (count, count);
            }
            l::InstructionKind::AsyncHandleRelease
            | l::InstructionKind::AsyncHandleArrayRelease => count += 1,
            l::InstructionKind::ExceptionResume | l::InstructionKind::ExceptionRestore => {
                let (minimum, maximum) = instruction
                    .raise_edge()
                    .map_or((0, 0), |edge| releases(function, edge, path));
                path.pop();
                return (count + minimum, count + maximum);
            }
            _ => {}
        }
    }
    let targets = match &block.terminator {
        l::Terminator::Branch(target) => vec![target.block],
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => {
            vec![then_target.block, else_target.block]
        }
        l::Terminator::Switch { arms, default, .. } => std::iter::once(default.block)
            .chain(arms.iter().map(|arm| arm.target.block))
            .collect(),
        // A finalizer that awaits continues the exception path after the resume.
        l::Terminator::Suspend { successor, .. } => vec![*successor],
        _ => Vec::new(),
    };
    let ranges: Vec<_> = targets
        .into_iter()
        .map(|target| block_releases(function, target, path))
        .collect();
    path.pop();
    (
        count + ranges.iter().map(|r| r.0).min().unwrap_or(0),
        count + ranges.iter().map(|r| r.1).max().unwrap_or(0),
    )
}

#[derive(Clone)]
struct Hooks {
    bindings: Vec<hir::UsingBinding>,
    /// The `finally` statements of a `try` statement: each exit runs them,
    /// as each exit runs the hooks of the bindings.
    finalizer: Option<Vec<hir::Stmt>>,
    owners: usize,
    caught: usize,
}

struct Walk<'a> {
    hir: &'a hir::Module,
    owners: usize,
    caught: usize,
    sites: Vec<(Pos, usize)>,
    hooks: Vec<Hooks>,
    controls: Vec<(usize, bool)>,
    /// The locals in scope, innermost last, each with whether it is
    /// immutable (compiler.md §255 rule 1).
    names: Vec<Vec<(String, bool)>>,
}

/// Whether `expr` runs user code: it contains a call, an async call, the
/// creation of an async handle by a call of an async function, an
/// `await`, a `new`, or a `yield`, outside the body of a lambda.
/// `Promise.all` does not run user code (compiler.md §255 rule 2).
fn runs_user_code(expr: &hir::Expr) -> bool {
    use hir::ExprKind as K;
    match &expr.kind {
        K::Lambda { .. } => false,
        K::Call { .. }
        | K::AsyncCall { .. }
        | K::AsyncHandleCreate { .. }
        | K::AsyncHandleAwait(_)
        | K::AsyncSuspend
        | K::New { .. }
        | K::Yield(_) => true,
        _ => expr.children().into_iter().any(|child| match child {
            hir::HirChild::Expr(child) => runs_user_code(child),
            hir::HirChild::Stmt(_) => false,
        }),
    }
}

/// Whether `expr` is a `FixedArray` place: a `FixedArray` local, global,
/// field, or element.
fn fixed_array_place(expr: &hir::Expr) -> bool {
    use hir::ExprKind as K;
    matches!(expr.ty, Type::FixedArray(..))
        && matches!(
            expr.kind,
            K::Local(..) | K::Global(_) | K::Field { .. } | K::Index { .. } | K::This
        )
}

impl<'a> Walk<'a> {
    /// Whether `expr` reads an immutable local.
    fn immutable_local(&self, expr: &hir::Expr) -> bool {
        let hir::ExprKind::Local(name, ..) = &expr.kind else {
            return false;
        };
        self.names
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|(local, _)| local == name)
            .is_some_and(|(_, immutable)| *immutable)
    }

    fn declare(&mut self, name: &str, immutable: bool) {
        if self.names.is_empty() {
            self.names.push(Vec::new());
        }
        if let Some(scope) = self.names.last_mut() {
            scope.push((name.to_string(), immutable));
        }
    }

    /// The parameters of the target of a call, for its defaults.
    fn target_params(&self, expr: &hir::Expr) -> Option<&'a [hir::Param]> {
        let module: &'a hir::Module = self.hir;
        use hir::ExprKind as K;
        let symbol = match &expr.kind {
            K::Call {
                callee: hir::Callee::Func(symbol) | hir::Callee::Method { name: symbol, .. },
                ..
            }
            | K::AsyncCall {
                callee:
                    hir::AsyncCallee::Function(symbol) | hir::AsyncCallee::Method { name: symbol, .. },
                ..
            }
            | K::AsyncHandleCreate {
                callee:
                    hir::AsyncCallee::Function(symbol) | hir::AsyncCallee::Method { name: symbol, .. },
                ..
            } => symbol,
            K::New { class, .. } => {
                return module
                    .classes
                    .get(class.0)
                    .and_then(|class| class.ctor.as_ref())
                    .map(|ctor| ctor.params.as_slice());
            }
            _ => return None,
        };
        all_declared_functions(module)
            .find(|function| function.symbol == *symbol)
            .map(|function| function.params.as_slice())
    }

    /// Whether `operand` is held at a site of compiler.md §255 rule 1 whose
    /// condition `runs` holds: a value of a counted type that is not a
    /// read of an immutable local.
    fn holds(&self, operand: &hir::Expr, runs: bool) -> bool {
        runs && owned(&operand.ty) && !self.immutable_local(operand)
    }

    /// Walks an expression that takes operand holds (compiler.md §255
    /// rules 1 and 3): a call, an array literal, an indexed read, and an
    /// assignment to an indexed place. A held operand owns its counts from
    /// its evaluation to the end of `expr`. Returns false for another
    /// expression.
    fn hold_sequence(&mut self, expr: &hir::Expr) -> bool {
        use hir::ExprKind as K;
        let owners = self.owners;
        match &expr.kind {
            K::Assign { target, value, .. } => {
                let K::Index { obj, index, .. } = &target.kind else {
                    return false;
                };
                let place = !fixed_array_place(obj);
                self.expression(obj);
                // Site 5: the index runs user code.
                if place && self.holds(obj, runs_user_code(index)) {
                    self.owners += 1;
                }
                self.expression(index);
                // Site 6: the value runs user code.
                if place && self.holds(obj, runs_user_code(value)) {
                    self.owners += 1;
                }
                self.trap_sites(target);
                self.expression(value);
            }
            K::Index { obj, index, .. } => {
                self.expression(obj);
                // Site 5, a read.
                if !fixed_array_place(obj) && self.holds(obj, runs_user_code(index)) {
                    self.owners += 1;
                }
                self.expression(index);
            }
            K::ArrayLit(_) | K::ArraySpreadLit(_) => {
                let elements: Vec<&hir::Expr> = match &expr.kind {
                    K::ArrayLit(elements) => elements.iter().collect(),
                    K::ArraySpreadLit(elements) => elements.iter().map(|e| &e.expr).collect(),
                    _ => Vec::new(),
                };
                // Site 4.
                for (position, element) in elements.iter().enumerate() {
                    self.expression(element);
                    if self.holds(
                        element,
                        elements[position + 1..].iter().any(|e| runs_user_code(e)),
                    ) {
                        self.owners += 1;
                    }
                }
            }
            K::Call { .. } | K::New { .. } | K::AsyncCall { .. } | K::AsyncHandleCreate { .. } => {
                self.call(expr);
            }
            _ => return false,
        }
        self.trap_sites(expr);
        self.owners = owners;
        true
    }

    /// Walks a call: the receiver (site 1), the arguments (site 2), and the
    /// default of each omitted parameter (site 3, rule 4), in order.
    fn call(&mut self, expr: &hir::Expr) {
        use hir::ExprKind as K;
        let (receiver, method, args): (Option<&hir::Expr>, Option<&hir::Symbol>, &[hir::Expr]) =
            match &expr.kind {
                K::Call {
                    callee: hir::Callee::Method { recv, name },
                    args,
                } => (Some(&**recv), Some(name), args),
                K::Call { callee, args } => {
                    if let hir::Callee::Value(value) = callee {
                        self.expression(value);
                    }
                    (None, None, args)
                }
                K::AsyncCall {
                    callee: hir::AsyncCallee::Method { receiver, .. },
                    args,
                }
                | K::AsyncHandleCreate {
                    callee: hir::AsyncCallee::Method { receiver, .. },
                    args,
                    ..
                } => (Some(&**receiver), None, args),
                K::AsyncCall { args, .. }
                | K::AsyncHandleCreate { args, .. }
                | K::New { args, .. } => (None, None, args),
                _ => return,
            };
        let params = self.target_params(expr).unwrap_or(&[]);
        let omitted: Vec<(usize, &hir::Param)> =
            params.iter().enumerate().skip(args.len()).collect();
        let defaults: Vec<&hir::Expr> = omitted
            .iter()
            .filter_map(|(_, param)| param.default.as_ref())
            .collect();
        let defaults_run = defaults.iter().any(|default| runs_user_code(default));
        if let Some(receiver) = receiver {
            self.expression(receiver);
            // Site 1: an argument or a parameter default runs user code, or
            // the method is `next` of a generator.
            let next = matches!(receiver.ty, Type::Generator(_))
                && method.is_some_and(|name| hir::source_name(name.full_text()) == "next");
            let runs = next
                || args.iter().any(runs_user_code)
                || params
                    .iter()
                    .filter_map(|param| param.default.as_ref())
                    .any(runs_user_code);
            if self.holds(receiver, runs) {
                self.owners += 1;
            }
        }
        // Site 2.
        for (position, argument) in args.iter().enumerate() {
            self.expression(argument);
            let later = args[position + 1..].iter().any(runs_user_code) || defaults_run;
            if self.holds(argument, later) {
                self.owners += 1;
            }
        }
        // Site 3 and rule 4: each default that the call evaluates. A
        // default that can raise is one edge at the call, with the owners
        // of the caller; its own edges belong to its body.
        for (position, (_, param)) in omitted
            .iter()
            .filter(|(_, param)| param.default.is_some())
            .enumerate()
        {
            let Some(default) = &param.default else {
                continue;
            };
            if param.default_can_raise {
                self.site(&default.pos);
            }
            let later = defaults[position + 1..].iter().any(|d| runs_user_code(d));
            if later && owned(&default.ty) {
                self.owners += 1;
            }
        }
    }

    fn trap_sites(&mut self, expr: &hir::Expr) {
        for trap in expr.trap_sites(self.hir) {
            if let hir::TrapSite::Raise { pos } = trap {
                self.site(&pos);
            }
        }
    }

    fn site(&mut self, pos: &Pos) {
        self.sites
            .push((pos.clone(), self.owners.saturating_sub(self.caught)));
    }

    fn expression(&mut self, expr: &hir::Expr) {
        if let hir::ExprKind::Lambda {
            params,
            body,
            captures,
            owns_environment,
            ..
        } = &expr.kind
        {
            let hooks = std::mem::take(&mut self.hooks);
            let controls = std::mem::take(&mut self.controls);
            let state = (self.owners, self.caught);
            self.owners = params.iter().filter(|p| owned(&p.ty)).count()
                + if *owns_environment {
                    captures.iter().filter(|capture| owned(&capture.ty)).count()
                } else {
                    0
                };
            self.caught = 0;
            // A parameter is not an immutable local. A capture is one when
            // the lambda owns its environment (compiler.md §255 rule 1).
            let names = std::mem::replace(
                &mut self.names,
                vec![params
                    .iter()
                    .map(|p| (p.name.clone(), false))
                    .chain(captures.iter().map(|c| (c.name.clone(), *owns_environment)))
                    .collect()],
            );
            self.sequence(body);
            self.names = names;
            (self.owners, self.caught) = state;
            self.hooks = hooks;
            self.controls = controls;
            return;
        }
        if self.hold_sequence(expr) {
            return;
        }
        for child in expr.children() {
            match child {
                hir::HirChild::Expr(expr) => self.expression(expr),
                hir::HirChild::Stmt(stmt) => self.statement(stmt),
            }
        }
        self.trap_sites(expr);
    }

    fn sequence(&mut self, body: &[hir::Stmt]) {
        let owners = self.owners;
        self.names.push(Vec::new());
        for statement in body {
            self.statement(statement);
            if stops_statement_sequence(self.hir, statement) {
                break;
            }
        }
        self.names.pop();
        self.owners = owners;
    }

    fn exit_hooks(&mut self, depth: usize, returning: bool) {
        let state = (self.owners, self.caught);
        let hooks = self.hooks.clone();
        for (index, hook) in hooks.iter().enumerate().skip(depth).rev() {
            self.owners = hook.owners + usize::from(returning);
            self.caught = hook.caught;
            for binding in hook.bindings.iter().rev() {
                self.statement(&binding.hook());
            }
            if let Some(finalizer) = &hook.finalizer {
                let stops = !sequence_exits(self.hir, finalizer).next;
                self.finalizer(&hooks[..index], finalizer);
                if stops {
                    break;
                }
            }
        }
        (self.owners, self.caught) = state;
    }

    /// Walks one placement of a `finally` block, inside the scopes of
    /// `outer`. The lowering places it at each exit of its `try` block.
    fn finalizer(&mut self, outer: &[Hooks], finalizer: &[hir::Stmt]) {
        let hooks = std::mem::replace(&mut self.hooks, outer.to_vec());
        let controls = std::mem::take(&mut self.controls);
        self.sequence(finalizer);
        self.hooks = hooks;
        self.controls = controls;
    }

    fn statement(&mut self, stmt: &hir::Stmt) {
        use hir::Stmt as S;
        match stmt {
            S::Let {
                name,
                ty,
                init,
                mutable,
                ..
            } => {
                self.expression(init);
                self.owners += usize::from(owned(ty));
                self.declare(name, !*mutable);
            }
            S::Expr(expr) => self.expression(expr),
            S::Return { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
                self.exit_hooks(0, value.as_ref().is_some_and(|value| owned(&value.ty)));
            }
            S::Throw { value, pos } => {
                self.expression(value);
                self.site(pos);
            }
            S::If {
                cond,
                then,
                els,
                pos: _,
            } => {
                self.expression(cond);
                self.sequence(then);
                if let Some(els) = els {
                    self.sequence(els);
                }
            }
            S::While { cond, body, pos: _ } => {
                self.expression(cond);
                self.controls.push((self.hooks.len(), true));
                self.sequence(body);
                self.controls.pop();
            }
            S::DoWhile { body, cond, pos: _ } => {
                self.controls.push((self.hooks.len(), true));
                self.sequence(body);
                self.controls.pop();
                self.expression(cond);
            }
            S::For {
                init,
                cond,
                step,
                body,
                pos: _,
            } => {
                let owners = self.owners;
                if let Some(init) = init {
                    self.statement(init);
                }
                if let Some(cond) = cond {
                    self.expression(cond);
                }
                self.controls.push((self.hooks.len(), true));
                self.sequence(body);
                self.controls.pop();
                self.sequence(step);
                self.owners = owners;
            }
            S::ForOf {
                ty,
                subject,
                body,
                kind: hir::ForOfKind::FixedArrayValues,
                name,
                pos: _,
            } => {
                self.expression(subject);
                let owners = self.owners;
                // compiler.md §255 rule 3: the base of a subject of a loop
                // over a `FixedArray` place owns its count of site 5 to the
                // end of the loop.
                let held_base = match &subject.kind {
                    hir::ExprKind::Index { obj, index, .. } => {
                        !fixed_array_place(obj) && self.holds(obj, runs_user_code(index))
                    }
                    _ => false,
                };
                self.owners +=
                    usize::from(owned(ty)) + traversal_owners(subject) + usize::from(held_base);
                self.controls.push((self.hooks.len(), true));
                // A `for…of` binding is not an immutable local (compiler.md
                // §255 rule 1).
                self.names.push(vec![(name.clone(), false)]);
                self.sequence(body);
                self.names.pop();
                self.controls.pop();
                self.owners = owners;
            }
            S::ForOf {
                ty,
                subject,
                body,
                name,
                kind: _,
                pos: _,
            }
            | S::GeneratorForOf {
                ty,
                subject,
                body,
                name,
                mutable: _,
                pos: _,
            } => {
                // A `const` binding of a `for…of` over a generator is an
                // immutable local; a binding of a `for…of` over an array is
                // not (compiler.md §255 rule 1).
                let immutable = matches!(stmt, S::GeneratorForOf { mutable: false, .. });
                self.expression(subject);
                let owners = self.owners;
                self.owners += usize::from(owned(ty));
                if matches!(
                    stmt,
                    S::GeneratorForOf {
                        name: _,
                        ty: _,
                        mutable: _,
                        subject: _,
                        body: _,
                        pos: _
                    }
                ) {
                    self.owners += usize::from(owned(&Type::iter_result(ty.clone())));
                }
                self.owners += usize::from(owned(&subject.ty));
                self.controls.push((self.hooks.len(), true));
                self.names.push(vec![(name.clone(), immutable)]);
                self.sequence(body);
                self.names.pop();
                self.controls.pop();
                self.owners = owners;
            }
            S::Switch {
                disc,
                cases,
                pos: _,
            } => {
                self.expression(disc);
                self.controls.push((self.hooks.len(), false));
                for case in cases {
                    if let Some(test) = &case.test {
                        self.expression(test);
                    }
                    self.sequence(&case.body);
                }
                self.controls.pop();
            }
            S::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                let caught = self.caught;
                self.caught = self.owners;
                self.sequence(body);
                self.caught = caught;
                if try_body_raises(self.hir, body) {
                    self.sequence(handler);
                }
            }
            S::Block(body) => self.sequence(body),
            S::Using {
                bindings,
                body,
                finalizer,
                pos,
            } => {
                self.hooks.push(Hooks {
                    bindings: bindings.clone(),
                    finalizer: finalizer.clone(),
                    owners: self.owners,
                    caught: self.caught,
                });
                self.sequence(body);
                if super::using::end_places_hooks(self.hir, body) {
                    self.exit_hooks(self.hooks.len() - 1, false);
                }
                let hook = self.hooks.pop();
                // The exception path of the `try` block runs the `finally`
                // block once more, then raises the exception again.
                if let Some((hook, finalizer)) = hook
                    .as_ref()
                    .and_then(|hook| Some((hook, hook.finalizer.as_ref()?)))
                    .filter(|_| try_body_raises(self.hir, body))
                {
                    let state = (self.owners, self.caught);
                    (self.owners, self.caught) = (hook.owners, hook.caught);
                    let outer = self.hooks.clone();
                    self.finalizer(&outer, finalizer);
                    // A `finally` block that completes restores the
                    // exception at the position of the statement.
                    if sequence_exits(self.hir, finalizer).next {
                        self.site(pos);
                    }
                    (self.owners, self.caught) = state;
                }
            }
            S::Break(_) | S::Continue(_) => {
                if let Some(&(depth, _)) = self
                    .controls
                    .iter()
                    .rev()
                    .find(|(_, is_loop)| matches!(stmt, S::Break(_)) || *is_loop)
                {
                    self.exit_hooks(depth, false);
                }
            }
        }
    }
}

#[test]
fn a_return_hook_counts_the_pending_owner_at_each_placement() {
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "return-hook.ts",
        r#"async function value(): Promise<i32> { return 7; }
class R { [Symbol.dispose](): void { throw new Error("hook"); } }
function returning(early: boolean): Promise<i32> {
  const h: Promise<i32> = value();
  {
    using r = new R();
    if (early) { return h; }
  }
  return h;
}
export async function main(): Promise<void> { const h = returning(true); await h; }
"#,
    )])
    .expect("checks");
    let mut lir = subscript_codegen::lir::lower_module(&hir).expect("lowers");
    let mut findings = Vec::new();
    compare(&hir, &lir, &mut findings);
    assert!(findings.is_empty(), "{findings:?}");
    let landing = lir
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .find(|block| {
            matches!(
                block.instructions.first().map(|i| &i.kind),
                Some(l::InstructionKind::ExceptionPark)
            ) && block
                .instructions
                .iter()
                .filter(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease))
                .count()
                == 2
        })
        .expect("the return hook releases the local and returned owners");
    let release = landing
        .instructions
        .iter()
        .position(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease))
        .expect("release");
    landing.instructions.remove(release);
    compare(&hir, &lir, &mut findings);
    assert!(
        findings.iter().any(
            |f| f.contains("exception edge releases 1 owned handles; lexical scopes require 2")
        ),
        "{findings:?}"
    );
}

/// A `finally` block runs at each exit of its `try` block: the `return`,
/// the end, and the exception path. A raise site in it releases the owners
/// of the enclosing scopes at each placement. Removing the release of one
/// placement fires.
// Cost: one checker call, one lowering, and two fact comparisons; no
// native build.
#[test]
fn a_finally_raise_counts_the_owners_at_each_placement() {
    let source = r#"async function value(n: i32): Promise<i32> { await Context.suspend(); return n; }
function boom(b: boolean): void { if (b) { throw new Error("boom"); } }
async function fin(b: boolean): Promise<i32> {
  const h: Promise<i32> = value(1);
  try { boom(b); if (b) { return 2; } } finally { boom(!b); }
  return await h;
}
export async function main(): Promise<void> { print(`${await fin(false)}`); }
"#;
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "finally-placement.ts",
        source,
    )])
    .expect("checks");
    let mut lir = subscript_codegen::lir::lower_module(&hir).expect("lowers");
    let mut findings = Vec::new();
    compare(&hir, &lir, &mut findings);
    assert!(findings.is_empty(), "{findings:?}");
    let site = Pos::new("finally-placement.ts", 5, 51);
    let function = lir
        .functions
        .iter_mut()
        .find(|function| function.source_name == "fin")
        .expect("fin");
    let landings: Vec<l::BlockId> = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|instruction| instruction.pos == site)
        .filter_map(l::Instruction::raise_edge)
        .filter_map(|edge| match edge {
            l::RaiseEdge::Handler(block) => Some(*block),
            l::RaiseEdge::Propagate => None,
        })
        .collect();
    // The return, the end, and the exception path each place the `finally`.
    assert_eq!(landings.len(), 3, "{landings:?}");
    let landing = &mut function.blocks[landings[0].0 as usize];
    let release = landing
        .instructions
        .iter()
        .position(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease))
        .expect("the landing releases the handle local");
    landing.instructions.remove(release);
    compare(&hir, &lir, &mut findings);
    assert!(
        findings.iter().any(
            |f| f.contains("exception edge releases 0 owned handles; lexical scopes require 1")
        ),
        "{findings:?}"
    );
}

/// A `finally` block that awaits continues the exception path after its
/// resume. The exception edge of the awaited receiver reaches the release
/// only through the two suspensions; removing that release fires.
// Cost: one checker call, one lowering, and two fact comparisons; no
// native build.
#[test]
fn an_awaiting_finalizer_keeps_the_exception_path_release() {
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "awaiting-finalizer.ts",
        r#"async function turn(): Promise<void> {}
async function fin(h: Promise<i32>, f: () => void): Promise<i32> {
  try { return await h; } finally { f(); await turn(); await turn(); }
}
async function value(n: i32): Promise<i32> { await Context.suspend(); return n; }
export async function main(): Promise<void> { print(`${await fin(value(4), (): void => {})}`); }
"#,
    )])
    .expect("checks");
    let mut lir = subscript_codegen::lir::lower_module(&hir).expect("lowers");
    let mut findings = Vec::new();
    compare(&hir, &lir, &mut findings);
    assert!(findings.is_empty(), "{findings:?}");
    let function = lir
        .functions
        .iter_mut()
        .find(|function| function.source_name == "fin")
        .expect("fin");
    let restored = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| instruction.kind == l::InstructionKind::ExceptionRestore)
        .and_then(l::Instruction::raise_edge)
        .and_then(|edge| match edge {
            l::RaiseEdge::Handler(block) => Some(*block),
            l::RaiseEdge::Propagate => None,
        })
        .expect("the finalizer restores the exception into a release landing");
    let landing = &mut function.blocks[restored.0 as usize];
    let release = landing
        .instructions
        .iter()
        .position(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease))
        .expect("release");
    landing.instructions.remove(release);
    compare(&hir, &lir, &mut findings);
    assert!(
        findings.iter().any(
            |f| f.contains("exception edge releases 0 owned handles; lexical scopes require 1")
        ),
        "{findings:?}"
    );
}

/// compiler.md §255 rules 1 to 4: in `two(v(), v())`, the first argument
/// is held while the second runs user code, to the end of the call. The
/// exception edge of the `await` releases the hold. The control passes;
/// removing that release fires.
// Cost: one checker call, one lowering, and two fact comparisons; no
// native build.
#[test]
fn an_operand_hold_is_released_on_the_exception_edge() {
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "operand-hold.ts",
        r#"async function v(): Promise<i32> { return 1; }
async function two(a: Promise<i32>, b: Promise<i32>): Promise<i32> { return (await a) + (await b); }
export async function main(): Promise<void> { print(`${await two(v(), v())}`); }
"#,
    )])
    .expect("checks");
    let mut lir = subscript_codegen::lir::lower_module(&hir).expect("lowers");
    let mut findings = Vec::new();
    compare(&hir, &lir, &mut findings);
    assert!(findings.is_empty(), "{findings:?}");
    let function = lir
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .expect("main");
    let landing = function
        .blocks
        .iter_mut()
        .find(|block| {
            matches!(
                block.instructions.first().map(|i| &i.kind),
                Some(l::InstructionKind::ExceptionPark)
            )
        })
        .expect("the await has a release landing");
    let release = landing
        .instructions
        .iter()
        .position(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease))
        .expect("the landing releases the hold");
    landing.instructions.remove(release);
    compare(&hir, &lir, &mut findings);
    assert!(
        findings.iter().any(
            |f| f.contains("exception edge releases 0 owned handles; lexical scopes require 1")
        ),
        "{findings:?}"
    );
}

/// compiler.md §255 rule 4: a parameter default is a body of its own, so
/// its edges count once, and each call that evaluates a default that can
/// raise adds one edge at the call. Two calls, three calls, and two
/// functions whose defaults call each other pass. Removing the edge of
/// one call fires.
// Cost: four checker calls, four lowerings, and five fact comparisons; no
// native build.
#[test]
fn a_default_is_a_body_of_its_own() {
    let parse = "function parse(s: string): i32 { if (s == \"x\") { throw new Error(\"p\"); } return 1; }\n\
                 function f(a: i32, b: i32 = parse(\"x\")): i32 { return a + b; }\n";
    let calls = |count: usize| {
        let body: String = (0..count)
            .map(|n| format!("try {{ f({n}); }} catch (e) {{ print(`{n}`); }} "))
            .collect();
        format!("{parse}export function main(): void {{ {body}}}\n")
    };
    let mutual = "function m1(n: i32, acc: i32 = n > 0 ? m2(n - 1) : 0): i32 { return n + acc; }\n\
                  function m2(n: i32, acc: i32 = n > 0 ? m1(n - 1) : 0): i32 { return n + acc; }\n\
                  export function main(): void { print(`${m1(3)}`); }\n";
    for source in [calls(2), calls(3), mutual.to_string()] {
        let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "defaults.ts",
            source.as_str(),
        )])
        .expect("checks");
        let lir = subscript_codegen::lir::lower_module(&hir).expect("lowers");
        let mut findings = Vec::new();
        compare(&hir, &lir, &mut findings);
        assert!(findings.is_empty(), "{source}: {findings:?}");
    }
    let source = calls(2);
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "defaults.ts",
        source.as_str(),
    )])
    .expect("checks");
    let mut lir = subscript_codegen::lir::lower_module(&hir).expect("lowers");
    let default = Pos::new("defaults.ts", 2, 29);
    let main = lir
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .expect("main");
    let (block, index) = main
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, b)| {
            b.instructions
                .iter()
                .position(|i| i.pos == default && i.raise_edge().is_some())
                .map(|index| (block, index))
        })
        .expect("a call evaluates the default");
    main.blocks[block].instructions.remove(index);
    let mut findings = Vec::new();
    compare(&hir, &lir, &mut findings);
    assert!(
        findings
            .iter()
            .any(|f| f.contains("2 exception edges; lexical scopes require 3")),
        "{findings:?}"
    );
}
