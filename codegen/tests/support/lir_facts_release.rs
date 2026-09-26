//! Exception-exit ownership facts derived from lexical HIR scopes (§116.1 rule 4b).

use super::{all_declared_functions, stops_statement_sequence, try_body_raises};
use std::collections::BTreeMap;
use subscript_compiler::{hir, lir as l, Pos, Type};

pub(super) fn compare(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut walk = Walk {
        hir,
        owners: 0,
        caught: 0,
        sites: Vec::new(),
        hooks: Vec::new(),
        controls: Vec::new(),
    };
    for function in all_declared_functions(hir) {
        walk.owners = function.params.iter().filter(|p| owned(&p.ty)).count();
        walk.sequence(&function.body);
    }
    walk.owners = 0;
    walk.sequence(&hir.top_level);
    let mut sites = BTreeMap::<(String, u32, u32), Vec<usize>>::new();
    for (pos, required) in walk.sites {
        sites
            .entry((pos.file, pos.line, pos.col))
            .or_default()
            .push(required);
    }
    for ((file, line, col), mut required) in sites {
        let pos = Pos::new(file, line, col);
        let mut observed = Vec::new();
        for function in &lir.functions {
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                if instruction.pos != pos
                    || matches!(instruction.kind, l::InstructionKind::ExceptionResume)
                {
                    continue;
                }
                let Some(edge) = instruction.raise_edge() else {
                    continue;
                };
                if let l::RaiseEdge::Handler(id) = edge {
                    if function.blocks.get(id.0 as usize).is_some_and(|block| {
                        matches!(&block.terminator, l::Terminator::Trap(trap) if trap.kind == l::TrapKind::DisposeRaisedDuringExit)
                    }) {
                        continue;
                    }
                }
                let (minimum, maximum) = releases(function, edge, &mut Vec::new());
                observed.push(minimum);
                if minimum != maximum {
                    findings.push(format!("{pos}: exception edge releases between {minimum} and {maximum} owned handles"));
                }
            }
        }
        required.sort_unstable();
        observed.sort_unstable();
        for (released, required) in observed.iter().zip(&required) {
            if released != required {
                findings.push(format!("{pos}: exception edge releases {released} owned handles; lexical scopes require {required}"));
            }
        }
        if observed.len() != required.len() {
            findings.push(format!(
                "{pos}: {} exception edges; lexical scopes require {}",
                observed.len(),
                required.len()
            ));
        }
    }
}

fn owned(ty: &Type) -> bool {
    matches!(ty, Type::AsyncHandle(_))
        || matches!(ty, Type::Array(t) if matches!(**t, Type::AsyncHandle(_)))
}

fn releases(
    function: &l::Function,
    edge: &l::RaiseEdge,
    path: &mut Vec<l::BlockId>,
) -> (usize, usize) {
    match edge {
        l::RaiseEdge::Propagate => (0, 0),
        l::RaiseEdge::Handler(block) => block_releases(function, *block, path),
    }
}

fn block_releases(
    function: &l::Function,
    id: l::BlockId,
    path: &mut Vec<l::BlockId>,
) -> (usize, usize) {
    if path.contains(&id) {
        return (0, 0);
    }
    let Some(block) = function.blocks.get(id.0 as usize) else {
        return (0, 0);
    };
    path.push(id);
    let mut count = 0;
    for instruction in &block.instructions {
        match instruction.kind {
            l::InstructionKind::CatchEntry => {
                path.pop();
                return (count, count);
            }
            l::InstructionKind::AsyncHandleRelease
            | l::InstructionKind::AsyncHandleArrayRelease => count += 1,
            l::InstructionKind::ExceptionResume => {
                let (minimum, maximum) = instruction
                    .raise_edge()
                    .map_or((0, 0), |edge| releases(function, edge, path));
                path.pop();
                return (count + minimum, count + maximum);
            }
            _ => {}
        }
    }
    let targets = match &block.terminator {
        l::Terminator::Branch(target) => vec![target.block],
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => {
            vec![then_target.block, else_target.block]
        }
        l::Terminator::Switch { arms, default, .. } => std::iter::once(default.block)
            .chain(arms.iter().map(|arm| arm.target.block))
            .collect(),
        _ => Vec::new(),
    };
    let ranges: Vec<_> = targets
        .into_iter()
        .map(|target| block_releases(function, target, path))
        .collect();
    path.pop();
    (
        count + ranges.iter().map(|r| r.0).min().unwrap_or(0),
        count + ranges.iter().map(|r| r.1).max().unwrap_or(0),
    )
}

#[derive(Clone)]
struct Hooks {
    bindings: Vec<hir::UsingBinding>,
    owners: usize,
    caught: usize,
}

struct Walk<'a> {
    hir: &'a hir::Module,
    owners: usize,
    caught: usize,
    sites: Vec<(Pos, usize)>,
    hooks: Vec<Hooks>,
    controls: Vec<(usize, bool)>,
}

