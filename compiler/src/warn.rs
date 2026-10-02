//! Warnings computed from an accepted, checked HIR module.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::hir::{self, AmbientFn, Callee, Expr, ExprKind, MapFn, SetFn, Stmt};
use crate::Pos;

/// Stable code carried by every warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WarnCode {
    /// A reference-class allocation repeats in a loop without escaping or
    /// being released.
    W001,
    /// A local is used after `Context.free(local)` in the same block.
    W002,
    /// A callback-info aggregate registers freshly allocated userdata in a
    /// loop.
    W003,
    /// A value-type copy is written through but never read.
    W004,
}

impl WarnCode {
    /// Every stable warning code, in numeric order.
    pub const ALL: [Self; 4] = [Self::W001, Self::W002, Self::W003, Self::W004];

    /// The stable textual form of the code, e.g. `"W001"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WarnCode::W001 => "W001",
            WarnCode::W002 => "W002",
            WarnCode::W003 => "W003",
            WarnCode::W004 => "W004",
        }
    }

    /// A one-line explanation of the warning rule.
    #[must_use]
    pub fn explanation(self) -> &'static str {
        match self {
            WarnCode::W001 => {
                "A reference-class allocation repeated by a loop should escape the iteration or be released."
            }
            WarnCode::W002 => {
                "A local should not be used after `Context.free(local)` without an intervening reassignment."
            }
            WarnCode::W003 => {
                "Fresh callback userdata registered in a loop creates and roots a new binding record per iteration."
            }
            WarnCode::W004 => {
                "A value-type copy that is written through and never read leaves its source unchanged."
            }
        }
    }
}

impl fmt::Display for WarnCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One non-fatal warning produced for an accepted program.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Warning {
    /// The stable warning code.
    pub code: WarnCode,
    /// Free-form human-readable message.
    pub message: String,
    /// Position of the construct that caused the warning.
    pub pos: Pos,
}

impl Warning {
    /// Builds a warning.
    #[must_use]
    pub fn new(code: WarnCode, message: impl Into<String>, pos: Pos) -> Self {
        Self {
            code,
            message: message.into(),
            pos,
        }
    }
}

/// Computes warnings for a module returned successfully by
/// [`crate::check_program`].
///
/// Warning analysis does not affect acceptance and must not be run for a
/// rejected program.
#[must_use]
pub fn check_warnings(module: &hir::Module) -> Vec<Warning> {
    let mut checker = WarningChecker {
        module,
        warnings: Vec::new(),
    };

    for owner in module.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(expression) => {
                checker.analyze_lambdas_in_expr(expression);
            }
            hir::ExpressionOwner::Body {
                statements,
                function,
            } => checker.analyze_body(
                statements,
                function.map_or(&[], |function| function.params.as_slice()),
            ),
        }
    }

    checker.warnings
}

struct WarningChecker<'m> {
    module: &'m hir::Module,
    warnings: Vec<Warning>,
}

fn walk_statements<S: Clone>(
    statements: &[Stmt],
    loop_depth: usize,
    state: &mut S,
    visit: &mut impl FnMut(&Stmt, &[Stmt], usize, &mut S),
) {
    for (index, statement) in statements.iter().enumerate() {
        let remaining = &statements[index + 1..];
        match statement {
            Stmt::While { body, .. } | Stmt::ForOf { body, .. } => {
                let mut nested = state.clone();
                visit(statement, remaining, loop_depth, &mut nested);
                walk_statements(body, loop_depth + 1, &mut nested, visit);
            }
            Stmt::For { init, body, .. } => {
                let mut nested = state.clone();
                if let Some(init) = init {
                    walk_statements(
                        std::slice::from_ref(init.as_ref()),
                        loop_depth,
                        &mut nested,
                        visit,
                    );
                }
                visit(statement, remaining, loop_depth, &mut nested);
                walk_statements(body, loop_depth + 1, &mut nested, visit);
            }
            Stmt::If { then, els, .. } => {
                let mut branch = state.clone();
                visit(statement, remaining, loop_depth, &mut branch);
                walk_statements(then, loop_depth, &mut branch.clone(), visit);
                if let Some(els) = els {
                    walk_statements(els, loop_depth, &mut branch, visit);
                }
            }
            Stmt::Switch { cases, .. } => {
                let mut branch = state.clone();
                visit(statement, remaining, loop_depth, &mut branch);
                for case in cases {
                    walk_statements(&case.body, loop_depth, &mut branch.clone(), visit);
                }
            }
            Stmt::Block(body) => {
                let mut nested = state.clone();
                visit(statement, remaining, loop_depth, &mut nested);
                walk_statements(body, loop_depth, &mut nested, visit);
            }
            Stmt::Using { body, .. } => {
                let mut nested = state.clone();
                visit(statement, remaining, loop_depth, &mut nested);
                walk_statements(body, loop_depth, &mut nested, visit);
            }
            Stmt::Try { body, handler, .. } => {
                let mut branch = state.clone();
                visit(statement, remaining, loop_depth, &mut branch);
                walk_statements(body, loop_depth, &mut branch.clone(), visit);
                walk_statements(handler, loop_depth, &mut branch, visit);
            }
            _ => visit(statement, remaining, loop_depth, state),
        }
    }
}

