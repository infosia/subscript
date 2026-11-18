//! Value sources and escape constraints under compiler.md §332 rules 1 to 3.

use super::*;

type Parameter = (ModuleFunction, usize);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ModuleValueSource {
    Made(ModuleFunction),
    Parameter(ModuleFunction, usize),
    Global(hir::Symbol),
    Escaped,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ModuleValues(Vec<ModuleValueSource>);

#[derive(Clone)]
pub(super) enum ModuleCallSource {
    Expression(ModuleValues),
    Unspecified(ModuleValues),
}

impl ModuleValues {
    pub(super) fn made(unit: ModuleFunction) -> Self {
        Self(vec![ModuleValueSource::Made(unit)])
    }

    pub(super) fn unknown() -> Self {
        Self(vec![ModuleValueSource::Escaped])
    }

    fn union(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }
}

impl ModuleEffectScanner<'_> {
    // compiler.md §332 rule 1 item 2: a write classifies the parameter for its whole unit.
    pub(super) fn parameter_assignment(&mut self, target: &hir::Expr) {
        if let hir::ExprKind::Local(name, ..) = &target.kind {
            if let Some(values) = self.locals.get(name) {
                for source in &values.0 {
                    if let ModuleValueSource::Parameter(unit, index) = source {
                        self.effects
                            .assigned_parameters
                            .insert((unit.clone(), *index));
                    }
                }
            }
        }
    }

    pub(super) fn parameters(&mut self, unit: ModuleFunction, params: &[hir::Param]) {
        for (index, param) in params.iter().enumerate() {
            self.locals.insert(
                param.name.clone(),
                ModuleValues(vec![ModuleValueSource::Parameter(unit.clone(), index)]),
            );
        }
        self.effects.parameters = params
            .iter()
            .map(|param| {
                param
                    .default
                    .as_ref()
                    .map_or_else(ModuleValues::default, |v| self.values(v))
            })
            .collect();
    }

    // compiler.md §332 rules 1 and 5: every expression kind names its value source.
    pub(super) fn values(&self, expression: &hir::Expr) -> ModuleValues {
        use hir::ExprKind as K;
        match &expression.kind {
            K::Lambda { id, .. } | K::LambdaMember(id) => {
                ModuleValues::made(ModuleFunction::Lambda(*id))
            }
            K::FuncRef(name) => ModuleValues::made(ModuleFunction::Free(name.clone())),
            K::Local(name, ..) => self
                .locals
                .get(name)
                .cloned()
                .unwrap_or_else(ModuleValues::unknown),
            K::Global(name, _) => ModuleValues(vec![ModuleValueSource::Global(name.clone())]),
            K::Cond { then, els, .. } => self.values(then).union(self.values(els)),
            K::Binary {
                op: hir::BinOp::Or,
                left,
                right,
            } => self.values(left).union(self.values(right)),
            K::Cast(value) | K::Seq { value, .. } => self.values(value),
            K::Call { callee, .. } => {
                if let Some(unit) = self.direct_callee(callee).filter(|unit| {
                    self.generators
                        .is_some_and(|generators| generators.contains(unit))
                }) {
                    // compiler.md §137 rule 5b: a generator creation makes a value.
                    ModuleValues::made(unit)
                } else {
                    ModuleValues::unknown()
                }
            }
            K::Field { .. }
            | K::Index { .. }
            | K::Yield(_)
            | K::AsyncHandleAwait(_)
            | K::AsyncCall { .. }
            | K::AsyncHandleCreate { .. }
            | K::AsyncHandleTransfer { .. }
            | K::Assign { .. }
            | K::New { .. }
            | K::RawNew { .. }
            | K::DescriptorLit { .. }
            | K::TaskGroup { .. }
            | K::AsyncAll { .. } => ModuleValues::unknown(),
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::EnumMember { .. }
            | K::Unary { .. }
            | K::Binary { .. }
            | K::AbsenceTest { .. }
            | K::Zero
            | K::Unassigned
            | K::Length(_)
            | K::ArrayLit(_)
            | K::ArraySpreadLit(_)
            | K::Template(_)
            | K::AsyncSuspend => ModuleValues::default(),
        }
    }

    pub(super) fn direct_callee(&self, callee: &hir::Callee) -> Option<ModuleFunction> {
        use hir::Callee as C;
        match callee {
            C::Func(name) => Some(ModuleFunction::Free(name.clone())),
            C::Method { recv, name } => Self::class_of(&recv.ty).and_then(|id| {
                self.classes
                    .get(id.0)
                    .filter(|class| class.methods.iter().any(|m| m.symbol == *name))
                    .map(|_| ModuleFunction::Method(id, name.clone()))
            }),
            C::Value(_)
            | C::NamedLambda { .. }
            | C::LambdaSelf { .. }
            | C::Standard(_)
            | C::Foreign(_)
            | C::Ambient(_)
            | C::ContextBytes { .. }
            | C::Math(_)
            | C::Num(_)
            | C::Date(_)
            | C::Json(_)
            | C::Text(_)
            | C::Str(_)
            | C::Regex(_)
            | C::Arr(_)
            | C::Map(_)
            | C::Set(_)
            | C::Worker(_) => None,
        }
    }

    pub(super) fn call_uses(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        direct: Option<ModuleFunction>,
        callback: bool,
    ) {
        use hir::Callee as C;
        let known = match callee {
            C::Value(value) | C::NamedLambda { value, .. } => {
                self.expr_use(value, false);
                Some(self.values(value))
            }
            C::LambdaSelf { id, .. } => Some(ModuleValues::made(ModuleFunction::Lambda(*id))),
            C::Method { recv, .. } => {
                self.expr(recv);
                direct.map(ModuleValues::made)
            }
            C::Func(_) => direct.map(ModuleValues::made),
            C::Standard(_)
            | C::Foreign(_)
            | C::Ambient(_)
            | C::ContextBytes { .. }
            | C::Math(_)
            | C::Num(_)
            | C::Date(_)
            | C::Json(_)
            | C::Text(_)
            | C::Str(_)
            | C::Regex(_)
            | C::Arr(_)
            | C::Map(_)
            | C::Set(_)
            | C::Worker(_) => None,
        };
        self.argument_sources(args, known, callback);
    }

    pub(super) fn argument_uses(
        &mut self,
        args: &[hir::Expr],
        direct: Option<ModuleFunction>,
        callback: bool,
    ) {
        self.argument_sources(args, direct.map(ModuleValues::made), callback);
    }

    fn argument_sources(
        &mut self,
        args: &[hir::Expr],
        callee: Option<ModuleValues>,
        callback: bool,
    ) {
        if let Some(callee) = callee {
            self.effects
                .arguments
                .push((callee, args.iter().map(|v| self.values(v)).collect()));
            for arg in args {
                self.expr_use(arg, false);
            }
        } else {
            for (index, arg) in args.iter().enumerate() {
                self.expr_use(arg, !(callback && index == 1));
            }
        }
    }
}

