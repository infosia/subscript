//! Script execution and store effects (compiler.md §124).

use std::collections::{HashMap, HashSet};

use super::*;

/// Source proofs for loop stores, without provisional checked types (§304).
#[derive(Default)]
pub(crate) struct LoopValueSources {
    pub(crate) globals: HashMap<Symbol, Type>,
    pub(crate) const_reads: HashSet<(String, u32, u32)>,
}

/// Loop proofs use source forms; checked flow reads the apparent value type (§304 rules 1 and 2).
#[derive(Clone, Copy)]
pub(crate) enum StoreTypes<'a> {
    Source(&'a LoopValueSources),
    Checked(&'a dyn Fn(&Type) -> Type),
}

/// Effects of one evaluated HIR subtree (compiler.md §124).
#[derive(Clone, Debug, Default)]
pub(crate) struct NarrowingEffects {
    pub(crate) script: bool,
    pub(crate) fields: HashSet<String>,
    /// Every field replacement ends paths through that field, even with a non-null value (§304).
    pub(crate) replaced_fields: HashSet<String>,
    pub(crate) globals: HashSet<Symbol>,
    pub(crate) locals: HashSet<String>,
    /// Exact receiver stores end element-check facts (§163).
    pub(crate) stores: HashSet<String>,
    /// Indexed stores keep the receiver and the key, including unknown keys.
    pub(crate) indexed_stores: HashSet<(String, Option<String>)>,
    /// Same-path nullable stores end facts without a C17 diagnostic (§159 rule 5).
    pub(crate) nullable_stores: HashSet<String>,
    /// Checked nullable stores, independently derived for the final loop check.
    pub(crate) nullable_store_sites: Vec<(String, Pos)>,
    /// Checked nullable field stores also end facts through other receivers (§304 rule 2c).
    pub(crate) nullable_field_store_sites: Vec<(String, Pos)>,
    /// A store preserves its target only while each source local stays non-null (§304 rule 1b).
    pub(crate) dependent_stores: Vec<(String, Option<String>, Vec<String>)>,
    /// The source summary needs lexical const proofs, including hidden body bindings (§304 rule 4).
    pub(crate) const_sources: bool,
}

impl NarrowingEffects {
    pub(crate) fn merge(&mut self, other: Self) {
        self.script |= other.script;
        self.const_sources |= other.const_sources;
        self.fields.extend(other.fields);
        self.replaced_fields.extend(other.replaced_fields);
        self.globals.extend(other.globals);
        self.locals.extend(other.locals);
        self.stores.extend(other.stores);
        self.indexed_stores.extend(other.indexed_stores);
        self.nullable_stores.extend(other.nullable_stores);
        self.nullable_store_sites.extend(other.nullable_store_sites);
        self.nullable_field_store_sites
            .extend(other.nullable_field_store_sites);
        self.dependent_stores.extend(other.dependent_stores);
    }

    /// Whether a nullable store ends this path without a C17 note (§159 rule 5).
    pub(crate) fn nullable_store_ends(&self, path: &str) -> bool {
        self.nullable_stores
            .iter()
            .any(|stored| path == stored || path.starts_with(&format!("{stored}.")))
    }

    pub(crate) fn body(
        body: &[Stmt],
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        store_types: StoreTypes<'_>,
    ) -> Self {
        let mut effects = Self::default();
        for statement in body {
            effects.merge(statement.narrowing_effects(classes, helpers, store_types));
        }
        for statement in body {
            if let Stmt::Let { name, .. } = statement {
                effects.hide_binding(name);
            }
        }
        effects
    }

    fn hide_binding(&mut self, name: &str) {
        self.dependent_stores.retain(|(path, field, dependencies)| {
            if path == name || path.starts_with(&format!("{name}.")) {
                return false;
            }
            if dependencies.iter().any(|dependency| dependency == name) {
                self.nullable_stores.insert(path.clone());
                if let Some(field) = field {
                    self.fields.insert(field.clone());
                }
                false
            } else {
                true
            }
        });
        self.stores
            .retain(|path| path != name && !path.starts_with(&format!("{name}.")));
        self.indexed_stores
            .retain(|(path, _)| path != name && !path.starts_with(&format!("{name}.")));
        self.nullable_store_sites
            .retain(|(path, _)| path != name && !path.starts_with(&format!("{name}.")));
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
            ExprKind::TaskGroup { .. } | ExprKind::AsyncAll { .. } => false,
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
            | ExprKind::Global(_, _)
            | ExprKind::FuncRef(_)
            | ExprKind::LambdaMember(_)
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
            | ExprKind::Cond { .. }
            | ExprKind::Seq { .. } => false,
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

    /// Rule 3a uses declared path types without provisional narrowing facts.
    pub(crate) fn non_null_without_facts(
        &self,
        classes: &[ClassDef],
        sources: &LoopValueSources,
    ) -> bool {
        let non_null = |ty: &Type| {
            !matches!(
                ty,
                Type::Null
                    | Type::Nullable(_, _)
                    | Type::TypeParameter(_)
                    | Type::GenericNumber
                    | Type::Error
            )
        };
        match &self.kind {
            ExprKind::Global(symbol, _) => sources.globals.get(symbol).is_some_and(non_null),
            ExprKind::Local(_, declared, annotated) => {
                *annotated && non_null(declared)
                    || sources.const_reads.contains(&(
                        self.pos.file.clone(),
                        self.pos.line,
                        self.pos.col,
                    ))
            }
            ExprKind::Field { obj, name } => match &obj.ty {
                Type::Class(id) => classes
                    .get(id.0)
                    .and_then(|class| class.fields.iter().find(|field| field.name == *name))
                    .is_some_and(|field| field.written_non_null),
                _ => false,
            },
            ExprKind::Cond { then, els, .. } => {
                then.non_null_without_facts(classes, sources)
                    && els.non_null_without_facts(classes, sources)
            }
            ExprKind::Seq { value, .. } => value.non_null_without_facts(classes, sources),
            ExprKind::Assign {
                op: None, value, ..
            } => value.non_null_without_facts(classes, sources),
            ExprKind::New { .. }
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::EnumMember { .. } => true,
            _ => false,
        }
    }

    /// Only whole-local facts can supply provisional non-null values (§304 rule 1b).
    fn non_null_dependencies(
        &self,
        classes: &[ClassDef],
        sources: &LoopValueSources,
    ) -> Option<Vec<String>> {
        if self.non_null_without_facts(classes, sources) {
            return Some(Vec::new());
        }
        match &self.kind {
            ExprKind::Local(name, ..) => Some(vec![name.clone()]),
            ExprKind::Cond { then, els, .. } => {
                let mut dependencies = then.non_null_dependencies(classes, sources)?;
                dependencies.extend(els.non_null_dependencies(classes, sources)?);
                Some(dependencies)
            }
            ExprKind::Seq { value, .. }
            | ExprKind::Assign {
                op: None, value, ..
            } => value.non_null_dependencies(classes, sources),
            _ => None,
        }
    }

    /// Effects owned by this operation, excluding its already evaluated operands.
    pub(crate) fn operation_narrowing_effects(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        store_types: StoreTypes<'_>,
    ) -> NarrowingEffects {
        self.own_effects(classes, helpers, store_types, &mut HashSet::new())
    }

    fn own_effects(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        store_types: StoreTypes<'_>,
        visiting: &mut HashSet<ClassId>,
    ) -> NarrowingEffects {
        let mut effects = NarrowingEffects {
            script: self.ends_shared_narrowing(classes, helpers),
            ..Default::default()
        };
        if let ExprKind::Assign { target, .. } = &self.kind {
            if let Some(path) = store_path(target) {
                effects.stores.insert(path);
            }
            if let ExprKind::Index {
                obj, element_key, ..
            } = &target.kind
            {
                if let Some(receiver) = store_path(obj) {
                    effects
                        .indexed_stores
                        .insert((receiver, element_key.clone()));
                }
            }
        }
        if let ExprKind::Assign {
            op: None,
            target,
            value,
            ..
        } = &self.kind
        {
            let dependencies = match store_types {
                StoreTypes::Source(sources) => value.non_null_dependencies(classes, sources),
                StoreTypes::Checked(_) => None,
            };
            let nullable = match store_types {
                StoreTypes::Source(_) => dependencies.is_none(),
                StoreTypes::Checked(apparent_type) => matches!(
                    apparent_type(&value.ty),
                    Type::Null
                        | Type::Nullable(_, _)
                        | Type::TypeParameter(_)
                        | Type::GenericUnion(_)
                        | Type::Error
                ),
            };
            let field = match &target.kind {
                ExprKind::Field { name, .. } => Some(name.clone()),
                _ => None,
            };
            if nullable {
                if let Some(name) = &field {
                    effects.fields.insert(name.clone());
                    if matches!(store_types, StoreTypes::Checked(_)) {
                        effects
                            .nullable_field_store_sites
                            .push((name.clone(), self.pos.clone()));
                    }
                }
                if let Some(path) = store_path(target) {
                    if matches!(store_types, StoreTypes::Checked(_)) {
                        effects
                            .nullable_store_sites
                            .push((path.clone(), self.pos.clone()));
                    }
                    effects.nullable_stores.insert(path);
                }
            } else if let Some(dependencies) = dependencies.filter(|deps| !deps.is_empty()) {
                effects.const_sources = true;
                if let Some(path) = store_path(target) {
                    effects.dependent_stores.push((path, field, dependencies));
                } else if let Some(field) = field {
                    // A field receiver need not be a path, but its sources still constrain the store.
                    effects
                        .dependent_stores
                        .push((String::new(), Some(field), dependencies));
                }
            }
        }
        match &self.kind {
            ExprKind::Assign { target, .. } => match &target.kind {
                ExprKind::Field { name, .. } => {
                    effects.replaced_fields.insert(name.clone());
                    if !matches!(self.kind, ExprKind::Assign { op: None, .. }) {
                        effects.fields.insert(name.clone());
                    }
                    if !target.is_shared_location(classes) {
                        if let Some(path) = local_store_path(target) {
                            effects.locals.insert(path);
                        }
                    }
                }
                ExprKind::Global(name, _) => {
                    effects.globals.insert(name.clone());
                }
                ExprKind::Local(name, _, _) => {
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
                            effects.merge(init.effects_with_visiting(
                                classes,
                                helpers,
                                store_types,
                                visiting,
                            ));
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
        store_types: StoreTypes<'_>,
    ) -> NarrowingEffects {
        self.effects_with_visiting(classes, helpers, store_types, &mut HashSet::new())
    }

    fn effects_with_visiting(
        &self,
        classes: &[ClassDef],
        helpers: &HashSet<Symbol>,
        store_types: StoreTypes<'_>,
        visiting: &mut HashSet<ClassId>,
    ) -> NarrowingEffects {
        let mut effects = self.own_effects(classes, helpers, store_types, visiting);
        if !matches!(self.kind, ExprKind::Lambda { .. }) {
            for child in self.children() {
                if let HirChild::Expr(expression) = child {
                    effects.merge(expression.effects_with_visiting(
                        classes,
                        helpers,
                        store_types,
                        visiting,
                    ));
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
        store_types: StoreTypes<'_>,
    ) -> NarrowingEffects {
        let script = match self {
            Stmt::Using {
                bindings: _,
                body: _,
                finalizer: _,
                pos: _,
            }
            | Stmt::GeneratorForOf {
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                body: _,
                pos: _,
            } => true,
            Stmt::Let { dispose, .. } => *dispose,
            Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::If {
                cond: _,
                then: _,
                els: _,
                pos: _,
            }
            | Stmt::While {
                cond: _,
                body: _,
                pos: _,
            }
            | Stmt::DoWhile {
                body: _,
                cond: _,
                pos: _,
            }
            | Stmt::For {
                init: _,
                cond: _,
                step: _,
                body: _,
                pos: _,
            }
            | Stmt::ForOf {
                name: _,
                ty: _,
                subject: _,
                kind: _,
                body: _,
                pos: _,
            }
            | Stmt::Switch {
                disc: _,
                cases: _,
                pos: _,
            }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Block(_)
            | Stmt::Throw { .. }
            | Stmt::Try {
                body: _,
                binding: _,
                handler: _,
                pos: _,
            } => false,
        };
        let mut effects = NarrowingEffects {
            script,
            ..Default::default()
        };
        for child in self.children() {
            if let HirChild::Expr(expression) = child {
                effects.merge(expression.narrowing_effects(classes, helpers, store_types));
            }
        }
        // A body-local receiver is another binding, even when it has an outer name.
        let body_effects =
            |body: &[Stmt]| NarrowingEffects::body(body, classes, helpers, store_types);
        match self {
            Stmt::If {
                then,
                els,
                cond: _,
                pos: _,
            } => {
                effects.merge(body_effects(then));
                if let Some(els) = els {
                    effects.merge(body_effects(els));
                }
            }
            Stmt::While {
                body,
                cond: _,
                pos: _,
            }
            | Stmt::DoWhile {
                body,
                cond: _,
                pos: _,
            }
            | Stmt::Block(body) => effects.merge(body_effects(body)),
            Stmt::For {
                init,
                body,
                cond: _,
                step,
                pos: _,
            } => {
                effects.merge(body_effects(body));
                effects.merge(body_effects(step));
                if let Some(init) = init {
                    effects.merge(init.narrowing_effects(classes, helpers, store_types));
                    if let Stmt::Let { name, .. } = init.as_ref() {
                        effects.hide_binding(name);
                    }
                }
            }
            Stmt::ForOf {
                name,
                body,
                ty: _,
                subject: _,
                kind: _,
                pos: _,
            }
            | Stmt::GeneratorForOf {
                name,
                body,
                ty: _,
                mutable: _,
                subject: _,
                pos: _,
            } => {
                effects.merge(body_effects(body));
                effects.hide_binding(name);
            }
            Stmt::Switch {
                cases,
                disc: _,
                pos: _,
            } => {
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
                pos: _,
            } => {
                effects.merge(body_effects(body));
                let mut handler_effects = body_effects(handler);
                if let Some((name, _)) = binding {
                    handler_effects.hide_binding(name);
                }
                effects.merge(handler_effects);
            }
            Stmt::Using {
                body,
                bindings,
                finalizer,
                pos: _,
            } => {
                effects.merge(body_effects(body));
                if let Some(finalizer) = finalizer {
                    effects.merge(body_effects(finalizer));
                }
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
            Callee::Standard(_)
            | Callee::Foreign(_)
            | Callee::Value(_)
            | Callee::NamedLambda { .. }
            | Callee::LambdaSelf { .. } => true,
            Callee::Method { recv, name } => {
                matches!(recv.ty, Type::Class(_) | Type::Interface(_))
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
        ExprKind::Local(name, _, _) => Some(name.clone()),
        ExprKind::Field { obj, name } => local_store_path(obj).map(|root| format!("{root}.{name}")),
        _ => None,
    }
}

fn store_path(expression: &Expr) -> Option<String> {
    match &expression.kind {
        ExprKind::Global(name, _) => Some(format!("[[global]]{}", name.full_text())),
        ExprKind::Local(name, _, _) => Some(name.clone()),
        ExprKind::This => Some("this".into()),
        ExprKind::Field { obj, name } => store_path(obj).map(|root| format!("{root}.{name}")),
        ExprKind::Index {
            obj,
            element_key: Some(key),
            ..
        } => store_path(obj).map(|root| format!("{root}.[[element:{key}]]")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_global_store_value_requires_its_declared_type() {
        use super::*;
        let symbol = Symbol::from_full_text("[[test]]global");
        let value = Expr {
            pending_work: None,
            kind: ExprKind::Global(symbol.clone(), true),
            ty: Type::I32,
            pos: crate::diag::Pos::new("test.ts", 1, 1),
        };
        assert!(!value.non_null_without_facts(&[], &LoopValueSources::default()));
        let globals = LoopValueSources {
            globals: HashMap::from([(
                symbol.clone(),
                Type::Nullable(Box::new(Type::I32), crate::types::Absence::Null),
            )]),
            ..Default::default()
        };
        assert!(!value.non_null_without_facts(&[], &globals));
        let globals = LoopValueSources {
            globals: HashMap::from([(symbol, Type::I32)]),
            ..Default::default()
        };
        assert!(value.non_null_without_facts(&[], &globals));
    }
}
