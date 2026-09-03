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
                    | ExprKind::Local(_) => AllocationSink::Escape,
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
        let mut freed = HashSet::new();
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
                _ => {
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
            ) {
                // v1 carries no freed-state facts through a control-flow
                // join. The condition/discriminant above is still a direct
                // use in this block; statements after the join are not.
                freed.clear();
            }
        }
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

    fn warn_w002_expr_uses(&mut self, expr: &Expr, freed: &HashSet<String>) {
        match &expr.kind {
            ExprKind::Local(name) if freed.contains(name) => self.push(Warning::new(
                WarnCode::W002,
                format!(
                    "`{name}` is used after `Context.free({name})` without an intervening reassignment"
                ),
                expr.pos.clone(),
            )),
            ExprKind::Unary { operand, .. } | ExprKind::Cast(operand) => {
                self.warn_w002_expr_uses(operand, freed);
            }
            ExprKind::Binary { op, left, right } => {
                self.warn_w002_expr_uses(left, freed);
                if !matches!(op, hir::BinOp::And | hir::BinOp::Or) {
                    self.warn_w002_expr_uses(right, freed);
                }
                return;
            }
            ExprKind::Assign { op, target, value } => {
                if op.is_some() || !matches!(target.kind, ExprKind::Local(_)) {
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
        let mut for_of_subjects = HashMap::new();
        collect_for_of_subject_origins(body, &mut for_of_subjects);

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
        collect_w004_local_bindings(
            self.module,
            body,
            &bound_names,
            &for_of_subjects,
            &mut bindings,
        );
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
            Stmt::Expr(_) | Stmt::Return { .. } | Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
}

fn collect_for_of_subject_origins(stmts: &[Stmt], origins: &mut HashMap<String, String>) {
    for stmt in stmts {
        if let Stmt::Let { name, init, .. } = stmt {
            if name.starts_with("[[for.of#") && name.ends_with(".subject]]") {
                origins.insert(name.clone(), render_source_expr(init));
            }
        }
        match stmt {
            Stmt::If { then, els, .. } => {
                collect_for_of_subject_origins(then, origins);
                if let Some(els) = els {
                    collect_for_of_subject_origins(els, origins);
                }
            }
            Stmt::While { body, .. } | Stmt::ForOf { body, .. } | Stmt::Block(body) => {
                collect_for_of_subject_origins(body, origins)
            }
            Stmt::For { init, body, .. } => {
                if let Some(init) = init {
                    collect_for_of_subject_origins(std::slice::from_ref(init.as_ref()), origins);
                }
                collect_for_of_subject_origins(body, origins);
            }
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_for_of_subject_origins(&case.body, origins);
                }
            }
            Stmt::Let { .. }
            | Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_) => {}
        }
    }
}

fn collect_w004_local_bindings(
    module: &hir::Module,
    stmts: &[Stmt],
    bound_names: &HashMap<String, usize>,
    for_of_subjects: &HashMap<String, String>,
    bindings: &mut Vec<CopyBinding>,
) {
    for stmt in stmts {
        if let Stmt::Let { name, ty, init, .. } = stmt {
            if !name.starts_with("[[")
                && bound_names.get(name) == Some(&1)
                && is_value_type(module, ty)
            {
                if let Some(place) = generator_for_of_subject_source(init, for_of_subjects)
                    .or_else(|| copy_place_source(init))
                {
                    bindings.push(CopyBinding {
                        name: name.clone(),
                        origin: CopyOrigin::Place(place),
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
                    origin: CopyOrigin::Place(for_of_subject_source(
                        subject,
                        *kind,
                        for_of_subjects,
                    )),
                    field_writes: Vec::new(),
                    read: false,
                });
            }
        }

        match stmt {
            Stmt::If { then, els, .. } => {
                collect_w004_local_bindings(module, then, bound_names, for_of_subjects, bindings);
                if let Some(els) = els {
                    collect_w004_local_bindings(
                        module,
                        els,
                        bound_names,
                        for_of_subjects,
                        bindings,
                    );
                }
            }
            Stmt::While { body, .. } | Stmt::ForOf { body, .. } | Stmt::Block(body) => {
                collect_w004_local_bindings(module, body, bound_names, for_of_subjects, bindings)
            }
            Stmt::For { init, body, .. } => {
                if let Some(init) = init {
                    collect_w004_local_bindings(
                        module,
                        std::slice::from_ref(init.as_ref()),
                        bound_names,
                        for_of_subjects,
                        bindings,
                    );
                }
                collect_w004_local_bindings(module, body, bound_names, for_of_subjects, bindings);
            }
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_w004_local_bindings(
                        module,
                        &case.body,
                        bound_names,
                        for_of_subjects,
                        bindings,
                    );
                }
            }
            Stmt::Let { .. }
            | Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_) => {}
        }
    }
}