impl WarningChecker<'_> {
    fn analyze_body(&mut self, body: &[Stmt], params: &[hir::Param]) {
        let collect_mutes = contains_collect_in_stmts(body);
        self.analyze_w001_stmts(body, 0, collect_mutes);
        self.analyze_w003_stmts(body, 0, &HashSet::new());
        self.analyze_w002_block(body);
        self.analyze_w004_body(body, params);
        self.analyze_lambdas_in_stmts(body);
    }

    fn push(&mut self, warning: Warning) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    fn analyze_w001_stmts(&mut self, stmts: &[Stmt], loop_depth: usize, collect_mutes: bool) {
        walk_statements(
            stmts,
            loop_depth,
            &mut (),
            &mut |stmt, remaining, loop_depth, _| match stmt {
                Stmt::Let { name, init, .. } => {
                    if !collect_mutes
                        && loop_depth > 0
                        && is_reference_allocation(self.module, init)
                    {
                        let mut use_state = CandidateUse::default();
                        scan_candidate_stmts(remaining, name, &mut use_state);
                        if !use_state.escaped && !use_state.released {
                            self.push(Warning::new(
                                WarnCode::W001,
                                format!(
                                    "`{name}` is allocated in each loop iteration but neither escapes the iteration nor is released"
                                ),
                                init.pos.clone(),
                            ));
                        }
                        self.scan_allocation_children(init, loop_depth, collect_mutes);
                    } else {
                        self.scan_w001_expr(
                            init,
                            loop_depth,
                            collect_mutes,
                            AllocationSink::LocalBinding,
                        );
                    }
                }
                Stmt::Return { value, .. } => {
                    if let Some(value) = value {
                        self.scan_w001_expr(
                            value,
                            loop_depth,
                            collect_mutes,
                            AllocationSink::Escape,
                        );
                    }
                }
                // A thrown object leaves the iteration (compiler.md §115.2).
                Stmt::Throw { value, .. } => {
                    self.scan_w001_expr(value, loop_depth, collect_mutes, AllocationSink::Escape);
                }
                _ => {
                    for child in stmt.children() {
                        if let hir::HirChild::Expr(expr) = child {
                            self.scan_w001_expr(
                                expr,
                                loop_depth,
                                collect_mutes,
                                AllocationSink::Use,
                            );
                        }
                    }
                }
            },
        );
    }

    fn scan_w001_expr(
        &mut self,
        expr: &Expr,
        loop_depth: usize,
        collect_mutes: bool,
        sink: AllocationSink,
    ) {
        if is_reference_allocation(self.module, expr) {
            if !collect_mutes && loop_depth > 0 && sink == AllocationSink::Use {
                self.push(Warning::new(
                    WarnCode::W001,
                    "this reference-class allocation is repeated by the loop but neither escapes the iteration nor is released",
                    expr.pos.clone(),
                ));
            }
            self.scan_allocation_children(expr, loop_depth, collect_mutes);
            return;
        }

        match &expr.kind {
            ExprKind::Assign { target, value, .. } => {
                self.scan_w001_expr(target, loop_depth, collect_mutes, AllocationSink::Use);
                let value_sink = match target.kind {
                    ExprKind::Global(_)
                    | ExprKind::Field { .. }
                    | ExprKind::Index { .. }
                    | ExprKind::Local(..) => AllocationSink::Escape,
                    _ => AllocationSink::Use,
                };
                self.scan_w001_expr(value, loop_depth, collect_mutes, value_sink);
                return;
            }
            ExprKind::Cast(inner) => {
                self.scan_w001_expr(inner, loop_depth, collect_mutes, sink);
                return;
            }
            ExprKind::Call { callee, args } => {
                match callee {
                    Callee::Value(value) => self.scan_w001_expr(
                        value,
                        loop_depth,
                        collect_mutes,
                        AllocationSink::Escape,
                    ),
                    Callee::Method { recv, .. } => {
                        self.scan_w001_expr(recv, loop_depth, collect_mutes, AllocationSink::Escape)
                    }
                    _ => {}
                }
                let argument_sink = if matches!(callee, Callee::Ambient(AmbientFn::UnsafeDelete)) {
                    AllocationSink::Release
                } else {
                    AllocationSink::Escape
                };
                for arg in args {
                    self.scan_w001_expr(arg, loop_depth, collect_mutes, argument_sink);
                }
                return;
            }
            ExprKind::AsyncCall { callee, args }
            | ExprKind::AsyncHandleCreate { callee, args, .. } => {
                if let Some(receiver) = callee.receiver() {
                    self.scan_w001_expr(
                        receiver,
                        loop_depth,
                        collect_mutes,
                        AllocationSink::Escape,
                    );
                }
                for arg in args {
                    self.scan_w001_expr(arg, loop_depth, collect_mutes, AllocationSink::Escape);
                }
                return;
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    self.scan_w001_expr(arg, loop_depth, collect_mutes, AllocationSink::Escape);
                }
                return;
            }
            ExprKind::DescriptorLit { fields, .. } => {
                for value in fields.iter().flatten() {
                    self.scan_w001_expr(value, loop_depth, collect_mutes, AllocationSink::Escape);
                }
                return;
            }
            ExprKind::ArrayLit(elems) => {
                for elem in elems {
                    self.scan_w001_expr(elem, loop_depth, collect_mutes, AllocationSink::Escape);
                }
                return;
            }
            ExprKind::ArraySpreadLit(elems) => {
                for elem in elems {
                    self.scan_w001_expr(
                        &elem.expr,
                        loop_depth,
                        collect_mutes,
                        AllocationSink::Escape,
                    );
                }
                return;
            }
            ExprKind::Lambda { .. } => return,
            ExprKind::Yield(value) => {
                if let Some(value) = value {
                    self.scan_w001_expr(value, loop_depth, collect_mutes, AllocationSink::Escape);
                }
                return;
            }
            ExprKind::Cond { cond, then, els } => {
                self.scan_w001_expr(cond, loop_depth, collect_mutes, AllocationSink::Use);
                self.scan_w001_expr(then, loop_depth, collect_mutes, sink);
                self.scan_w001_expr(els, loop_depth, collect_mutes, sink);
                return;
            }
            _ => {}
        }
        for child in expr.children() {
            if let hir::HirChild::Expr(child) = child {
                self.scan_w001_expr(child, loop_depth, collect_mutes, AllocationSink::Use);
            }
        }
    }

    fn scan_allocation_children(&mut self, expr: &Expr, loop_depth: usize, collect_mutes: bool) {
        match &expr.kind {
            ExprKind::New { args, .. } | ExprKind::Call { args, .. } => {
                for arg in args {
                    self.scan_w001_expr(arg, loop_depth, collect_mutes, AllocationSink::Escape);
                }
            }
            ExprKind::AsyncCall { callee, args }
            | ExprKind::AsyncHandleCreate { callee, args, .. } => {
                if let Some(receiver) = callee.receiver() {
                    self.scan_w001_expr(
                        receiver,
                        loop_depth,
                        collect_mutes,
                        AllocationSink::Escape,
                    );
                }
                for arg in args {
                    self.scan_w001_expr(arg, loop_depth, collect_mutes, AllocationSink::Escape);
                }
            }
            ExprKind::AsyncHandleAwait(handle)
            | ExprKind::AsyncHandleTransfer { value: handle, .. } => {
                self.scan_w001_expr(handle, loop_depth, collect_mutes, AllocationSink::Use);
            }
            ExprKind::DescriptorLit { fields, .. } => {
                for value in fields.iter().flatten() {
                    self.scan_w001_expr(value, loop_depth, collect_mutes, AllocationSink::Escape);
                }
            }
            _ => {}
        }
    }

    fn analyze_w003_stmts(
        &mut self,
        stmts: &[Stmt],
        loop_depth: usize,
        inherited_fresh: &HashSet<String>,
    ) {
        let mut fresh = inherited_fresh.clone();
        walk_statements(
            stmts,
            loop_depth,
            &mut fresh,
            &mut |stmt, _, loop_depth, fresh| match stmt {
                Stmt::Let { name, init, .. } => {
                    self.scan_w003_expr(init, loop_depth, fresh);
                    fresh.remove(name);
                    if loop_depth > 0 && is_reference_new_allocation(self.module, init) {
                        fresh.insert(name.clone());
                    }
                }
                Stmt::Expr(expr) => {
                    self.scan_w003_expr(expr, loop_depth, fresh);
                    if let Some(name) = directly_reassigned_local(stmt) {
                        fresh.remove(name);
                    }
                }
                Stmt::ForOf { name, subject, .. } => {
                    self.scan_w003_expr(subject, loop_depth, fresh);
                    fresh.remove(name);
                }
                _ => {
                    for child in stmt.children() {
                        if let hir::HirChild::Expr(expr) = child {
                            self.scan_w003_expr(expr, loop_depth, fresh);
                        }
                    }
                }
            },
        );
    }

    fn scan_w003_expr(&mut self, expr: &Expr, loop_depth: usize, fresh: &HashSet<String>) {
        if loop_depth > 0 && callback_info_has_fresh_userdata(self.module, expr, fresh) {
            self.push(Warning::new(
                WarnCode::W003,
                "this callback-info aggregate registers freshly allocated userdata in each loop iteration",
                expr.pos.clone(),
            ));
        }

        if matches!(expr.kind, ExprKind::Lambda { .. }) {
            return;
        }
        for child in expr.children() {
            if let hir::HirChild::Expr(child) = child {
                self.scan_w003_expr(child, loop_depth, fresh);
            }
        }
    }

    fn analyze_w002_block(&mut self, stmts: &[Stmt]) {
        self.analyze_w002_sequence(stmts, HashSet::new());
    }

    /// The end of a `using` scope calls the hook of each binding
    /// (compiler.md §115.5 rule 5): a use of each binding name.
    fn analyze_w002_using(
        &mut self,
        bindings: &[hir::UsingBinding],
        body: &[Stmt],
        freed: &HashSet<String>,
    ) {
        let freed = self.analyze_w002_sequence(body, freed.clone());
        for binding in bindings.iter().rev() {
            self.warn_w002_direct_uses(&binding.hook(), &freed);
        }
    }

    /// `stmts` after statements that left `freed` freed. The body of a
    /// `using` scope continues the sequence of its block. Returns the
    /// names that stay freed at the end of the sequence.
    fn analyze_w002_sequence(
        &mut self,
        stmts: &[Stmt],
        mut freed: HashSet<String>,
    ) -> HashSet<String> {
        for stmt in stmts {
            self.warn_w002_direct_uses(stmt, &freed);

            if let Some(name) = directly_reassigned_local(stmt) {
                freed.remove(name);
            }
            if let Stmt::Let { name, .. } = stmt {
                freed.remove(name);
            }
            if let Some(name) = direct_free_local(stmt) {
                freed.insert(name.to_string());
            }

            match stmt {
                Stmt::If { then, els, .. } => {
                    self.analyze_w002_block(then);
                    if let Some(els) = els {
                        self.analyze_w002_block(els);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::For { body, .. }
                | Stmt::ForOf { body, .. }
                | Stmt::Block(body) => self.analyze_w002_block(body),
                Stmt::Switch { cases, .. } => {
                    for case in cases {
                        self.analyze_w002_block(&case.body);
                    }
                }
                Stmt::Using { bindings, body, .. } => {
                    self.analyze_w002_using(bindings, body, &freed);
                }
                Stmt::Let { .. }
                | Stmt::Expr(_)
                | Stmt::Return { .. }
                | Stmt::Break(_)
                | Stmt::Continue(_)
                | Stmt::Throw { .. }
                | Stmt::Try { .. } => {
                    for child in stmt.children() {
                        if let hir::HirChild::Stmt(child) = child {
                            self.analyze_w002_block(std::slice::from_ref(child));
                        }
                    }
                }
            }

            if matches!(
                stmt,
                Stmt::If { .. }
                    | Stmt::While { .. }
                    | Stmt::For { .. }
                    | Stmt::ForOf { .. }
                    | Stmt::Switch { .. }
                    | Stmt::Block(_)
                    | Stmt::Try { .. }
                    | Stmt::Using { .. }
            ) {
                // v1 carries no freed-state facts through a control-flow
                // join. The condition/discriminant above is still a direct
                // use in this block; statements after the join are not.
                freed.clear();
            }
        }
        freed
    }

    fn warn_w002_direct_uses(&mut self, stmt: &Stmt, freed: &HashSet<String>) {
        let for_init = match stmt {
            Stmt::For { init, .. } => init.as_deref(),
            _ => None,
        };
        for child in stmt.children() {
            match child {
                hir::HirChild::Expr(expr) => self.warn_w002_expr_uses(expr, freed),
                hir::HirChild::Stmt(child)
                    if for_init.is_some_and(|init| std::ptr::eq(init, child)) =>
                {
                    self.warn_w002_direct_uses(child, freed);
                }
                hir::HirChild::Stmt(_) => {}
            }
        }
    }

    /// Reports every direct use of a freed name inside one expression.
    ///
    /// The tail loop visits every expression child, so an arm that walks
    /// a child itself must return. An arm that walks a child and then
    /// falls through visits that child twice, which costs `2^n` on a
    /// chain of `n` such nodes (`specs/blocks/compiler.md` §113.2
    /// rule 2: each syntax node is visited one time). A node whose
    /// children all take the default walk needs no arm.
    fn warn_w002_expr_uses(&mut self, expr: &Expr, freed: &HashSet<String>) {
        match &expr.kind {
            ExprKind::Local(name, _) if freed.contains(name) => self.push(Warning::new(
                WarnCode::W002,
                format!(
                    "`{name}` is used after `Context.free({name})` without an intervening reassignment"
                ),
                expr.pos.clone(),
            )),
            ExprKind::Binary { op, left, right } => {
                self.warn_w002_expr_uses(left, freed);
                if !matches!(op, hir::BinOp::And | hir::BinOp::Or) {
                    self.warn_w002_expr_uses(right, freed);
                }
                return;
            }
            ExprKind::Assign { op, target, value, .. } => {
                if op.is_some() || !matches!(target.kind, ExprKind::Local(..)) {
                    self.warn_w002_expr_uses(target, freed);
                }
                self.warn_w002_expr_uses(value, freed);
                return;
            }
            ExprKind::Cond { cond, .. } => {
                self.warn_w002_expr_uses(cond, freed);
                return;
            }
            ExprKind::Lambda { .. } => return,
            _ => {}
        }
        for child in expr.children() {
            if let hir::HirChild::Expr(child) = child {
                self.warn_w002_expr_uses(child, freed);
            }
        }
    }

    fn analyze_w004_body(&mut self, body: &[Stmt], params: &[hir::Param]) {
        let mut bound_names = HashMap::new();
        for param in params {
            count_bound_name(&mut bound_names, &param.name);
        }
        count_w004_bound_names(body, &mut bound_names);
        let mut origins = HashMap::new();
        collect_synthesized_origins(body, &mut origins);

        let mut bindings = params
            .iter()
            .filter(|param| {
                bound_names.get(&param.name) == Some(&1) && is_value_type(self.module, &param.ty)
            })
            .map(|param| CopyBinding {
                name: param.name.clone(),
                origin: CopyOrigin::Parameter,
                field_writes: Vec::new(),
                read: false,
            })
            .collect::<Vec<_>>();
        collect_w004_local_bindings(self.module, body, &bound_names, &origins, &mut bindings);
        scan_w004_stmts(body, &mut bindings);

        for binding in bindings {
            if binding.read {
                continue;
            }
            let message = match binding.origin {
                CopyOrigin::Parameter => format!(
                    "`{}` is a value-type parameter copy that is written through but never read",
                    binding.name
                ),
                CopyOrigin::Place(place) => format!(
                    "`{}` is copied from `{place}`, then written through but never read",
                    binding.name
                ),
            };
            for pos in binding.field_writes {
                self.push(Warning::new(WarnCode::W004, message.clone(), pos));
            }
        }
    }

    fn analyze_lambdas_in_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            for child in stmt.children() {
                match child {
                    hir::HirChild::Expr(expr) => self.analyze_lambdas_in_expr(expr),
                    hir::HirChild::Stmt(stmt) => {
                        self.analyze_lambdas_in_stmts(std::slice::from_ref(stmt));
                    }
                }
            }
        }
    }

    fn analyze_lambdas_in_expr(&mut self, expr: &Expr) {
        if let ExprKind::Lambda { params, body, .. } = &expr.kind {
            for param in params {
                if let Some(default) = &param.default {
                    self.analyze_lambdas_in_expr(default);
                }
            }
            self.analyze_body(body, params);
            return;
        }
        for child in expr.children() {
            if let hir::HirChild::Expr(child) = child {
                self.analyze_lambdas_in_expr(child);
            }
        }
    }
}

