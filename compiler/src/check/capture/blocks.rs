//! Counted capture blocks share the §118 local equations (§175).

use super::*;

pub(super) struct Local<'a> {
    pub(super) name: &'a str,
    pub(super) block: usize,
    pub(super) counted: bool,
}

impl<'a> Analysis<'a> {
    pub(super) fn enter_block(&mut self) -> usize {
        let parent = self.block;
        self.block = self.block_parents.len();
        self.block_parents.push(Some(parent));
        parent
    }

    pub(super) fn scoped_sequence(&mut self, body: &'a [Stmt], env: &mut Env) {
        let parent = self.enter_block();
        self.sequence(body, env);
        self.block = parent;
    }

    // The existing walk detects counted captures without a second body scan.
    pub(super) fn collect_block_equations(&mut self, start: usize) {
        if self.counted_capture {
            self.block_equations.extend(start..self.equations.len());
        }
    }

    // Each source id names both its block and the binding for the diagnostic.
    fn blocks(&self, e: &Expr, blocks: &mut BTreeSet<usize>) {
        if !self.carries(&e.ty) {
            return;
        }
        match &e.kind {
            E::Local(..) => {
                if let Some(id) = self.bindings.get(&(e as *const Expr as usize)) {
                    blocks.extend(&self.capture_blocks[*id]);
                }
            }
            E::Lambda { is_async, .. } => {
                if let Some(ids) = self.capture_bindings.get(&(e as *const Expr as usize)) {
                    for id in ids {
                        if !*is_async && self.locals[*id].counted {
                            blocks.insert(*id);
                        }
                        blocks.extend(&self.capture_blocks[*id]);
                    }
                }
            }
            _ => {
                self.visit_flow(e, |input| self.blocks(input, blocks));
            }
        }
    }

    // Both facts use the same §118 clean values and container operands.
    // The predicate can stop at the first true value without an operand allocation.
    pub(super) fn flow_inputs<'b>(
        &self,
        e: &'b Expr,
        mut visit: impl FnMut(&'b Expr) -> bool,
    ) -> Option<bool> {
        Some(match &e.kind {
            E::Null
            | E::Zero
            | E::FuncRef(_)
            | E::Global(_)
            | E::This
            | E::AsyncHandleAwait(_)
            | E::AsyncCall { .. } => false,
            E::Field { obj, .. } if matches!(obj.ty, Type::IterResult(_)) => visit(obj),
            E::Field { .. } => false,
            E::Index { obj, .. } => visit(obj),
            E::Cast(v) | E::Assign { value: v, .. } => visit(v),
            E::Cond { then, els, .. } => visit(then) || visit(els),
            E::New { args, .. } | E::ArrayLit(args) => args.iter().any(visit),
            E::ArraySpreadLit(values) => values.iter().any(|v| visit(&v.expr)),
            E::DescriptorLit { fields, .. } => fields.iter().flatten().any(visit),
            E::Call {
                callee: Callee::Method { recv, .. },
                ..
            } if !matches!(recv.ty, Type::Class(_)) => visit(recv),
            E::Call { callee, args } if self.generator(callee) => {
                args.iter().any(&mut visit)
                    || match callee {
                        Callee::Value(v) | Callee::Method { recv: v, .. } => visit(v),
                        _ => false,
                    }
            }
            E::Call {
                callee: Callee::Map(MapFn::GetOr),
                args,
            } => args.first().is_some_and(&mut visit) || args.get(2).is_some_and(visit),
            E::Call {
                callee: Callee::Arr(_) | Callee::Map(_) | Callee::Set(_),
                args,
            } => args.first().is_some_and(visit),
            E::Call { .. } => false,
            _ => return None,
        })
    }

    fn visit_flow<'b>(&self, e: &'b Expr, mut visit: impl FnMut(&'b Expr)) {
        if self
            .flow_inputs(e, |input| {
                visit(input);
                false
            })
            .is_none()
        {
            for child in e.children() {
                if let HirChild::Expr(input) = child {
                    visit(input);
                }
            }
        }
    }

    pub(super) fn solve_blocks(&mut self) -> Vec<BTreeSet<usize>> {
        self.capture_blocks
            .resize_with(self.locals.len(), BTreeSet::new);
        let mut values = vec![BTreeSet::new(); self.block_equations.len()];
        loop {
            let mut changed = false;
            for (index, equation) in self.block_equations.iter().enumerate() {
                let (id, e) = self.equations[*equation];
                let blocks = &mut values[index];
                self.blocks(e, blocks);
                let before = self.capture_blocks[id].len();
                self.capture_blocks[id].extend(blocks.iter().copied());
                changed |= before != self.capture_blocks[id].len();
            }
            if !changed {
                return values;
            }
        }
    }

    fn inside(&self, mut block: usize, ancestor: usize) -> bool {
        loop {
            if block == ancestor {
                return true;
            }
            let Some(parent) = self.block_parents[block] else {
                return false;
            };
            block = parent;
        }
    }

    pub(super) fn check_blocks(
        &self,
        values: &[BTreeSet<usize>],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let boundaries = self.boundary_values();
        for (equation, blocks) in self.block_equations.iter().zip(values) {
            let (id, e) = self.equations[*equation];
            if boundaries.contains(&(e as *const Expr as usize)) {
                continue;
            }
            let local = &self.locals[id];
            if let Some(source) = blocks
                .iter()
                .copied()
                .find(|source| !self.inside(local.block, self.locals[*source].block))
            {
                diagnostics.push(diagnostic(
                    RejectionSite::CaptureOutlivesBlock,
                    format!(
                        "captured binding `{}` is released when its block exits; local `{}` is declared outside that block",
                        self.locals[source].name, local.name
                    ),
                    e.pos.clone(),
                ));
            }
        }
    }

    // Follow value flow backward, including aliases and captured carriers.
    fn boundary_values(&self) -> HashSet<usize> {
        let mut work: Vec<_> = self
            .escapes
            .iter()
            .map(|(_, e)| *e)
            .chain(
                self.calls
                    .iter()
                    .filter(|(_, p, _)| self.escaping.contains(&(*p as *const hir::Param as usize)))
                    .map(|(_, _, e)| *e),
            )
            .filter(|e| self.fact(e))
            .collect();
        let mut expressions = HashSet::new();
        if work.is_empty() {
            return expressions;
        }
        let mut definitions = vec![Vec::new(); self.locals.len()];
        for (id, value) in &self.equations {
            definitions[*id].push(*value);
        }
        let mut locals = HashSet::new();
        while let Some(e) = work.pop() {
            if !self.fact(e) || !expressions.insert(e as *const Expr as usize) {
                continue;
            }
            let ids = match &e.kind {
                E::Local(..) => self
                    .bindings
                    .get(&(e as *const Expr as usize))
                    .map(std::slice::from_ref)
                    .unwrap_or_default(),
                E::Lambda { .. } => self
                    .capture_bindings
                    .get(&(e as *const Expr as usize))
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                _ => {
                    self.visit_flow(e, |input| work.push(input));
                    continue;
                }
            };
            for id in ids {
                if locals.insert(*id) {
                    work.extend(&definitions[*id]);
                }
            }
        }
        expressions
    }
}