fn for_of_subject_source(
    subject: &Expr,
    kind: hir::ForOfKind,
    for_of_subjects: &HashMap<String, String>,
) -> String {
    let source = if let ExprKind::Local(subject_name) = &subject.kind {
        if let Some(origin) = for_of_subjects.get(subject_name) {
            origin.clone()
        } else {
            render_source_expr(subject)
        }
    } else {
        render_source_expr(subject)
    };
    match kind {
        hir::ForOfKind::MapValues => format!("{source}.values(…)"),
        hir::ForOfKind::ArrayKeys => format!("{source}.keys(…)"),
        _ => source,
    }
}

fn generator_for_of_subject_source(
    init: &Expr,
    for_of_subjects: &HashMap<String, String>,
) -> Option<String> {
    let ExprKind::Field { obj, name } = &init.kind else {
        return None;
    };
    if name != "value" {
        return None;
    }
    let ExprKind::Local(step_name) = &obj.kind else {
        return None;
    };
    let subject_name = step_name.strip_suffix(".step]]")?;
    for_of_subjects
        .get(&format!("{subject_name}.subject]]"))
        .cloned()
}

fn copy_place_source(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Local(name) | ExprKind::Global(name) => Some(name.clone()),
        ExprKind::Field { .. } if field_chain_has_copy_root(expr) => render_place_expr(expr),
        ExprKind::Index { .. } => render_place_expr(expr),
        _ => None,
    }
}

fn field_chain_has_copy_root(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Field { obj, .. } => field_chain_has_copy_root(obj),
        ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::This | ExprKind::Index { .. } => true,
        _ => false,
    }
}

fn render_source_expr(expr: &Expr) -> String {
    render_place_expr(expr)
        .or_else(|| render_call_expr(expr))
        .unwrap_or_else(|| "…".to_string())
}

fn render_call_expr(expr: &Expr) -> Option<String> {
    let ExprKind::Call { callee, .. } = &expr.kind else {
        return None;
    };
    let callee = match callee {
        Callee::Func(name) | Callee::Foreign(name) => name.clone(),
        Callee::Value(value) => render_place_expr(value)?,
        Callee::Method { recv, name } => format!("{}.{}", render_source_expr(recv), name),
        _ => return None,
    };
    Some(format!("{callee}(…)"))
}

fn render_place_expr(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Local(name) | ExprKind::Global(name) => Some(name.clone()),
        ExprKind::This => Some("this".to_string()),
        ExprKind::Field { obj, name } => Some(format!("{}.{}", render_place_expr(obj)?, name)),
        ExprKind::Index { obj, index, .. } => Some(format!(
            "{}[{}]",
            render_place_expr(obj)?,
            render_index_expr(index)
        )),
        _ => None,
    }
}

