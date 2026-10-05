//! Shared field and local assignment flow (§108 and §158).

use std::collections::{HashMap, HashSet};

use super::{hir, Checker};
use crate::diag::Pos;

#[derive(Clone, Copy)]
enum Binding<'a> {
    Field(&'a str),
    Local(&'a str, &'a Pos),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    held: bool,
    active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Exit {
    Next(State),
    Break(State),
    Continue(State),
    Return(State),
}

struct Flow<'a, 'b> {
    checker: &'a Checker<'b>,
    binding: Binding<'a>,
    reads: Vec<(Pos, bool)>,
}

impl Flow<'_, '_> {
    fn target(&self, expression: &hir::Expr, state: State) -> bool {
        match (self.binding, &expression.kind) {
            (Binding::Field(field), hir::ExprKind::Field { obj, name }) => {
                name == field && matches!(obj.kind, hir::ExprKind::This)
            }
            (Binding::Local(binding, _), hir::ExprKind::Local(name, _)) => {
                state.active && name == binding
            }
            _ => false,
        }
    }

    fn expression(&mut self, expression: &hir::Expr, state: &mut State) {
        if self.target(expression, *state) {
            let pos = match &expression.kind {
                hir::ExprKind::Field { obj, .. } => &obj.pos,
                _ => &expression.pos,
            };
            self.reads.push((pos.clone(), state.held));
        }
        match &expression.kind {
            hir::ExprKind::Assign {
                target,
                value,
                op,
                update,
            } => {
                let writes_binding = self.target(target, *state);
                // Evaluate the address before the RHS. A plain target is not a read.
                if !writes_binding || op.is_some() || update.is_some() {
                    self.expression(target, state);
                }
                self.expression(value, state);
                if writes_binding && op.is_none() && update.is_none() {
                    state.held = true;
                }
            }
            hir::ExprKind::Binary {
                op: hir::BinOp::And | hir::BinOp::Or,
                left,
                right,
            } => {
                self.expression(left, state);
                let mut conditional = *state;
                self.expression(right, &mut conditional);
                state.held &= conditional.held;
            }
            hir::ExprKind::Cond { cond, then, els } => {
                self.expression(cond, state);
                let (mut yes, mut no) = (*state, *state);
                self.expression(then, &mut yes);
                self.expression(els, &mut no);
                state.held = yes.held && no.held;
            }
            // A lambda owns separate locals. C5 rejects mutable captures before this pass.
            hir::ExprKind::Lambda { .. } => {}
            _ => {
                for child in expression.children() {
                    if let hir::HirChild::Expr(child) = child {
                        self.expression(child, state);
                    }
                }
            }
        }
    }

    fn body(&mut self, body: &[hir::Stmt], entry: State) -> Vec<Exit> {
        self.statements(body.iter(), entry)
    }

    fn statements<'s>(
        &mut self,
        body: impl IntoIterator<Item = &'s hir::Stmt>,
        entry: State,
    ) -> Vec<Exit> {
        let mut paths = vec![Exit::Next(entry)];
        for statement in body {
            let mut next = Vec::new();
            for path in paths {
                if let Exit::Next(state) = path {
                    next.extend(self.statement(statement, state));
                } else {
                    next.push(path);
                }
            }
            next.sort_unstable();
            next.dedup();
            paths = next;
        }
        paths
    }

    fn restore_scope(paths: Vec<Exit>, entry: State) -> Vec<Exit> {
        paths
            .into_iter()
            .map(|exit| {
                let restore = |mut state: State| {
                    state.active = entry.active;
                    state
                };
                match exit {
                    Exit::Next(state) => Exit::Next(restore(state)),
                    Exit::Break(state) => Exit::Break(restore(state)),
                    Exit::Continue(state) => Exit::Continue(restore(state)),
                    Exit::Return(state) => Exit::Return(restore(state)),
                }
            })
            .collect()
    }

    fn scope(&mut self, body: &[hir::Stmt], entry: State) -> Vec<Exit> {
        Self::restore_scope(self.body(body, entry), entry)
    }

    fn statement(&mut self, statement: &hir::Stmt, mut state: State) -> Vec<Exit> {
        match statement {
            hir::Stmt::Let {
                name, init, pos, ..
            } => {
                self.expression(init, &mut state);
                if let Binding::Local(binding, declaration) = self.binding {
                    if name == binding {
                        state.active = pos == declaration;
                        if state.active {
                            state.held = !matches!(init.kind, hir::ExprKind::Unassigned);
                        }
                    }
                }
            }
            hir::Stmt::Expr(value) => {
                self.expression(value, &mut state);
                if matches!(
                    value.kind,
                    hir::ExprKind::Call {
                        callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                        ..
                    }
                ) {
                    return Vec::new();
                }
            }
            hir::Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expression(value, &mut state);
                }
                return vec![Exit::Return(state)];
            }
            hir::Stmt::Throw { value, .. } => {
                self.expression(value, &mut state);
                return Vec::new();
            }
            hir::Stmt::Break(_) => return vec![Exit::Break(state)],
            hir::Stmt::Continue(_) => return vec![Exit::Continue(state)],
            hir::Stmt::Block(body) | hir::Stmt::Using { body, .. } => {
                return self.scope(body, state)
            }
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                self.expression(cond, &mut state);
                let mut exits = Vec::new();
                if !matches!(cond.kind, hir::ExprKind::Bool(false)) {
                    exits.extend(self.scope(then, state));
                }
                if !matches!(cond.kind, hir::ExprKind::Bool(true)) {
                    exits.extend(self.scope(els.as_deref().unwrap_or(&[]), state));
                }
                return exits;
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                self.expression(disc, &mut state);
                let dispatch = state;
                let mut entries = vec![state; cases.len()];
                for (index, case) in cases.iter().enumerate() {
                    if let Some(test) = &case.test {
                        let mut test_state = dispatch;
                        self.expression(test, &mut test_state);
                        // TypeScript does not carry case-test assignments along dispatch.
                        entries[index] = dispatch;
                    }
                }
                // Default dispatch follows every failed test, regardless of its source position.
                for (index, case) in cases.iter().enumerate() {
                    if case.test.is_none() {
                        entries[index] = dispatch;
                    }
                }
                let mut exits = Vec::new();
                if !cases.iter().any(|case| case.test.is_none())
                    && !self.checker.source_enum_switch_exhaustive(disc, cases)
                    && !self.checker.string_alias_switch_exhaustive(disc, cases)
                {
                    exits.push(Exit::Next(dispatch));
                }
                let mut live = Vec::new();
                for (case, entry) in cases.iter().zip(entries) {
                    live.push(entry);
                    live.sort_unstable();
                    live.dedup();
                    let mut fallthrough = Vec::new();
                    for entry in live {
                        for exit in self.body(&case.body, entry) {
                            match exit {
                                Exit::Next(state) => fallthrough.push(state),
                                Exit::Break(state) => exits.push(Exit::Next(state)),
                                other => exits.push(other),
                            }
                        }
                    }
                    live = fallthrough;
                }
                exits.extend(live.into_iter().map(Exit::Next));
                return Self::restore_scope(exits, state);
            }
            hir::Stmt::Try {
                body,
                handler,
                binding,
                ..
            } => {
                // An exception can reach the catch before any assignment in the try.
                let mut exits = self.scope(body, state);
                let mut catch_entry = state;
                if matches!((self.binding, binding), (Binding::Local(name, _), Some((caught, _))) if name == caught)
                {
                    catch_entry.active = false;
                }
                exits.extend(Self::restore_scope(self.scope(handler, catch_entry), state));
                return exits;
            }
            hir::Stmt::While { cond, body, .. } => {
                self.expression(cond, &mut state);
                if matches!(cond.kind, hir::ExprKind::Bool(false)) {
                    return vec![Exit::Next(state)];
                }
                return self.loop_body(
                    body,
                    state,
                    matches!(cond.kind, hir::ExprKind::Bool(true)),
                    None,
                );
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let outer = state;
                if let Some(init) = init {
                    let paths = self.statement(init, state);
                    let Some(Exit::Next(entry)) = paths.first() else {
                        return paths;
                    };
                    state = *entry;
                }
                if let Some(cond) = cond {
                    self.expression(cond, &mut state);
                }
                return Self::restore_scope(
                    self.loop_body(
                        body,
                        state,
                        cond.as_ref()
                            .is_none_or(|cond| matches!(cond.kind, hir::ExprKind::Bool(true))),
                        step.as_ref(),
                    ),
                    outer,
                );
            }
            hir::Stmt::ForOf {
                subject,
                body,
                name,
                ..
            } => {
                self.expression(subject, &mut state);
                let entry = state;
                if matches!(self.binding, Binding::Local(binding, _) if binding == name) {
                    state.active = false;
                }
                return Self::restore_scope(self.loop_body(body, state, false, None), entry);
            }
        }
        vec![Exit::Next(state)]
    }

    fn loop_body(
        &mut self,
        body: &[hir::Stmt],
        state: State,
        infinite: bool,
        step: Option<&hir::Expr>,
    ) -> Vec<Exit> {
        let mut exits = if infinite {
            Vec::new()
        } else {
            vec![Exit::Next(state)]
        };
        for exit in self.scope(body, state) {
            match exit {
                Exit::Break(state) => exits.push(Exit::Next(state)),
                Exit::Return(state) => exits.push(Exit::Return(state)),
                Exit::Next(mut state) | Exit::Continue(mut state) => {
                    if let Some(step) = step {
                        self.expression(step, &mut state);
                    }
                }
            }
        }
        exits
    }
}

