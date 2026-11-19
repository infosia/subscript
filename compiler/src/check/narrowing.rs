//! Shared-location narrowing effects (compiler.md §124).

use super::expr::path_key;
use super::{Checker, FactSet, FnCtx, NarrowingFact};
use crate::check::rejection::RejectionSite;
use crate::hir::{BinOp, ExprKind};
use crate::types::Type;
use crate::{diag::Pos, hir};

impl Checker<'_> {
    pub(super) fn narrowing_paths(
        &self,
        cond: &hir::Expr,
        fx: &mut FnCtx,
    ) -> (Vec<NarrowingFact>, Vec<NarrowingFact>) {
        if let hir::ExprKind::Unary {
            op: hir::UnOp::Not,
            operand,
        } = &cond.kind
        {
            let (yes, no) = self.narrowing_paths(operand, fx);
            return (no, yes);
        }
        if let hir::ExprKind::Binary {
            op: op @ (hir::BinOp::And | hir::BinOp::Or),
            left,
            right,
        } = &cond.kind
        {
            // `a && b` is true, and `a || b` is false, when both operands
            // are; a kill in `b` ends a fact of `a` (compiler.md §124).
            let and = matches!(op, hir::BinOp::And);
            let split = |(yes, no): (Vec<NarrowingFact>, Vec<NarrowingFact>)| {
                if and {
                    (yes, no)
                } else {
                    (no, yes)
                }
            };
            let (mut first, left_decides) = split(self.narrowing_paths(left, fx));
            self.end_facts_at(&mut first, right, fx);
            let (second, right_decides) = split(self.narrowing_paths(right, fx));
            first.extend(second);
            // `a || b` is true, and `a && b` is false, when either operand
            // decides. `b` does not run when `a` decides, so a kill in `b`
            // does not end the fact of `a` (compiler.md §227.1 rules 1 to 4).
            let either = facts_on_both_arms(left_decides, &right_decides);
            if and {
                (first, either)
            } else {
                (either, first)
            }
        } else {
            let (mut yes, mut no) =
                leaf_paths(cond, |ty| self.apparent_type(ty), self.narrowing_classes());
            if let hir::ExprKind::Binary { left, right, .. } = &cond.kind {
                // The read of the left operand comes before the right operand
                // runs, so a kill there ends the fact (compiler.md §219.1 rule 4).
                let read = path_key(assignment_target(left));
                if yes
                    .iter()
                    .chain(&no)
                    .all(|fact| Some(&fact.key) == read.as_ref())
                {
                    self.end_facts_at(&mut yes, right, fx);
                    self.end_facts_at(&mut no, right, fx);
                }
            }
            (yes, no)
        }
    }

    /// Removes each fact of `facts` that a kill in `later` ends. The later
    /// expression runs after the facts are true (compiler.md §124).
    fn end_facts_at(&self, facts: &mut Vec<NarrowingFact>, later: &hir::Expr, fx: &mut FnCtx) {
        if facts.is_empty() {
            return;
        }
        let effects = later.narrowing_effects(
            self.narrowing_classes(),
            self.narrowing_helpers(),
            hir::StoreTypes::Checked(&|ty| self.apparent_type(ty)),
        );
        facts.retain(|key| {
            let (shared, local) = path_kills(key, &effects);
            if shared && !local {
                fx.ended_shared_narrowing.insert_fact(key.clone());
            }
            !shared && !local
        });
    }

    pub(super) fn end_shared_narrowing(&self, expression: &hir::Expr, fx: &mut FnCtx) {
        fx.replace_finally_causes_at_facts();
        self.apply_operation_effects(expression, fx);
        if fx
            .loop_ended_narrowing
            .iter()
            .any(|notes| !notes.is_empty())
        {
            let (yes, no) = leaf_paths(
                expression,
                |ty| self.apparent_type(ty),
                self.narrowing_classes(),
            );
            for fact in yes.iter().chain(&no) {
                fx.end_loop_note_at_test(&fact.key);
            }
        }
        if let hir::ExprKind::Assign {
            op: None,
            target,
            value,
            ..
        } = &expression.kind
        {
            if matches!(
                self.apparent_type(&target.ty),
                crate::types::Type::Nullable(_, _)
            ) && !matches!(
                self.apparent_type(&value.ty),
                crate::types::Type::Null
                    | crate::types::Type::Nullable(_, _)
                    | crate::types::Type::Error
            ) {
                if let Some(key) =
                    path_key(target).filter(|_| !matches!(target.kind, ExprKind::Index { .. }))
                {
                    fx.narrowed.replace_fact(NarrowingFact {
                        kind: crate::check::narrowing_fact::FactKind::Narrowing,
                        key: key.clone(),
                        shared: target.is_shared_location(self.narrowing_classes()),
                    });
                    fx.ended_shared_narrowing.remove(&key);
                }
            }
        }
    }

    /// Applies the effects that the operation of `expression` owns, and adds no fact.
    pub(super) fn apply_operation_effects(&self, expression: &hir::Expr, fx: &mut FnCtx) {
        if fx.has_narrowing_facts() {
            self.apply_narrowing_effects(
                &expression.operation_narrowing_effects(
                    self.narrowing_classes(),
                    self.narrowing_helpers(),
                    hir::StoreTypes::Checked(&|ty| self.apparent_type(ty)),
                ),
                fx,
            );
        }
    }

    /// An element check selects C24 row 32 without changing the value type.
    pub(super) fn indexed_nullable_error(
        &mut self,
        expression: &hir::Expr,
        fx: &FnCtx,
        use_site: RejectionSite,
        pos: Pos,
    ) -> bool {
        if !matches!(expression.kind, ExprKind::Index { .. }) {
            return false;
        }
        let checked = path_key(expression).is_some_and(|key| {
            fx.narrowed
                .get(&key)
                .is_some_and(|fact| !fact.narrows_type())
        });
        let site = match (use_site, checked) {
            (RejectionSite::NullableMember, true) => RejectionSite::IndexedMemberChecked,
            (RejectionSite::NullableMember, false) => RejectionSite::IndexedMemberUnchecked,
            (RejectionSite::NullableCall, true) => RejectionSite::IndexedCallChecked,
            (RejectionSite::NullableCall, false) => RejectionSite::IndexedCallUnchecked,
            (_, true) => RejectionSite::IndexedAssignmentChecked,
            (_, false) => RejectionSite::IndexedAssignmentUnchecked,
        };
        self.reject_subset(site, format!(
            "`{}` may be null here; copy the element to a `const` local and test the local\nnote: const v = xs[i]; if (v !== null) {{ v.x }}",
            self.type_name(&expression.ty),
        ), pos);
        true
    }

    pub(super) fn nullable_use_error(
        &mut self,
        expression: &hir::Expr,
        fx: &FnCtx,
        sites: (RejectionSite, RejectionSite),
        message: String,
        pos: Pos,
    ) {
        let nullable_function = matches!(self.apparent_type(&expression.ty), crate::types::Type::Nullable(inner, _) if matches!(self.apparent_type(&inner), crate::types::Type::Func(_)));
        if (sites.0 != RejectionSite::NullableCall || nullable_function)
            && self.indexed_nullable_error(expression, fx, sites.0, pos.clone())
        {
            return;
        }
        if (sites.0 != RejectionSite::NullableCall || nullable_function)
            && path_key(expression).is_some_and(|key| fx.finally_ended_path(&key))
        {
            let site = match sites.0 {
                RejectionSite::NullableMember => RejectionSite::NullableMemberFinallyJump,
                RejectionSite::NullableCall => RejectionSite::NullableCallFinallyJump,
                _ => sites.0,
            };
            self.reject_subset(site, message, pos);
        } else if (sites.0 != RejectionSite::NullableCall || nullable_function)
            && path_key(expression).is_some_and(|key| fx.ended_shared_narrowing.contains(&key))
        {
            self.reject_subset(sites.1, message, pos);
        } else if (sites.0 != RejectionSite::NullableCall || nullable_function)
            && fx.loop_ended_fact(expression)
        {
            // §308 refines the ordinary nullable sites; C17 owns shared effects.
            let site = match sites.0 {
                RejectionSite::NullableMember => RejectionSite::NullableMemberLoopHead,
                RejectionSite::NullableCall => RejectionSite::NullableCallLoopHead,
                _ => sites.0,
            };
            self.reject_subset(site, message, pos);
        } else {
            self.reject_subset(sites.0, message, pos);
        }
    }
}

