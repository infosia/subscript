//! Checker-owned aggregate and stack-frame layout limits.
//!
//! One aggregate may occupy at most [`MAX_AGGREGATE_BYTES`] bytes.
//! This is the signed 32-bit direct-displacement range used by the
//! Cranelift lowering for class and global offsets. An accumulated
//! Cranelift frame is separately limited to [`MAX_FRAME_BYTES`] so its
//! mandatory final ABI alignment cannot reach `2^31`. Array byte
//! lengths, nested-array products, field offsets, environment/frame
//! members, and final padding are all included in their applicable
//! bound.

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::hir;
use crate::types::{
    scalar_size_align, HandleClass, HandleKind, Type, CRANELIFT_FRAME_ALIGNMENT,
    MAX_AGGREGATE_BYTES, MAX_FRAME_BYTES,
};

use super::Checker;

#[derive(Clone, Copy)]
struct Layout {
    size: u64,
    align: u64,
}

#[derive(Clone, Copy)]
enum Outcome {
    Layout(Layout),
    TooLarge,
    Invalid,
}

/// Result of laying out a type without consulting class definitions.
pub(super) enum IndependentLayout {
    Fits,
    TooLarge,
    DependsOnClass,
}

/// Checks a `FixedArray` while its annotation is being resolved when
/// its layout does not depend on a class declared elsewhere.
pub(super) fn class_independent_layout(ty: &Type) -> IndependentLayout {
    match independent_type_layout(ty) {
        Outcome::Layout(_) => IndependentLayout::Fits,
        Outcome::TooLarge => IndependentLayout::TooLarge,
        Outcome::Invalid => IndependentLayout::DependsOnClass,
    }
}

fn limit() -> u64 {
    u64::from(MAX_AGGREGATE_BYTES)
}

fn round_up(value: u64, align: u64) -> Outcome {
    let Some(mask) = align.checked_sub(1) else {
        return Outcome::Invalid;
    };
    let Some(sum) = value.checked_add(mask) else {
        return Outcome::TooLarge;
    };
    let rounded = sum & !mask;
    if rounded > limit() {
        Outcome::TooLarge
    } else {
        Outcome::Layout(Layout {
            size: rounded,
            align,
        })
    }
}

fn array_layout(elem: Outcome, length: u32) -> Outcome {
    let Outcome::Layout(elem) = elem else {
        return elem;
    };
    let Outcome::Layout(stride) = round_up(elem.size, elem.align) else {
        return Outcome::TooLarge;
    };
    let Some(size) = stride.size.checked_mul(u64::from(length)) else {
        return Outcome::TooLarge;
    };
    if size > limit() {
        Outcome::TooLarge
    } else {
        Outcome::Layout(Layout {
            size,
            align: elem.align,
        })
    }
}

fn iter_result_layout(value: Outcome) -> Outcome {
    let Outcome::Layout(value) = value else {
        return value;
    };
    let align = value.align.max(1);
    let Outcome::Layout(value_offset) = round_up(1, align) else {
        return Outcome::TooLarge;
    };
    let Some(end) = value_offset.size.checked_add(value.size) else {
        return Outcome::TooLarge;
    };
    round_up(end, align)
}

fn independent_type_layout(ty: &Type) -> Outcome {
    if let Some((size, align)) = scalar_size_align(ty) {
        return scalar(u64::from(size), u64::from(align));
    }
    match ty {
        Type::FixedArray(elem, length) => array_layout(independent_type_layout(elem), *length),
        Type::IterResult(value) => iter_result_layout(independent_type_layout(value)),
        Type::Class(_) => Outcome::Invalid,
        _ => Outcome::Invalid,
    }
}

fn scalar(size: u64, align: u64) -> Outcome {
    Outcome::Layout(Layout { size, align })
}

fn raw_round_up(value: u64, align: u64) -> Option<u64> {
    let mask = align.checked_sub(1)?;
    value.checked_add(mask).map(|sum| sum & !mask)
}