/// Index root statements that can change a local's state or produce a read.
/// Keep exits and loops for every local, because they can remove a path.
#[derive(Default)]
struct LocalStatements<'a> {
    names: HashMap<&'a str, Vec<usize>>,
    exits: Vec<usize>,
}

#[derive(Default)]
struct Exits {
    stops: bool,
    breaks: bool,
    continues: bool,
}

impl<'a> LocalStatements<'a> {
    fn new(body: &'a [hir::Stmt]) -> Self {
        fn expression<'a>(value: &'a hir::Expr, names: &mut HashSet<&'a str>) {
            match &value.kind {
                hir::ExprKind::Local(name, _) => {
                    names.insert(name);
                }
                hir::ExprKind::Lambda { .. } => return,
                _ => {}
            }
            for child in value.children() {
                if let hir::HirChild::Expr(child) = child {
                    expression(child, names);
                }
            }
        }
        fn statement<'a>(value: &'a hir::Stmt, names: &mut HashSet<&'a str>) -> Exits {
            let mut exits = Exits::default();
            match value {
                hir::Stmt::Let { name, .. } | hir::Stmt::ForOf { name, .. } => {
                    names.insert(name);
                }
                hir::Stmt::Try {
                    binding: Some((name, _)),
                    ..
                } => {
                    names.insert(name);
                }
                hir::Stmt::Return { .. }
                | hir::Stmt::Throw { .. }
                | hir::Stmt::While { .. }
                | hir::Stmt::For { .. } => exits.stops = true,
                hir::Stmt::Break(_) => exits.breaks = true,
                hir::Stmt::Continue(_) => exits.continues = true,
                hir::Stmt::Expr(value)
                    if matches!(
                        value.kind,
                        hir::ExprKind::Call {
                            callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                            ..
                        }
                    ) =>
                {
                    exits.stops = true
                }
                _ => {}
            }
            for child in value.children() {
                match child {
                    hir::HirChild::Expr(child) => expression(child, names),
                    hir::HirChild::Stmt(child) => {
                        let child = statement(child, names);
                        exits.stops |= child.stops;
                        exits.breaks |= child.breaks;
                        exits.continues |= child.continues;
                    }
                }
            }
            match value {
                hir::Stmt::Switch { cases, .. } => {
                    exits.breaks = false;
                    // An empty exhaustive switch can remove every path.
                    exits.stops |= cases.is_empty();
                }
                hir::Stmt::While { .. } | hir::Stmt::For { .. } | hir::Stmt::ForOf { .. } => {
                    exits.breaks = false;
                    exits.continues = false;
                }
                _ => {}
            }
            exits
        }
        let mut index = Self::default();
        for (i, value) in body.iter().enumerate() {
            let mut names = HashSet::new();
            let exits = statement(value, &mut names);
            if exits.stops || exits.breaks || exits.continues {
                index.exits.push(i);
            } else {
                for name in names {
                    index.names.entry(name).or_default().push(i);
                }
            }
        }
        index
    }

    fn for_local(&self, name: &str) -> Vec<usize> {
        let mut own = self
            .names
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .peekable();
        let mut exits = self.exits.iter().copied().peekable();
        let mut selected = Vec::new();
        loop {
            match (own.peek(), exits.peek()) {
                (Some(a), Some(b)) if a < b => selected.extend(own.next()),
                (_, Some(_)) => selected.extend(exits.next()),
                (Some(_), None) => selected.extend(own.next()),
                (None, None) => break,
            }
        }
        selected
    }
}