#[derive(Default)]
pub(super) struct ModuleValueFlow {
    pub(super) globals: HashMap<hir::Symbol, ModuleValues>,
    dirty: bool,
    registered: HashSet<ModuleFunction>,
    parameters: HashMap<Parameter, HashSet<ModuleFunction>>,
    escaping_parameters: HashSet<Parameter>,
    assigned_parameters: HashSet<Parameter>,
    escaped: HashSet<ModuleFunction>,
    defaults: Vec<(Parameter, ModuleValues)>,
    calls: Vec<(ModuleValues, Vec<ModuleValues>)>,
    escapes: Vec<ModuleValues>,
    escape_calls: Vec<(ModuleValues, Vec<ModuleValues>)>,
    parameter_counts: HashMap<ModuleFunction, usize>,
    call_sources: Vec<ModuleCallSource>,
    preloaded_sources: bool,
    non_known_calls: HashMap<ModuleFunction, HashSet<usize>>,
}

impl ModuleValueFlow {
    // compiler.md §332 rule 2: escape uses belong to units, independent of their run order.
    pub(super) fn preload_escapes<'a>(
        &mut self,
        summaries: impl Iterator<Item = &'a ModuleEffects>,
    ) {
        self.dirty = true;
        for summary in summaries {
            self.call_sources
                .extend(summary.call_sources.iter().cloned());
            self.assignments(summary);
            self.escapes.extend(
                summary
                    .escapes
                    .iter()
                    .filter(|values| !values.0.is_empty())
                    .cloned(),
            );
            self.escape_calls.extend(
                summary
                    .arguments
                    .iter()
                    .filter(|(_, args)| !args.is_empty())
                    .cloned(),
            );
        }
        self.preloaded_sources = true;
    }

    pub(super) fn register(&mut self, unit: &ModuleFunction, summary: &ModuleEffects) {
        if !self.registered.insert(unit.clone()) {
            return;
        }
        self.dirty = true;
        self.assignments(summary);
        self.parameter_counts
            .insert(unit.clone(), summary.parameters.len());
        for (index, default) in summary.parameters.iter().enumerate() {
            if !default.0.is_empty() {
                self.defaults.push(((unit.clone(), index), default.clone()));
            }
        }
        self.add(summary);
    }

    fn assignments(&mut self, summary: &ModuleEffects) {
        self.assigned_parameters
            .extend(summary.assigned_parameters.iter().cloned());
        // compiler.md §332 rule 2: the parameter's write also makes its incoming values escape.
        self.escaping_parameters
            .extend(summary.assigned_parameters.iter().cloned());
    }

    pub(super) fn add(&mut self, summary: &ModuleEffects) {
        self.dirty |=
            !summary.arguments.is_empty() || summary.escapes.iter().any(|v| !v.0.is_empty());
        if !self.preloaded_sources && !summary.call_sources.is_empty() {
            self.dirty = true;
            self.call_sources
                .extend(summary.call_sources.iter().cloned());
        }
        self.calls.extend(summary.arguments.iter().cloned());
        self.escapes
            .extend(summary.escapes.iter().filter(|v| !v.0.is_empty()).cloned());
    }

    pub(super) fn call_source(&mut self, source: ModuleCallSource) {
        self.call_sources.push(source);
        self.dirty = true;
    }

    pub(super) fn non_known_call_count(&self, unit: &ModuleFunction) -> usize {
        self.non_known_calls.get(unit).map_or(0, HashSet::len)
    }

    // compiler.md §332 rule 3: a parameter list is complete only with no non-known callers.
    fn count_non_known_calls(&mut self) -> bool {
        let mut changed = false;
        // Site zero represents an escape. Other sites index the call sources.
        for unit in &self.escaped {
            changed |= self
                .non_known_calls
                .entry(unit.clone())
                .or_default()
                .insert(0);
        }
        for (index, source) in self.call_sources.iter().enumerate() {
            let targets = match source {
                ModuleCallSource::Expression(values) => {
                    if self
                        .expand(values, &mut HashSet::new())
                        .iter()
                        .any(|source| matches!(source, ModuleValueSource::Escaped))
                    {
                        self.escaped.clone()
                    } else {
                        HashSet::new()
                    }
                }
                ModuleCallSource::Unspecified(values) => self.values(values),
            };
            for unit in targets {
                changed |= self
                    .non_known_calls
                    .entry(unit)
                    .or_default()
                    .insert(index + 1);
            }
        }
        changed
    }

    fn expand(
        &self,
        sources: &ModuleValues,
        seen: &mut HashSet<hir::Symbol>,
    ) -> Vec<ModuleValueSource> {
        let mut result = Vec::new();
        for source in &sources.0 {
            if let ModuleValueSource::Global(name) = source {
                if seen.insert(name.clone()) {
                    if let Some(values) = self.globals.get(name) {
                        result.extend(self.expand(values, seen));
                    } else {
                        result.push(ModuleValueSource::Escaped);
                    }
                    seen.remove(name);
                }
            } else if let ModuleValueSource::Parameter(unit, index) = source {
                if self.assigned_parameters.contains(&(unit.clone(), *index)) {
                    result.push(ModuleValueSource::Escaped);
                } else {
                    result.push(source.clone());
                }
            } else {
                result.push(source.clone());
            }
        }
        result
    }

    pub(super) fn values(&self, sources: &ModuleValues) -> HashSet<ModuleFunction> {
        self.source_values(&self.expand(sources, &mut HashSet::new()))
    }

    fn source_values(&self, sources: &[ModuleValueSource]) -> HashSet<ModuleFunction> {
        let mut result = HashSet::new();
        for source in sources {
            match source {
                ModuleValueSource::Made(unit) => {
                    result.insert(unit.clone());
                }
                ModuleValueSource::Parameter(unit, index) => {
                    if let Some(values) = self.parameters.get(&(unit.clone(), *index)) {
                        result.extend(values.iter().cloned());
                    }
                }
                ModuleValueSource::Escaped | ModuleValueSource::Global(_) => {
                    result.extend(self.escaped.iter().cloned())
                }
            }
        }
        result
    }

    fn escape(&mut self, sources: &ModuleValues) -> bool {
        let mut changed = false;
        for source in self.expand(sources, &mut HashSet::new()) {
            match source {
                ModuleValueSource::Made(unit) => changed |= self.escaped.insert(unit),
                ModuleValueSource::Parameter(unit, index) => {
                    changed |= self.escaping_parameters.insert((unit.clone(), index));
                    if let Some(values) = self.parameters.get(&(unit, index)) {
                        for value in values {
                            changed |= self.escaped.insert(value.clone());
                        }
                    }
                }
                ModuleValueSource::Escaped | ModuleValueSource::Global(_) => {}
            }
        }
        changed
    }

    fn transfer(&mut self, target: Parameter, sources: &ModuleValues) -> bool {
        let values = self.values(sources);
        let target_values = self.parameters.entry(target.clone()).or_default();
        let mut changed = false;
        for value in values {
            changed |= target_values.insert(value);
        }
        if self.escaping_parameters.contains(&target) {
            changed |= self.escape(sources);
        }
        changed
    }

    // compiler.md §332 rules 2 and 3: known calls and escapes form one fixed point.
    pub(super) fn solve(&mut self, summaries: &HashMap<ModuleFunction, ModuleEffects>) {
        if !self.dirty {
            return;
        }
        loop {
            let mut changed = false;
            for sources in self.escapes.clone() {
                changed |= self.escape(&sources);
            }
            for (callee, args) in self.escape_calls.clone() {
                let sources = self.expand(&callee, &mut HashSet::new());
                let unknown = sources
                    .iter()
                    .any(|source| matches!(source, ModuleValueSource::Escaped));
                let targets = self.source_values(&sources);
                for (index, arg) in args.iter().enumerate() {
                    if unknown
                        || targets
                            .iter()
                            .any(|unit| self.escaping_parameters.contains(&(unit.clone(), index)))
                    {
                        changed |= self.escape(arg);
                    }
                }
            }
            for (target, sources) in self.defaults.clone() {
                changed |= self.transfer(target, &sources);
            }
            for (callee, args) in self.calls.clone() {
                let mut sources = self.expand(&callee, &mut HashSet::new());
                // compiler.md §332 rule 2: an unknown branch makes its arguments escape.
                if sources
                    .iter()
                    .any(|source| matches!(source, ModuleValueSource::Escaped))
                {
                    for arg in &args {
                        changed |= self.escape(arg);
                    }
                }
                sources.retain(|source| !matches!(source, ModuleValueSource::Escaped));
                for unit in self.source_values(&sources) {
                    if let Some(summary) = summaries.get(&unit) {
                        changed |= !self.registered.contains(&unit);
                        self.register(&unit, summary);
                        for (index, arg) in args.iter().enumerate() {
                            changed |= self.transfer((unit.clone(), index), arg);
                        }
                    } else {
                        for arg in &args {
                            changed |= self.escape(arg);
                        }
                    }
                }
            }
            for (unit, count) in self.parameter_counts.clone() {
                if self.non_known_call_count(&unit) != 0 {
                    for index in 0..count {
                        changed |= self.transfer((unit.clone(), index), &ModuleValues::unknown());
                    }
                }
            }
            changed |= self.count_non_known_calls();
            if !changed {
                self.dirty = false;
                break;
            }
        }
    }
}
