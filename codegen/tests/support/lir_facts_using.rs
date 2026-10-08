//! The hook placements of `using` scopes that LIR must carry
//! (`compiler.md` §115.5 rules 5–7), derived from the HIR and the
//! contract.
//!
//! The walk calls neither the lowering nor the checker's §101 predicate
//! (`compiler.md` §101.3): it derives each placement again from the
//! statement shapes, so a lowering that drops or adds a hook disagrees
//! with it.

use subscript_compiler::hir;
use subscript_compiler::Pos;

use super::{
    sequence_exits, stops_statement_sequence, trap_key, try_body_raises, walk_expr,
    walk_module_expressions, walk_statement_expression_roots, TrapKey,
};

/// The trap sites and the trap terminator positions of every hook
/// placement.
#[derive(Default)]
pub(super) struct HookFacts {
    /// One key per trap site of a placed hook call, and one
    /// `DisposeRaisedDuringExit` key per raise site of a hook on an
    /// exception edge.
    pub(super) traps: Vec<TrapKey>,
    /// The position of each `DisposeRaisedDuringExit` trap terminator.
    pub(super) trap_positions: Vec<Pos>,
    pub(super) finalizers: Vec<Vec<hir::Stmt>>,
}

pub(super) fn hook_facts(hir: &hir::Module) -> HookFacts {
    let mut walk = Walk {
        hir,
        nodes: Vec::new(),
        controls: Vec::new(),
        facts: HookFacts::default(),
    };
    for function in super::all_declared_functions(hir) {
        walk.sequence(&function.body);
    }
    walk.sequence(&hir.top_level);
    let mut lambdas = Vec::new();
    walk_module_expressions(hir, &mut |expression| {
        if let hir::ExprKind::Lambda { body, .. } = &expression.kind {
            lambdas.push(body);
        }
    });
    for body in lambdas {
        walk.sequence(body);
    }
    walk.facts
}

/// Whether a hook of `binding` can raise.
pub(super) fn hook_raises(hir: &hir::Module, binding: &hir::UsingBinding) -> bool {
    hook_sites(hir, binding)
        .iter()
        .any(|site| matches!(site, hir::TrapSite::Raise { .. }))
}

/// Whether a normal exit of a `using` scope with `body` places its hooks:
/// the end of the body, or a `return`, `break`, or `continue` that leaves
/// the scope.
pub(super) fn has_normal_exit(hir: &hir::Module, body: &[hir::Stmt]) -> bool {
    end_places_hooks(hir, body) || leaves(hir, body, 0, 0)
}

fn hook_sites(hir: &hir::Module, binding: &hir::UsingBinding) -> Vec<hir::TrapSite> {
    let hook = binding.hook();
    let mut sites = Vec::new();
    walk_statement_expression_roots(hir, std::slice::from_ref(&hook), &mut |root| {
        walk_expr(hir, root, &mut |node| sites.extend(node.trap_sites(hir)));
    });
    sites
}

/// The lowering places the hooks at the end of a body only where the body
/// lowers to its end and control can arrive there (`compiler.md` §101).
pub(super) fn end_places_hooks(hir: &hir::Module, body: &[hir::Stmt]) -> bool {
    sequence_exits(hir, body).next && arrives(body).next
}

/// Whether a reached statement of `statements` leaves the enclosing
/// scope. `loops` and `switches` count the constructs between the scope
/// and the statement that capture a `break` or a `continue`.
fn leaves(hir: &hir::Module, statements: &[hir::Stmt], loops: usize, switches: usize) -> bool {
    for statement in statements {
        let leaves_here = match statement {
            hir::Stmt::Return { .. } => true,
            hir::Stmt::Break(_) => loops == 0 && switches == 0,
            hir::Stmt::Continue(_) => loops == 0,
            hir::Stmt::If {
                then,
                els,
                cond: _,
                pos: _,
            } => {
                leaves(hir, then, loops, switches)
                    || els
                        .as_deref()
                        .is_some_and(|els| leaves(hir, els, loops, switches))
            }
            hir::Stmt::While {
                body,
                cond: _,
                pos: _,
            }
            | hir::Stmt::For {
                body,
                init: _,
                cond: _,
                step: _,
                pos: _,
            }
            | hir::Stmt::ForOf {
                body,
                name: _,
                ty: _,
                subject: _,
                kind: _,
                pos: _,
            }
            | hir::Stmt::GeneratorForOf {
                body,
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                pos: _,
            } => leaves(hir, body, loops + 1, switches),
            hir::Stmt::Switch {
                cases,
                disc: _,
                pos: _,
            } => cases
                .iter()
                .any(|case| leaves(hir, &case.body, loops, switches + 1)),
            hir::Stmt::Block(body) => leaves(hir, body, loops, switches),
            hir::Stmt::Using {
                body,
                bindings: _,
                finalizer,
                pos: _,
            } => {
                leaves(hir, body, loops, switches)
                    || finalizer
                        .as_ref()
                        .is_some_and(|body| leaves(hir, body, loops, switches))
            }
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                leaves(hir, body, loops, switches)
                    || (try_body_raises(hir, body) && leaves(hir, handler, loops, switches))
            }
            hir::Stmt::Let { .. } | hir::Stmt::Expr(_) | hir::Stmt::Throw { .. } => false,
        };
        if leaves_here {
            return true;
        }
        if stops_statement_sequence(hir, statement) {
            break;
        }
    }
    false
}