impl Walk<'_> {
    fn site(&mut self, pos: &Pos) {
        self.sites
            .push((pos.clone(), self.owners.saturating_sub(self.caught)));
    }

    fn expression(&mut self, expr: &hir::Expr) {
        if let hir::ExprKind::Lambda { params, body, .. } = &expr.kind {
            let hooks = std::mem::take(&mut self.hooks);
            let controls = std::mem::take(&mut self.controls);
            let state = (self.owners, self.caught);
            self.owners = params.iter().filter(|p| owned(&p.ty)).count();
            self.caught = 0;
            self.sequence(body);
            (self.owners, self.caught) = state;
            self.hooks = hooks;
            self.controls = controls;
            return;
        }
        for child in expr.children() {
            match child {
                hir::HirChild::Expr(expr) => self.expression(expr),
                hir::HirChild::Stmt(stmt) => self.statement(stmt),
            }
        }
        for trap in expr.trap_sites(self.hir) {
            if let hir::TrapSite::Raise { pos } = trap {
                self.site(&pos);
            }
        }
    }

    fn sequence(&mut self, body: &[hir::Stmt]) {
        let owners = self.owners;
        for statement in body {
            self.statement(statement);
            if stops_statement_sequence(self.hir, statement) {
                break;
            }
        }
        self.owners = owners;
    }

    fn exit_hooks(&mut self, depth: usize, returning: bool) {
        let state = (self.owners, self.caught);
        for hook in self.hooks[depth..].to_vec().iter().rev() {
            self.owners = hook.owners + usize::from(returning);
            self.caught = hook.caught;
            for binding in hook.bindings.iter().rev() {
                self.statement(&binding.hook());
            }
        }
        (self.owners, self.caught) = state;
    }

    fn statement(&mut self, stmt: &hir::Stmt) {
        use hir::Stmt as S;
        match stmt {
            S::Let { ty, init, .. } => {
                self.expression(init);
                self.owners += usize::from(owned(ty));
            }
            S::Expr(expr) => self.expression(expr),
            S::Return { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
                self.exit_hooks(0, value.as_ref().is_some_and(|value| owned(&value.ty)));
            }
            S::Throw { value, pos } => {
                self.expression(value);
                self.site(pos);
            }
            S::If {
                cond, then, els, ..
            } => {
                self.expression(cond);
                self.sequence(then);
                if let Some(els) = els {
                    self.sequence(els);
                }
            }
            S::While { cond, body, .. } => {
                self.expression(cond);
                self.controls.push((self.hooks.len(), true));
                self.sequence(body);
                self.controls.pop();
            }
            S::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let owners = self.owners;
                if let Some(init) = init {
                    self.statement(init);
                }
                if let Some(cond) = cond {
                    self.expression(cond);
                }
                self.controls.push((self.hooks.len(), true));
                self.sequence(body);
                self.controls.pop();
                if let Some(step) = step {
                    self.expression(step);
                }
                self.owners = owners;
            }
            S::ForOf {
                ty, subject, body, ..
            } => {
                self.expression(subject);
                let owners = self.owners;
                self.owners += usize::from(owned(ty));
                self.controls.push((self.hooks.len(), true));
                self.sequence(body);
                self.controls.pop();
                self.owners = owners;
            }
            S::Switch { disc, cases, .. } => {
                self.expression(disc);
                self.controls.push((self.hooks.len(), false));
                for case in cases {
                    if let Some(test) = &case.test {
                        self.expression(test);
                    }
                    self.sequence(&case.body);
                }
                self.controls.pop();
            }
            S::Try { body, handler, .. } => {
                let caught = self.caught;
                self.caught = self.owners;
                self.sequence(body);
                self.caught = caught;
                if try_body_raises(self.hir, body) {
                    self.sequence(handler);
                }
            }
            S::Block(body) => self.sequence(body),
            S::Using { bindings, body, .. } => {
                self.hooks.push(Hooks {
                    bindings: bindings.clone(),
                    owners: self.owners,
                    caught: self.caught,
                });
                self.sequence(body);
                if super::using::end_places_hooks(self.hir, body) {
                    self.exit_hooks(self.hooks.len() - 1, false);
                }
                self.hooks.pop();
            }
            S::Break(_) | S::Continue(_) => {
                if let Some(&(depth, _)) = self
                    .controls
                    .iter()
                    .rev()
                    .find(|(_, is_loop)| matches!(stmt, S::Break(_)) || *is_loop)
                {
                    self.exit_hooks(depth, false);
                }
            }
        }
    }
}

#[test]
fn a_return_hook_counts_the_pending_owner_at_each_placement() {
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "return-hook.ts",
        r#"async function value(): Promise<i32> { return 7; }
class R { [Symbol.dispose](): void { throw new Error("hook"); } }
function returning(early: boolean): Promise<i32> {
  const h: Promise<i32> = value();
  {
    using r = new R();
    if (early) { return h; }
  }
  return h;
}
export async function main(): Promise<void> { const h = returning(true); await h; }
"#,
    )])
    .expect("checks");
    let mut lir = subscript_codegen::lir::lower_module(&hir).expect("lowers");
    let mut findings = Vec::new();
    compare(&hir, &lir, &mut findings);
    assert!(findings.is_empty(), "{findings:?}");
    let landing = lir
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .find(|block| {
            matches!(
                block.instructions.first().map(|i| &i.kind),
                Some(l::InstructionKind::ExceptionPark)
            ) && block
                .instructions
                .iter()
                .filter(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease))
                .count()
                == 2
        })
        .expect("the return hook releases the local and returned owners");
    let release = landing
        .instructions
        .iter()
        .position(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease))
        .expect("release");
    landing.instructions.remove(release);
    compare(&hir, &lir, &mut findings);
    assert!(
        findings.iter().any(
            |f| f.contains("exception edge releases 1 owned handles; lexical scopes require 2")
        ),
        "{findings:?}"
    );
}
