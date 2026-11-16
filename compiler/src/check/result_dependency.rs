//! Result dependencies retained across local initializers (§306 rules 1 and 2).
use super::*;
use swc_common::Span;

#[derive(Default)]
pub(super) struct ResultDependencies {
    nodes: transaction::Map<Span, (Pos, bool)>,
    edges: transaction::Map<Span, Vec<Vec<Span>>>,
    // New reads invalidate the transitive sets; repeated reads reuse them.
    reached: transaction::Map<Span, HashSet<Span>>,
    pub rejected: transaction::Set<Span>,
}

impl ResultDependencies {
    pub(super) fn begin(&mut self) {
        self.nodes.begin();
        self.edges.begin();
        self.reached.begin();
        self.rejected.begin();
    }

    pub(super) fn commit(&mut self) {
        self.nodes.commit();
        self.edges.commit();
        self.reached.commit();
        self.rejected.commit();
    }

    pub(super) fn rollback(&mut self) {
        self.nodes.rollback();
        self.edges.rollback();
        self.reached.rollback();
        self.rejected.rollback();
    }

    pub fn register(&mut self, span: Span, pos: Pos, provisional: bool) {
        self.nodes.insert(span, (pos, provisional));
    }

    fn reached(&mut self, from: Span) -> &HashSet<Span> {
        self.reached
            .entry(from)
            .or_insert_with(|| Self::walk(&self.edges, from, &HashSet::new()))
    }

    fn walk(
        edges: &transaction::Map<Span, Vec<Vec<Span>>>,
        from: Span,
        excluded: &HashSet<(Span, usize)>,
    ) -> HashSet<Span> {
        let mut pending = vec![from];
        let mut seen = HashSet::new();
        while let Some(node) = pending.pop() {
            if seen.insert(node) {
                for (index, targets) in edges.get(&node).into_iter().flatten().enumerate() {
                    if !excluded.contains(&(node, index)) {
                        pending.extend(targets);
                    }
                }
            }
        }
        seen
    }

    fn read(&mut self, owner: Span, targets: Vec<(Span, bool)>) -> Vec<Pos> {
        // A call through a value is one read of all its sources, not an
        // independent read of each possible lambda (§306 rule 1c).
        let targets: Vec<_> = targets
            .into_iter()
            .filter(|&(target, through_value)| target != owner || through_value)
            .map(|(target, _)| target)
            .collect();
        if targets.is_empty() {
            return Vec::new();
        }
        let reads = self.edges.entry(owner).or_default();
        if !reads.contains(&targets) {
            reads.push(targets.clone());
            // Only ancestors of this owner can gain paths through its new read.
            let invalidated: Vec<_> = self
                .reached
                .iter()
                .filter_map(|(key, reached)| reached.contains(&owner).then_some(*key))
                .collect();
            for key in invalidated {
                self.reached.remove(&key);
            }
        }
        if !targets
            .iter()
            .any(|&target| self.reached(target).contains(&owner))
        {
            return Vec::new();
        }
        let forward = self.reached(owner).clone();
        let mut component: HashSet<_> = forward
            .into_iter()
            .filter(|&node| self.reached(node).contains(&owner))
            .collect();
        let mut excluded = HashSet::new();
        loop {
            let mut changed = false;
            for &node in &component {
                for (index, targets) in self.edges.get(&node).into_iter().flatten().enumerate() {
                    if targets.iter().any(|target| !component.contains(target)) {
                        changed |= excluded.insert((node, index));
                    }
                }
            }
            if !changed {
                break;
            }
            // Removing a mixed-source read can split the candidate cycle.
            // Repeat until every remaining read has all its sources inside it.
            let forward = Self::walk(&self.edges, owner, &excluded);
            component.retain(|&node| {
                forward.contains(&node) && Self::walk(&self.edges, node, &excluded).contains(&owner)
            });
        }
        let has_cycle = self
            .edges
            .get(&owner)
            .into_iter()
            .flatten()
            .enumerate()
            .any(|(index, targets)| {
                !excluded.contains(&(owner, index))
                    && targets.iter().any(|target| component.contains(target))
            });
        if !has_cycle
            || !component
                .iter()
                .any(|node| self.nodes.get(node).is_some_and(|node| node.1))
        {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for node in component {
            if let Some((pos, _)) = self.nodes.get(&node) {
                if self.rejected.insert(node) {
                    diagnostics.push(pos.clone());
                }
            }
        }
        diagnostics.sort_by(|a, b| (&a.file, a.line, a.col).cmp(&(&b.file, b.line, b.col)));
        diagnostics
    }
}

impl Checker<'_> {
    /// The type of an unannotated binding comes from these lambda values.
    /// An annotation and an annotated call result stop that dependency.
    /// None means at least one source is independent of lambda inference,
    /// so a call through the entire value does not need a cyclic result.
    pub(super) fn lambda_value_sources(
        &self,
        source: &ast::Expr,
        fx: &FnCtx,
    ) -> Option<Vec<(Span, bool)>> {
        self.value_sources(source, Some(fx), self.cur_file, &mut HashSet::new())
    }