#[derive(Debug)]
struct CopyBinding {
    name: String,
    origin: CopyOrigin,
    field_writes: Vec<Pos>,
    read: bool,
}

#[derive(Debug)]
enum CopyOrigin {
    Parameter,
    Place(String),
}

fn is_value_type(module: &hir::Module, ty: &crate::types::Type) -> bool {
    match ty {
        crate::types::Type::Class(class) => module
            .classes
            .get(class.0)
            .is_some_and(|definition| definition.is_value),
        crate::types::Type::FixedArray(_, _) => true,
        _ => false,
    }
}

fn count_bound_name(counts: &mut HashMap<String, usize>, name: &str) {
    if name.starts_with("[[") {
        return;
    }
    *counts.entry(name.to_string()).or_default() += 1;
}

fn count_w004_bound_names(stmts: &[Stmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, .. } => count_bound_name(counts, name),
            Stmt::ForOf { name, body, .. } => {
                count_bound_name(counts, name);
                count_w004_bound_names(body, counts);
            }
            Stmt::If { then, els, .. } => {
                count_w004_bound_names(then, counts);
                if let Some(els) = els {
                    count_w004_bound_names(els, counts);
                }
            }
            Stmt::While { body, .. } | Stmt::Block(body) => {
                count_w004_bound_names(body, counts);
            }
            Stmt::Using { body, .. } => count_w004_bound_names(body, counts),
            Stmt::For { init, body, .. } => {
                if let Some(init) = init {
                    count_w004_bound_names(std::slice::from_ref(init.as_ref()), counts);
                }
                count_w004_bound_names(body, counts);
            }
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    count_w004_bound_names(&case.body, counts);
                }
            }
            Stmt::Try {
                body,
                binding,
                handler,
                ..
            } => {
                count_w004_bound_names(body, counts);
                if let Some((binding, _)) = binding {
                    count_bound_name(counts, binding);
                }
                count_w004_bound_names(handler, counts);
            }
            Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Throw { .. } => {}
        }
    }
}