/// Whether control can arrive after a statement sequence: a loop whose
/// condition is absent or the literal `true` is left only by a `break`,
/// and a literal `if` condition selects its arm (`compiler.md` §101.2).
#[derive(Clone, Copy)]
struct Arrival {
    next: bool,
    breaks: bool,
}

fn arrives(statements: &[hir::Stmt]) -> Arrival {
    let mut result = Arrival {
        next: true,
        breaks: false,
    };
    for statement in statements {
        if !result.next {
            break;
        }
        let exits = arrives_after(statement);
        result = Arrival {
            next: exits.next,
            breaks: result.breaks || exits.breaks,
        };
    }
    result
}

fn arrives_after(statement: &hir::Stmt) -> Arrival {
    const NEXT: Arrival = Arrival {
        next: true,
        breaks: false,
    };
    const STOP: Arrival = Arrival {
        next: false,
        breaks: false,
    };
    let literal_true = |cond: &hir::Expr| matches!(cond.kind, hir::ExprKind::Bool(true));
    match statement {
        hir::Stmt::Let { .. }
        | hir::Stmt::ForOf {
            name: _,
            ty: _,
            subject: _,
            kind: _,
            body: _,
            pos: _,
        }
        | hir::Stmt::GeneratorForOf {
            name: _,
            ty: _,
            mutable: _,
            subject: _,
            body: _,
            pos: _,
        } => NEXT,
        hir::Stmt::Return { .. } | hir::Stmt::Continue(_) | hir::Stmt::Throw { .. } => STOP,
        hir::Stmt::Break(_) => Arrival {
            next: false,
            breaks: true,
        },
        hir::Stmt::Expr(expression) => {
            if matches!(
                &expression.kind,
                hir::ExprKind::Call {
                    callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                    ..
                }
            ) {
                STOP
            } else {
                NEXT
            }
        }
        hir::Stmt::Block(body) => arrives(body),
        hir::Stmt::Using {
            body,
            bindings: _,
            finalizer,
            pos: _,
        } => {
            let head = arrives(body);
            let tail = finalizer.as_ref().map_or(NEXT, |body| arrives(body));
            Arrival {
                next: head.next && tail.next,
                breaks: tail.breaks || (tail.next && head.breaks),
            }
        }
        hir::Stmt::Try {
            body,
            handler,
            binding: _,
            pos: _,
        } => {
            let (body, handler) = (arrives(body), arrives(handler));
            Arrival {
                next: body.next || handler.next,
                breaks: body.breaks || handler.breaks,
            }
        }
        hir::Stmt::If {
            cond,
            then,
            els,
            pos: _,
        } => {
            let then = arrives(then);
            let els = els.as_deref().map_or(NEXT, arrives);
            match cond.kind {
                hir::ExprKind::Bool(true) => then,
                hir::ExprKind::Bool(false) => els,
                _ => Arrival {
                    next: then.next || els.next,
                    breaks: then.breaks || els.breaks,
                },
            }
        }
        hir::Stmt::While { cond, body, pos: _ } => Arrival {
            next: !literal_true(cond) || arrives(body).breaks,
            breaks: false,
        },
        hir::Stmt::For {
            init,
            cond,
            body,
            step: _,
            pos: _,
        } => {
            let init = init.as_deref().map_or(NEXT, arrives_after);
            if !init.next {
                return init;
            }
            Arrival {
                next: cond.as_ref().is_some_and(|cond| !literal_true(cond)) || arrives(body).breaks,
                breaks: init.breaks,
            }
        }
        hir::Stmt::Switch {
            cases,
            disc: _,
            pos: _,
        } => {
            let mut next = !cases.iter().any(|case| case.test.is_none());
            let mut suffix = NEXT;
            for case in cases.iter().rev() {
                let arm = arrives(&case.body);
                suffix = if arm.next {
                    Arrival {
                        next: suffix.next,
                        breaks: arm.breaks || suffix.breaks,
                    }
                } else {
                    arm
                };
                next = next || suffix.next || suffix.breaks;
            }
            Arrival {
                next,
                breaks: false,
            }
        }
    }
}

