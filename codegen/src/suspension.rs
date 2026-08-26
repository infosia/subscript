//! Plans typed storage for values that survive a coroutine suspension.

use subscript_compiler::hir;
use subscript_compiler::types::Type;

use crate::layout::Layouts;

#[cfg(test)]
thread_local! {
    static SPILL_REQUEST_TRACE: std::cell::RefCell<Option<Vec<SpillKind>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn record_spill_request(kind: &SpillKind) {
    SPILL_REQUEST_TRACE.with(|trace| {
        if let Some(requests) = trace.borrow_mut().as_mut() {
            requests.push(kind.clone());
        }
    });
}

#[cfg(test)]
fn capture_spill_requests<T>(run: impl FnOnce() -> T) -> (T, Vec<SpillKind>) {
    SPILL_REQUEST_TRACE.with(|trace| {
        assert!(trace.borrow().is_none(), "nested spill request trace");
        *trace.borrow_mut() = Some(Vec::new());
    });
    let result = run();
    let requests = SPILL_REQUEST_TRACE.with(|trace| {
        trace
            .borrow_mut()
            .take()
            .expect("spill request trace was active")
    });
    (result, requests)
}

/// One typed spill member in a coroutine frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SpillKind {
    /// A language value with its storage type.
    Value(Type),
    /// An address of an assignable language value.
    Address,
    /// A capturing-lambda environment with its capture fields.
    LambdaEnv(Vec<(String, Type)>),
}

/// The canonical kind for a spill of an evaluated HIR expression.
///
/// Declared parameter and container types can be wider than the expression
/// that produced the value (for example `null` passed to `Box | null`).  A
/// spill stores that produced value, so every planner/emitter request must be
/// derived from the expression itself.
pub(crate) fn spill_kind(expr: &hir::Expr) -> SpillKind {
    SpillKind::Value(expr.ty.clone())
}

/// One use of a typed spill member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpillEvent {
    /// The required member type.
    pub(crate) kind: SpillKind,
    /// The member index in the frame.
    pub(crate) slot: usize,
}

/// Typed spill members and their uses for one coroutine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SpillPlan {
    /// The members in frame order.
    pub(crate) slots: Vec<SpillKind>,
    /// The member uses in lowering order.
    pub(crate) events: Vec<SpillEvent>,
}

struct Planner {
    plan: SpillPlan,
    /// `active[slot]` is true for the complete value live range.
    /// `acquire` can reuse only an inactive slot of the same kind.
    active: Vec<bool>,
}

type LiveId = usize;

#[derive(Clone, Debug)]
enum EvalEvent {
    Acquire { live: LiveId, kind: SpillKind },
    Release(LiveId),
}

/// Builds the single evaluation-order trace consumed by spill layout.
struct TraceBuilder<'a> {
    module: &'a hir::Module,
    layouts: &'a Layouts,
    events: Vec<EvalEvent>,
    next_live: LiveId,
}

impl Planner {
    fn acquire(&mut self, kind: SpillKind) -> usize {
        // A caller must hold the returned slot until the value is dead.
        // `release` ends that same live range and permits exact-kind reuse.
        let slot = self
            .plan
            .slots
            .iter()
            .zip(&self.active)
            .position(|(slot_kind, active)| !*active && *slot_kind == kind)
            .unwrap_or_else(|| {
                self.plan.slots.push(kind.clone());
                self.active.push(false);
                self.plan.slots.len() - 1
            });
        self.active[slot] = true;
        self.plan.events.push(SpillEvent { kind, slot });
        slot
    }

    fn release(&mut self, slot: usize) {
        self.active[slot] = false;
    }

    fn from_trace(events: &[EvalEvent], live_count: usize) -> Self {
        let mut planner = Self {
            plan: SpillPlan::default(),
            active: Vec::new(),
        };
        let mut live_slots = vec![None; live_count];

        for event in events {
            match event {
                EvalEvent::Acquire { live, kind } => {
                    let slot = planner.acquire(kind.clone());
                    live_slots[*live] = Some(slot);
                }
                EvalEvent::Release(live) => {
                    let slot = live_slots[*live]
                        .take()
                        .expect("evaluation trace released an inactive spill");
                    planner.release(slot);
                }
            }
        }
        planner
    }
}