struct FrameBudget {
    end: u64,
    exceeded: bool,
}

impl FrameBudget {
    fn new() -> Self {
        Self {
            end: 0,
            exceeded: false,
        }
    }
}

fn walk_lets<'h>(stmts: &'h [hir::Stmt], out: &mut Vec<(&'h Type, &'h Pos)>) {
    for stmt in stmts {
        match stmt {
            hir::Stmt::Let { ty, pos, .. } => out.push((ty, pos)),
            hir::Stmt::ForOf { ty, pos, .. } => out.push((ty, pos)),
            _ => {}
        }
        for child in stmt.children() {
            if let hir::HirChild::Stmt(stmt) = child {
                walk_lets(std::slice::from_ref(stmt), out);
            }
        }
    }
}

fn count_async_calls_expr(expr: &hir::Expr) -> u64 {
    u64::from(matches!(expr.kind, hir::ExprKind::AsyncCall { .. }))
        + expr
            .children()
            .into_iter()
            .map(|child| match child {
                hir::HirChild::Expr(expr) => count_async_calls_expr(expr),
                hir::HirChild::Stmt(stmt) => count_async_calls(std::slice::from_ref(stmt)),
            })
            .sum::<u64>()
}

fn count_async_calls(stmts: &[hir::Stmt]) -> u64 {
    stmts
        .iter()
        .flat_map(hir::Stmt::children)
        .map(|child| match child {
            hir::HirChild::Expr(expr) => count_async_calls_expr(expr),
            hir::HirChild::Stmt(stmt) => count_async_calls(std::slice::from_ref(stmt)),
        })
        .sum()
}