struct Walk<'h> {
    hir: &'h hir::Module,
    /// The bindings of each enclosing `using` scope, outermost first.
    nodes: Vec<(Vec<hir::UsingBinding>, Option<Vec<hir::Stmt>>)>,
    /// Each enclosing `break` target: whether it is a loop, and the depth
    /// of `nodes` where it starts.
    controls: Vec<(bool, usize)>,
    facts: HookFacts,
}

impl Walk<'_> {
    fn sequence(&mut self, statements: &[hir::Stmt]) {
        for statement in statements {
            self.statement(statement);
            if stops_statement_sequence(self.hir, statement) {
                break;
            }
        }
    }

    fn statement(&mut self, statement: &hir::Stmt) {
        // Each generator suspension adds a close exit through its lexical cleanup scopes.
        let mut yields = 0;
        for child in statement.children() {
            if let hir::HirChild::Expr(expr) = child {
                walk_expr(self.hir, expr, &mut |expr| {
                    if matches!(expr.kind, hir::ExprKind::Yield(_)) {
                        yields += 1;
                    }
                });
            }
        }
        for _ in 0..yields {
            self.leave(0);
        }
        match statement {
            hir::Stmt::Return { .. } => self.leave(0),
            hir::Stmt::Break(_) => {
                if let Some(&(_, depth)) = self.controls.last() {
                    self.leave(depth);
                }
            }
            hir::Stmt::Continue(_) => {
                if let Some(&(_, depth)) = self.controls.iter().rev().find(|(is_loop, _)| *is_loop)
                {
                    self.leave(depth);
                }
            }
            hir::Stmt::If {
                then,
                els,
                cond: _,
                pos: _,
            } => {
                self.sequence(then);
                if let Some(els) = els {
                    self.sequence(els);
                }
            }
            hir::Stmt::While {
                body,
                cond: _,
                pos: _,
            }
            | hir::Stmt::For {
                body,
                init: _,
                cond: _,
                step: _,
                pos: _,
            }
            | hir::Stmt::ForOf {
                body,
                name: _,
                ty: _,
                subject: _,
                kind: _,
                pos: _,
            }
            | hir::Stmt::GeneratorForOf {
                body,
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                pos: _,
            } => {
                self.controls.push((true, self.nodes.len()));
                self.sequence(body);
                self.controls.pop();
            }
            hir::Stmt::Switch {
                cases,
                disc: _,
                pos: _,
            } => {
                self.controls.push((false, self.nodes.len()));
                for case in cases {
                    self.sequence(&case.body);
                }
                self.controls.pop();
            }
            hir::Stmt::Block(body) => self.sequence(body),
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                self.sequence(body);
                if try_body_raises(self.hir, body) {
                    self.sequence(handler);
                }
            }
            hir::Stmt::Using {
                bindings,
                body,
                finalizer,
                pos: _,
            } => self.using(bindings, body, finalizer.as_deref()),
            hir::Stmt::Let { .. } | hir::Stmt::Expr(_) | hir::Stmt::Throw { .. } => {}
        }
    }

    fn using(
        &mut self,
        bindings: &[hir::UsingBinding],
        body: &[hir::Stmt],
        finalizer: Option<&[hir::Stmt]>,
    ) {
        self.nodes
            .push((bindings.to_vec(), finalizer.map(<[hir::Stmt]>::to_vec)));
        self.sequence(body);
        self.nodes.pop();
        if let Some(finalizer) = finalizer {
            if end_places_hooks(self.hir, body) {
                self.place_finalizer(finalizer);
            }
            if try_body_raises(self.hir, body) {
                self.place_finalizer(finalizer);
            }
            return;
        }
        if end_places_hooks(self.hir, body) {
            for binding in bindings.iter().rev() {
                self.place(binding);
            }
        }
        // The edge of the innermost binding receives the raise sites of the
        // body. The edge of each earlier binding receives the resume of
        // the next edge and the raise sites of the next binding's hooks on
        // the normal exits (compiler.md §115.5 rules 3 and 7).
        let normal_exit = has_normal_exit(self.hir, body);
        let mut landed = try_body_raises(self.hir, body);
        for binding in bindings.iter().rev() {
            if landed {
                self.place_on_edge(binding);
            }
            landed = landed || (normal_exit && hook_raises(self.hir, binding));
        }
    }

    /// The hooks of the scopes from `first` to the innermost, innermost
    /// first, in reverse declaration order.
    fn leave(&mut self, first: usize) {
        let nodes = self.nodes.clone();
        for index in (first..nodes.len()).rev() {
            self.nodes = nodes[..index].to_vec();
            if let Some(finalizer) = &nodes[index].1 {
                self.place_finalizer(finalizer);
                if !sequence_exits(self.hir, finalizer).next {
                    break;
                }
            } else {
                for binding in nodes[index].0.iter().rev() {
                    self.place(binding);
                }
            }
        }
        self.nodes = nodes;
    }

    fn place_finalizer(&mut self, body: &[hir::Stmt]) {
        self.facts.finalizers.push(body.to_vec());
        self.sequence(body);
    }

    fn place(&mut self, binding: &hir::UsingBinding) {
        let sites = hook_sites(self.hir, binding);
        self.facts
            .traps
            .extend(sites.iter().map(super::hir_trap_key));
    }

    fn place_on_edge(&mut self, binding: &hir::UsingBinding) {
        let sites = hook_sites(self.hir, binding);
        for site in &sites {
            self.facts.traps.push(super::hir_trap_key(site));
            if matches!(site, hir::TrapSite::Raise { .. }) {
                self.facts.traps.push(trap_key(
                    &binding.pos,
                    "DisposeRaisedDuringExit".to_string(),
                ));
                self.facts.trap_positions.push(binding.pos.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use subscript_compiler::lir as l;
    use subscript_compiler::{check_program, hir, SourceFile};

    use super::super::dropped_facts;

    /// A raising hook with a normal exit and an exception edge.
    const SOURCE: &str = "class F { [Symbol.dispose](): void { throw new Error(\"f\"); } }
        function fail(flag: boolean): void { if (flag) { throw new Error(\"body\"); } }
        export function main(): void {
          try {
            using r = new F();
            fail(false);
          } catch { print(\"caught\"); }
        }";

    fn checked() -> (hir::Module, l::Module) {
        let hir =
            check_program(&[SourceFile::new("hook-facts.ts", SOURCE)]).expect("the witness checks");
        let lir = subscript_codegen::lir::lower_module(&hir).expect("the witness lowers");
        (hir, lir)
    }

    fn hook_ids(lir: &l::Module) -> Vec<l::MethodId> {
        lir.classes
            .iter()
            .flat_map(|class| &class.methods)
            .filter(|method| method.source_name == hir::DISPOSE_METHOD_NAME)
            .map(|method| method.id)
            .collect()
    }

    #[test]
    fn a_dropped_hook_call_is_a_finding() {
        let (hir, mut lir) = checked();
        assert!(dropped_facts(&hir, &lir).is_empty());
        let hooks = hook_ids(&lir);
        let mut calls = 0;
        for block in lir
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
        {
            calls += block
                .instructions
                .iter()
                .filter(|instruction| {
                    matches!(&instruction.kind, l::InstructionKind::Call(target)
                        if matches!(target.kind, l::CallTargetKind::Method(id) if hooks.contains(&id)))
                })
                .count();
        }
        assert_eq!(calls, 2, "the end of the scope and the exception edge");
        let block = lir
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
            .find(|block| {
                block.instructions.iter().any(|instruction| {
                    matches!(&instruction.kind, l::InstructionKind::Call(target)
                        if matches!(target.kind, l::CallTargetKind::Method(id) if hooks.contains(&id)))
                })
            })
            .expect("a hook call");
        block.instructions.retain(|instruction| {
            !matches!(&instruction.kind, l::InstructionKind::Call(target)
                if matches!(target.kind, l::CallTargetKind::Method(id) if hooks.contains(&id)))
        });
        let findings = dropped_facts(&hir, &lir);
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("trap \"Call\" carries")),
            "{findings:#?}"
        );
    }

    #[test]
    fn a_dropped_edge_trap_is_a_finding() {
        let (hir, mut lir) = checked();
        assert!(dropped_facts(&hir, &lir).is_empty());
        let mut dropped = 0;
        for block in lir
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
        {
            if let l::Terminator::Trap(trap) = &block.terminator {
                if trap.kind == l::TrapKind::DisposeRaisedDuringExit {
                    block.terminator = l::Terminator::Unreachable {
                        pos: trap.pos.clone(),
                    };
                    dropped += 1;
                }
            }
        }
        assert_eq!(dropped, 1, "one raise site in the hook on the edge");
        let findings = dropped_facts(&hir, &lir);
        assert!(
            findings
                .iter()
                .any(|finding| finding.contains("DisposeRaisedDuringExit")),
            "{findings:#?}"
        );
    }
}