/// Maps every checker-synthesized local that W004 can name back to the
/// user's expression it holds: a `for…of` subject, a pattern source,
/// and a pattern element (warnings.md §2, rendering).
fn collect_synthesized_origins(stmts: &[Stmt], origins: &mut HashMap<String, String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, init, .. } if is_synthesized_source(name) => {
                origins.insert(name.clone(), render_source_expr(init, origins));
            }
            Stmt::ForOf {
                name,
                subject,
                kind,
                ..
            } if is_pattern_storage(name, ".element]]") => {
                origins.insert(name.clone(), for_of_element_source(subject, *kind, origins));
            }
            _ => {}
        }
        match stmt {
            Stmt::If { then, els, .. } => {
                collect_synthesized_origins(then, origins);
                if let Some(els) = els {
                    collect_synthesized_origins(els, origins);
                }
            }
            Stmt::While { body, .. } | Stmt::ForOf { body, .. } | Stmt::Block(body) => {
                collect_synthesized_origins(body, origins)
            }
            Stmt::Using { body, .. } => collect_synthesized_origins(body, origins),
            Stmt::For { init, body, .. } => {
                if let Some(init) = init {
                    collect_synthesized_origins(std::slice::from_ref(init.as_ref()), origins);
                }
                collect_synthesized_origins(body, origins);
            }
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_synthesized_origins(&case.body, origins);
                }
            }
            Stmt::Try { body, handler, .. } => {
                collect_synthesized_origins(body, origins);
                collect_synthesized_origins(handler, origins);
            }
            Stmt::Let { .. }
            | Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Throw { .. } => {}
        }
    }
}

