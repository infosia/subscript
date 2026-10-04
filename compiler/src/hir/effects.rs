//! Script execution and store effects (compiler.md §124).

use std::collections::HashSet;

use super::*;

/// Effects of one evaluated HIR subtree (compiler.md §124).
#[derive(Clone, Debug, Default)]
pub(crate) struct NarrowingEffects {
    pub(crate) script: bool,
    pub(crate) fields: HashSet<String>,
    pub(crate) globals: HashSet<Symbol>,
    pub(crate) locals: HashSet<String>,
    /// Same-path nullable stores end facts without a C17 diagnostic (§159 rule 5).
    pub(crate) nullable_stores: HashSet<String>,
}

impl NarrowingEffects {
    pub(crate) fn merge(&mut self, other: Self) {
        self.script |= other.script;
        self.fields.extend(other.fields);
        self.globals.extend(other.globals);
        self.locals.extend(other.locals);
        self.nullable_stores.extend(other.nullable_stores);
    }

    /// Whether a nullable store ends this path without a C17 note (§159 rule 5).
    pub(crate) fn nullable_store_ends(&self, path: &str) -> bool {
        self.nullable_stores
            .iter()
            .any(|stored| path == stored || path.starts_with(&format!("{stored}.")))
    }

    pub(crate) fn body(body: &[Stmt], classes: &[ClassDef], helpers: &HashSet<Symbol>) -> Self {
        let mut effects = Self::default();
        for statement in body {
            effects.merge(statement.narrowing_effects(classes, helpers));
        }
        for statement in body {
            if let Stmt::Let { name, .. } = statement {
                effects.hide_binding(name);
            }
        }
        effects
    }

    fn hide_binding(&mut self, name: &str) {
        self.nullable_stores
            .retain(|path| path != name && !path.starts_with(&format!("{name}.")));
    }
}

impl Expr {
    /// Whether this operation itself can run script code (compiler.md §124).
    pub(crate) fn ends_shared_narrowing(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
    ) -> bool {
        self.runs_script(classes, helpers, &mut HashSet::new())
    }