struct Validator<'a> {
    classes: &'a [hir::ClassDef],
    handle_classes: &'a [HandleClass],
    states: Vec<Option<Outcome>>,
    visiting: Vec<bool>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Validator<'a> {
    fn new(classes: &'a [hir::ClassDef], handle_classes: &'a [HandleClass]) -> Self {
        Self {
            classes,
            handle_classes,
            states: vec![None; classes.len()],
            visiting: vec![false; classes.len()],
            diagnostics: Vec::new(),
        }
    }

    fn type_layout(&mut self, ty: &Type) -> Outcome {
        match ty {
            Type::Class(id) => {
                let Some(class) = self.classes.get(id.0) else {
                    return Outcome::Invalid;
                };
                if class.is_value {
                    self.class_layout(id.0)
                } else {
                    scalar(8, 8)
                }
            }
            Type::FixedArray(elem, length) => array_layout(self.type_layout(elem), *length),
            Type::IterResult(value) => iter_result_layout(self.type_layout(value)),
            _ => independent_type_layout(ty),
        }
    }

    fn class_layout(&mut self, id: usize) -> Outcome {
        if let Some(outcome) = self.states.get(id).copied().flatten() {
            return outcome;
        }
        let Some(class) = self.classes.get(id) else {
            return Outcome::Invalid;
        };
        if self.visiting[id] {
            // Containment-cycle reporting remains the existing codegen
            // internal error; this validator is responsible only for
            // representable-size limits.
            return Outcome::Invalid;
        }
        self.visiting[id] = true;

        let mut size = 0u64;
        let mut align = 1u64;
        let mut outcome = Outcome::Invalid;
        let mut complete = true;
        for field in &class.fields {
            let Outcome::Layout(field_layout) = self.type_layout(&field.ty) else {
                complete = false;
                break;
            };
            let Outcome::Layout(aligned) = round_up(size, field_layout.align) else {
                self.class_too_large(class, field);
                complete = false;
                break;
            };
            let Some(end) = aligned.size.checked_add(field_layout.size) else {
                self.class_too_large(class, field);
                complete = false;
                break;
            };
            if end > limit() {
                self.class_too_large(class, field);
                complete = false;
                break;
            }
            size = end;
            align = align.max(field_layout.align);
        }
        if complete {
            let natural_align = align;
            if let Some(override_) = &class.alignment_override {
                if u64::from(override_.value) < natural_align {
                    self.diagnostics.push(Diagnostic::new(
                        RuleCode::S100,
                        format!(
                            "requested alignment {} is below the natural alignment {} for `{}`",
                            override_.value, natural_align, class.name
                        ),
                        override_.pos.clone(),
                    ));
                }
                align = align.max(u64::from(override_.value));
            }
            match round_up(size.max(1), align) {
                Outcome::Layout(layout) => outcome = Outcome::Layout(layout),
                _ => {
                    let pos = class
                        .fields
                        .last()
                        .map_or_else(|| class.pos.clone(), |field| field.pos.clone());
                    self.diagnostics.push(Diagnostic::new(
                        RuleCode::S100,
                        format!(
                            "`{}` layout exceeds the supported aggregate limit of {} bytes \
                             after final alignment",
                            class.name, MAX_AGGREGATE_BYTES
                        ),
                        pos,
                    ));
                    outcome = Outcome::TooLarge;
                }
            }
        }

        self.visiting[id] = false;
        self.states[id] = Some(outcome);
        outcome
    }

    fn class_too_large(&mut self, class: &hir::ClassDef, field: &hir::Field) {
        self.diagnostics.push(Diagnostic::new(
            RuleCode::S100,
            format!(
                "`{}` layout exceeds the supported aggregate limit of {} bytes \
                 while placing field `{}`",
                class.name, MAX_AGGREGATE_BYTES, field.name
            ),
            field.pos.clone(),
        ));
    }

    fn is_aggregate(&self, ty: &Type) -> bool {
        match ty {
            Type::Class(id) => self.classes.get(id.0).is_some_and(|class| class.is_value),
            Type::FixedArray(..) | Type::IterResult(_) => true,
            _ => false,
        }
    }

    fn is_managed(&self, ty: &Type) -> bool {
        // Fact filter: does this value point to a Context allocation that the marker can reach?
        ty.handle_kind(self.handle_classes)
            .is_some_and(HandleKind::is_collector_managed)
    }

    fn has_managed_interior(&self, ty: &Type) -> bool {
        self.is_managed(ty)
            || match ty {
                Type::FixedArray(elem, _) => self.has_managed_interior(elem),
                Type::IterResult(value) => self.has_managed_interior(value),
                _ => false,
            }
    }

    fn managed_words(&mut self, ty: &Type) -> Option<u64> {
        if self.is_managed(ty) {
            return Some(1);
        }
        if !self.has_managed_interior(ty) {
            return Some(0);
        }
        let Outcome::Layout(layout) = self.type_layout(ty) else {
            return None;
        };
        raw_round_up(layout.size, 8).map(|size| size / 8)
    }

    fn add_frame_slot(
        &mut self,
        frame: &mut FrameBudget,
        layout: Layout,
        description: &str,
        pos: &Pos,
    ) {
        if frame.exceeded {
            return;
        }
        let align = layout.align.max(8);
        let end =
            raw_round_up(frame.end, align).and_then(|start| start.checked_add(layout.size.max(1)));
        let final_size =
            end.and_then(|end| raw_round_up(end, u64::from(CRANELIFT_FRAME_ALIGNMENT)));
        if final_size.is_none_or(|size| size > u64::from(MAX_FRAME_BYTES)) {
            self.diagnostics.push(Diagnostic::new(
                RuleCode::S100,
                format!(
                    "{description} makes the accumulated Cranelift stack frame exceed \
                     the supported frame limit of {MAX_FRAME_BYTES} bytes"
                ),
                pos.clone(),
            ));
            frame.exceeded = true;
        }
        frame.end = end.unwrap_or(u64::MAX);
    }

    fn sequence_layout<'t>(
        &mut self,
        start: u64,
        members: impl IntoIterator<Item = &'t Type>,
        final_align: u64,
    ) -> Option<Layout> {
        let mut end = start;
        let mut align = final_align.max(1);
        for ty in members {
            let Outcome::Layout(member) = self.type_layout(ty) else {
                return None;
            };
            end = raw_round_up(end, member.align)
                .and_then(|offset| offset.checked_add(member.size))?;
            if end > limit() {
                return None;
            }
            align = align.max(member.align);
        }
        let size = raw_round_up(end.max(1), align)?;
        (size <= limit()).then_some(Layout { size, align })
    }

    fn validate_generator_layout(&mut self, function: &hir::Function, receiver: Option<&Type>) {
        let mut end = 16u64;
        let mut last_pos = &function.pos;
        if let Some(receiver) = receiver {
            let Outcome::Layout(layout) = self.type_layout(receiver) else {
                return;
            };
            let next = raw_round_up(end, layout.align.max(1))
                .and_then(|offset| offset.checked_add(layout.size.max(1)));
            if next.is_none_or(|size| size > limit()) {
                self.diagnostics.push(Diagnostic::new(
                    RuleCode::S100,
                    format!(
                        "async frame layout exceeds the supported aggregate limit of \
                         {MAX_AGGREGATE_BYTES} bytes while placing the method receiver"
                    ),
                    function.pos.clone(),
                ));
                return;
            }
            end = next.expect("async receiver size checked above");
        }
        for param in &function.params {
            last_pos = &param.pos;
            let Outcome::Layout(layout) = self.type_layout(&param.ty) else {
                continue;
            };
            let next = raw_round_up(end, layout.align.max(1))
                .and_then(|offset| offset.checked_add(layout.size.max(1)));
            if next.is_none_or(|size| size > limit()) {
                self.diagnostics.push(Diagnostic::new(
                    RuleCode::S100,
                    format!(
                        "generator frame layout exceeds the supported aggregate limit of \
                         {MAX_AGGREGATE_BYTES} bytes while placing parameter `{}`",
                        param.name
                    ),
                    param.pos.clone(),
                ));
                return;
            }
            end = next.expect("generator size checked above");
        }
        let mut lets = Vec::new();
        walk_lets(&function.body, &mut lets);
        for (ty, pos) in lets {
            last_pos = pos;
            let Outcome::Layout(layout) = self.type_layout(ty) else {
                continue;
            };
            let next = raw_round_up(end, layout.align.max(1))
                .and_then(|offset| offset.checked_add(layout.size.max(1)));
            if next.is_none_or(|size| size > limit()) {
                self.diagnostics.push(Diagnostic::new(
                    RuleCode::S100,
                    format!(
                        "generator frame layout exceeds the supported aggregate limit of \
                         {MAX_AGGREGATE_BYTES} bytes while placing this local"
                    ),
                    pos.clone(),
                ));
                return;
            }
            end = next.expect("generator size checked above");
        }
        if function.is_async {
            let child_bytes = count_async_calls(&function.body).checked_mul(8);
            let next = child_bytes.and_then(|bytes| end.checked_add(bytes));
            if next.is_none_or(|size| size > limit()) {
                self.diagnostics.push(Diagnostic::new(
                    RuleCode::S100,
                    format!(
                        "async frame layout exceeds the supported aggregate limit of \
                         {MAX_AGGREGATE_BYTES} bytes while placing awaited child frames"
                    ),
                    function.pos.clone(),
                ));
                return;
            }
            end = next.expect("async child-frame size checked above");
        }
        if raw_round_up(end, 8).is_none_or(|size| size > limit()) {
            self.diagnostics.push(Diagnostic::new(
                RuleCode::S100,
                format!(
                    "generator frame layout exceeds the supported aggregate limit of \
                     {MAX_AGGREGATE_BYTES} bytes after final alignment"
                ),
                last_pos.clone(),
            ));
        }
    }

    fn closure_layout(&mut self, captures: &[hir::Capture], pos: &Pos) -> Option<Layout> {
        if captures.is_empty() {
            return None;
        }
        let layout = self.sequence_layout(0, captures.iter().map(|capture| &capture.ty), 1);
        if layout.is_none() {
            self.diagnostics.push(Diagnostic::new(
                RuleCode::S100,
                format!(
                    "closure environment layout exceeds the supported aggregate limit of \
                     {MAX_AGGREGATE_BYTES} bytes"
                ),
                pos.clone(),
            ));
        }
        layout
    }

    fn closure_storage_layout(&mut self, captures: &[hir::Capture]) -> Option<Layout> {
        (!captures.is_empty())
            .then(|| self.sequence_layout(0, captures.iter().map(|capture| &capture.ty), 1))
            .flatten()
    }

    fn validate_closures_expr(&mut self, expr: &hir::Expr) {
        if let hir::ExprKind::Lambda { captures, .. } = &expr.kind {
            self.closure_layout(captures, &expr.pos);
        }
        for child in expr.children() {
            match child {
                hir::HirChild::Expr(expr) => self.validate_closures_expr(expr),
                hir::HirChild::Stmt(stmt) => {
                    self.validate_closures_stmts(std::slice::from_ref(stmt));
                }
            }
        }
    }

    fn validate_closures_stmts(&mut self, stmts: &[hir::Stmt]) {
        for stmt in stmts {
            for child in stmt.children() {
                match child {
                    hir::HirChild::Expr(expr) => self.validate_closures_expr(expr),
                    hir::HirChild::Stmt(stmt) => {
                        self.validate_closures_stmts(std::slice::from_ref(stmt));
                    }
                }
            }
        }
    }

    fn add_type_slot(&mut self, frame: &mut FrameBudget, ty: &Type, description: &str, pos: &Pos) {
        if let Outcome::Layout(layout) = self.type_layout(ty) {
            self.add_frame_slot(frame, layout, description, pos);
        }
    }

    fn expression_builds_into_destination(&self, expr: &hir::Expr) -> bool {
        match &expr.kind {
            hir::ExprKind::New { class, .. } => self
                .classes
                .get(class.0)
                .is_some_and(|class| class.is_value),
            hir::ExprKind::ArrayLit(_) => matches!(expr.ty, Type::FixedArray(..)),
            hir::ExprKind::Call { callee, .. } => match callee {
                hir::Callee::Func(_) | hir::Callee::Value(_) => true,
                hir::Callee::Method { recv, name } => {
                    name != "next" && matches!(recv.ty, Type::Class(_))
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn validate_expr_frame(
        &mut self,
        expr: &hir::Expr,
        destination: bool,
        frame: &mut FrameBudget,
    ) {
        use hir::ExprKind as K;

        let result_needs_slot = self.is_aggregate(&expr.ty)
            && !(destination && self.expression_builds_into_destination(expr))
            && matches!(
                &expr.kind,
                K::Zero | K::Call { .. } | K::New { .. } | K::ArrayLit(_) | K::ArraySpreadLit(_)
            );
        if result_needs_slot {
            self.add_type_slot(frame, &expr.ty, "aggregate expression storage", &expr.pos);
        }

        match &expr.kind {
            K::Lambda {
                params,
                body,
                captures,
                ..
            } => {
                if let Some(layout) = self.closure_storage_layout(captures) {
                    self.add_frame_slot(frame, layout, "closure environment storage", &expr.pos);
                }
                self.validate_plain_frame(params, body, &expr.pos, false);
                return;
            }
            K::AsyncHandleTransfer { value, .. } => {
                self.validate_expr_frame(value, destination, frame);
                return;
            }
            K::Cond { cond, then, els } => {
                self.validate_expr_frame(cond, false, frame);
                self.validate_expr_frame(then, destination, frame);
                self.validate_expr_frame(els, destination, frame);
                return;
            }
            _ => {}
        }

        for child in expr.children() {
            match child {
                hir::HirChild::Expr(child) => self.validate_expr_frame(child, false, frame),
                hir::HirChild::Stmt(_) => {}
            }
        }

        match &expr.kind {
            K::Call { args, .. } => {
                for arg in args {
                    if self.is_aggregate(&arg.ty) {
                        self.add_type_slot(
                            frame,
                            &arg.ty,
                            "by-value aggregate argument copy",
                            &arg.pos,
                        );
                    }
                }
                // Runtime-backed calls use at most one word or function
                // pair of scalar scratch per argument/result. Reserving
                // one pair for every call is conservative for script
                // calls and covers those runtime out/materialize slots.
                self.add_frame_slot(
                    frame,
                    Layout { size: 16, align: 8 },
                    "call scratch storage",
                    &expr.pos,
                );
            }
            K::AsyncCall { args, .. } | K::AsyncHandleCreate { args, .. } => {
                for arg in args {
                    if self.is_aggregate(&arg.ty) {
                        self.add_type_slot(
                            frame,
                            &arg.ty,
                            "async-call aggregate argument copy",
                            &arg.pos,
                        );
                    }
                }
                self.add_frame_slot(
                    frame,
                    Layout { size: 16, align: 8 },
                    "async-call scratch storage",
                    &expr.pos,
                );
            }
            K::AsyncHandleAwait(_) => {
                self.add_frame_slot(
                    frame,
                    Layout { size: 16, align: 8 },
                    "held-async-await scratch storage",
                    &expr.pos,
                );
            }
            K::New { args, .. } => {
                for arg in args {
                    if self.is_aggregate(&arg.ty) {
                        self.add_type_slot(
                            frame,
                            &arg.ty,
                            "constructor aggregate argument copy",
                            &arg.pos,
                        );
                    }
                }
            }
            K::DescriptorLit { fields, .. } => {
                for value in fields.iter().flatten() {
                    if self.is_aggregate(&value.ty) {
                        self.add_type_slot(
                            frame,
                            &value.ty,
                            "descriptor aggregate member value",
                            &value.pos,
                        );
                    }
                }
            }
            K::ArrayLit(elems) => {
                for elem in elems {
                    if matches!(expr.ty, Type::Array(_)) && !self.is_aggregate(&elem.ty) {
                        self.add_frame_slot(
                            frame,
                            Layout { size: 8, align: 8 },
                            "dynamic-array element scratch storage",
                            &elem.pos,
                        );
                    }
                }
            }
            K::ArraySpreadLit(elems) => {
                for elem in elems {
                    self.add_frame_slot(
                        frame,
                        Layout { size: 8, align: 8 },
                        "dynamic-array spread scratch storage",
                        &elem.expr.pos,
                    );
                }
            }
            _ => {}
        }
    }

    fn validate_stmts_frame(
        &mut self,
        stmts: &[hir::Stmt],
        frame: &mut FrameBudget,
        generator: bool,
    ) {
        for stmt in stmts {
            match stmt {
                hir::Stmt::Let { ty, init, pos, .. } => {
                    if !generator && self.is_aggregate(ty) && !self.has_managed_interior(ty) {
                        self.add_type_slot(frame, ty, "local aggregate storage", pos);
                    }
                    let destination = !generator && self.is_aggregate(ty);
                    self.validate_expr_frame(init, destination, frame);
                }
                hir::Stmt::ForOf {
                    ty, subject, pos, ..
                } => {
                    if !generator && self.is_aggregate(ty) && !self.has_managed_interior(ty) {
                        self.add_type_slot(frame, ty, "`for…of` binding storage", pos);
                    }
                    self.validate_expr_frame(subject, false, frame);
                }
                _ => {}
            }
            for child in stmt.children() {
                match child {
                    hir::HirChild::Expr(expr)
                        if !matches!(stmt, hir::Stmt::Let { .. } | hir::Stmt::ForOf { .. }) =>
                    {
                        self.validate_expr_frame(expr, false, frame);
                    }
                    hir::HirChild::Expr(_) => {}
                    hir::HirChild::Stmt(stmt) => {
                        self.validate_stmts_frame(std::slice::from_ref(stmt), frame, generator);
                    }
                }
            }
        }
    }

    fn validate_plain_frame(
        &mut self,
        params: &[hir::Param],
        body: &[hir::Stmt],
        pos: &Pos,
        generator: bool,
    ) {
        let mut frame = FrameBudget::new();
        if !generator {
            let mut words = 0u64;
            for param in params {
                let Some(count) = self.managed_words(&param.ty) else {
                    continue;
                };
                words = words.saturating_add(count);
            }
            let mut lets = Vec::new();
            walk_lets(body, &mut lets);
            for (ty, _) in lets {
                let Some(count) = self.managed_words(ty) else {
                    continue;
                };
                words = words.saturating_add(count);
            }
            if words > 0 {
                self.add_frame_slot(
                    &mut frame,
                    Layout {
                        size: words.saturating_mul(8),
                        align: 8,
                    },
                    "managed shadow storage",
                    pos,
                );
            }
        }
        self.validate_stmts_frame(body, &mut frame, generator);
    }

    fn validate_function(&mut self, function: &hir::Function, receiver: Option<&Type>) {
        if function.is_generator || function.is_async {
            self.validate_generator_layout(function, receiver);
        }
        self.validate_closures_stmts(&function.body);
        self.validate_plain_frame(
            &function.params,
            &function.body,
            &function.pos,
            function.is_generator || function.is_async,
        );
    }

    fn validate(
        mut self,
        pending: &[(Type, Pos, &'static str)],
        functions: &[hir::Function],
        globals: &[hir::Global],
        top_level: &[hir::Stmt],
    ) -> Vec<Diagnostic> {
        for (ty, pos, description) in pending {
            if matches!(self.type_layout(ty), Outcome::TooLarge) {
                self.diagnostics.push(Diagnostic::new(
                    RuleCode::S100,
                    format!(
                        "{description} exceeds the supported aggregate limit of \
                         {MAX_AGGREGATE_BYTES} bytes"
                    ),
                    pos.clone(),
                ));
            }
        }
        for id in 0..self.classes.len() {
            self.class_layout(id);
        }
        for function in functions {
            self.validate_function(function, None);
        }
        let class_functions: Vec<(hir::Function, Option<Type>)> = self
            .classes
            .iter()
            .enumerate()
            .flat_map(|(class_index, class)| {
                class
                    .ctor
                    .iter()
                    .cloned()
                    .map(|constructor| (constructor, None))
                    .chain(class.methods.iter().cloned().map(move |method| {
                        (
                            method,
                            Some(Type::Class(crate::types::ClassId(class_index))),
                        )
                    }))
            })
            .collect();
        for (function, receiver) in &class_functions {
            self.validate_function(function, receiver.as_ref());
        }
        let mut init_frame = FrameBudget::new();
        for global in globals {
            self.validate_closures_expr(&global.init);
            self.validate_expr_frame(&global.init, false, &mut init_frame);
        }
        self.validate_closures_stmts(top_level);
        self.validate_stmts_frame(top_level, &mut init_frame, false);
        self.diagnostics
    }
}

impl Checker<'_> {
    pub(super) fn validate_layouts(&mut self) {
        let diagnostics = Validator::new(&self.classes, &self.type_handle_classes).validate(
            &self.pending_layouts,
            &self.functions,
            &self.globals,
            &self.top_level,
        );
        self.diags.extend(diagnostics);
    }
}
