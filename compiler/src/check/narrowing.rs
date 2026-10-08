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
            let pick = |facts: (Vec<NarrowingFact>, Vec<NarrowingFact>)| {
                if and {
                    facts.0
                } else {
                    facts.1
                }
            };
            let mut first = pick(self.narrowing_paths(left, fx));
            if !first.is_empty() {
                let effects = right.narrowing_effects(
                    self.narrowing_classes(),
                    self.narrowing_helpers(),
                    self.narrowing_globals(),
                );
                first.retain(|key| {
                    let (shared, local) = path_kills(key, &effects);
                    if shared && !local {
                        fx.ended_shared_narrowing.insert_fact(key.clone());
                    }
                    !shared && !local
                });
            }
            first.extend(pick(self.narrowing_paths(right, fx)));
            if and {
                (first, Vec::new())
            } else {
                (Vec::new(), first)
            }
        } else {
            leaf_paths(cond, |ty| self.apparent_type(ty), self.narrowing_classes())
        }
    }

    pub(super) fn end_shared_narrowing(&self, expression: &hir::Expr, fx: &mut FnCtx) {
        if fx.has_narrowing_facts() {
            self.apply_narrowing_effects(
                &expression.operation_narrowing_effects(
                    self.narrowing_classes(),
                    self.narrowing_helpers(),
                    None,
                ),
                fx,
            );
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
                crate::types::Type::Nullable(_)
            ) && !matches!(
                self.apparent_type(&value.ty),
                crate::types::Type::Null
                    | crate::types::Type::Nullable(_)
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
        let nullable_function = matches!(self.apparent_type(&expression.ty), crate::types::Type::Nullable(inner) if matches!(self.apparent_type(&inner), crate::types::Type::Func(_)));
        if (sites.0 != RejectionSite::NullableCall || nullable_function)
            && self.indexed_nullable_error(expression, fx, sites.0, pos.clone())
        {
            return;
        }
        if (sites.0 != RejectionSite::NullableCall || nullable_function)
            && path_key(expression).is_some_and(|key| fx.ended_shared_narrowing.contains(&key))
        {
            self.reject_subset(sites.1, message, pos);
        } else {
            self.reject_subset(sites.0, message, pos);
        }
    }
}

/// HIR effects available before flow checks reach a loop head (compiler.md §124).
#[derive(Default)]
pub(super) struct Analysis {
    classes: Vec<hir::ClassDef>,
    globals: std::collections::HashMap<hir::Symbol, Type>,
    helpers: std::collections::HashSet<hir::Symbol>,
    loops: std::collections::HashMap<(String, u32, u32), hir::NarrowingEffects>,
}

fn position_key(pos: &Pos) -> (String, u32, u32) {
    (pos.file.clone(), pos.line, pos.col)
}

impl Analysis {
    pub(super) fn from_module(module: &mut hir::Module) -> Self {
        let classes = module.classes.clone();
        let mut analysis = Self {
            helpers: module.synthesized_helpers.clone(),
            globals: module
                .globals
                .iter()
                .map(|global| (global.symbol.clone(), global.ty.clone()))
                .collect(),
            ..Default::default()
        };
        for owner in module.expression_owners_mut() {
            match owner {
                hir::ExpressionOwnerMut::Expr(expression) => {
                    analysis.expression(expression, &classes)
                }
                hir::ExpressionOwnerMut::Body(body) => {
                    for statement in body {
                        analysis.statement(statement, &classes);
                    }
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
    }

    fn function(&mut self, function: &hir::Function, classes: &[hir::ClassDef]) {
        for default in function
            .params
            .iter()
            .filter_map(|parameter| parameter.default.as_ref())
        {
            self.expression(default, classes);
        }
        for statement in &function.body {
            self.statement(statement, classes);
        }
    }

    fn expression(&mut self, expression: &hir::Expr, classes: &[hir::ClassDef]) {
        for child in expression.children() {
            self.child(child, classes);
        }
    }

    fn statement(&mut self, statement: &hir::Stmt, classes: &[hir::ClassDef]) {
        if let hir::Stmt::While {
            pos,
            cond: _,
            body: _,
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
            self.loops
                .entry(position_key(pos))
                .or_default()
                .merge(statement.narrowing_effects(classes, &self.helpers, Some(&self.globals)));
        }
        for child in statement.children() {
            self.child(child, classes);
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
            globals: self
                .globals
                .iter()
                .map(|global| (global.symbol.clone(), global.ty.clone()))
                .collect(),
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
                analysis.expression(init, &self.classes);
            }
            for function in class.ctor.iter().chain(class.methods.iter()) {
                analysis.function(function, &self.classes);
            }
        }
        analysis
    }

    fn narrowing_globals(&self) -> Option<&std::collections::HashMap<hir::Symbol, Type>> {
        self.narrowing_analysis
            .as_ref()
            .map(|analysis| &analysis.globals)
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
        fx.narrowed.retain(|key| {
            let (shared_kill, local_kill) = kills(key);
            if shared_kill {
                fx.ended_shared_narrowing.insert_fact(key.clone());
            }
            if local_kill {
                fx.ended_shared_narrowing.remove(key);
            }
            !shared_kill && !local_kill
        });
    }

    pub(super) fn end_loop_narrowing(&self, pos: &Pos, fx: &mut FnCtx) {
        if !fx.has_narrowing_facts() {
            return;
        }
        if let Some(effects) = self
            .narrowing_analysis
            .as_ref()
            .and_then(|analysis| analysis.loops.get(&position_key(pos)))
        {
            self.apply_flow_effects(effects, fx, true);
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
                self.narrowing_globals(),
            )
        })
    }
}