    fn value_sources(
        &self,
        source: &ast::Expr,
        fx: Option<&FnCtx>,
        file: usize,
        seen: &mut HashSet<String>,
    ) -> Option<Vec<(Span, bool)>> {
        match expr::unparen_expr(source) {
            ast::Expr::Arrow(arrow) if arrow.return_type.is_none() => {
                Some(vec![(arrow.span, false)])
            }
            ast::Expr::Ident(name) => {
                for scope in fx.into_iter().flat_map(|fx| fx.scopes.iter().rev()) {
                    if scope.owns(name.sym.as_ref()) {
                        return scope
                            .result_sources
                            .get(name.sym.as_ref())
                            .cloned()
                            .flatten();
                    }
                }
                let binding = self
                    .file_scopes
                    .get(file)
                    .and_then(|scope| scope.get(name.sym.as_ref()))
                    .or_else(|| self.ambient_scope.get(name.sym.as_ref()));
                let Some(ScopeItem::Global(name)) = binding
                    .filter(|binding| !binding.type_only)
                    .map(|binding| self.binding_item(&binding.item))
                else {
                    return None;
                };
                if !seen.insert(name.clone()) {
                    return None;
                }
                let sources = self
                    .global_sigs
                    .get(&name)
                    .filter(|sig| !sig.annotated)
                    .and_then(|sig| sig.initializer.as_ref())
                    .and_then(|init| self.value_sources(&init.expression, None, init.file, seen));
                seen.remove(&name);
                sources
            }
            ast::Expr::Array(array) => array
                .elems
                .iter()
                .flatten()
                .map(|item| self.value_sources(&item.expr, fx, file, seen))
                .collect::<Option<Vec<_>>>()
                .map(|sources| sources.into_iter().flatten().collect()),
            ast::Expr::Member(member) if matches!(member.prop, ast::MemberProp::Computed(_)) => {
                self.value_sources(&member.obj, fx, file, seen)
                    .map(|sources| sources.into_iter().map(|(span, _)| (span, true)).collect())
            }
            ast::Expr::Cond(cond) => {
                let mut sources = self.value_sources(&cond.cons, fx, file, seen)?;
                sources.extend(self.value_sources(&cond.alt, fx, file, seen)?);
                Some(sources.into_iter().map(|(span, _)| (span, true)).collect())
            }
            _ => None,
        }
    }

    pub(super) fn read_call_result(&mut self, call: &ast::CallExpr, fx: &FnCtx) {
        if !self.inferring_result_operand {
            return;
        }
        let Some(owner) = self
            .lambda_result_inference
            .last()
            .map(|result| result.span)
        else {
            return;
        };
        let ast::Callee::Expr(callee) = &call.callee else {
            return;
        };
        let sources = self.lambda_value_sources(callee, fx).unwrap_or_default();
        let diagnostics = self.result_dependencies.read(owner, sources);
        for result in &mut self.lambda_result_inference {
            result.rejected |= self.result_dependencies.rejected.contains(&result.span);
        }
        for pos in diagnostics {
            self.cycle(pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::BytePos;

    #[test]
    /// Cost: less than 0.001 seconds; no compiler or engine subprocess.
    fn a_new_read_invalidates_only_its_ancestors_and_rollback_restores_them() {
        let [a, b, c, d, e] = [1, 2, 3, 4, 5].map(|pos| Span::new(BytePos(pos), BytePos(pos + 1)));
        let mut dependencies = ResultDependencies::default();
        dependencies.read(a, vec![(b, false)]);
        dependencies.read(c, vec![(d, false)]);
        assert_eq!(dependencies.reached(a), &HashSet::from([a, b]));
        assert_eq!(dependencies.reached(c), &HashSet::from([c, d]));
        dependencies.begin();
        dependencies.read(b, vec![(e, false)]);
        assert!(!dependencies.reached.contains_key(&a));
        assert_eq!(dependencies.reached.get(&c), Some(&HashSet::from([c, d])));
        assert_eq!(dependencies.reached(a), &HashSet::from([a, b, e]));
        dependencies.rollback();
        assert_eq!(dependencies.reached(a), &HashSet::from([a, b]));
        assert_eq!(dependencies.reached(c), &HashSet::from([c, d]));
    }
}