/// HIR effects available before flow checks reach a loop head (compiler.md §124).
#[derive(Default)]
pub(super) struct Analysis {
    classes: Vec<hir::ClassDef>,
    sources: hir::LoopValueSources,
    const_scopes: Vec<std::collections::HashMap<String, bool>>,
    needs_const_pass: bool,
    helpers: std::collections::HashSet<hir::Symbol>,
    loops: std::collections::HashMap<(String, u32, u32), hir::NarrowingEffects>,
    /// The test of each `do…while`, by loop position (compiler.md §203.1 rule 4).
    do_while_tests: std::collections::HashMap<(String, u32, u32), hir::Expr>,
}

fn position_key(pos: &Pos) -> (String, u32, u32) {
    (pos.file.clone(), pos.line, pos.col)
}

impl Analysis {
    pub(super) fn from_module(module: &mut hir::Module) -> Self {
        let classes = module.classes.clone();
        let mut analysis = Self {
            helpers: module.synthesized_helpers.clone(),
            sources: hir::LoopValueSources {
                globals: module
                    .globals
                    .iter()
                    .map(|global| (global.symbol.clone(), global.ty.clone()))
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        for owner in module.expression_owners_mut() {
            match owner {
                hir::ExpressionOwnerMut::Expr(expression) => {
                    analysis.owner(|analysis| analysis.expression(expression, &classes))
                }
                hir::ExpressionOwnerMut::Body(body) => {
                    analysis.owner(|analysis| analysis.body(body, &classes));
                }
            }
        }
        analysis.classes = classes;
        analysis
    }

    /// Adds the loop effects of `other` (compiler.md §135.1 rule 1).
    pub(super) fn merge_loops(&mut self, other: Self) {
        for (key, effects) in other.loops {
            self.loops.entry(key).or_default().merge(effects);
        }
        for (key, test) in other.do_while_tests {
            self.do_while_tests.entry(key).or_insert(test);
        }
    }

    /// Derive const proofs only when the first source summary needs them (§304 rule 4).
    /// Both visits derive summaries; neither visit checks a body again.
    fn owner(&mut self, visit: impl Fn(&mut Self)) {
        let prior_loops = std::mem::take(&mut self.loops);
        let prior_need = std::mem::replace(&mut self.needs_const_pass, false);
        visit(self);
        if self.needs_const_pass {
            self.loops.clear();
            self.const_scopes.push(Default::default());
            visit(self);
            self.const_scopes.pop();
        }
        let loops = std::mem::replace(&mut self.loops, prior_loops);
        for (key, effects) in loops {
            self.loops.entry(key).or_default().merge(effects);
        }
        self.needs_const_pass = prior_need;
    }

    fn function(&mut self, function: &hir::Function, classes: &[hir::ClassDef]) {
        self.owner(|analysis| {
            let track = !analysis.const_scopes.is_empty();
            if track {
                analysis.const_scopes.push(
                    function
                        .params
                        .iter()
                        .map(|param| (param.name.clone(), false))
                        .collect(),
                );
            }
            for default in function
                .params
                .iter()
                .filter_map(|parameter| parameter.default.as_ref())
            {
                analysis.expression(default, classes);
            }
            analysis.body(&function.body, classes);
            if track {
                analysis.const_scopes.pop();
            }
        });
    }

    fn expression(&mut self, expression: &hir::Expr, classes: &[hir::ClassDef]) {
        if let ExprKind::Local(name, ..) = &expression.kind {
            if self
                .const_scopes
                .iter()
                .rev()
                .find_map(|scope| scope.get(name))
                .copied()
                == Some(true)
            {
                self.sources
                    .const_reads
                    .insert(position_key(&expression.pos));
            }
        }
        if let ExprKind::Lambda { params, body, .. } = &expression.kind {
            let track = !self.const_scopes.is_empty();
            if track {
                self.const_scopes.push(
                    params
                        .iter()
                        .map(|param| (param.name.clone(), false))
                        .collect(),
                );
            }
            for default in params.iter().filter_map(|param| param.default.as_ref()) {
                self.expression(default, classes);
            }
            self.body(body, classes);
            if track {
                self.const_scopes.pop();
            }
        } else {
            for child in expression.children() {
                self.child(child, classes);
            }
        }
    }

    fn body(&mut self, body: &[hir::Stmt], classes: &[hir::ClassDef]) {
        // Reserve all names before initializers, so a shadow never reads an outer proof (§304 rule 1a).
        let track = !self.const_scopes.is_empty();
        if track {
            self.const_scopes.push(
                body.iter()
                    .filter_map(|statement| match statement {
                        hir::Stmt::Let { name, .. } => Some((name.clone(), false)),
                        _ => None,
                    })
                    .collect(),
            );
        }
        for statement in body {
            self.statement(statement, classes);
        }
        if track {
            self.const_scopes.pop();
        }
    }

    fn statement(&mut self, statement: &hir::Stmt, classes: &[hir::ClassDef]) {
        let for_scope = matches!(statement, hir::Stmt::For { .. }) && !self.const_scopes.is_empty();
        if for_scope {
            self.const_scopes.push(Default::default());
        }
        if let hir::Stmt::For {
            init: Some(init), ..
        } = statement
        {
            self.statement(init, classes);
        }
        for child in statement.children() {
            if let hir::HirChild::Expr(expression) = child {
                self.expression(expression, classes);
            }
        }
        match statement {
            hir::Stmt::Let {
                name,
                mutable,
                init,
                ..
            } => {
                if self.const_scopes.is_empty() {
                    return;
                }
                let non_null = !mutable && init.non_null_without_facts(classes, &self.sources);
                if let Some(scope) = self.const_scopes.last_mut() {
                    scope.insert(name.clone(), non_null);
                }
            }
            hir::Stmt::If { then, els, .. } => {
                self.body(then, classes);
                if let Some(els) = els {
                    self.body(els, classes);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::DoWhile { body, .. }
            | hir::Stmt::Block(body) => self.body(body, classes),
            hir::Stmt::For { body, step, .. } => {
                self.body(body, classes);
                self.body(step, classes);
            }
            hir::Stmt::ForOf { name, body, .. } | hir::Stmt::GeneratorForOf { name, body, .. } => {
                self.const_scopes.push([(name.clone(), false)].into());
                self.body(body, classes);
                self.const_scopes.pop();
            }
            hir::Stmt::Switch { cases, .. } => {
                self.const_scopes.push(
                    cases
                        .iter()
                        .flat_map(|case| &case.body)
                        .filter_map(|statement| match statement {
                            hir::Stmt::Let { name, .. } => Some((name.clone(), false)),
                            _ => None,
                        })
                        .collect(),
                );
                for case in cases {
                    self.body(&case.body, classes);
                }
                self.const_scopes.pop();
            }
            hir::Stmt::Try {
                body,
                binding,
                handler,
                ..
            } => {
                self.body(body, classes);
                self.const_scopes.push(
                    binding
                        .iter()
                        .map(|(name, _)| (name.clone(), false))
                        .collect(),
                );
                self.body(handler, classes);
                self.const_scopes.pop();
            }
            hir::Stmt::Using {
                bindings,
                body,
                finalizer,
                ..
            } => {
                self.const_scopes.push(
                    bindings
                        .iter()
                        .map(|binding| (binding.name.clone(), false))
                        .collect(),
                );
                self.body(body, classes);
                if let Some(finalizer) = finalizer {
                    self.body(finalizer, classes);
                }
                self.const_scopes.pop();
            }
            hir::Stmt::Expr(_)
            | hir::Stmt::Return { .. }
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_)
            | hir::Stmt::Throw { .. } => {}
        }
        if let hir::Stmt::While {
            pos,
            cond: _,
            body: _,
        }
        | hir::Stmt::DoWhile {
            pos,
            body: _,
            cond: _,
        }
        | hir::Stmt::For {
            pos,
            init: _,
            cond: _,
            step: _,
            body: _,
        }
        | hir::Stmt::ForOf {
            pos,
            name: _,
            ty: _,
            subject: _,
            kind: _,
            body: _,
        }
        | hir::Stmt::GeneratorForOf {
            pos,
            name: _,
            ty: _,
            mutable: _,
            subject: _,
            body: _,
        } = statement
        {
            let effects = statement.narrowing_effects(
                classes,
                &self.helpers,
                hir::StoreTypes::Source(&self.sources),
            );
            self.needs_const_pass |= effects.const_sources;
            self.loops
                .entry(position_key(pos))
                .or_default()
                .merge(effects);
        }
        if let hir::Stmt::DoWhile { cond, pos, body: _ } = statement {
            self.do_while_tests.insert(position_key(pos), cond.clone());
        }
        if for_scope {
            self.const_scopes.pop();
        }
    }

    fn child(&mut self, child: hir::HirChild<'_>, classes: &[hir::ClassDef]) {
        match child {
            hir::HirChild::Expr(expression) => self.expression(expression, classes),
            hir::HirChild::Stmt(statement) => self.statement(statement, classes),
        }
    }
}

impl Checker<'_> {
    /// The loop effects of the bodies that the opaque check added after
    /// `functions` functions, `classes` classes, and `methods[i]` methods
    /// of each earlier class (compiler.md §135.1 rule 1).
    pub(super) fn opaque_body_loop_effects(
        &self,
        functions: usize,
        classes: usize,
        methods: &[usize],
    ) -> Analysis {
        let mut analysis = Analysis {
            helpers: self.narrowing_helpers().clone(),
            sources: hir::LoopValueSources {
                globals: self
                    .globals
                    .iter()
                    .map(|global| (global.symbol.clone(), global.ty.clone()))
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        for function in self.functions.iter().skip(functions) {
            analysis.function(function, &self.classes);
        }
        for (index, class) in self.classes.iter().enumerate() {
            if index < classes {
                let first = methods.get(index).copied().unwrap_or(0);
                for method in class.methods.iter().skip(first) {
                    analysis.function(method, &self.classes);
                }
                continue;
            }
            for init in class.fields.iter().filter_map(|field| field.init.as_ref()) {
                analysis.owner(|analysis| analysis.expression(init, &self.classes));
            }
            for function in class.ctor.iter().chain(class.methods.iter()) {
                analysis.function(function, &self.classes);
            }
        }
        analysis
    }

    fn narrowing_helpers(&self) -> &std::collections::HashSet<hir::Symbol> {
        self.narrowing_analysis
            .as_ref()
            .map_or(&self.narrowing_helper_symbols, |analysis| &analysis.helpers)
    }

    fn narrowing_classes(&self) -> &[hir::ClassDef] {
        self.narrowing_analysis
            .as_ref()
            .map_or(&self.classes, |analysis| &analysis.classes)
    }

    pub(super) fn apply_narrowing_effects(&self, effects: &hir::NarrowingEffects, fx: &mut FnCtx) {
        self.apply_flow_effects(effects, fx, false);
    }

    fn apply_flow_effects(&self, effects: &hir::NarrowingEffects, fx: &mut FnCtx, loop_head: bool) {
        let kills = |key: &NarrowingFact| {
            if loop_head {
                loop_path_kills(key, effects)
            } else {
                path_kills(key, effects)
            }
        };
        if !loop_head {
            fx.end_loop_notes_at_stores(effects);
            fx.end_finally_causes_at_stores(effects);
        }
        if !fx.has_narrowing_facts() {
            return;
        }
        fx.ended_shared_narrowing.retain(|key| !kills(key).1);
        for index in fx.shadowed_narrowing_scopes.iter() {
            let scope = &mut fx.scopes[*index];
            scope.shadowed_narrowing.retain(|key| {
                // A shadowed path belongs to another binding, even with the same spelling.
                if kills(key).0 {
                    scope.shadowed_ended_shared.insert_fact(key.clone());
                    false
                } else {
                    true
                }
            });
        }
        let mut loop_ended = Vec::new();
        fx.narrowed.retain(|key| {
            let (shared_kill, local_kill) = kills(key);
            if loop_head && key.narrows_type() && (local_kill || shared_kill) {
                loop_ended.push(key.clone());
            }
            if shared_kill {
                fx.ended_shared_narrowing.insert_fact(key.clone());
            }
            if local_kill {
                fx.ended_shared_narrowing.remove(key);
            }
            !shared_kill && !local_kill
        });
        for fact in loop_ended {
            fx.record_loop_ended_fact(&fact);
        }
    }

    pub(super) fn end_loop_narrowing(&self, pos: &Pos, fx: &mut FnCtx) {
        fx.loop_ended_narrowing.push(Default::default());
        if !fx.has_narrowing_facts() {
            return;
        }
        if let Some(effects) = self
            .narrowing_analysis
            .as_ref()
            .and_then(|analysis| analysis.loops.get(&position_key(pos)))
        {
            if effects.dependent_stores.is_empty() {
                self.apply_flow_effects(effects, fx, true);
                return;
            }
            let mut resolved = effects.clone();
            let mut remaining: Vec<_> = resolved.dependent_stores.drain(..).collect();
            loop {
                self.apply_flow_effects(&resolved, fx, true);
                let mut changed = false;
                remaining.retain(|(path, field, sources)| {
                    if sources.iter().all(|source| {
                        fx.narrowed
                            .get(source)
                            .is_some_and(NarrowingFact::narrows_type)
                    }) {
                        return true;
                    }
                    resolved.nullable_stores.insert(path.clone());
                    if let Some(field) = field {
                        resolved.fields.insert(field.clone());
                    }
                    changed = true;
                    false
                });
                if !changed {
                    break;
                }
            }
        }
    }

    /// Adds each fact of `before` that the `do…while` test at `pos` gives
    /// on `true` (compiler.md §203.1 rule 4). The test of the resolved HIR
    /// supplies the facts, because the body is checked before the test.
    /// Such a fact holds before the loop and on the edge from the test
    /// back to the body.
    pub(super) fn enter_do_while_body(
        &self,
        pos: &Pos,
        before: &super::NarrowingFacts,
        fx: &mut FnCtx,
    ) {
        let Some(test) = self
            .narrowing_analysis
            .as_ref()
            .and_then(|analysis| analysis.do_while_tests.get(&position_key(pos)))
        else {
            return;
        };
        let notes = fx.ended_shared_narrowing.clone();
        let (on_true, _) = self.narrowing_paths(test, fx);
        fx.ended_shared_narrowing = notes;
        for fact in on_true {
            if before
                .get(fact.key.as_str())
                .is_some_and(|prior| prior.kind == fact.kind)
            {
                fx.ended_shared_narrowing.remove(&fact.key);
                fx.narrowed.replace_fact(fact);
            }
        }
    }

    pub(super) fn end_scope_narrowing(&self, fx: &mut FnCtx) {
        if !fx.has_narrowing_facts() {
            return;
        }
        if fx.scopes.last().is_some_and(|scope| scope.dispose_on_exit) {
            self.apply_narrowing_effects(
                &hir::NarrowingEffects {
                    script: true,
                    ..Default::default()
                },
                fx,
            );
        }
    }
}

impl Checker<'_> {
    pub(super) fn body_narrowing_effects(
        &self,
        body: &[hir::Stmt],
        fx: &FnCtx,
        incoming: &super::NarrowingFacts,
        notes: &super::NarrowingFacts,
    ) -> Option<hir::NarrowingEffects> {
        (fx.has_narrowing_facts() || !incoming.is_empty() || !notes.is_empty()).then(|| {
            hir::NarrowingEffects::body(
                body,
                self.narrowing_classes(),
                self.narrowing_helpers(),
                hir::StoreTypes::Checked(&|ty| self.apparent_type(ty)),
            )
        })
    }
}

impl<'p> FnCtx<'p> {
    pub(super) fn has_narrowing_facts(&self) -> bool {
        !self.finally_ended_narrowing.is_empty()
            || !self.narrowed.is_empty()
            || self
                .loop_ended_narrowing
                .iter()
                .any(|notes| !notes.is_empty())
            || !self.ended_shared_narrowing.is_empty()
            || !self.shadowed_narrowing_scopes.is_empty()
    }

    pub(super) fn narrowing_note_paths(&self) -> std::collections::HashSet<NarrowingFact> {
        self.narrowed
            .union(&self.ended_shared_narrowing)
            .cloned()
            .collect()
    }

    pub(super) fn finish_narrowing_join(
        &mut self,
        eligible: &std::collections::HashSet<NarrowingFact>,
    ) {
        self.ended_shared_narrowing
            .retain(|key| eligible.contains(key) && !self.narrowed.contains(key));
    }
}

fn path_kills(key: &NarrowingFact, effects: &hir::NarrowingEffects) -> (bool, bool) {
    if let crate::check::narrowing_fact::FactKind::ElementCheck {
        receiver,
        key: element,
        ..
    } = &key.kind
    {
        let killed = effects
            .stores
            .iter()
            .any(|stored| receiver == stored || receiver.starts_with(&format!("{stored}.")))
            || effects
                .indexed_stores
                .contains(&(receiver.clone(), Some(element.clone())));
        return (false, killed);
    }
    let root = super::stmt::root_of(key);
    let global = root.starts_with("[[global]]");
    let shared_kill = shared_path_kills(key, effects);
    let local_kill = effects.nullable_store_ends(key)
        || !global
            && effects.locals.iter().any(|stored| {
                key.key == *stored && !stored.contains('.')
                    || key.starts_with(&format!("{stored}."))
            });
    (shared_kill, local_kill)
}

fn shared_path_kills(key: &NarrowingFact, effects: &hir::NarrowingEffects) -> bool {
    key.shared
        && (effects.script
            || key.split('.').skip(1).enumerate().any(|(index, field)| {
                effects.fields.contains(field)
                    || index + 2 < key.split('.').count() && effects.replaced_fields.contains(field)
            }))
        || effects.globals.iter().any(|name| {
            let stored = format!("[[global]]{}", name.full_text());
            key.key == *stored || key.starts_with(&format!("{stored}."))
        })
}

/// compiler.md §162 rule 3 keeps a whole-local fact through a non-null store.
fn loop_path_kills(key: &NarrowingFact, effects: &hir::NarrowingEffects) -> (bool, bool) {
    if !key.narrows_type() {
        return path_kills(key, effects);
    }
    let shared_kill = shared_path_kills(key, effects);
    let local_kill = effects.nullable_store_ends(key)
        || !super::stmt::root_of(key).starts_with("[[global]]")
            && effects
                .locals
                .iter()
                .any(|stored| key.starts_with(&format!("{stored}.")));
    (shared_kill, local_kill)
}

/// The location that a condition operand tests: the target of an assignment
/// expression, else the operand (compiler.md §159 rule 1a).
fn assignment_target(operand: &hir::Expr) -> &hir::Expr {
    match &operand.kind {
        ExprKind::Assign {
            op: None, target, ..
        } => target,
        _ => operand,
    }
}

/// True when a value of type `ty` is never absent. A type parameter with no
/// constraint can be a nullable type, and the poison type states nothing, so
/// neither counts (compiler.md §219.1 rule 4). The match names each type, so
/// a new type must state its absence. `check_expr` makes a `void` operand
/// the poison expression, so an operand never has the type `Void`.
fn has_no_absence(ty: &Type, apparent_type: &impl Fn(&Type) -> Type) -> bool {
    match apparent_type(ty) {
        Type::ConstrainedInstance(instance) => has_no_absence(&instance.instance, apparent_type),
        Type::Null | Type::Nullable(_, _) | Type::TypeParameter(_) | Type::Error | Type::Void => {
            false
        }
        Type::GenericUnion(members) => members
            .iter()
            .all(|member| has_no_absence(member, apparent_type)),
        Type::I8
        | Type::U8
        | Type::I16
        | Type::U16
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::F32
        | Type::F64
        | Type::F16
        | Type::Bool
        | Type::Str
        | Type::Date
        | Type::RegExp
        | Type::TaskGroup
        | Type::Object
        | Type::Class(_)
        | Type::Interface(_)
        | Type::GenericNumber
        | Type::Enum(_)
        | Type::StringAlias(_)
        | Type::FixedArray(_, _)
        | Type::Array(_)
        | Type::Map(_, _)
        | Type::Set(_)
        | Type::Worker(_, _)
        | Type::Inbox(_)
        | Type::Outbox(_)
        | Type::Func(_)
        | Type::Generator(_)
        | Type::AsyncHandle(_)
        | Type::IterResult(_) => true,
    }
}

/// The facts that both arms give: the same path with the same kind. The
/// result is shared if either fact is shared (compiler.md §227.1 rule 3).
fn facts_on_both_arms(left: Vec<NarrowingFact>, right: &[NarrowingFact]) -> Vec<NarrowingFact> {
    left.into_iter()
        .filter_map(|mut fact| {
            let other = right
                .iter()
                .find(|other| other.key == fact.key && other.kind == fact.kind)?;
            fact.shared |= other.shared;
            Some(fact)
        })
        .collect()
}

/// Narrowing facts derived from a checked leaf condition: paths known
/// non-null or known present when the condition is true / false.
/// `Checker::narrowing_paths` splits `&&` and `||` and applies the kills
/// of compiler.md §124 before it reaches a leaf.
pub(super) fn leaf_paths(
    cond: &hir::Expr,
    apparent_type: impl Fn(&Type) -> Type,
    classes: &[hir::ClassDef],
) -> (Vec<NarrowingFact>, Vec<NarrowingFact>) {
    fn const_bindings(value: &hir::Expr) -> Vec<(String, String)> {
        let mut bindings = Vec::new();
        let mut receiver = value;
        loop {
            match &receiver.kind {
                ExprKind::Index {
                    obj,
                    index,
                    element_key,
                    ..
                } => {
                    if let (ExprKind::Local(name, _, _), Some(identity)) =
                        (&index.kind, element_key)
                    {
                        if identity.starts_with("const:") {
                            bindings.push((name.clone(), identity.clone()));
                        }
                    }
                    receiver = obj;
                }
                ExprKind::Field { obj, .. } => receiver = obj,
                _ => break,
            }
        }
        bindings
    }
    let fact = |value: &hir::Expr| {
        let key = path_key(value)?;
        let kind = match &value.kind {
            ExprKind::Index {
                obj,
                element_key: Some(element),
                ..
            } => crate::check::narrowing_fact::FactKind::ElementCheck {
                receiver: path_key(obj)?,
                key: element.clone(),
                const_bindings: const_bindings(value),
            },
            _ => crate::check::narrowing_fact::FactKind::Narrowing,
        };
        Some(NarrowingFact {
            key,
            kind,
            shared: value.is_shared_location(classes),
        })
    };
    if let Some(value) = super::exception::instanceof_narrowed_value(cond) {
        return (fact(value).into_iter().collect(), Vec::new());
    }
    if let ExprKind::AbsenceTest { value, negated } = &cond.kind {
        if let Some(key) = fact(value) {
            return if *negated {
                (vec![key], Vec::new())
            } else {
                (Vec::new(), vec![key])
            };
        }
        return (Vec::new(), Vec::new());
    }
    if let ExprKind::Binary { op, left, right } = &cond.kind {
        // The fact of `operand` when `operand` can be absent.
        let nullable_fact = |operand: &hir::Expr| {
            let target = assignment_target(operand);
            let compared_type = if matches!(target.kind, ExprKind::Index { .. }) {
                &target.ty
            } else {
                &operand.ty
            };
            matches!(apparent_type(compared_type), Type::Nullable(_, _))
                .then(|| fact(target))
                .flatten()
        };
        let equal = matches!(op, BinOp::Eq | BinOp::StrEq);
        match op {
            BinOp::Eq | BinOp::Ne | BinOp::StrEq | BinOp::StrNe => {
                let null_test = if matches!(left.kind, ExprKind::Null) {
                    Some(right)
                } else if matches!(right.kind, ExprKind::Null) {
                    Some(left)
                } else {
                    None
                };
                if let Some(tested) = null_test {
                    return match nullable_fact(tested) {
                        // `p === null` → p is non-null when false.
                        Some(key) if equal => (Vec::new(), vec![key]),
                        // `p !== null` → p is non-null when true.
                        Some(key) => (vec![key], Vec::new()),
                        None => (Vec::new(), Vec::new()),
                    };
                }
                // §219.1 rule 4: equality with a value that has no absence
                // shows that the other operand is present.
                let present = |value: &hir::Expr| has_no_absence(&value.ty, &apparent_type);
                let tested = if present(right) {
                    nullable_fact(left)
                } else if present(left) {
                    nullable_fact(right)
                } else {
                    None
                };
                match tested {
                    Some(key) if equal => (vec![key], Vec::new()),
                    Some(key) => (Vec::new(), vec![key]),
                    None => (Vec::new(), Vec::new()),
                }
            }
            _ => (Vec::new(), Vec::new()),
        }
    } else {
        (Vec::new(), Vec::new())
    }
}

impl Checker<'_> {
    /// Records each synthesized helper once, when the checker creates it.
    pub(super) fn push_narrowing_helper(&mut self, helper: hir::Function) {
        self.narrowing_helper_symbols.insert(helper.symbol.clone());
        self.functions.push(helper);
    }
}

impl Checker<'_> {
    /// Compares retained entry facts with stores from the final checked body (§162 rule 3b).
    pub(super) fn check_loop_stores<'a>(
        &mut self,
        body: &[hir::Stmt],
        expressions: impl Iterator<Item = &'a hir::Expr>,
        kept: &super::NarrowingFacts,
    ) {
        if self.narrowing_analysis.is_none() || kept.is_empty() {
            return;
        }
        let mut effects = hir::NarrowingEffects::body(
            body,
            &self.classes,
            self.narrowing_helpers(),
            hir::StoreTypes::Checked(&|ty| self.apparent_type(ty)),
        );
        for expression in expressions {
            effects.merge(expression.narrowing_effects(
                &self.classes,
                self.narrowing_helpers(),
                hir::StoreTypes::Checked(&|ty| self.apparent_type(ty)),
            ));
        }
        for diagnostic in checked_loop_store_diagnostics(effects, kept) {
            self.reject_subset(
                RejectionSite::NullableMember,
                diagnostic.message,
                diagnostic.pos,
            );
        }
    }
}