impl FnCtx {
    pub(super) fn has_narrowing_facts(&self) -> bool {
        !self.narrowed.is_empty()
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
    let shared = key.shared;
    let shared_kill = shared
        && (effects.script
            || key
                .split('.')
                .skip(1)
                .any(|field| effects.fields.contains(field)))
        || effects.globals.iter().any(|name| {
            let stored = format!("[[global]]{}", name.full_text());
            key.key == *stored || key.starts_with(&format!("{stored}."))
        });
    let same_path = |stored: &String| key.key == *stored || key.starts_with(&format!("{stored}."));
    let local_kill =
        effects.nullable_store_ends(key) || !global && effects.locals.iter().any(same_path);
    (shared_kill, local_kill)
}

/// compiler.md §162 rule 3 keeps a whole-local fact through a non-null store.
fn loop_path_kills(key: &NarrowingFact, effects: &hir::NarrowingEffects) -> (bool, bool) {
    if !key.narrows_type() {
        return path_kills(key, effects);
    }
    let shared_kill = path_kills(key, effects).0;
    let local_kill = effects.nullable_store_ends(key)
        || !super::stmt::root_of(key).starts_with("[[global]]")
            && effects.locals.iter().any(|stored| {
                key.starts_with(&format!("{stored}.")) || stored.contains('.') && key.key == *stored
            });
    (shared_kill, local_kill)
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
        match op {
            BinOp::Eq | BinOp::Ne => {
                let (null_side, other) = if matches!(left.kind, ExprKind::Null) {
                    (Some(()), right)
                } else if matches!(right.kind, ExprKind::Null) {
                    (Some(()), left)
                } else {
                    (None, left)
                };
                let target = match &other.kind {
                    ExprKind::Assign {
                        op: None, target, ..
                    } => target.as_ref(),
                    _ => other,
                };
                let compared_type = if matches!(target.kind, ExprKind::Index { .. }) {
                    &target.ty
                } else {
                    &other.ty
                };
                if null_side.is_some() && matches!(apparent_type(compared_type), Type::Nullable(_))
                {
                    if let Some(key) = fact(target) {
                        return match op {
                            // `p === null` → p is non-null when false.
                            BinOp::Eq => (Vec::new(), vec![key]),
                            // `p !== null` → p is non-null when true.
                            _ => (vec![key], Vec::new()),
                        };
                    }
                }
                (Vec::new(), Vec::new())
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
        let mut effects =
            hir::NarrowingEffects::body(body, &self.classes, self.narrowing_helpers(), None);
        for expression in expressions {
            effects.merge(expression.narrowing_effects(
                &self.classes,
                self.narrowing_helpers(),
                None,
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
    effects.nullable_store_sites.into_iter().filter_map(|(path, pos)| {
        kept.iter().any(|fact| fact.narrows_type() && (fact.key == path || fact.starts_with(&format!("{path}.")))).then(|| {
            let path = hir::source_name(path.strip_prefix("[[global]]").unwrap_or(&path));
            super::rejection::diagnostic(
                RejectionSite::NullableMember,
                format!("`{path}` may be null at the loop head: this store can set it to null\nnote: the loop keeps the null check of `{path}` made before the loop"),
                pos,
            )
        })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_exit_discards_only_facts_that_need_its_const_binding() {
        use crate::check::narrowing_fact::FactKind;
        for nested in [false, true] {
            let mut fx = FnCtx::new(Type::Void, false, None, Default::default());
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
                        ExprKind::Global(hir::Symbol::from_full_text(symbol))
                    } else {
                        ExprKind::Local(
                            path.into(),
                            Type::Nullable(Box::new(Type::Class(crate::types::ClassId(0)))),
                            true,
                        )
                    },
                    ty: Type::Nullable(Box::new(Type::Class(crate::types::ClassId(0)))),
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
                let effects = hir::NarrowingEffects::body(&body, &[], &Default::default(), None);
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
}