fn is_synthesized_source(name: &str) -> bool {
    (name.starts_with("[[for.of#") && name.ends_with(".subject]]"))
        || is_pattern_storage(name, ".source]]")
}

fn is_pattern_storage(name: &str, suffix: &str) -> bool {
    name.starts_with("[[pattern#") && name.ends_with(suffix)
}

/// The local a field or index chain roots in, when it roots in one.
fn place_root_local(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Local(name, _) => Some(name),
        ExprKind::Field { obj, .. } | ExprKind::Index { obj, .. } => place_root_local(obj),
        _ => None,
    }
}

fn collect_w004_local_bindings(
    module: &hir::Module,
    stmts: &[Stmt],
    bound_names: &HashMap<String, usize>,
    origins: &HashMap<String, String>,
    bindings: &mut Vec<CopyBinding>,
) {
    for stmt in stmts {
        if let Stmt::Let { name, ty, init, .. } = stmt {
            if !name.starts_with("[[")
                && bound_names.get(name) == Some(&1)
                && is_value_type(module, ty)
            {
                // A name a parameter pattern binds is a copy the call made,
                // exactly as a value-typed parameter is.
                let origin = if place_root_local(init)
                    .is_some_and(|root| is_pattern_storage(root, ".parameter]]"))
                {
                    Some(CopyOrigin::Parameter)
                } else {
                    generator_for_of_subject_source(init, origins)
                        .or_else(|| copy_place_source(init, origins))
                        .map(CopyOrigin::Place)
                };
                if let Some(origin) = origin {
                    bindings.push(CopyBinding {
                        name: name.clone(),
                        origin,
                        field_writes: Vec::new(),
                        read: false,
                    });
                }
            }
        }
        if let Stmt::ForOf {
            name,
            ty,
            subject,
            kind,
            ..
        } = stmt
        {
            if !name.starts_with("[[")
                && bound_names.get(name) == Some(&1)
                && is_value_type(module, ty)
            {
                bindings.push(CopyBinding {
                    name: name.clone(),
                    origin: CopyOrigin::Place(for_of_subject_source(subject, *kind, origins)),
                    field_writes: Vec::new(),
                    read: false,
                });
            }
        }

        match stmt {
            Stmt::If { then, els, .. } => {
                collect_w004_local_bindings(module, then, bound_names, origins, bindings);
                if let Some(els) = els {
                    collect_w004_local_bindings(module, els, bound_names, origins, bindings);
                }
            }
            Stmt::While { body, .. } | Stmt::ForOf { body, .. } | Stmt::Block(body) => {
                collect_w004_local_bindings(module, body, bound_names, origins, bindings)
            }
            Stmt::Using { body, .. } => {
                collect_w004_local_bindings(module, body, bound_names, origins, bindings)
            }
            Stmt::For { init, body, .. } => {
                if let Some(init) = init {
                    collect_w004_local_bindings(
                        module,
                        std::slice::from_ref(init.as_ref()),
                        bound_names,
                        origins,
                        bindings,
                    );
                }
                collect_w004_local_bindings(module, body, bound_names, origins, bindings);
            }
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_w004_local_bindings(module, &case.body, bound_names, origins, bindings);
                }
            }
            Stmt::Try { body, handler, .. } => {
                collect_w004_local_bindings(module, body, bound_names, origins, bindings);
                collect_w004_local_bindings(module, handler, bound_names, origins, bindings);
            }
            Stmt::Let { .. }
            | Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Throw { .. } => {}
        }
    }
}