impl<'a> TraceBuilder<'a> {
    fn acquire(&mut self, kind: SpillKind) -> LiveId {
        let live = self.next_live;
        self.next_live += 1;
        self.events.push(EvalEvent::Acquire { live, kind });
        live
    }

    fn release(&mut self, live: LiveId) {
        self.events.push(EvalEvent::Release(live));
    }

    fn plan_sequence(&mut self, exprs: &[hir::Expr]) -> Result<(), String> {
        let mut saved = Vec::new();
        for (index, expr) in exprs.iter().enumerate() {
            let later_suspends = exprs[index + 1..].iter().any(suspends_expr);
            self.plan_expr(expr)?;
            if later_suspends && expr.ty != Type::Void {
                saved.push(self.acquire(spill_kind(expr)));
            }
        }
        for live in saved.into_iter().rev() {
            self.release(live);
        }
        Ok(())
    }

    fn plan_index_parts(&mut self, obj: &hir::Expr, index: &hir::Expr) -> Result<(), String> {
        let save_obj = suspends_expr(index);
        self.plan_expr(obj)?;
        let saved_obj = save_obj.then(|| self.acquire(spill_kind(obj)));
        self.plan_expr(index)?;
        if let Some(live) = saved_obj {
            self.release(live);
        }
        Ok(())
    }

    fn plan_place(&mut self, target: &hir::Expr) -> Result<(), String> {
        use hir::ExprKind as K;
        match &target.kind {
            K::Field { obj, .. } => self.plan_expr(obj),
            K::Index { obj, index, .. } if matches!(obj.ty, Type::FixedArray(..)) => {
                self.plan_expr(obj)?;
                let saved = suspends_expr(index).then(|| self.acquire(SpillKind::Address));
                self.plan_expr(index)?;
                if let Some(slot) = saved {
                    self.release(slot);
                }
                Ok(())
            }
            K::Index { obj, index, .. } => self.plan_index_parts(obj, index),
            _ => Ok(()),
        }
    }

    fn place_slots(&mut self, target: &hir::Expr) -> Vec<LiveId> {
        use hir::ExprKind as K;
        match &target.kind {
            K::Local(_) => Vec::new(),
            K::Index { obj, index, .. } if matches!(obj.ty, Type::Array(_)) => vec![
                self.acquire(spill_kind(obj)),
                self.acquire(spill_kind(index)),
            ],
            K::Global(_) | K::This | K::Field { .. } | K::Index { .. } => {
                vec![self.acquire(SpillKind::Address)]
            }
            _ => Vec::new(),
        }
    }

    fn receiver_kind(&self, recv: &hir::Expr) -> Result<SpillKind, String> {
        use hir::ExprKind as K;
        if let Type::Class(class) = recv.ty {
            if self.layouts.class(class.0)?.is_value
                && matches!(
                    recv.kind,
                    K::Local(_) | K::Global(_) | K::Field { .. } | K::Index { .. } | K::This
                )
            {
                return Ok(SpillKind::Address);
            }
        }
        Ok(spill_kind(recv))
    }

    fn plan_template(&mut self, parts: &[hir::TplPart]) -> Result<(), String> {
        let Some((first, rest)) = parts.split_first() else {
            return Ok(());
        };
        self.plan_template_part(first)?;
        let save_acc = rest.iter().any(suspends_template_part);
        let saved_acc = save_acc.then(|| self.acquire(SpillKind::Value(Type::Str)));
        for part in rest {
            self.plan_template_part(part)?;
        }
        if let Some(slot) = saved_acc {
            self.release(slot);
        }
        Ok(())
    }

    fn plan_template_part(&mut self, part: &hir::TplPart) -> Result<(), String> {
        if let hir::TplPart::Expr(expr) = part {
            self.plan_expr(expr)?;
        }
        Ok(())
    }

