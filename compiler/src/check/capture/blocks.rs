//! Counted capture blocks share the §118 local equations (§175).

use super::*;

pub(super) struct Local<'a> {
    pub(super) name: &'a str,
    pub(super) block: usize,
    pub(super) counted: bool,
    pub(super) initializer: Option<&'a Expr>,
    pub(super) array_element: Option<ArrayElement<'a>>,
}

// compiler.md §331 rules 2 and 3: the form selects its text and example.
pub(super) struct ArrayElement<'a> {
    pub(super) array: &'a Expr,
    pub(super) for_of: bool,
}

const FOR_OF_ARRAY_ELEMENT_EXAMPLE: crate::divergence::DivergenceEntry = crate::divergence::DivergenceEntry {
        ts: "async function work(n: i32): Promise<i32> { return n; } export function main(): void { const hs: Promise<i32>[] = [work(1), work(2)]; let f: () => Promise<i32> = () => work(0); for (const h of hs) { f = () => h; } }",
        subscript: "async function work(n: i32): Promise<i32> { return n; } export async function main(): Promise<void> { const hs: Promise<i32>[] = [work(1), work(2)]; let f: () => Promise<i32> = () => work(0); { f = () => hs[1]; } print(`${await f()}`); }",
        why: "A lambda borrows a captured array element. The array can drop that element before the lambda runs. Capture the array and read the element inside the lambda.",
        collision: "C5",
};

pub(in crate::check) const ARRAY_ELEMENT_EXAMPLES: [crate::divergence::DivergenceEntry; 2] = [
    FOR_OF_ARRAY_ELEMENT_EXAMPLE,
    crate::divergence::DivergenceEntry {
        ts: "async function work(n: i32): Promise<i32> { return n; } export function main(): void { const hs: Promise<i32>[] = [work(1), work(2)]; let f: () => Promise<i32> = () => work(0); { const h = hs[1]; f = () => h; } }",
        ..FOR_OF_ARRAY_ELEMENT_EXAMPLE
    },
];

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
            E::Lambda {
                owns_environment, ..
            } => {
                if let Some(ids) = self.capture_bindings.get(&(e as *const Expr as usize)) {
                    for id in ids {
                        if !*owns_environment && self.locals[*id].counted {
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
    ) -> Option<bool>
    where
        'a: 'b,
    {
        Some(match &e.kind {
            E::LambdaMember(id) => self.members.get(id).is_some_and(|member| visit(member)),
            E::Null
            | E::Zero
            | E::FuncRef(_)
            | E::Global(_, _)
            | E::This
            | E::AsyncHandleAwait(_)
            | E::AsyncCall { .. } => false,
            E::Field { obj, .. } if matches!(obj.ty, Type::IterResult(_)) => visit(obj),
            E::Field { .. } => false,
            E::Index { obj, .. } => visit(obj),
            E::Cast(v) | E::Assign { value: v, .. } => visit(v),
            E::Cond { then, els, .. } => visit(then) || visit(els),
            E::Seq { value, .. } => visit(value),
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
                        Callee::Value(v)
                        | Callee::NamedLambda { value: v, .. }
                        | Callee::Method { recv: v, .. } => visit(v),
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

    fn visit_flow<'b>(&self, e: &'b Expr, mut visit: impl FnMut(&'b Expr))
    where
        'a: 'b,
    {
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

    // compiler.md §331 rules 2 and 2a: the holder must cover the receiving local's block.
    fn array_name(&self, array: &Expr, destination: usize) -> Option<String> {
        match &array.kind {
            E::Local(name, ..) => {
                let id = self.bindings.get(&(array as *const Expr as usize))?;
                let holder = &self.locals[*id];
                if name.starts_with("[[pattern#") {
                    if let Some(initializer) = holder.initializer {
                        return self.array_name(initializer, destination);
                    }
                    return holder.array_element.as_ref().and_then(|element| {
                        self.array_name(element.array, destination)
                            .map(|_| "an array".to_owned())
                    });
                }
                self.inside(destination, holder.block)
                    .then(|| format!("array `{name}`"))
            }
            E::Field { name, .. } => Some(format!("array `{name}`")),
            E::Global(symbol, _) => Some(format!(
                "array `{}`",
                super::super::identity::module_declaration_label(self.module, symbol)
            )),
            E::Index { obj, .. } => self
                .array_name(obj, destination)
                .map(|_| "an array".to_owned()),
            E::Cast(value) => self.array_name(value, destination),
            _ => None,
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
                let binding = &self.locals[source];
                let array = binding.array_element.as_ref().and_then(|element| {
                    self.array_name(element.array, local.block)
                        .map(|name| (element, name))
                });
                let message = if let Some((_, array)) = &array {
                    format!(
                        "captured binding `{}` borrows an element of {array}; the array can drop that element before the lambda runs; local `{}` is declared outside that block",
                        binding.name, local.name
                    )
                } else {
                    format!(
                        "captured binding `{}` is released when its block exits; local `{}` is declared outside that block",
                        binding.name, local.name
                    )
                };
                let mut rejection =
                    diagnostic(RejectionSite::CaptureOutlivesBlock, message, e.pos.clone());
                if let Some((element, _)) = array {
                    rejection.example = Some(&ARRAY_ELEMENT_EXAMPLES[usize::from(!element.for_of)]);
                }
                diagnostics.push(rejection);
            }
        }
    }

    // Follow value flow backward, including aliases and captured carriers.
    fn boundary_values(&self) -> HashSet<usize> {
        let work: Vec<_> = self
            .escapes
            .iter()
            .map(|(_, e)| *e)
            .chain(
                self.calls
                    .iter()
                    .filter(|(_, p, _)| {
                        self.escaping
                            .contains_key(&(*p as *const hir::Param as usize))
                    })
                    .map(|(_, _, e)| *e),
            )
            .filter(|e| self.fact(e))
            .collect();
        self.trace_values(work, true)
    }
    pub(super) fn owned_values(&self) -> HashSet<usize> {
        let work: Vec<_> = self
            .escapes
            .iter()
            .filter(|(kind, _)| super::destination(kind) == hir::EscapeKind::Managed)
            .map(|(_, e)| *e)
            .chain(
                self.calls
                    .iter()
                    .filter(|(_, p, _)| {
                        self.escaping.get(&(*p as *const hir::Param as usize))
                            == Some(&hir::EscapeKind::Managed)
                    })
                    .map(|(_, _, e)| *e),
            )
            .collect();
        self.trace_values(work, false)
    }
    fn trace_values(&self, mut work: Vec<&'a Expr>, require_fact: bool) -> HashSet<usize> {
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
            if (require_fact && !self.fact(e)) || !expressions.insert(e as *const Expr as usize) {
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