fn for_of_subject_source(
    subject: &Expr,
    kind: hir::ForOfKind,
    origins: &HashMap<String, String>,
) -> String {
    let source = render_source_expr(subject, origins);
    match kind {
        hir::ForOfKind::MapValues => format!("{source}.values(…)"),
        hir::ForOfKind::ArrayKeys => format!("{source}.keys(…)"),
        _ => source,
    }
}

/// The element a pattern reads: an indexed array element renders with
/// the `…` index, and every other element renders as its subject.
fn for_of_element_source(
    subject: &Expr,
    kind: hir::ForOfKind,
    origins: &HashMap<String, String>,
) -> String {
    let source = for_of_subject_source(subject, kind, origins);
    match kind {
        hir::ForOfKind::ArrayValues | hir::ForOfKind::FixedArrayValues => format!("{source}[…]"),
        _ => source,
    }
}

fn generator_for_of_subject_source(
    init: &Expr,
    origins: &HashMap<String, String>,
) -> Option<String> {
    let ExprKind::Field { obj, name } = &init.kind else {
        return None;
    };
    if name != "value" {
        return None;
    }
    let ExprKind::Local(step_name, _) = &obj.kind else {
        return None;
    };
    let subject_name = step_name.strip_suffix(".step]]")?;
    origins.get(&format!("{subject_name}.subject]]")).cloned()
}

fn copy_place_source(expr: &Expr, origins: &HashMap<String, String>) -> Option<String> {
    match &expr.kind {
        ExprKind::Local(..) | ExprKind::Global(_) => render_place_expr(expr, origins),
        ExprKind::Field { .. } if field_chain_has_copy_root(expr) => {
            render_place_expr(expr, origins)
        }
        ExprKind::Index { .. } => render_place_expr(expr, origins),
        _ => None,
    }
}

fn field_chain_has_copy_root(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Field { obj, .. } => field_chain_has_copy_root(obj),
        ExprKind::Local(..) | ExprKind::Global(_) | ExprKind::This | ExprKind::Index { .. } => true,
        _ => false,
    }
}

fn render_source_expr(expr: &Expr, origins: &HashMap<String, String>) -> String {
    render_place_expr(expr, origins)
        .or_else(|| render_call_expr(expr, origins))
        .unwrap_or_else(|| "…".to_string())
}

fn render_call_expr(expr: &Expr, origins: &HashMap<String, String>) -> Option<String> {
    let ExprKind::Call { callee, .. } = &expr.kind else {
        return None;
    };
    let callee = match callee {
        Callee::Func(name) => name.source_name(),
        Callee::Foreign(name) => name.clone(),
        Callee::Value(value) => render_place_expr(value, origins)?,
        Callee::Method { recv, name } => {
            format!(
                "{}.{}",
                render_source_expr(recv, origins),
                name.source_name()
            )
        }
        _ => return None,
    };
    Some(format!("{callee}(…)"))
}

/// A synthesized local renders as the expression it holds, and as `…`
/// when no origin is recorded for it; its own name never renders.
fn render_local(name: &str, origins: &HashMap<String, String>) -> String {
    match origins.get(name) {
        Some(origin) => origin.clone(),
        None if name.starts_with("[[") => "…".to_string(),
        None => name.to_string(),
    }
}

fn render_place_expr(expr: &Expr, origins: &HashMap<String, String>) -> Option<String> {
    match &expr.kind {
        ExprKind::Local(name, _) => Some(render_local(name, origins)),
        ExprKind::Global(name) => Some(name.source_name()),
        ExprKind::This => Some("this".to_string()),
        ExprKind::Field { obj, name } => {
            Some(format!("{}.{}", render_place_expr(obj, origins)?, name))
        }
        ExprKind::Index { obj, index, .. } => Some(format!(
            "{}[{}]",
            render_place_expr(obj, origins)?,
            render_index_expr(index, origins)
        )),
        _ => None,
    }
}

fn render_index_expr(expr: &Expr, origins: &HashMap<String, String>) -> String {
    match &expr.kind {
        ExprKind::Local(name, _) => render_local(name, origins),
        ExprKind::Global(name) => name.source_name(),
        ExprKind::This => "this".to_string(),
        ExprKind::Field { .. } | ExprKind::Index { .. } => {
            render_place_expr(expr, origins).unwrap_or_else(|| "…".to_string())
        }
        _ => "…".to_string(),
    }
}

fn scan_w004_stmts(stmts: &[Stmt], bindings: &mut [CopyBinding]) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr(expr) => {
                scan_w004_discarded_expr(expr, bindings);
                continue;
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(init) = init {
                    scan_w004_stmts(std::slice::from_ref(init.as_ref()), bindings);
                }
                if let Some(cond) = cond {
                    scan_w004_expr(cond, bindings);
                }
                if let Some(step) = step {
                    scan_w004_discarded_expr(step, bindings);
                }
                scan_w004_stmts(body, bindings);
                continue;
            }
            _ => {}
        }
        for child in stmt.children() {
            match child {
                hir::HirChild::Expr(expr) => scan_w004_expr(expr, bindings),
                hir::HirChild::Stmt(stmt) => {
                    scan_w004_stmts(std::slice::from_ref(stmt), bindings);
                }
            }
        }
    }
}