    fn plan_expr(&mut self, expr: &hir::Expr) -> Result<(), String> {
        use hir::ExprKind as K;

        match &expr.kind {
            K::Unary { operand, .. }
            | K::Cast(operand)
            | K::Field { obj: operand, .. }
            | K::JsonResultValue(operand)
            | K::Length(operand) => self.plan_expr(operand)?,
            K::Binary { op, left, right } => {
                self.plan_expr(left)?;
                if !matches!(op, hir::BinOp::And | hir::BinOp::Or)
                    && suspends_expr(right)
                    && left.ty != Type::Void
                {
                    let saved = self.acquire(spill_kind(left));
                    self.plan_expr(right)?;
                    self.release(saved);
                } else {
                    self.plan_expr(right)?;
                }
            }
            K::Assign { op, target, value } => {
                self.plan_place(target)?;
                let value_suspends = suspends_expr(value);
                let place_slots = if value_suspends {
                    self.place_slots(target)
                } else {
                    Vec::new()
                };
                let current = if value_suspends && op.is_some() {
                    Some(self.acquire(spill_kind(target)))
                } else {
                    None
                };
                self.plan_expr(value)?;
                if let Some(slot) = current {
                    self.release(slot);
                }
                for slot in place_slots.into_iter().rev() {
                    self.release(slot);
                }
            }
            K::Call { callee, args } => {
                let args_suspend = args.iter().any(suspends_expr);
                let saved_callee = match callee {
                    hir::Callee::Value(value) => {
                        self.plan_expr(value)?;
                        args_suspend.then(|| self.acquire(spill_kind(value)))
                    }
                    hir::Callee::Method { recv, .. } => {
                        self.plan_expr(recv)?;
                        if args_suspend {
                            Some(self.acquire(self.receiver_kind(recv)?))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                self.plan_sequence(args)?;
                if let Some(slot) = saved_callee {
                    self.release(slot);
                }
            }
            K::New { class, args } => {
                let args_suspend = args.iter().any(suspends_expr);
                let saved_this =
                    args_suspend.then(|| self.acquire(SpillKind::Value(Type::Class(*class))));
                self.plan_sequence(args)?;
                if let Some(slot) = saved_this {
                    self.release(slot);
                }
            }
            K::DescriptorLit { class, fields } => {
                let descriptor = self
                    .module
                    .classes
                    .get(class.0)
                    .ok_or_else(|| "descriptor class id is out of range".to_string())?;
                let values: Vec<&hir::Expr> = fields
                    .iter()
                    .zip(&descriptor.fields)
                    .map(|(slot, field)| {
                        slot.as_ref().or(field.init.as_ref()).ok_or_else(|| {
                            format!(
                                "descriptor member `{}` has no literal value or default",
                                field.name
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?;
                let fields_suspend = values.iter().any(|value| suspends_expr(value));
                let saved_this =
                    fields_suspend.then(|| self.acquire(SpillKind::Value(Type::Class(*class))));
                for value in values {
                    self.plan_expr(value)?;
                }
                if let Some(slot) = saved_this {
                    self.release(slot);
                }
            }
            K::Index { obj, index, .. } => self.plan_index_parts(obj, index)?,
            K::ArrayLit(elems) => {
                if matches!(expr.ty, Type::Array(_)) {
                    let save_handle = elems.iter().any(suspends_expr);
                    let handle = save_handle.then(|| self.acquire(spill_kind(expr)));
                    for elem in elems {
                        self.plan_expr(elem)?;
                    }
                    if let Some(slot) = handle {
                        self.release(slot);
                    }
                } else {
                    self.plan_sequence(elems)?;
                }
            }
            K::ArraySpreadLit(elems) => {
                let save_handle = elems.iter().any(|elem| suspends_expr(&elem.expr));
                let handle = save_handle.then(|| self.acquire(spill_kind(expr)));
                for elem in elems {
                    self.plan_expr(&elem.expr)?;
                }
                if let Some(slot) = handle {
                    self.release(slot);
                }
            }
            K::Template(parts) => self.plan_template(parts)?,
            K::Lambda { captures, .. } if !captures.is_empty() => {
                self.acquire(SpillKind::LambdaEnv(
                    captures
                        .iter()
                        .map(|capture| (capture.name.clone(), capture.ty.clone()))
                        .collect(),
                ));
            }
            K::Yield(Some(value)) => self.plan_expr(value)?,
            K::AsyncCall { callee, args } => {
                let args_suspend = args.iter().any(suspends_expr);
                let saved_receiver = if let hir::AsyncCallee::Method { receiver, .. } = callee {
                    self.plan_expr(receiver)?;
                    args_suspend.then(|| self.acquire(spill_kind(receiver)))
                } else {
                    None
                };
                self.plan_sequence(args)?;
                if let Some(slot) = saved_receiver {
                    self.release(slot);
                }
            }
            K::Cond { cond, then, els } => {
                self.plan_expr(cond)?;
                self.plan_expr(then)?;
                self.plan_expr(els)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn plan_stmts(&mut self, stmts: &[hir::Stmt]) -> Result<(), String> {
        for stmt in stmts {
            match stmt {
                hir::Stmt::Let { init, .. } | hir::Stmt::Expr(init) => self.plan_expr(init)?,
                hir::Stmt::Return {
                    value: Some(value), ..
                } => self.plan_expr(value)?,
                hir::Stmt::Return { value: None, .. } => {}
                hir::Stmt::If {
                    cond, then, els, ..
                } => {
                    self.plan_expr(cond)?;
                    self.plan_stmts(then)?;
                    if let Some(els) = els {
                        self.plan_stmts(els)?;
                    }
                }
                hir::Stmt::While { cond, body, .. } => {
                    self.plan_expr(cond)?;
                    self.plan_stmts(body)?;
                }
                hir::Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                    ..
                } => {
                    if let Some(init) = init {
                        self.plan_stmts(std::slice::from_ref(init))?;
                    }
                    if let Some(cond) = cond {
                        self.plan_expr(cond)?;
                        self.plan_stmts(body)?;
                        if let Some(step) = step {
                            self.plan_expr(step)?;
                        }
                    } else {
                        self.plan_stmts(body)?;
                        if let Some(step) = step {
                            self.plan_expr(step)?;
                        }
                    }
                }
                hir::Stmt::ForOf { subject, body, .. } => {
                    self.plan_expr(subject)?;
                    if suspends_stmts(body) {
                        let subject_slot = self.acquire(spill_kind(subject));
                        let index_slot = self.acquire(SpillKind::Value(Type::U64));
                        let bound_slot = self.acquire(SpillKind::Value(Type::U64));
                        self.plan_stmts(body)?;
                        self.release(bound_slot);
                        self.release(index_slot);
                        self.release(subject_slot);
                    } else {
                        self.plan_stmts(body)?;
                    }
                }
                hir::Stmt::Switch { disc, cases, .. } => {
                    let tests_suspend = cases
                        .iter()
                        .filter_map(|case| case.test.as_ref())
                        .any(suspends_expr);
                    self.plan_expr(disc)?;
                    let saved_disc = tests_suspend.then(|| self.acquire(spill_kind(disc)));
                    for case in cases {
                        if let Some(test) = &case.test {
                            self.plan_expr(test)?;
                        }
                    }
                    if let Some(slot) = saved_disc {
                        self.release(slot);
                    }
                    for case in cases {
                        self.plan_stmts(&case.body)?;
                    }
                }
                hir::Stmt::Block(body) => self.plan_stmts(body)?,
                _ => {}
            }
        }
        Ok(())
    }
}

fn suspends_template_part(part: &hir::TplPart) -> bool {
    matches!(part, hir::TplPart::Expr(expr) if suspends_expr(expr))
}

/// True when both tiers must evaluate all call operands before family dispatch.
pub(crate) fn prepares_call_operands(callee: &hir::Callee, args: &[hir::Expr]) -> bool {
    matches!(
        callee,
        hir::Callee::Math(_)
            | hir::Callee::Num(_)
            | hir::Callee::Date(_)
            | hir::Callee::Json(_)
            | hir::Callee::Str(_)
            | hir::Callee::Regex(_)
            | hir::Callee::Arr(_)
            | hir::Callee::Map(_)
            | hir::Callee::Set(_)
            | hir::Callee::Worker(_)
            | hir::Callee::Ambient(_)
            | hir::Callee::ContextBytes { .. }
    ) || matches!(callee, hir::Callee::Foreign(_)) && args.iter().any(suspends_expr)
}

pub(crate) fn suspends_expr(expr: &hir::Expr) -> bool {
    use hir::ExprKind as K;
    match &expr.kind {
        K::Yield(_) | K::AsyncSuspend | K::AsyncCall { .. } => true,
        K::Unary { operand, .. }
        | K::Cast(operand)
        | K::Field { obj: operand, .. }
        | K::JsonResultValue(operand)
        | K::Length(operand) => suspends_expr(operand),
        K::Binary { left, right, .. }
        | K::Assign {
            target: left,
            value: right,
            ..
        }
        | K::Index {
            obj: left,
            index: right,
            ..
        } => suspends_expr(left) || suspends_expr(right),
        K::Call { callee, args } => {
            let callee_suspends = match callee {
                hir::Callee::Value(value) => suspends_expr(value),
                hir::Callee::Method { recv, .. } => suspends_expr(recv),
                _ => false,
            };
            callee_suspends || args.iter().any(suspends_expr)
        }
        K::New { args, .. } | K::ArrayLit(args) => args.iter().any(suspends_expr),
        K::DescriptorLit { fields, .. } => fields.iter().flatten().any(suspends_expr),
        K::ArraySpreadLit(elements) => elements.iter().any(|element| suspends_expr(&element.expr)),
        K::Template(parts) => parts.iter().any(suspends_template_part),
        K::Cond { cond, then, els } => {
            suspends_expr(cond) || suspends_expr(then) || suspends_expr(els)
        }
        _ => false,
    }
}

pub(crate) fn suspends_stmts(stmts: &[hir::Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        hir::Stmt::Let { init, .. } | hir::Stmt::Expr(init) => suspends_expr(init),
        hir::Stmt::Return { value, .. } => value.as_ref().is_some_and(suspends_expr),
        hir::Stmt::If {
            cond, then, els, ..
        } => {
            suspends_expr(cond)
                || suspends_stmts(then)
                || els.as_ref().is_some_and(|body| suspends_stmts(body))
        }
        hir::Stmt::While { cond, body, .. } => suspends_expr(cond) || suspends_stmts(body),
        hir::Stmt::For {
            init,
            cond,
            step,
            body,
            ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| suspends_stmts(std::slice::from_ref(stmt)))
                || cond.as_ref().is_some_and(suspends_expr)
                || step.as_ref().is_some_and(suspends_expr)
                || suspends_stmts(body)
        }
        hir::Stmt::ForOf { subject, body, .. } => suspends_expr(subject) || suspends_stmts(body),
        hir::Stmt::Switch { disc, cases, .. } => {
            suspends_expr(disc)
                || cases.iter().any(|case| {
                    case.test.as_ref().is_some_and(suspends_expr) || suspends_stmts(&case.body)
                })
        }
        hir::Stmt::Block(body) => suspends_stmts(body),
        _ => false,
    })
}

/// Calculates reusable typed spill members for one coroutine.
pub(crate) fn spill_plan(
    module: &hir::Module,
    layouts: &Layouts,
    body: &[hir::Stmt],
) -> Result<SpillPlan, String> {
    let mut trace = TraceBuilder {
        module,
        layouts,
        events: Vec::new(),
        next_live: 0,
    };
    trace.plan_stmts(body)?;
    let planner = Planner::from_trace(&trace.events, trace.next_live);
    Ok(planner.plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit_c;
    use subscript_compiler::{check_program, SourceFile};

    fn main_plan(source: &str) -> SpillPlan {
        let module = check_program(&[SourceFile::new("slot.ts", source)]).expect("clean check");
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main function");
        let layouts = Layouts::build(&module).expect("layouts");
        spill_plan(&module, &layouts, &function.body).expect("spill plan")
    }

    #[test]
    fn spill_plan_holds_only_a_value_that_crosses_a_suspension() {
        let live = main_plan(
            "async function first(): Promise<i32> { return 1; }\n\
             async function second(): Promise<i32> { return 2; }\n\
             export async function main(): Promise<void> {\n\
               const values: FixedArray<i32, 2> = [await first(), await second()];\n\
               print(`${values.length}`);\n\
             }\n",
        );
        assert!(
            live.slots.contains(&SpillKind::Value(Type::I32)),
            "the first result needs a typed frame member"
        );

        let dead = main_plan(
            "export async function main(): Promise<void> {\n\
               1 + 2;\n\
               await Context.suspend();\n\
             }\n",
        );
        assert!(
            dead.slots.is_empty(),
            "an arithmetic result that dies before suspension needs no member"
        );
    }

    #[test]
    fn every_statement_form_requests_the_planned_spill_kinds_on_the_ship_tier() {
        let source = SourceFile::new(
            "order.ts",
            "class Cell { n: i32 = 0; }\n\
             async function av(label: string, value: i32): Promise<i32> {\n\
               print(label);\n\
               await Context.suspend();\n\
               return value;\n\
             }\n\
             async function avs(label: string): Promise<i32[]> {\n\
               print(label);\n\
               await Context.suspend();\n\
               return [7];\n\
             }\n\
             async function returned(): Promise<i32> {\n\
               return await av(\"return-value\", 12);\n\
             }\n\
             export async function main(): Promise<void> {\n\
               const xs: i32[] = [];\n\
               const map = new Map<i32, string>();\n\
               const cell = new Cell();\n\
               let i: i32 = 0;\n\
               const letValue: i32 = await av(\"let-init\", 1);\n\
               print(`${letValue}`);\n\
               const captureFactor: i32 = 3;\n\
               const captured = (): i32 => captureFactor * 2;\n\
               await av(\"lambda-live\", 0);\n\
               print(`${captured()}`);\n\
               xs.push(await av(\"expr\", 1));\n\
               print(`${xs.indexOf(await av(\"intrinsic-receiver\", 1))}`);\n\
               map.set(await av(\"intrinsic-key\", 2), `${await av(\"intrinsic-value\", 3)}`);\n\
               print(`${Math.max((await av(\"intrinsic-left\", 4)) as f64, (await av(\"intrinsic-right\", 5)) as f64)}`);\n\
               if (i + await av(\"if\", 0) === 0) {\n\
                 xs.push(await av(\"then\", 2));\n\
               } else {\n\
                 xs.push(await av(\"else\", 3));\n\
               }\n\
               while ((await av(\"while-cond\", 0)) < 0) {\n\
                 xs.push(await av(\"while-body\", 4));\n\
               }\n\
               for (\n\
                 i = await av(\"for-init\", 0);\n\
                 (await av(\"for-cond\", 0)) < 0;\n\
                 cell.n = await av(\"for-step\", 5)\n\
               ) {\n\
                 xs.push(await av(\"for-body\", 6));\n\
                 i = i + 1;\n\
               }\n\
               for (const value of await avs(\"for-of-subject\")) {\n\
                 xs.push(await av(\"for-of-body\", value));\n\
               }\n\
               switch (await av(\"switch-disc\", 2)) {\n\
                 case 1:\n\
                   xs.push(await av(\"case-body\", 8));\n\
                   break;\n\
                 case 1 + await av(\"case-test\", 1):\n\
                   cell.n = await av(\"case-two\", 9);\n\
                   break;\n\
               }\n\
               { xs.push(await av(\"block\", 10)); }\n\
               const returnValue: i32 = await returned();\n\
               print(`${returnValue}`);\n\
               return;\n\
             }\n",
        );
        let module = check_program(std::slice::from_ref(&source)).expect("clean order source");
        let layouts = Layouts::build(&module).expect("layouts");
        let planned: Vec<SpillKind> = module
            .functions
            .iter()
            .flat_map(|function| {
                spill_plan(&module, &layouts, &function.body)
                    .expect("spill plan")
                    .events
                    .into_iter()
                    .map(|event| event.kind)
            })
            .collect();
        assert!(
            planned
                .iter()
                .any(|kind| matches!(kind, SpillKind::LambdaEnv(_))),
            "the order source must request a lambda environment"
        );

        let (ship_result, ship_requests) = capture_spill_requests(|| emit_c(&module));
        ship_result.expect("ship lowering");
        assert_eq!(ship_requests, planned, "ship spill request order");
        // The dev half of this test retires with `suspension.rs` at §68.4
        // step 4: dev now reads LIR and no longer requests planner spill slots
        // from this module.
    }

    #[test]
    fn every_capturing_lambda_in_a_coroutine_uses_a_frame_environment() {
        let before_only = main_plan(
            "export async function main(): Promise<void> {\n\
               const factor: i32 = 3;\n\
               const f = (): i32 => factor * 5;\n\
               print(`${f()}`);\n\
               await Context.suspend();\n\
             }\n",
        );
        assert_eq!(
            before_only
                .slots
                .iter()
                .filter(|kind| matches!(kind, SpillKind::LambdaEnv(_)))
                .count(),
            1,
            "a capture used only before suspension still lives in the frame"
        );

        let loop_plan = main_plan(
            "export async function main(): Promise<void> {\n\
               const factor: i32 = 3;\n\
               const f = (): i32 => factor * 5;\n\
               let i: i32 = 0;\n\
               while (i < 3) {\n\
                 print(`${f()}`);\n\
                 await Context.suspend();\n\
                 i = i + 1;\n\
               }\n\
             }\n",
        );
        assert!(loop_plan
            .slots
            .iter()
            .any(|kind| matches!(kind, SpillKind::LambdaEnv(_))));

        let assignment_plan = main_plan(
            "export async function main(): Promise<void> {\n\
               const factor: i32 = 3;\n\
               let f = (): i32 => 0;\n\
               f = (): i32 => factor * 2;\n\
               await Context.suspend();\n\
               print(`${f()}`);\n\
             }\n",
        );
        assert!(assignment_plan
            .slots
            .iter()
            .any(|kind| matches!(kind, SpillKind::LambdaEnv(_))));
    }

    #[test]
    fn simultaneous_equal_lambda_environments_use_distinct_slots() {
        let plan = main_plan(
            "export async function main(): Promise<void> {\n\
               const factor: i32 = 3;\n\
               const outer = (): i32 => factor * 2;\n\
               {\n\
                 const factor: i32 = 50;\n\
                 const inner = (): i32 => factor * 2;\n\
                 await Context.suspend();\n\
                 print(`${inner()}`);\n\
               }\n\
               await Context.suspend();\n\
               print(`${outer()}`);\n\
             }\n",
        );
        let lambda_slots: Vec<&SpillKind> = plan
            .slots
            .iter()
            .filter(|kind| matches!(kind, SpillKind::LambdaEnv(_)))
            .collect();
        assert_eq!(lambda_slots.len(), 2, "both environments stay live");
        let lambda_events: Vec<usize> = plan
            .events
            .iter()
            .filter(|event| matches!(event.kind, SpillKind::LambdaEnv(_)))
            .map(|event| event.slot)
            .collect();
        assert_eq!(lambda_events.len(), 2);
        assert_ne!(lambda_events[0], lambda_events[1]);
    }

    #[test]
    fn outer_assignment_and_sibling_use_distinct_lambda_slots() {
        let plan = main_plan(
            "export async function main(): Promise<void> {\n\
               let keep = (): i32 => 0;\n\
               {\n\
                 const factor: i32 = 2;\n\
                 keep = (): i32 => factor * 10;\n\
               }\n\
               await Context.suspend();\n\
               {\n\
                 const factor: i32 = 30;\n\
                 const sibling = (): i32 => factor * 10;\n\
                 await Context.suspend();\n\
                 print(`${keep()},${sibling()}`);\n\
               }\n\
             }\n",
        );
        let lambda_events: Vec<usize> = plan
            .events
            .iter()
            .filter(|event| matches!(event.kind, SpillKind::LambdaEnv(_)))
            .map(|event| event.slot)
            .collect();
        assert_eq!(lambda_events.len(), 2);
        assert_ne!(lambda_events[0], lambda_events[1]);
    }
}