/// The checked store type and the retained entry fact have separate derivations.
fn checked_loop_store_diagnostics(
    effects: hir::NarrowingEffects,
    kept: &super::NarrowingFacts,
) -> Vec<crate::diag::Diagnostic> {
    let mut errors = Vec::new();
    for (path, pos) in effects.nullable_store_sites {
        if let Some(fact) = kept.iter().find(|fact| {
            fact.narrows_type() && (fact.key == path || fact.starts_with(&format!("{path}.")))
        }) {
            errors.push(loop_store_diagnostic(&fact.key, pos));
        }
    }
    for (field, pos) in effects.nullable_field_store_sites {
        if errors.iter().any(|error| error.pos == pos) {
            continue;
        }
        if let Some(fact) = kept.iter().find(|fact| {
            fact.narrows_type()
                && fact.shared
                && fact.split('.').skip(1).any(|segment| segment == field)
        }) {
            errors.push(loop_store_diagnostic(&fact.key, pos));
        }
    }
    errors
}

fn loop_store_diagnostic(path: &str, pos: Pos) -> crate::diag::Diagnostic {
    let path = hir::source_name(path.strip_prefix("[[global]]").unwrap_or(path));
    super::rejection::diagnostic(
        RejectionSite::NullableMember,
        format!("`{path}` may be null at the loop head: this store can set it to null\nnote: the loop keeps the null check of `{path}` made before the loop"),
        pos,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cost: less than 0.01 seconds for four HIR bodies; no compiler or engine subprocess.
    #[test]
    fn checked_field_store_closes_retained_facts_through_any_receiver() {
        for receiver in ["h", "k"] {
            for nullable in [false, true] {
                let ty = Type::Nullable(
                    Box::new(Type::Class(crate::types::ClassId(0))),
                    crate::types::Absence::Null,
                );
                let pos = Pos::new("field.ts", 4, 3);
                let expression = |kind, ty| hir::Expr {
                    pending_work: None,
                    kind,
                    ty,
                    pos: pos.clone(),
                };
                let target = expression(
                    ExprKind::Field {
                        obj: Box::new(expression(
                            ExprKind::Local(
                                receiver.into(),
                                Type::Class(crate::types::ClassId(1)),
                                true,
                            ),
                            Type::Class(crate::types::ClassId(1)),
                        )),
                        name: "f".into(),
                    },
                    ty.clone(),
                );
                let value = expression(
                    ExprKind::Local("p".into(), ty.clone(), true),
                    if nullable {
                        ty
                    } else {
                        Type::Class(crate::types::ClassId(0))
                    },
                );
                let store = expression(
                    ExprKind::Assign {
                        update: None,
                        op: None,
                        target: Box::new(target),
                        value: Box::new(value),
                    },
                    Type::Void,
                );
                let body = [hir::Stmt::Expr(store)];
                let kept = [NarrowingFact {
                    key: "h.f".into(),
                    shared: true,
                    kind: super::super::narrowing_fact::FactKind::Narrowing,
                }]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into();
                // The nullable body violates the entry invariant. No derived summary record changes.
                let effects = hir::NarrowingEffects::body(
                    &body,
                    &[],
                    &Default::default(),
                    hir::StoreTypes::Checked(&Type::clone),
                );
                let errors = checked_loop_store_diagnostics(effects, &kept);
                assert_eq!(errors.len(), usize::from(nullable));
                if nullable {
                    assert_eq!(errors[0].code, crate::diag::RuleCode::S011);
                    assert_eq!(errors[0].pos, pos);
                    assert_eq!(errors[0].divergence, None);
                    assert!(errors[0]
                        .message
                        .contains("loop keeps the null check of `h.f`"));
                }
            }
        }
    }

    #[test]
    fn scope_exit_discards_only_facts_that_need_its_const_binding() {
        use crate::check::narrowing_fact::FactKind;
        for nested in [false, true] {
            let mut fx = FnCtx::new(
                Type::Void,
                false,
                None,
                crate::check::Evaluation::AtOnce,
                Default::default(),
            );
            let mut scope = super::super::Scope::default();
            scope.const_keys.insert("i".into(), "inner-i".into());
            scope.vars.insert(
                "i".into(),
                super::super::Local {
                    annotated: false,
                    ty: Type::I32,
                    mutable: false,
                    async_origins: Default::default(),
                    caught: false,
                    function_value_required: None,
                },
            );
            fx.scopes.push(scope);
            let mut keys = Vec::new();
            for identity in ["inner-i", "outer-i"] {
                let receiver = if nested {
                    format!("xs.[[element:{identity}]]")
                } else {
                    "xs".into()
                };
                let element = if nested { "int:0" } else { identity };
                let key = format!("{receiver}.[[element:{element}]]");
                keys.push(key.clone());
                fx.narrowed.insert_fact(NarrowingFact {
                    key,
                    shared: false,
                    kind: FactKind::ElementCheck {
                        receiver,
                        key: if nested {
                            "int:0".into()
                        } else {
                            identity.into()
                        },
                        const_bindings: vec![("i".into(), identity.into())],
                    },
                });
            }
            fx.pop_scope();
            assert!(!fx.narrowed.contains(&keys[0]));
            assert!(fx.narrowed.contains(&keys[1]));
        }
    }

    #[test]
    fn final_checked_body_must_preserve_the_retained_loop_fact() {
        for path in ["a", "[[global]][[identity:module:00]]a"] {
            let kept = super::super::NarrowingFacts::from(std::collections::BTreeSet::from([
                NarrowingFact {
                    kind: crate::check::narrowing_fact::FactKind::Narrowing,
                    key: path.into(),
                    shared: false,
                },
            ]));
            for (ty, rejected) in [
                (Type::Null, true),
                (Type::Class(crate::types::ClassId(0)), false),
            ] {
                let pos = Pos::new("test.ts", 3, 5);
                let value = hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Local("p".into(), ty.clone(), false),
                    ty,
                    pos: pos.clone(),
                };
                let target = hir::Expr {
                    pending_work: None,
                    kind: if let Some(symbol) = path.strip_prefix("[[global]]") {
                        ExprKind::Global(hir::Symbol::from_full_text(symbol), true)
                    } else {
                        ExprKind::Local(
                            path.into(),
                            Type::Nullable(
                                Box::new(Type::Class(crate::types::ClassId(0))),
                                crate::types::Absence::Null,
                            ),
                            true,
                        )
                    },
                    ty: Type::Nullable(
                        Box::new(Type::Class(crate::types::ClassId(0))),
                        crate::types::Absence::Null,
                    ),
                    pos: pos.clone(),
                };
                let store = hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Assign {
                        update: None,
                        op: None,
                        target: Box::new(target),
                        value: Box::new(value),
                    },
                    ty: Type::Class(crate::types::ClassId(0)),
                    pos: pos.clone(),
                };
                // Build the violating body. Do not alter a summary record after its derivation.
                let body = [hir::Stmt::Expr(store)];
                let effects = hir::NarrowingEffects::body(
                    &body,
                    &[],
                    &Default::default(),
                    hir::StoreTypes::Checked(&Type::clone),
                );
                let errors = checked_loop_store_diagnostics(effects, &kept);
                assert_eq!(!errors.is_empty(), rejected);
                if rejected {
                    assert_eq!(errors[0].code, crate::diag::RuleCode::S011);
                    assert_eq!(errors[0].divergence, None);
                    assert_eq!(errors[0].pos, pos);
                    assert_eq!(
                        errors[0].message,
                        "`a` may be null at the loop head: this store can set it to null\nnote: the loop keeps the null check of `a` made before the loop"
                    );
                }
            }
        }
    }

    fn fact(
        key: &str,
        kind: crate::check::narrowing_fact::FactKind,
        shared: bool,
    ) -> NarrowingFact {
        NarrowingFact {
            key: key.into(),
            shared,
            kind,
        }
    }

    #[test]
    fn both_arms_match_on_path_and_kind_and_join_shared() {
        use crate::check::narrowing_fact::FactKind;
        let element = FactKind::ElementCheck {
            receiver: "xs".into(),
            key: "int:0".into(),
            const_bindings: Vec::new(),
        };
        let left = vec![
            fact("a", FactKind::Narrowing, false),
            fact("b", FactKind::Narrowing, false),
            fact("c", FactKind::Narrowing, false),
        ];
        let right = vec![
            fact("a", FactKind::Narrowing, true),
            fact("c", element.clone(), false),
            fact("d", FactKind::Narrowing, false),
        ];
        let both = facts_on_both_arms(left, &right);
        // Rule 1: only `a` is on both arms. Rule 3: `c` has two kinds, so it
        // gives no fact, and the fact of `a` is shared because one arm is.
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].key, "a");
        assert_eq!(both[0].kind, FactKind::Narrowing);
        assert!(both[0].shared);
        let same = facts_on_both_arms(
            vec![fact("c", element.clone(), false)],
            &[fact("c", element.clone(), false)],
        );
        assert_eq!(same.len(), 1);
        assert_eq!(same[0].kind, element);
        assert!(!same[0].shared);
    }

    fn checks(body: &str) -> Result<(), Vec<crate::Diagnostic>> {
        let source = format!(
            "class H {{ f: string | null = null; }}\n\
             let g: string | null = null;\n\
             function call(): boolean {{ return true; }}\n\
             function body(a: string | null, b: string | null, h: H, xs: (string | null)[]): i32 {{\n\
             {body}\n\
             return 0;\n\
             }}\n\
             export function main(): void {{ print(`${{body(null, null, new H(), [null])}}`); }}\n"
        );
        crate::check_program(&[crate::SourceFile::entry("or.ts", &source)]).map(|_| ())
    }

    fn rejects_s011(body: &str) -> Vec<crate::Diagnostic> {
        let errors = checks(body).expect_err(body);
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(errors[0].code, crate::diag::RuleCode::S011, "{body}");
        errors
    }

    #[test]
    fn a_fact_on_both_arms_of_or_narrows() {
        for body in [
            // Rule 1.
            r#"if (a === "x" || a === "y") { return a.length; }"#,
            r#"if (a === "x" || a === "y" || a === "z") { return a.length; }"#,
            r#"if (a !== null || a === "y") { return a.length; }"#,
            r#"return a === "x" || a === "y" ? a.length : 0;"#,
            r#"if (h.f === "x" || h.f === "y") { return h.f.length; }"#,
            // Rule 2.
            r#"if (a !== "x" && a !== "y") { return 0; } return a.length;"#,
            r#"if (!(a === "x" || a === "y")) { return 0; } return a.length;"#,
            // Rule 4: a call in `b` does not end the fact of `a`, because
            // `b` does not run when `a` decides; `b` tests after the call.
            r#"if (g === "x" || (call() && g === "y")) { return g.length; }"#,
            // Rule 4: a local fact stays through a call in `b`.
            r#"if (a === "x" || (a === "y" && call())) { return a.length; }"#,
        ] {
            assert!(checks(body).is_ok(), "{body}: {:?}", checks(body));
        }
    }

    #[test]
    fn a_fact_on_one_arm_or_ended_in_b_gives_no_narrowing() {
        for body in [
            // Rule 6: a fact on one arm only.
            r#"if (a === "x" || b === "y") { return a.length; }"#,
            r#"if (a === "x" || a === null) { return a.length; }"#,
            r#"if (a === "x" && b === "y") { return 0; } return a.length;"#,
            // Rule 4: a call in `b` after its test ends the shared fact of `b`.
            r#"if (g === "x" || (g === "y" && call())) { return g.length; }"#,
            r#"if (h.f === "x" || (h.f === "y" && call())) { return h.f.length; }"#,
            // Rule 3: the joined fact is shared, so a later call ends it.
            r#"if (h.f === "x" || h.f === "y") { call(); return h.f.length; }"#,
        ] {
            rejects_s011(body);
        }
    }

    #[test]
    fn an_element_check_on_both_arms_selects_the_checked_diagnostic() {
        let checked = rejects_s011(
            r#"const i: i32 = 0; if (xs[i] === "x" || xs[i] === "y") { return xs[i].length; }"#,
        );
        assert_eq!(
            checked[0].divergence,
            Some(crate::divergence::Divergence::IndexedReadNullCheck)
        );
        let unchecked = rejects_s011(
            r#"const i: i32 = 0; if (xs[i] === "x" || b === "y") { return xs[i].length; }"#,
        );
        assert_eq!(unchecked[0].divergence, None);
    }
}