fn scan_w004_expr(expr: &Expr, bindings: &mut [CopyBinding]) {
    match &expr.kind {
        ExprKind::Local(name, _) => {
            mark_w004_read(bindings, name);
            return;
        }
        ExprKind::Assign { target, value, .. } => {
            scan_w004_assignment_target(target, &expr.pos, bindings);
            if let Some(name) = w004_assignment_local_root(target) {
                mark_w004_read(bindings, name);
            }
            scan_w004_expr(value, bindings);
            return;
        }
        ExprKind::Lambda { captures, .. } => {
            for capture in captures {
                mark_w004_read(bindings, &capture.name);
            }
            return;
        }
        _ => {}
    }

    for child in expr.children() {
        // Lambda is the only expression kind with statement children, and it
        // returns above after recording its captures.
        if let hir::HirChild::Expr(child) = child {
            scan_w004_expr(child, bindings);
        }
    }
}

fn scan_w004_discarded_expr(expr: &Expr, bindings: &mut [CopyBinding]) {
    if let ExprKind::Assign { target, value, .. } = &expr.kind {
        scan_w004_assignment_target(target, &expr.pos, bindings);
        scan_w004_expr(value, bindings);
    } else {
        scan_w004_expr(expr, bindings);
    }
}

fn scan_w004_assignment_target(target: &Expr, pos: &Pos, bindings: &mut [CopyBinding]) {
    let mut indices = Vec::new();
    let Some((root, has_field_or_index)) = w004_assignment_root(target, &mut indices) else {
        scan_w004_expr(target, bindings);
        return;
    };
    for index in indices {
        scan_w004_expr(index, bindings);
    }
    if let ExprKind::Local(name, _) = &root.kind {
        for binding in bindings.iter_mut().filter(|binding| binding.name == *name) {
            if has_field_or_index {
                binding.field_writes.push(pos.clone());
            }
        }
    }
}

fn w004_assignment_root<'a>(
    target: &'a Expr,
    indices: &mut Vec<&'a Expr>,
) -> Option<(&'a Expr, bool)> {
    match &target.kind {
        ExprKind::Local(..) | ExprKind::Global(_) | ExprKind::This => Some((target, false)),
        ExprKind::Field { obj, .. } => {
            let (root, _) = w004_assignment_root(obj, indices)?;
            Some((root, true))
        }
        ExprKind::Index { obj, index, .. } => {
            indices.push(index);
            let (root, _) = w004_assignment_root(obj, indices)?;
            Some((root, true))
        }
        _ => None,
    }
}

fn w004_assignment_local_root(target: &Expr) -> Option<&str> {
    match &target.kind {
        ExprKind::Local(name, _) => Some(name),
        ExprKind::Field { obj, .. } | ExprKind::Index { obj, .. } => {
            w004_assignment_local_root(obj)
        }
        _ => None,
    }
}

fn mark_w004_read(bindings: &mut [CopyBinding], name: &str) {
    for binding in bindings.iter_mut().filter(|binding| binding.name == name) {
        binding.read = true;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AllocationSink {
    Use,
    Escape,
    Release,
    LocalBinding,
}

fn is_reference_allocation(module: &hir::Module, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::New { class, .. } | ExprKind::DescriptorLit { class, .. } => module
            .classes
            .get(class.0)
            .is_some_and(|definition| !definition.is_value),
        ExprKind::Call {
            callee: Callee::Map(MapFn::New) | Callee::Set(SetFn::New),
            ..
        } => true,
        _ => false,
    }
}

fn is_reference_new_allocation(module: &hir::Module, expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::New { .. } | ExprKind::DescriptorLit { .. }
    ) && is_reference_allocation(module, expr)
}

fn is_callback_userdata_slot(ty: &crate::types::Type) -> bool {
    matches!(ty, crate::types::Type::Object)
        || matches!(
            ty,
            crate::types::Type::Nullable(inner) if **inner == crate::types::Type::Object
        )
}

fn callback_info_has_fresh_userdata(
    module: &hir::Module,
    expr: &Expr,
    fresh: &HashSet<String>,
) -> bool {
    let ExprKind::New { class, args } = &expr.kind else {
        return false;
    };
    let Some(definition) = module.classes.get(class.0) else {
        return false;
    };
    if !definition.is_boundary {
        return false;
    }

    for (index, field) in definition.fields.iter().enumerate() {
        if !matches!(
            field.foreign_provenance.as_ref(),
            Some(hir::ForeignTypeProvenance::Callback { .. })
        ) {
            continue;
        }

        let first_userdata = index + 1;
        let Some(first_field) = definition.fields.get(first_userdata) else {
            continue;
        };
        if !is_callback_userdata_slot(&first_field.ty) {
            continue;
        }
        if args
            .get(first_userdata)
            .is_some_and(|arg| is_fresh_userdata_argument(module, arg, fresh))
        {
            return true;
        }

        let second_userdata = index + 2;
        if definition
            .fields
            .get(second_userdata)
            .is_some_and(|field| is_callback_userdata_slot(&field.ty))
            && args
                .get(second_userdata)
                .is_some_and(|arg| is_fresh_userdata_argument(module, arg, fresh))
        {
            return true;
        }
    }
    false
}

fn is_fresh_userdata_argument(module: &hir::Module, expr: &Expr, fresh: &HashSet<String>) -> bool {
    // Conditional userdata is a recorded W003 candidate, not a decided case.
    match &expr.kind {
        ExprKind::Cast(inner) => is_fresh_userdata_argument(module, inner, fresh),
        ExprKind::Local(name, _) => fresh.contains(name),
        _ => is_reference_new_allocation(module, expr),
    }
}

fn contains_collect_in_stmts(stmts: &[Stmt]) -> bool {
    stmts.iter().any(contains_collect_in_stmt)
}

fn contains_collect_in_stmt(stmt: &Stmt) -> bool {
    stmt.children().into_iter().any(|child| match child {
        hir::HirChild::Expr(expr) => contains_collect_in_expr(expr),
        hir::HirChild::Stmt(stmt) => contains_collect_in_stmt(stmt),
    })
}