fn initial(binding: Binding<'_>) -> State {
    State {
        held: false,
        active: matches!(binding, Binding::Field(_)),
    }
}

pub(super) fn field_on_normal_exit(checker: &Checker<'_>, body: &[hir::Stmt], field: &str) -> bool {
    let binding = Binding::Field(field);
    let mut flow = Flow {
        checker,
        binding,
        reads: Vec::new(),
    };
    flow.body(body, initial(binding))
        .into_iter()
        .all(|exit| match exit {
            Exit::Next(state)
            | Exit::Return(state)
            | Exit::Break(state)
            | Exit::Continue(state) => state.held,
        })
}

pub(super) fn field_held_before_read(
    checker: &Checker<'_>,
    body: &[hir::Stmt],
    field: &str,
    pos: &Pos,
) -> bool {
    let binding = Binding::Field(field);
    let mut flow = Flow {
        checker,
        binding,
        reads: Vec::new(),
    };
    flow.body(body, initial(binding));
    let reads: Vec<_> = flow.reads.iter().filter(|(read, _)| read == pos).collect();
    !reads.is_empty() && reads.iter().all(|(_, held)| *held)
}

/// Check each owning function before lowering consumes the zero-storage marker.
pub(super) fn local_diagnostics(checker: &Checker<'_>) -> Vec<(String, Pos)> {
    fn declarations<'a>(body: &'a [hir::Stmt], out: &mut Vec<(&'a str, &'a Pos)>) {
        for statement in body {
            if let hir::Stmt::Let {
                name, init, pos, ..
            } = statement
            {
                if matches!(init.kind, hir::ExprKind::Unassigned) {
                    out.push((name, pos));
                }
            }
            for child in statement.children() {
                if let hir::HirChild::Stmt(child) = child {
                    declarations(std::slice::from_ref(child), out);
                }
            }
        }
    }
    fn lambdas(checker: &Checker<'_>, expression: &hir::Expr, out: &mut Vec<(String, Pos)>) {
        if let hir::ExprKind::Lambda { body, params, .. } = &expression.kind {
            function(checker, body, out);
            defaults(checker, params, out);
        } else {
            for child in expression.children() {
                if let hir::HirChild::Expr(child) = child {
                    lambdas(checker, child, out);
                }
            }
        }
    }
    fn nested(checker: &Checker<'_>, body: &[hir::Stmt], out: &mut Vec<(String, Pos)>) {
        for statement in body {
            for child in statement.children() {
                match child {
                    hir::HirChild::Expr(child) => lambdas(checker, child, out),
                    hir::HirChild::Stmt(child) => nested(checker, std::slice::from_ref(child), out),
                }
            }
        }
    }
    fn function(checker: &Checker<'_>, body: &[hir::Stmt], out: &mut Vec<(String, Pos)>) {
        let mut locals = Vec::new();
        declarations(body, &mut locals);
        let index = (!locals.is_empty()).then(|| LocalStatements::new(body));
        for (name, pos) in locals {
            let binding = Binding::Local(name, pos);
            let mut flow = Flow {
                checker,
                binding,
                reads: Vec::new(),
            };
            if let Some(index) = &index {
                let selected = index.for_local(name);
                flow.statements(selected.iter().map(|&i| &body[i]), initial(binding));
            }
            for (pos, held) in flow.reads {
                if !held
                    && !out
                        .iter()
                        .any(|(existing, read)| existing == name && read == &pos)
                {
                    out.push((name.to_owned(), pos));
                }
            }
        }
        nested(checker, body, out);
    }
    fn defaults(checker: &Checker<'_>, params: &[hir::Param], out: &mut Vec<(String, Pos)>) {
        for param in params {
            if let Some(value) = &param.default {
                lambdas(checker, value, out);
            }
        }
    }
    let mut out = Vec::new();
    for global in &checker.globals {
        lambdas(checker, &global.init, &mut out);
    }
    for f in &checker.functions {
        function(checker, &f.body, &mut out);
        defaults(checker, &f.params, &mut out);
    }
    for class in &checker.classes {
        for method in &class.methods {
            function(checker, &method.body, &mut out);
            defaults(checker, &method.params, &mut out);
        }
        if let Some(ctor) = &class.ctor {
            function(checker, &ctor.body, &mut out);
            defaults(checker, &ctor.params, &mut out);
        }
        for field in &class.fields {
            if let Some(init) = &field.init {
                lambdas(checker, init, &mut out);
            }
        }
    }
    function(checker, &checker.top_level, &mut out);
    out
}

impl Checker<'_> {
    /// Emit assignment diagnostics while the owning bodies remain available.
    pub(super) fn check_local_assignments(&mut self) {
        for (name, pos) in local_diagnostics(self) {
            self.reject_subset(
                super::RejectionSite::LocalReadUnassigned,
                format!("local `{name}` is read before assignment; assign `{name}` on every path before this read, or give it an initializer"),
                pos,
            );
        }
    }
}