    fn runs_script(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        visiting: &mut HashSet<ClassId>,
    ) -> bool {
        match &self.kind {
            ExprKind::Call { callee, .. } => callee.runs_script(helpers),
            ExprKind::New { class, .. } | ExprKind::DescriptorLit { class, .. } => {
                if !visiting.insert(*class) {
                    return false;
                }
                let runs = classes.get(class.0).is_some_and(|definition| {
                    definition.ctor.is_some()
                        || definition.fields.iter().enumerate().any(|(index, field)| {
                            if matches!(&self.kind, ExprKind::DescriptorLit { fields, .. } if fields.get(index).is_some_and(Option::is_some)) {
                                return false;
                            }
                            field
                                .init
                                .as_ref()
                                .is_some_and(|init| init.subtree_runs_script(classes, helpers, visiting))
                        })
                });
                visiting.remove(class);
                runs
            }
            ExprKind::AsyncHandleTransfer { value, .. } => {
                value.runs_script(classes, helpers, visiting)
            }
            ExprKind::AsyncCall { .. }
            | ExprKind::AsyncHandleCreate { .. }
            | ExprKind::AsyncHandleAwait(_)
            | ExprKind::AsyncSuspend
            | ExprKind::Yield(_) => true,
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Null
            | ExprKind::This
            | ExprKind::Local(..)
            | ExprKind::Global(_)
            | ExprKind::FuncRef(_)
            | ExprKind::EnumMember { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::AbsenceTest { .. }
            | ExprKind::Assign { .. }
            | ExprKind::Cast(_)
            | ExprKind::Zero
            | ExprKind::Unassigned
            | ExprKind::RawNew { .. }
            | ExprKind::Field { .. }
            | ExprKind::Length(_)
            | ExprKind::Index { .. }
            | ExprKind::ArrayLit(_)
            | ExprKind::ArraySpreadLit(_)
            | ExprKind::Template(_)
            | ExprKind::Lambda { .. }
            | ExprKind::Cond { .. } => false,
        }
    }

    fn subtree_runs_script(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        visiting: &mut HashSet<ClassId>,
    ) -> bool {
        if matches!(self.kind, ExprKind::Lambda { .. }) {
            return false;
        }
        self.runs_script(classes, helpers, visiting)
            || self.children().into_iter().any(|child| match child {
                HirChild::Expr(expression) => {
                    expression.subtree_runs_script(classes, helpers, visiting)
                }
                HirChild::Stmt(_) => false,
            })
    }

    /// Effects owned by this operation, excluding its already evaluated operands.
    pub(crate) fn operation_narrowing_effects(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
    ) -> NarrowingEffects {
        self.own_effects(classes, helpers, &mut HashSet::new())
    }

    fn own_effects(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        visiting: &mut HashSet<ClassId>,
    ) -> NarrowingEffects {
        let mut effects = NarrowingEffects {
            script: self.ends_shared_narrowing(classes, helpers),
            ..Default::default()
        };
        if let ExprKind::Assign {
            op: None,
            target,
            value,
            ..
        } = &self.kind
        {
            if matches!(value.ty, Type::Null | Type::Nullable(_)) {
                if let Some(path) = store_path(target) {
                    effects.nullable_stores.insert(path);
                }
            }
        }
        match &self.kind {
            ExprKind::Assign { target, .. } => match &target.kind {
                ExprKind::Field { name, .. } => {
                    effects.fields.insert(name.clone());
                    if !target.is_shared_location(classes) {
                        if let Some(path) = local_store_path(target) {
                            effects.locals.insert(path);
                        }
                    }
                }
                ExprKind::Global(name) => {
                    effects.globals.insert(name.clone());
                }
                ExprKind::Local(name, _) => {
                    effects.locals.insert(name.clone());
                }
                _ => {}
            },
            ExprKind::New { class, .. } | ExprKind::DescriptorLit { class, .. }
                if visiting.insert(*class) =>
            {
                if let Some(definition) = classes.get(class.0) {
                    for (index, field) in definition.fields.iter().enumerate() {
                        if matches!(&self.kind, ExprKind::DescriptorLit { fields, .. } if fields.get(index).is_some_and(Option::is_some))
                        {
                            continue;
                        }
                        if let Some(init) = &field.init {
                            effects.merge(init.effects_with_visiting(classes, helpers, visiting));
                        }
                    }
                }
                visiting.remove(class);
            }
            _ => {}
        }
        effects
    }

    pub(crate) fn narrowing_effects(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
    ) -> NarrowingEffects {
        self.effects_with_visiting(classes, helpers, &mut HashSet::new())
    }

    fn effects_with_visiting(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        visiting: &mut HashSet<ClassId>,
    ) -> NarrowingEffects {
        let mut effects = self.own_effects(classes, helpers, visiting);
        if !matches!(self.kind, ExprKind::Lambda { .. }) {
            for child in self.children() {
                if let HirChild::Expr(expression) = child {
                    effects.merge(expression.effects_with_visiting(classes, helpers, visiting));
                }
            }
        }
        effects
    }
}

impl Stmt {
    /// Effects include every child that this statement can evaluate (compiler.md §124).
    pub(crate) fn narrowing_effects(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
    ) -> NarrowingEffects {
        let script = match self {
            Stmt::Using { .. } => true,
            Stmt::Let { dispose, .. } => *dispose,
            Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::If { .. }
            | Stmt::While { .. }
            | Stmt::For { .. }
            | Stmt::ForOf { .. }
            | Stmt::Switch { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Block(_)
            | Stmt::Throw { .. }
            | Stmt::Try { .. } => false,
        };
        let mut effects = NarrowingEffects {
            script,
            ..Default::default()
        };
        for child in self.children() {
            if let HirChild::Expr(expression) = child {
                effects.merge(expression.narrowing_effects(classes, helpers));
            }
        }
        // A body-local receiver is another binding, even when it has an outer name.
        let body_effects = |body: &[Stmt]| NarrowingEffects::body(body, classes, helpers);
        match self {
            Stmt::If { then, els, .. } => {
                effects.merge(body_effects(then));
                if let Some(els) = els {
                    effects.merge(body_effects(els));
                }
            }
            Stmt::While { body, .. } | Stmt::Block(body) => effects.merge(body_effects(body)),
            Stmt::For { init, body, .. } => {
                effects.merge(body_effects(body));
                if let Some(init) = init {
                    effects.merge(init.narrowing_effects(classes, helpers));
                    if let Stmt::Let { name, .. } = init.as_ref() {
                        effects.hide_binding(name);
                    }
                }
            }
            Stmt::ForOf { name, body, .. } => {
                effects.merge(body_effects(body));
                effects.hide_binding(name);
            }
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    effects.merge(body_effects(&case.body));
                }
                for statement in cases.iter().flat_map(|case| &case.body) {
                    if let Stmt::Let { name, .. } = statement {
                        effects.hide_binding(name);
                    }
                }
            }
            Stmt::Try {
                body,
                binding,
                handler,
                ..
            } => {
                effects.merge(body_effects(body));
                let mut handler_effects = body_effects(handler);
                if let Some((name, _)) = binding {
                    handler_effects.hide_binding(name);
                }
                effects.merge(handler_effects);
            }
            Stmt::Using { body, bindings, .. } => {
                effects.merge(body_effects(body));
                for binding in bindings {
                    effects.hide_binding(&binding.name);
                }
            }
            Stmt::Let { .. }
            | Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Throw { .. } => {}
        }
        effects
    }
}