fn contains_collect_in_expr(expr: &Expr) -> bool {
    if matches!(expr.kind, ExprKind::Lambda { .. }) {
        return false;
    }
    matches!(
        &expr.kind,
        ExprKind::Call {
            callee: Callee::Ambient(AmbientFn::Collect),
            ..
        }
    ) || expr.children().into_iter().any(|child| match child {
        hir::HirChild::Expr(expr) => contains_collect_in_expr(expr),
        hir::HirChild::Stmt(stmt) => contains_collect_in_stmt(stmt),
    })
}

#[derive(Debug, Default)]
struct CandidateUse {
    escaped: bool,
    released: bool,
}

fn scan_candidate_stmts(stmts: &[Stmt], name: &str, state: &mut CandidateUse) {
    for stmt in stmts {
        match stmt {
            Stmt::Let {
                name: declared,
                init,
                ..
            } => {
                scan_candidate_expr(init, name, state);
                if declared == name {
                    return;
                }
                if value_is_candidate(init, name) {
                    state.escaped = true;
                }
                continue;
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    if value_is_candidate(value, name) {
                        state.escaped = true;
                    }
                    scan_candidate_expr(value, name, state);
                }
                continue;
            }
            Stmt::Throw { value, .. } => {
                if value_is_candidate(value, name) {
                    state.escaped = true;
                }
                scan_candidate_expr(value, name, state);
                continue;
            }
            // Each exit of the scope calls the hook of each binding
            // (compiler.md §115.5 rule 5).
            Stmt::Using { bindings, .. } => {
                for binding in bindings {
                    scan_candidate_stmts(&[binding.hook()], name, state);
                }
            }
            Stmt::Expr(_)
            | Stmt::If { .. }
            | Stmt::While { .. }
            | Stmt::For { .. }
            | Stmt::ForOf { .. }
            | Stmt::Switch { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Block(_)
            | Stmt::Try { .. } => {}
        }
        for child in stmt.children() {
            match child {
                hir::HirChild::Expr(expr) => scan_candidate_expr(expr, name, state),
                hir::HirChild::Stmt(stmt) => {
                    scan_candidate_stmts(std::slice::from_ref(stmt), name, state);
                }
            }
        }
    }
}

fn scan_candidate_expr(expr: &Expr, name: &str, state: &mut CandidateUse) {
    match &expr.kind {
        ExprKind::Call { callee, args } => {
            if matches!(callee, Callee::Ambient(AmbientFn::UnsafeDelete))
                && args
                    .first()
                    .is_some_and(|arg| value_is_candidate(arg, name))
            {
                state.released = true;
            } else if args.iter().any(|arg| value_is_candidate(arg, name)) {
                state.escaped = true;
            }
            if let Callee::Method { recv, .. } = callee {
                if value_is_candidate(recv, name) {
                    state.escaped = true;
                }
            }
        }
        ExprKind::AsyncCall { callee, args } | ExprKind::AsyncHandleCreate { callee, args, .. } => {
            state.escaped |= callee
                .receiver()
                .is_some_and(|receiver| value_is_candidate(receiver, name))
                || args.iter().any(|arg| value_is_candidate(arg, name));
        }
        ExprKind::Assign { target, value, .. } => {
            if value_is_candidate(value, name)
                && matches!(
                    target.kind,
                    ExprKind::Local(..)
                        | ExprKind::Global(_)
                        | ExprKind::Field { .. }
                        | ExprKind::Index { .. }
                )
            {
                state.escaped = true;
            }
            if matches!(&target.kind, ExprKind::Local(target_name, _) if target_name == name) {
                state.escaped = true;
            }
        }
        ExprKind::Lambda { captures, .. } => {
            if captures.iter().any(|capture| capture.name == name) {
                state.escaped = true;
            }
            return;
        }
        ExprKind::ArrayLit(elems) => {
            state.escaped |= elems.iter().any(|elem| value_is_candidate(elem, name));
        }
        ExprKind::ArraySpreadLit(elems) => {
            state.escaped |= elems
                .iter()
                .any(|element| value_is_candidate(&element.expr, name));
        }
        ExprKind::New { args, .. } => {
            state.escaped |= args.iter().any(|arg| value_is_candidate(arg, name));
        }
        ExprKind::DescriptorLit { fields, .. } => {
            state.escaped |= fields
                .iter()
                .flatten()
                .any(|value| value_is_candidate(value, name));
        }
        ExprKind::Yield(value) => {
            state.escaped |= value
                .as_deref()
                .is_some_and(|value| value_is_candidate(value, name));
        }
        _ => {}
    }
    for child in expr.children() {
        match child {
            hir::HirChild::Expr(expr) => scan_candidate_expr(expr, name, state),
            hir::HirChild::Stmt(stmt) => {
                scan_candidate_stmts(std::slice::from_ref(stmt), name, state);
            }
        }
    }
}

fn value_is_candidate(expr: &Expr, name: &str) -> bool {
    expr.flow_leaves()
        .any(|leaf| matches!(&leaf.kind, ExprKind::Local(local, _) if local == name))
}

fn directly_reassigned_local(stmt: &Stmt) -> Option<&str> {
    let Stmt::Expr(Expr {
        kind: ExprKind::Assign {
            op: None, target, ..
        },
        ..
    }) = stmt
    else {
        return None;
    };
    let ExprKind::Local(name, _) = &target.kind else {
        return None;
    };
    Some(name)
}

fn direct_free_local(stmt: &Stmt) -> Option<&str> {
    let Stmt::Expr(Expr {
        kind:
            ExprKind::Call {
                callee: Callee::Ambient(AmbientFn::UnsafeDelete),
                args,
            },
        ..
    }) = stmt
    else {
        return None;
    };
    let ExprKind::Local(name, _) = &args.first()?.kind else {
        return None;
    };
    Some(name)
}

#[cfg(test)]
mod tests;
