//! Warnings computed from an accepted, checked HIR module.

use std::collections::HashSet;
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
}

impl WarnCode {
    /// Every stable warning code, in numeric order.
    pub const ALL: [Self; 3] = [Self::W001, Self::W002, Self::W003];

    /// The stable textual form of the code, e.g. `"W001"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WarnCode::W001 => "W001",
            WarnCode::W002 => "W002",
            WarnCode::W003 => "W003",
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

    for global in &module.globals {
        checker.analyze_lambdas_in_expr(&global.init);
    }
    for class in &module.classes {
        for field in &class.fields {
            if let Some(init) = &field.init {
                checker.analyze_lambdas_in_expr(init);
            }
        }
        if let Some(ctor) = &class.ctor {
            checker.analyze_function(ctor);
        }
        for method in &class.methods {
            checker.analyze_function(method);
        }
    }
    for function in &module.functions {
        checker.analyze_function(function);
    }
    checker.analyze_body(&module.top_level);

    checker.warnings
}

struct WarningChecker<'m> {
    module: &'m hir::Module,
    warnings: Vec<Warning>,
}

impl WarningChecker<'_> {
    fn analyze_function(&mut self, function: &hir::Function) {
        for param in &function.params {
            if let Some(default) = &param.default {
                self.analyze_lambdas_in_expr(default);
            }
        }
        self.analyze_body(&function.body);
    }

    fn analyze_body(&mut self, body: &[Stmt]) {
        let collect_mutes = contains_collect_in_stmts(body);
        self.analyze_w001_stmts(body, 0, collect_mutes);
        self.analyze_w003_stmts(body, 0, &HashSet::new());
        self.analyze_w002_block(body);
        self.analyze_lambdas_in_stmts(body);
    }

    fn push(&mut self, warning: Warning) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    fn analyze_w001_stmts(&mut self, stmts: &[Stmt], loop_depth: usize, collect_mutes: bool) {
        for (index, stmt) in stmts.iter().enumerate() {
            match stmt {
                Stmt::Let { name, init, .. } => {
                    if !collect_mutes
                        && loop_depth > 0
                        && is_reference_allocation(self.module, init)
                    {
                        let mut use_state = CandidateUse::default();
                        scan_candidate_stmts(&stmts[index + 1..], name, &mut use_state);
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
                Stmt::Expr(expr) => {
                    self.scan_w001_expr(expr, loop_depth, collect_mutes, AllocationSink::Use)
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
                Stmt::While { cond, body, .. } => {
                    self.scan_w001_expr(cond, loop_depth, collect_mutes, AllocationSink::Use);
                    self.analyze_w001_stmts(body, loop_depth + 1, collect_mutes);
                }
                Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                    ..
                } => {
                    if let Some(init) = init {
                        self.analyze_w001_stmts(
                            std::slice::from_ref(init.as_ref()),
                            loop_depth,
                            collect_mutes,
                        );
                    }
                    if let Some(cond) = cond {
                        self.scan_w001_expr(cond, loop_depth, collect_mutes, AllocationSink::Use);
                    }
                    if let Some(step) = step {
                        self.scan_w001_expr(step, loop_depth, collect_mutes, AllocationSink::Use);
                    }
                    self.analyze_w001_stmts(body, loop_depth + 1, collect_mutes);
                }
                Stmt::ForOf { subject, body, .. } => {
                    self.scan_w001_expr(subject, loop_depth, collect_mutes, AllocationSink::Use);
                    self.analyze_w001_stmts(body, loop_depth + 1, collect_mutes);
                }
                Stmt::If {
                    cond, then, els, ..
                } => {
                    self.scan_w001_expr(cond, loop_depth, collect_mutes, AllocationSink::Use);
                    self.analyze_w001_stmts(then, loop_depth, collect_mutes);
                    if let Some(els) = els {
                        self.analyze_w001_stmts(els, loop_depth, collect_mutes);
                    }
                }
                Stmt::Switch { disc, cases, .. } => {
                    self.scan_w001_expr(disc, loop_depth, collect_mutes, AllocationSink::Use);
                    for case in cases {
                        if let Some(test) = &case.test {
                            self.scan_w001_expr(
                                test,
                                loop_depth,
                                collect_mutes,
                                AllocationSink::Use,
                            );
                        }
                        self.analyze_w001_stmts(&case.body, loop_depth, collect_mutes);
                    }
                }
                Stmt::Block(body) => {
                    self.analyze_w001_stmts(body, loop_depth, collect_mutes);
                }
                _ => {
                    for child in stmt.children() {
                        match child {
                            hir::HirChild::Expr(expr) => self.scan_w001_expr(
                                expr,
                                loop_depth,
                                collect_mutes,
                                AllocationSink::Use,
                            ),
                            hir::HirChild::Stmt(stmt) => self.analyze_w001_stmts(
                                std::slice::from_ref(stmt),
                                loop_depth,
                                collect_mutes,
                            ),
                        }
                    }
                }
            }
        }
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
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, init, .. } => {
                    self.scan_w003_expr(init, loop_depth, &fresh);
                    fresh.remove(name);
                    if loop_depth > 0 && is_reference_new_allocation(self.module, init) {
                        fresh.insert(name.clone());
                    }
                }
                Stmt::Expr(expr) => {
                    self.scan_w003_expr(expr, loop_depth, &fresh);
                    if let Some(name) = directly_reassigned_local(stmt) {
                        fresh.remove(name);
                    }
                }
                Stmt::While { cond, body, .. } => {
                    self.scan_w003_expr(cond, loop_depth, &fresh);
                    self.analyze_w003_stmts(body, loop_depth + 1, &fresh);
                }
                Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                    ..
                } => {
                    let mut body_fresh = fresh.clone();
                    if let Some(init) = init {
                        self.scan_w003_stmt_header(init, loop_depth, &fresh);
                        if let Stmt::Let { name, init, .. } = init.as_ref() {
                            body_fresh.remove(name);
                            if loop_depth > 0 && is_reference_new_allocation(self.module, init) {
                                body_fresh.insert(name.clone());
                            }
                        }
                    }
                    if let Some(cond) = cond {
                        self.scan_w003_expr(cond, loop_depth, &body_fresh);
                    }
                    if let Some(step) = step {
                        self.scan_w003_expr(step, loop_depth, &body_fresh);
                    }
                    self.analyze_w003_stmts(body, loop_depth + 1, &body_fresh);
                }
                Stmt::ForOf {
                    name,
                    subject,
                    body,
                    ..
                } => {
                    self.scan_w003_expr(subject, loop_depth, &fresh);
                    let mut body_fresh = fresh.clone();
                    body_fresh.remove(name);
                    self.analyze_w003_stmts(body, loop_depth + 1, &body_fresh);
                }
                Stmt::If {
                    cond, then, els, ..
                } => {
                    self.scan_w003_expr(cond, loop_depth, &fresh);
                    self.analyze_w003_stmts(then, loop_depth, &fresh);
                    if let Some(els) = els {
                        self.analyze_w003_stmts(els, loop_depth, &fresh);
                    }
                }
                Stmt::Switch { disc, cases, .. } => {
                    self.scan_w003_expr(disc, loop_depth, &fresh);
                    for case in cases {
                        if let Some(test) = &case.test {
                            self.scan_w003_expr(test, loop_depth, &fresh);
                        }
                        self.analyze_w003_stmts(&case.body, loop_depth, &fresh);
                    }
                }
                Stmt::Block(body) => self.analyze_w003_stmts(body, loop_depth, &fresh),
                _ => {
                    for child in stmt.children() {
                        match child {
                            hir::HirChild::Expr(expr) => {
                                self.scan_w003_expr(expr, loop_depth, &fresh);
                            }
                            hir::HirChild::Stmt(stmt) => self.analyze_w003_stmts(
                                std::slice::from_ref(stmt),
                                loop_depth,
                                &fresh,
                            ),
                        }
                    }
                }
            }
        }
    }

    fn scan_w003_stmt_header(&mut self, stmt: &Stmt, loop_depth: usize, fresh: &HashSet<String>) {
        match stmt {
            Stmt::Let { init, .. } | Stmt::Expr(init) => {
                self.scan_w003_expr(init, loop_depth, fresh);
            }
            _ => {}
        }
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
        if let ExprKind::Lambda { body, .. } = &expr.kind {
            self.analyze_body(body);
            return;
        }
        for child in expr.children() {
            if let hir::HirChild::Expr(child) = child {
                self.analyze_lambdas_in_expr(child);
            }
        }
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
    if is_reference_new_allocation(module, expr) {
        return true;
    }
    match &expr.kind {
        ExprKind::Local(name) => fresh.contains(name),
        ExprKind::Cast(inner) => is_fresh_userdata_argument(module, inner, fresh),
        _ => false,
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
    match &expr.kind {
        ExprKind::Local(local) => local == name,
        ExprKind::Cast(inner) => value_is_candidate(inner, name),
        ExprKind::Cond { then, els, .. } => {
            value_is_candidate(then, name) || value_is_candidate(els, name)
        }
        ExprKind::Assign { value, .. } => value_is_candidate(value, name),
        _ => false,
    }
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
        for code in [WarnCode::W001, WarnCode::W002, WarnCode::W003] {
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
}