impl Callee {
    fn runs_script(&self, helpers: &HashSet<Symbol>) -> bool {
        match self {
            Callee::Func(name) => !helpers.contains(name),
            Callee::Foreign(_) | Callee::Value(_) => true,
            Callee::Method { recv, name } => {
                matches!(recv.ty, Type::Class(_))
                    || (matches!(recv.ty, Type::Generator(_)) && name.full_text() == "next")
            }
            Callee::Arr(function) => match function {
                ArrFn::ForEach
                | ArrFn::Map
                | ArrFn::Filter
                | ArrFn::Reduce
                | ArrFn::Some
                | ArrFn::Every
                | ArrFn::FindIndex
                | ArrFn::Sort
                | ArrFn::ReduceRight
                | ArrFn::Find
                | ArrFn::FindLast
                | ArrFn::FindLastIndex
                | ArrFn::FlatMap => true,
                ArrFn::IndexOf
                | ArrFn::LastIndexOf
                | ArrFn::Includes
                | ArrFn::Join
                | ArrFn::Slice
                | ArrFn::Fill
                | ArrFn::Reverse
                | ArrFn::Concat
                | ArrFn::Splice
                | ArrFn::Shift
                | ArrFn::Unshift
                | ArrFn::CopyWithin
                | ArrFn::At => false,
            },
            Callee::Map(function) => match function {
                MapFn::ForEach | MapFn::GroupBy => true,
                MapFn::New
                | MapFn::Size
                | MapFn::Get
                | MapFn::GetOr
                | MapFn::Set
                | MapFn::Has
                | MapFn::Delete
                | MapFn::Clear => false,
            },
            Callee::Set(function) => match function {
                SetFn::ForEach => true,
                SetFn::New
                | SetFn::Size
                | SetFn::Add
                | SetFn::Has
                | SetFn::Delete
                | SetFn::Clear
                | SetFn::Union
                | SetFn::Intersection
                | SetFn::Difference
                | SetFn::SymmetricDifference
                | SetFn::IsSubsetOf
                | SetFn::IsSupersetOf
                | SetFn::IsDisjointFrom => false,
            },
            Callee::Ambient(_)
            | Callee::ContextBytes { .. }
            | Callee::Math(_)
            | Callee::Num(_)
            | Callee::Date(_)
            | Callee::Json(_)
            | Callee::Text(_)
            | Callee::Str(_)
            | Callee::Regex(_)
            | Callee::Worker(_) => false,
        }
    }
}

fn local_store_path(expression: &Expr) -> Option<String> {
    match &expression.kind {
        ExprKind::Local(name, _) => Some(name.clone()),
        ExprKind::Field { obj, name } => local_store_path(obj).map(|root| format!("{root}.{name}")),
        _ => None,
    }
}

fn store_path(expression: &Expr) -> Option<String> {
    match &expression.kind {
        ExprKind::Global(name) => Some(format!("[[global]]{}", name.full_text())),
        ExprKind::Local(name, _) => Some(name.clone()),
        ExprKind::This => Some("this".into()),
        ExprKind::Field { obj, name } => store_path(obj).map(|root| format!("{root}.{name}")),
        _ => None,
    }
}