fn render_index_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Local(name) | ExprKind::Global(name) => name.clone(),
        ExprKind::This => "this".to_string(),
        ExprKind::Field { .. } | ExprKind::Index { .. } => {
            render_place_expr(expr).unwrap_or_else(|| "…".to_string())
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
        ExprKind::Local(name) => {
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
    if let ExprKind::Local(name) = &root.kind {
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
        ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::This => Some((target, false)),
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
        ExprKind::Local(name) => Some(name),
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
        ExprKind::Local(name) => fresh.contains(name),
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
            _ => {}
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
                    ExprKind::Local(_)
                        | ExprKind::Global(_)
                        | ExprKind::Field { .. }
                        | ExprKind::Index { .. }
                )
            {
                state.escaped = true;
            }
            if matches!(&target.kind, ExprKind::Local(target_name) if target_name == name) {
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
        .any(|leaf| matches!(&leaf.kind, ExprKind::Local(local) if local == name))
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
    let ExprKind::Local(name) = &target.kind else {
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
    let ExprKind::Local(name) = &args.first()?.kind else {
        return None;
    };
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check_program, SourceFile};

    fn warnings(source: &str) -> Vec<Warning> {
        let module = check_program(&[SourceFile::new("test.ts", source)])
            .expect("warning fixture must be accepted");
        check_warnings(&module)
    }

    fn callback_warnings(source: &str) -> Vec<Warning> {
        let mirror = "\
// @subscript-c-header include=\"warning-fixture.h\"
// @subscript-c-callback typedef=\"FixtureCallback\"
type FixtureCallback = (message: string, userdata1: object | null, userdata2: object | null) => void;
declare class FixtureCallbackInfo {
  callback: FixtureCallback;
  userdata1: object | null;
  userdata2: object | null;
  constructor(callback: FixtureCallback, userdata1: object | null, userdata2: object | null);
}
declare function fixtureRegister(info: FixtureCallbackInfo): void;
";
        let module = check_program(&[
            SourceFile::ambient("warning-fixture.generated.d.ts", mirror),
            SourceFile::new("test.ts", source),
        ])
        .expect("callback warning fixture must be accepted");
        check_warnings(&module)
    }

    #[test]
    fn every_warning_code_has_a_single_line_explanation() {
        for code in WarnCode::ALL {
            assert!(!code.explanation().is_empty(), "{code}");
            assert!(!code.explanation().contains('\n'), "{code}");
        }
    }

    #[test]
    fn warning_constructor_preserves_fields() {
        let warning = Warning::new(WarnCode::W001, "message", Pos::new("test.ts", 2, 3));
        assert_eq!(warning.code.as_str(), "W001");
        assert_eq!(warning.message, "message");
        assert_eq!(warning.pos, Pos::new("test.ts", 2, 3));
    }

    #[test]
    fn loop_allocation_warns_but_one_shot_and_collect_do_not() {
        let source =
            "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
                      export function main(): void {\n\
                      \x20 for (let i: i32 = 0; i < 2; i += 1) {\n\
                      \x20   const token: Token = new Token(i);\n\
                      \x20   print(`${token.value}`);\n\
                      \x20 }\n\
                      }\n";
        let result = warnings(source);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].code, WarnCode::W001);
        assert_eq!(result[0].pos.line, 4);

        let one_shot =
            "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
             export function main(): void {\n\
             \x20 const token: Token = new Token(1);\n\
             \x20 print(`${token.value}`);\n\
             }\n";
        assert!(warnings(one_shot).is_empty());

        let with_collect = source.replace(
            "  print(`${token.value}`);\n",
            "  print(`${token.value}`);\n  Context.collect();\n",
        );
        assert!(warnings(&with_collect).is_empty());
    }

    #[test]
    fn use_after_free_warns_until_reassignment() {
        let source =
            "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
                      export function main(): void {\n\
                      \x20 let token: Token = new Token(1);\n\
                      \x20 Context.free(token);\n\
                      \x20 print(`${token.value}`);\n\
                      \x20 token = new Token(2);\n\
                      \x20 print(`${token.value}`);\n\
                      }\n";
        let result = warnings(source);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].code, WarnCode::W002);
        assert_eq!(result[0].pos.line, 5);
    }

    #[test]
    fn use_after_free_state_does_not_cross_a_branch_join() {
        let source =
            "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
             export function main(): void {\n\
             \x20 let token: Token = new Token(1);\n\
             \x20 Context.free(token);\n\
             \x20 if (token.value === 1) {\n\
             \x20   token = new Token(2);\n\
             \x20 }\n\
             \x20 print(`${token.value}`);\n\
             }\n";
        let result = warnings(source);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].code, WarnCode::W002);
        assert_eq!(result[0].pos.line, 5);
    }

    #[test]
    fn fresh_callback_userdata_warns_for_local_and_direct_new_inside_loop_only() {
        let source =
            "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
             export function main(): void {\n\
             \x20 for (let i: i32 = 0; i < 2; i += 1) {\n\
             \x20   const local: Token = new Token(i);\n\
             \x20   fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, local, null));\n\
             \x20   fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, new Token(i), null));\n\
             \x20 }\n\
             \x20 const outside: Token = new Token(3);\n\
             \x20 fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, outside, null));\n\
             }\n";
        let result = callback_warnings(source);
        assert_eq!(
            result
                .iter()
                .filter(|warning| warning.code == WarnCode::W003)
                .count(),
            2
        );
        assert!(
            result
                .iter()
                .all(|warning| warning.code == WarnCode::W003),
            "fresh userdata escapes through the callback aggregate, so W001 must stay muted: {result:?}"
        );
        assert_eq!(
            result
                .iter()
                .map(|warning| warning.pos.line)
                .collect::<Vec<_>>(),
            [5, 6]
        );
    }

    #[test]
    fn conditional_fresh_callback_userdata_does_not_warn() {
        let source =
            "class LogSink { value: i32; constructor(value: i32) { this.value = value; } }\n\
             export function main(): void {\n\
             \x20 const keep: LogSink = new LogSink(0);\n\
             \x20 for (let i: i32 = 0; i < 2; i += 1) {\n\
             \x20   fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, i > 1 ? new LogSink(i) : keep, null));\n\
             \x20 }\n\
             }\n";
        assert!(callback_warnings(source)
            .iter()
            .all(|warning| warning.code != WarnCode::W003));
    }

    const W004_TYPES: &str = "\
@CStruct
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
  read(): f32 { return this.x; }
}
class State {
  point: Point;
  constructor(point: Point) { this.point = point; }
}
";

    fn w004_warnings(body: &str) -> Vec<Warning> {
        warnings(&format!("{W004_TYPES}\n{body}\n"))
    }

    fn only_w004(result: &[Warning]) -> Vec<&Warning> {
        result
            .iter()
            .filter(|warning| warning.code == WarnCode::W004)
            .collect()
    }

    #[test]
    fn write_only_value_parameter_warns() {
        let result = w004_warnings(
            "function mutate(point: Point): void { point.x = 2.0; }\n\
             export function main(): void { mutate(new Point(1.0)); }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("parameter copy"));
    }

    #[test]
    fn write_only_local_copied_from_field_warns() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const state: State = new State(new Point(1.0));\n\
               const alias: Point = state.point;\n\
               alias.x = 2.0;\n\
               Context.free(state);\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`state.point`"));
    }

    #[test]
    fn write_only_local_copied_from_local_warns() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const original: Point = new Point(1.0);\n\
               const alias: Point = original;\n\
               alias.x = 2.0;\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`original`"));
    }

    #[test]
    fn write_only_local_copied_from_index_warns() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const points: Point[] = [new Point(1.0)];\n\
               const alias: Point = points[0];\n\
               alias.x = 2.0;\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`points[…]`"));
    }

    #[test]
    fn parameter_shadowed_by_nested_copy_binding_does_not_warn() {
        let result = w004_warnings(
            "let source: Point = new Point(1.0);\n\
             function mutate(point: Point): void {\n\
               {\n\
                 const point: Point = source;\n\
                 point.x = 2.0;\n\
               }\n\
             }\n\
             export function main(): void { mutate(source); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn same_name_copy_bindings_in_sibling_blocks_do_not_warn() {
        let result = w004_warnings(
            "let source: Point = new Point(1.0);\n\
             export function main(): void {\n\
               { const copy: Point = source; copy.x = 2.0; }\n\
               { const copy: Point = source; copy.x = 3.0; }\n\
             }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn single_unshadowed_copy_binding_still_warns() {
        let result = w004_warnings(
            "let source: Point = new Point(1.0);\n\
             export function main(): void {\n\
               { const copy: Point = source; copy.x = 2.0; }\n\
             }",
        );
        assert_eq!(only_w004(&result).len(), 1, "{result:?}");
    }

    #[test]
    fn write_only_compound_assignment_warns() {
        let result = w004_warnings(
            "function mutate(point: Point): void { point.x += 1.0; }\n\
             export function main(): void { mutate(new Point(1.0)); }",
        );
        assert_eq!(only_w004(&result).len(), 1, "{result:?}");
    }

    #[test]
    fn write_only_for_step_assignment_warns() {
        let result = w004_warnings(
            "function mutate(copy: Point): void {\n\
               for (; false; copy.x += 1.0) {}\n\
             }\n\
             export function main(): void { mutate(new Point(1.0)); }",
        );
        assert_eq!(only_w004(&result).len(), 1, "{result:?}");
    }

    #[test]
    fn read_in_for_body_mutes_for_step_assignment() {
        let result = w004_warnings(
            "function mutate(copy: Point): void {\n\
               for (; false; copy.x += 1.0) { print(`${copy.x}`); }\n\
             }\n\
             export function main(): void { mutate(new Point(1.0)); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn write_only_fixed_array_parameter_warns() {
        let result = w004_warnings(
            "function mutate(value: FixedArray<i32, 3>): void { value[0] = 777; }\n\
             export function main(): void {\n\
               const value: FixedArray<i32, 3> = [1, 2, 3];\n\
               mutate(value);\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`value`"));
        assert!(warnings[0].message.contains("parameter copy"));
    }

    #[test]
    fn write_only_value_parameter_in_lambda_warns() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const points: Map<i32, Point> = new Map<i32, Point>();\n\
               points.set(1, new Point(1.0));\n\
               points.forEach((point: Point, key: i32): void => {\n\
                 point.x = 2.0;\n\
               });\n\
             }",
        );
        assert_eq!(only_w004(&result).len(), 1, "{result:?}");
    }

    #[test]
    fn lambda_capture_mutes_outer_copy_binding() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const state: State = new State(new Point(1.0));\n\
               const copy: Point = state.point;\n\
               copy.x = 2.0;\n\
               const values: i32[] = [1];\n\
               values.forEach((value: i32): void => { print(`${copy.x}:${value}`); });\n\
               Context.free(state);\n\
             }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn write_only_value_for_of_binding_warns() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const points: Point[] = [new Point(1.0)];\n\
               for (const point of points) { point.x = 2.0; }\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`points`"), "{result:?}");
    }

    #[test]
    fn read_after_write_mutes_value_for_of_binding() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const points: Point[] = [new Point(1.0)];\n\
               for (const point of points) {\n\
                 point.x = 2.0;\n\
                 print(`${point.x}`);\n\
               }\n\
             }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn value_for_of_call_subject_renders_the_callee() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const scores: Map<i32, Point> = new Map<i32, Point>();\n\
               scores.set(1, new Point(1.0));\n\
               for (const value of scores.values()) { value.x = 2.0; }\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(
            warnings[0].message.contains("`scores.values(…)`"),
            "{result:?}"
        );
    }

    #[test]
    fn fixed_array_for_of_synthetic_subject_is_not_a_binding() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const points: FixedArray<Point, 1> = [new Point(1.0)];\n\
               for (const point of points) { point.x = 2.0; }\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`point`"), "{result:?}");
        assert!(!warnings[0].message.contains("[["), "{result:?}");
    }

    #[test]
    fn local_copied_from_field_chain_rooted_in_index_warns() {
        let result = w004_warnings(
            "@CStruct\n\
             class Outer {\n\
               inner: Point;\n\
               constructor(inner: Point) { this.inner = inner; }\n\
             }\n\
             export function main(): void {\n\
               const values: Outer[] = [new Outer(new Point(1.0))];\n\
               const copy: Point = values[0].inner;\n\
               copy.x = 2.0;\n\
             }",
        );
        let warnings = only_w004(&result);
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`values[…].inner`"));
    }

    #[test]
    fn field_read_after_write_mutes_w004() {
        let result = w004_warnings(
            "function mutate(point: Point): f32 { point.x = 2.0; return point.x; }\n\
             export function main(): void { print(`${mutate(new Point(1.0))}`); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn two_unread_field_writes_produce_two_warnings() {
        let body = "function mutate(point: Point): void {\n\
                      point.x = 2.0;\n\
                      point.x = 3.0;\n\
                    }\n\
                    export function main(): void { mutate(new Point(1.0)); }";
        let result = w004_warnings(body);
        let warnings = only_w004(&result);
        let first_line = u32::try_from(W004_TYPES.lines().count())
            .expect("test fixture line count fits u32")
            + 3;
        assert_eq!(warnings.len(), 2, "{result:?}");
        assert_eq!(
            warnings
                .iter()
                .map(|warning| warning.pos.line)
                .collect::<Vec<_>>(),
            [first_line, first_line + 1]
        );
    }

    #[test]
    fn fixed_array_local_index_read_after_write_mutes_w004() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const original: FixedArray<i32, 3> = [1, 2, 3];\n\
               const copy: FixedArray<i32, 3> = original;\n\
               copy[0] = 777;\n\
               print(`${copy[0]}`);\n\
             }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn assignment_in_value_position_mutes_w004() {
        let result = w004_warnings(
            "function mutate(point: Point): void {\n\
               const value: f32 = (point.x = 2.0);\n\
               print(`${value}`);\n\
             }\n\
             export function main(): void { mutate(new Point(1.0)); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn method_call_on_binding_mutes_w004() {
        let result = w004_warnings(
            "function mutate(point: Point): f32 { point.x = 2.0; return point.read(); }\n\
             export function main(): void { print(`${mutate(new Point(1.0))}`); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn passing_binding_as_argument_mutes_w004() {
        let result = w004_warnings(
            "function consume(point: Point): void { print(`${point.x}`); }\n\
             function mutate(point: Point): void { point.x = 2.0; consume(point); }\n\
             export function main(): void { mutate(new Point(1.0)); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn returning_binding_mutes_w004() {
        let result = w004_warnings(
            "function mutate(point: Point): Point { point.x = 2.0; return point; }\n\
             export function main(): void { const result: Point = mutate(new Point(1.0)); print(`${result.x}`); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn using_binding_as_assignment_value_mutes_w004() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const state: State = new State(new Point(1.0));\n\
               const alias: Point = state.point;\n\
               alias.x = 2.0;\n\
               state.point = alias;\n\
               Context.free(state);\n\
             }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn read_before_write_inside_loop_mutes_w004() {
        let result = w004_warnings(
            "function mutate(point: Point): void {\n\
               for (let i: i32 = 0; i < 2; i += 1) {\n\
                 print(`${point.x}`);\n\
                 point.x = i as f32;\n\
               }\n\
             }\n\
             export function main(): void { mutate(new Point(1.0)); }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn new_initializer_is_not_a_copy_binding() {
        let result = w004_warnings(
            "export function main(): void {\n\
               const point: Point = new Point(1.0);\n\
               point.x = 2.0;\n\
             }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn call_initializer_is_not_a_copy_binding() {
        let result = w004_warnings(
            "function make(): Point { return new Point(1.0); }\n\
             export function main(): void {\n\
               const point: Point = make();\n\
               point.x = 2.0;\n\
             }",
        );
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }

    #[test]
    fn value_class_this_write_does_not_warn() {
        let source = "\
@CStruct
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
  set(other: Point): void {
    this.x = 2.0;
    other.x = 3.0;
  }
}
export function main(): void { const point: Point = new Point(1.0); point.set(new Point(2.0)); }
";
        let result = warnings(source);
        let warnings = only_w004(&result);
        let other_write_line = u32::try_from(
            source
                .lines()
                .position(|line| line.contains("other.x"))
                .expect("test source contains the parameter write")
                + 1,
        )
        .expect("test source line fits u32");
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`other`"), "{result:?}");
        assert_eq!(warnings[0].pos.line, other_write_line);
    }

    #[test]
    fn write_only_value_parameter_in_constructor_warns() {
        let source = "\
@CStruct
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
}
@CStruct
class Holder {
  point: Point;
  constructor(other: Point) {
    this.point = new Point(0.0);
    other.x = 3.0;
  }
}
export function main(): void { const holder: Holder = new Holder(new Point(1.0)); }
";
        let result = warnings(source);
        let warnings = only_w004(&result);
        let other_write_line = u32::try_from(
            source
                .lines()
                .position(|line| line.contains("other.x"))
                .expect("test source contains the constructor parameter write")
                + 1,
        )
        .expect("test source line fits u32");
        assert_eq!(warnings.len(), 1, "{result:?}");
        assert!(warnings[0].message.contains("`other`"), "{result:?}");
        assert_eq!(warnings[0].pos.line, other_write_line);
    }

    #[test]
    fn reference_class_parameter_does_not_warn() {
        let source = "\
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
}
function mutate(point: Point): void { point.x = 2.0; }
export function main(): void { const point: Point = new Point(1.0); mutate(point); Context.free(point); }
";
        let result = warnings(source);
        assert!(only_w004(&result).is_empty(), "{result:?}");
    }
}
