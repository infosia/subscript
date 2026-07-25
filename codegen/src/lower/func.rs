//! Function-body lowering: statements, expressions, value-class copy
//! semantics (C2), closures (C5), null story (C7), and the coroutine
//! CPS/state-machine transform (C8).
//!
//! # Coroutine transform
//!
//! A `function*` lowers to two functions. The *creator* has the
//! declared signature, allocates the coroutine frame in the Context
//! (state word, resume-function pointer, then every parameter and
//! local), stores the parameters, and returns the frame handle. The
//! *resume* function `(ctx, frame, out) -> done` holds the whole body
//! as a state machine: entry dispatches on the state word to the
//! start block or to the continuation block after the corresponding
//! `yield`; a `yield` writes its value through `out`, stores the next
//! state, and returns `done = 0`; falling off the body stores the
//! terminal state and returns `done = 1`. All locals live in the
//! frame, so suspension is a plain return — no fibers, no stack
//! switching (iOS-safe). `.next()` zero-fills a step-result slot,
//! calls the resume pointer stored in the frame, and records `done`
//! (C8: `value` is zero-initialized when `done`).

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    types, AbiParam, ArgumentPurpose, Block, BlockArg, InstBuilder, MemFlags, Signature,
    StackSlotData, StackSlotKind, Value,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module};
use subscript_compiler::{hir, Pos, Type};
use subscript_runtime::context as rtc;
use subscript_runtime::TrapKind;

use crate::layout::{has_managed_interior, is_managed, is_unsigned, managed_words, Repr};
use crate::lower::{internal, FnKey, GlobalSlot, ModLower};

/// A computed value.
#[derive(Debug, Clone, Copy)]
enum RV {
    /// No value (void).
    None,
    /// Scalar (numbers, booleans, handles/pointers).
    S(Value),
    /// Function value `(code, env)`.
    P(Value, Value),
    /// Aggregate viewed through a pointer to its storage.
    A(Value),
}

/// How a by-value boundary struct returned from a foreign call reaches its
/// language slot (§14.2). Either the callee wrote a caller `sret` slot, or
/// the result came back in registers to be stored chunk-by-chunk.
enum StructRet {
    /// A hidden result pointer was passed; the callee wrote the slot.
    Sret(Value),
    /// The struct was returned in registers; each `(byte offset, CLIF
    /// type)` chunk is one returned value to store into the slot.
    Reg {
        slot: Value,
        chunks: Vec<(u32, types::Type)>,
    },
}

/// Where a binding lives.
#[derive(Debug, Clone, Copy)]
enum Storage {
    /// SSA variable (scalar).
    Var(Variable),
    /// Two SSA variables (function value).
    Pair(Variable, Variable),
    /// Fixed address (aggregate stack slot, aggregate parameter,
    /// captured aggregate).
    Addr(Value),
    /// Slot `i` of the function's shadow frame (managed local).
    Shadow(u32),
    /// Byte offset into the coroutine frame.
    Frame(u32),
}

#[derive(Debug, Clone)]
struct Binding {
    ty: Type,
    storage: Storage,
    /// Proven inclusive integer range of the binding's value, when one
    /// is available (loop induction variables with constant bounds).
    /// `None` means unproven — the value may be anything of its type.
    /// Used only to elide provably-in-range bounds checks (§10.1); an
    /// absent or conservative range never removes a check that could
    /// fire.
    range: Option<Interval>,
}

/// An inclusive integer interval `[lo, hi]`, computed in `i64`.
///
/// This is the lattice of the proof-based bounds-check elimination
/// (`specs/blocks/compiler.md` §10.1): an interval is always a sound
/// over-approximation of the values an expression can take at runtime,
/// so a check is removed only when the whole interval is in range.
/// Arithmetic is done in `i128` and rejected (`None`) if the result
/// does not fit `i64`, so the interval itself never wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Interval {
    lo: i64,
    hi: i64,
}

impl Interval {
    fn point(v: i64) -> Interval {
        Interval { lo: v, hi: v }
    }

    /// Narrows an `i128` pair back into an `i64` interval, failing when
    /// either end does not fit (so the interval never silently wraps).
    fn fit(lo: i128, hi: i128) -> Option<Interval> {
        let lo = i64::try_from(lo).ok()?;
        let hi = i64::try_from(hi).ok()?;
        if lo > hi {
            return None;
        }
        Some(Interval { lo, hi })
    }

    fn add(self, o: Interval) -> Option<Interval> {
        Interval::fit(
            self.lo as i128 + o.lo as i128,
            self.hi as i128 + o.hi as i128,
        )
    }

    fn sub(self, o: Interval) -> Option<Interval> {
        Interval::fit(
            self.lo as i128 - o.hi as i128,
            self.hi as i128 - o.lo as i128,
        )
    }

    fn mul(self, o: Interval) -> Option<Interval> {
        let corners = [
            self.lo as i128 * o.lo as i128,
            self.lo as i128 * o.hi as i128,
            self.hi as i128 * o.lo as i128,
            self.hi as i128 * o.hi as i128,
        ];
        let lo = corners.iter().copied().min()?;
        let hi = corners.iter().copied().max()?;
        Interval::fit(lo, hi)
    }
}

/// Inclusive representable range of an integer type, `None` for
/// non-integers. Used to prove an induction variable's step cannot
/// overflow its type (which would break monotonicity and void the
/// interval).
fn int_type_range(ty: &Type) -> Option<Interval> {
    Some(match ty {
        Type::I8 => Interval {
            lo: i64::from(i8::MIN),
            hi: i64::from(i8::MAX),
        },
        Type::U8 => Interval {
            lo: 0,
            hi: i64::from(u8::MAX),
        },
        Type::I16 => Interval {
            lo: i64::from(i16::MIN),
            hi: i64::from(i16::MAX),
        },
        Type::U16 => Interval {
            lo: 0,
            hi: i64::from(u16::MAX),
        },
        Type::I32 => Interval {
            lo: i64::from(i32::MIN),
            hi: i64::from(i32::MAX),
        },
        Type::U32 => Interval {
            lo: 0,
            hi: i64::from(u32::MAX),
        },
        Type::I64 => Interval {
            lo: i64::MIN,
            hi: i64::MAX,
        },
        // u64's upper bound does not fit i64; the interval lattice is
        // i64, so u64 induction ranges are simply not proven (rare in
        // index position and never in the corpus).
        Type::U64 => return None,
        _ => return None,
    })
}

/// An assignable location.
///
/// Dynamic-array elements stay *unresolved* (`ArrayElem`) until the
/// moment of the access: resolving the element address early would
/// dangle if the value expression grows the same array (its storage
/// is reallocated on growth).
#[derive(Debug, Clone, Copy)]
enum Place {
    Var(Variable),
    Pair(Variable, Variable),
    Mem(Value, i32),
    ArrayElem {
        handle: Value,
        index: Value,
        pos_id: i64,
    },
}

struct LoopCtx {
    brk: Block,
    cont: Option<Block>,
}

struct GenCtx {
    frame: Value,
    out: Value,
    yield_ty: Type,
    resume_blocks: Vec<Block>,
    next_resume: usize,
    let_offsets: Vec<u32>,
    next_let: usize,
}

/// Terminal state word of a coroutine frame.
const GEN_DONE: i64 = 0x7FFF_FFFF;
/// Frame offset of the resume-function pointer.
const GEN_RESUME_OFF: i32 = 8;
/// First payload offset in a coroutine frame.
const GEN_PAYLOAD_OFF: u32 = 16;

fn flags() -> MemFlags {
    MemFlags::trusted()
}

fn shift_mask(ty: &Type) -> Result<i64, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => 7,
        Type::I16 | Type::U16 => 15,
        Type::I32 | Type::U32 => 31,
        Type::I64 | Type::U64 => 63,
        other => return Err(internal(format!("shift width for {other:?}"))),
    })
}

/// Frame offset of the reload epoch a coroutine frame was created in
/// (`LowerOptions::reload` only; the word is unused otherwise).
const GEN_EPOCH_OFF: i32 = 4;

/// Context byte offset, as the `i32` displacement Cranelift loads take.
fn ctx_off(offset: usize) -> Result<i32, String> {
    i32::try_from(offset).map_err(|_| internal("context offset does not fit in i32"))
}

/// Loads a function's *current* code address out of the Context's
/// per-function indirection table and imports its signature
/// (`LowerOptions::reload`). A hot reload repoints the table, so every
/// call emitted this way reaches the newly compiled body.
fn indirect_target<M: Module>(
    ml: &ModLower<'_, M>,
    b: &mut FunctionBuilder<'_>,
    ctx_v: Value,
    key: &FnKey,
) -> Result<(Value, cranelift_codegen::ir::SigRef), String> {
    let id = ml.func_id(key)?;
    let slot = ml.slot_of(key)?;
    let disp = i32::try_from(u64::from(slot) * 8)
        .map_err(|_| internal("function slot offset does not fit in i32"))?;
    let sig = ml.signature_of(id);
    let sigref = b.import_signature(sig);
    let table_off = ctx_off(rtc::Context::fn_table_offset())?;
    let table = b.ins().load(types::I64, flags(), ctx_v, table_off);
    let code = b.ins().load(types::I64, flags(), table, disp);
    Ok((code, sigref))
}

fn align_shift(align: u32) -> u8 {
    align.max(1).trailing_zeros() as u8
}

fn round_up(v: u32, a: u32) -> u32 {
    (v + a - 1) & !(a - 1)
}

/// True when a struct return's flattened leaf scalars form a pure
/// Homogeneous Floating-point Aggregate (AAPCS 6.4.2 / Win64): 1 to 4
/// members, **all** of the same fundamental float type (all `f32`, or all
/// `f64`). Such aggregates are returned in SIMD registers, so the integer-
/// register return path must not marshal them (§14.2 HFA guard). All-
/// integer, mixed integer+float, and >4-member returns are not HFAs.
fn is_pure_hfa_leaves(leaves: &[types::Type]) -> bool {
    if !matches!(leaves.len(), 1..=4) {
        return false;
    }
    leaves.iter().all(|t| *t == types::F32) || leaves.iter().all(|t| *t == types::F64)
}

/// True for a callback-info userdata slot — the boundary `object | null`
/// form (`Type::Nullable(Object)`) or a bare `object` (§14.4). A callback
/// field is followed by one such slot (single-userdata info) or two
/// (two-userdata info); this distinguishes a trailing userdata slot from
/// any other field.
fn is_userdata_slot(ty: &Type) -> bool {
    matches!(ty, Type::Object)
        || matches!(ty, Type::Nullable(inner) if **inner == Type::Object)
}

// ----- pre-passes -----

fn walk_lets<'h>(stmts: &'h [hir::Stmt], out: &mut Vec<&'h Type>) {
    for s in stmts {
        match s {
            hir::Stmt::Let { ty, .. } => out.push(ty),
            hir::Stmt::If { then, els, .. } => {
                walk_lets(then, out);
                if let Some(e) = els {
                    walk_lets(e, out);
                }
            }
            hir::Stmt::While { body, .. } => walk_lets(body, out),
            hir::Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    walk_lets(std::slice::from_ref(i), out);
                }
                walk_lets(body, out);
            }
            hir::Stmt::Switch { cases, .. } => {
                for c in cases {
                    walk_lets(&c.body, out);
                }
            }
            hir::Stmt::Block(b) => walk_lets(b, out),
            _ => {}
        }
    }
}

/// True when any statement assigns (plainly or compound, `++`/`--`
/// included — all lower to `Assign`) to a local named `name`. Used to
/// disqualify a loop counter from range proof when the body mutates it
/// outside the step (§10.1).
fn stmts_assign_to(stmts: &[hir::Stmt], name: &str) -> bool {
    stmts.iter().any(|s| stmt_assigns_to(s, name))
}

fn stmt_assigns_to(s: &hir::Stmt, name: &str) -> bool {
    match s {
        hir::Stmt::Let { init, .. } => expr_assigns_to(init, name),
        hir::Stmt::Expr(e) => expr_assigns_to(e, name),
        hir::Stmt::Return { value, .. } => {
            value.as_ref().is_some_and(|e| expr_assigns_to(e, name))
        }
        hir::Stmt::If { cond, then, els, .. } => {
            expr_assigns_to(cond, name)
                || stmts_assign_to(then, name)
                || els.as_ref().is_some_and(|e| stmts_assign_to(e, name))
        }
        hir::Stmt::While { cond, body, .. } => {
            expr_assigns_to(cond, name) || stmts_assign_to(body, name)
        }
        hir::Stmt::For {
            init, cond, step, body, ..
        } => {
            init.as_deref().is_some_and(|i| stmt_assigns_to(i, name))
                || cond.as_ref().is_some_and(|e| expr_assigns_to(e, name))
                || step.as_ref().is_some_and(|e| expr_assigns_to(e, name))
                || stmts_assign_to(body, name)
        }
        hir::Stmt::Switch { disc, cases, .. } => {
            expr_assigns_to(disc, name)
                || cases.iter().any(|c| {
                    c.test.as_ref().is_some_and(|e| expr_assigns_to(e, name))
                        || stmts_assign_to(&c.body, name)
                })
        }
        hir::Stmt::Block(b) => stmts_assign_to(b, name),
        hir::Stmt::Break(_) | hir::Stmt::Continue(_) => false,
        // `hir::Stmt` is `#[non_exhaustive]` across the crate boundary,
        // so this arm cannot be removed. Every current variant that can
        // carry an assignment is handled above; a future variant is
        // treated conservatively as possibly assigning, which only
        // declines an induction proof (soundness over optimization).
        _ => true,
    }
}

fn expr_assigns_to(e: &hir::Expr, name: &str) -> bool {
    use hir::ExprKind as K;
    match &e.kind {
        K::Assign { op: _, target, value } => {
            let hits_target = matches!(&target.kind, K::Local(n) if n == name);
            hits_target || expr_assigns_to(target, name) || expr_assigns_to(value, name)
        }
        K::Unary { operand, .. } => expr_assigns_to(operand, name),
        K::Binary { left, right, .. } => {
            expr_assigns_to(left, name) || expr_assigns_to(right, name)
        }
        K::Cast(inner) => expr_assigns_to(inner, name),
        K::Call { callee, args } => {
            let in_callee = match callee {
                hir::Callee::Value(v) => expr_assigns_to(v, name),
                hir::Callee::Method { recv, .. } => expr_assigns_to(recv, name),
                _ => false,
            };
            in_callee || args.iter().any(|a| expr_assigns_to(a, name))
        }
        K::New { args, .. } => args.iter().any(|a| expr_assigns_to(a, name)),
        K::Field { obj, .. } => expr_assigns_to(obj, name),
        K::Length(obj) => expr_assigns_to(obj, name),
        K::Index { obj, index } => {
            expr_assigns_to(obj, name) || expr_assigns_to(index, name)
        }
        K::ArrayLit(elems) => elems.iter().any(|x| expr_assigns_to(x, name)),
        K::Template(parts) => parts.iter().any(|p| match p {
            hir::TplPart::Expr(x) => expr_assigns_to(x, name),
            _ => false,
        }),
        K::Cond { cond, then, els } => {
            expr_assigns_to(cond, name)
                || expr_assigns_to(then, name)
                || expr_assigns_to(els, name)
        }
        K::Yield(arg) => arg.as_deref().is_some_and(|x| expr_assigns_to(x, name)),
        // A lambda cannot assign to an outer mutable local: C5 forbids a
        // capturing lambda from capturing a non-`const`, and a loop
        // counter is mutable while it is the loop variable. So a lambda
        // never reassigns the counter and needs no descent.
        K::Lambda { .. } => false,
        // Read-only leaves: none can assign to an outer local.
        K::Int(_)
        | K::Float(_)
        | K::Bool(_)
        | K::Str(_)
        | K::Null
        | K::This
        | K::Local(_)
        | K::Global(_)
        | K::FuncRef(_)
        | K::EnumMember { .. } => false,
        // `hir::ExprKind` is `#[non_exhaustive]` across the crate
        // boundary. Every current variant is handled above; a future
        // variant is treated conservatively as possibly assigning, which
        // only declines an induction proof (soundness over optimization).
        _ => true,
    }
}

fn count_yields_expr(e: &hir::Expr) -> usize {
    use hir::ExprKind as K;
    match &e.kind {
        K::Yield(arg) => 1 + arg.as_deref().map_or(0, count_yields_expr),
        K::Unary { operand, .. } => count_yields_expr(operand),
        K::Binary { left, right, .. } => count_yields_expr(left) + count_yields_expr(right),
        K::Assign { target, value, .. } => count_yields_expr(target) + count_yields_expr(value),
        K::Cast(inner) => count_yields_expr(inner),
        K::Call { callee, args } => {
            let mut n: usize = args.iter().map(count_yields_expr).sum();
            match callee {
                hir::Callee::Value(v) => n += count_yields_expr(v),
                hir::Callee::Method { recv, .. } => n += count_yields_expr(recv),
                _ => {}
            }
            n
        }
        K::New { args, .. } => args.iter().map(count_yields_expr).sum(),
        K::Field { obj, .. } => count_yields_expr(obj),
        K::Length(obj) => count_yields_expr(obj),
        K::Index { obj, index } => count_yields_expr(obj) + count_yields_expr(index),
        K::ArrayLit(elems) => elems.iter().map(count_yields_expr).sum(),
        K::Template(parts) => parts
            .iter()
            .map(|p| match p {
                hir::TplPart::Expr(e) => count_yields_expr(e),
                _ => 0,
            })
            .sum(),
        K::Cond { cond, then, els } => {
            count_yields_expr(cond) + count_yields_expr(then) + count_yields_expr(els)
        }
        // Lambda bodies are separate functions and cannot contain
        // yields of this generator (the checker scopes `yield` to the
        // innermost function).
        _ => 0,
    }
}

fn count_yields(stmts: &[hir::Stmt]) -> usize {
    let mut n = 0;
    for s in stmts {
        match s {
            hir::Stmt::Let { init, .. } => n += count_yields_expr(init),
            hir::Stmt::Expr(e) => n += count_yields_expr(e),
            hir::Stmt::Return { value, .. } => {
                n += value.as_ref().map_or(0, count_yields_expr);
            }
            hir::Stmt::If { cond, then, els, .. } => {
                n += count_yields_expr(cond) + count_yields(then);
                if let Some(e) = els {
                    n += count_yields(e);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                n += count_yields_expr(cond) + count_yields(body);
            }
            hir::Stmt::For {
                init, cond, step, body, ..
            } => {
                if let Some(i) = init {
                    n += count_yields(std::slice::from_ref(i));
                }
                n += cond.as_ref().map_or(0, count_yields_expr);
                n += step.as_ref().map_or(0, count_yields_expr);
                n += count_yields(body);
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                n += count_yields_expr(disc);
                for c in cases {
                    n += c.test.as_ref().map_or(0, count_yields_expr) + count_yields(&c.body);
                }
            }
            hir::Stmt::Block(b) => n += count_yields(b),
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
            _ => {}
        }
    }
    n
}

// ----- the body lowerer -----

struct Body<'f, 'm, 'a, M: Module> {
    ml: &'m mut ModLower<'a, M>,
    b: FunctionBuilder<'f>,
    ctx_v: Value,
    env_v: Option<Value>,
    sret_v: Option<Value>,
    this_v: Option<(Value, usize)>,
    ret_ty: Type,
    is_resume: bool,
    scopes: Vec<Vec<(String, Binding)>>,
    loops: Vec<LoopCtx>,
    unwind: Option<Block>,
    shadow_base: Option<Value>,
    next_shadow: u32,
    genc: Option<GenCtx>,
    term: bool,
}

impl<'f, 'm, 'a, M: Module> Body<'f, 'm, 'a, M> {
    // ----- small helpers -----

    fn pos_id(&mut self, pos: &Pos) -> i64 {
        i64::from(self.ml.pos_id(pos))
    }

    fn scope_push(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn scope_pop(&mut self) {
        self.scopes.pop();
    }

    fn bind(&mut self, name: &str, binding: Binding) {
        if let Some(s) = self.scopes.last_mut() {
            s.push((name.to_string(), binding));
        }
    }

    fn lookup(&self, name: &str) -> Result<Binding, String> {
        for scope in self.scopes.iter().rev() {
            for (n, b) in scope.iter().rev() {
                if n == name {
                    return Ok(b.clone());
                }
            }
        }
        Err(internal(format!("unbound local `{name}`")))
    }

    /// Records a proven integer range on an already-bound local
    /// (innermost binding of that name). No-op when the name is not
    /// found, which cannot remove a check.
    fn set_range(&mut self, name: &str, range: Interval) {
        for scope in self.scopes.iter_mut().rev() {
            for (n, b) in scope.iter_mut().rev() {
                if n == name {
                    b.range = Some(range);
                    return;
                }
            }
        }
    }

    // ----- proof-based bounds-check elimination (§10.1) -----

    /// Sound over-approximation of the integer value an expression can
    /// take at this program point, or `None` when no bound is proven.
    /// The result is only ever used to *keep* a check (when unproven or
    /// out of range) or *remove* one (when the whole interval is in
    /// range), so `None` is always the safe answer.
    fn interval_of(&self, e: &hir::Expr) -> Option<Interval> {
        use hir::ExprKind as K;
        if !e.ty.is_integer() {
            return None;
        }
        match &e.kind {
            K::Int(v) => Some(Interval::point(*v)),
            K::EnumMember { value, .. } => Some(Interval::point(*value)),
            K::Local(name) => self.lookup(name).ok().and_then(|b| b.range),
            K::Length(obj) => match &obj.ty {
                // A `FixedArray`'s length is its compile-time constant N.
                Type::FixedArray(_, n) => Some(Interval::point(i64::from(*n))),
                _ => None,
            },
            K::Binary { op, left, right } => {
                let l = self.interval_of(left)?;
                let r = self.interval_of(right)?;
                match op {
                    hir::BinOp::Add => l.add(r),
                    hir::BinOp::Sub => l.sub(r),
                    hir::BinOp::Mul => l.mul(r),
                    _ => None,
                }
            }
            K::Cast(inner) => {
                // Value-preserving only when the source is an integer
                // and every value of the source interval also fits the
                // integer target (a narrowing/reinterpreting cast could
                // change the value, so it yields no proof).
                if !inner.ty.is_integer() {
                    return None;
                }
                let iv = self.interval_of(inner)?;
                let target = int_type_range(&e.ty)?;
                if iv.lo >= target.lo && iv.hi <= target.hi {
                    Some(iv)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// True when `index` is proven within `[0, n)` for a `FixedArray`
    /// of length `n`, so its bounds check is provably dead and elided.
    fn index_in_bounds(&self, index: &hir::Expr, n: u32) -> bool {
        match self.interval_of(index) {
            Some(iv) => iv.lo >= 0 && iv.hi < i64::from(n),
            None => false,
        }
    }

    /// Recognizes a count-up `for` loop whose counter has a proven,
    /// non-wrapping range, returning `(counter name, range)`.
    ///
    /// Exact proof conditions (all required; any miss yields `None` and
    /// the loop's indices stay checked):
    /// - init is `let <name>: <int> = START` with `START` a proven
    ///   constant;
    /// - cond is `<name> < BOUND` or `<name> <= BOUND` with `BOUND` a
    ///   proven interval (a constant, another proven counter, or a
    ///   `FixedArray` length);
    /// - step is `<name> += STEP` (which `<name>++` also lowers to) with
    ///   `STEP` a proven positive constant;
    /// - the body never reassigns `<name>` (so it stays monotonic);
    /// - the counter cannot overflow its type across the range, which
    ///   would break monotonicity.
    ///
    /// The range is `[START, BOUND-1]` for `<` and `[START, BOUND]` for
    /// `<=`; the counter only enters the body while the condition holds
    /// and only increases, so the interval covers every value the body
    /// can observe.
    fn induction_interval(
        &self,
        init: Option<&hir::Stmt>,
        cond: Option<&hir::Expr>,
        step: Option<&hir::Expr>,
        body: &[hir::Stmt],
    ) -> Option<(String, Interval)> {
        use hir::ExprKind as K;
        let (name, ty, start_iv) = match init? {
            hir::Stmt::Let { name, ty, init, .. } if ty.is_integer() => {
                (name.clone(), ty.clone(), self.interval_of(init)?)
            }
            _ => return None,
        };
        // A constant start.
        if start_iv.lo != start_iv.hi {
            return None;
        }
        let start = start_iv.lo;
        // cond: name </<= BOUND.
        let (op, bound_iv) = match &cond?.kind {
            K::Binary { op, left, right } => match &left.kind {
                K::Local(n) if *n == name => (*op, self.interval_of(right)?),
                _ => return None,
            },
            _ => return None,
        };
        let hi = match op {
            hir::BinOp::Lt => bound_iv.hi.checked_sub(1)?,
            hir::BinOp::Le => bound_iv.hi,
            _ => return None,
        };
        // step: name += STEP, STEP a positive constant.
        let step_iv = match &step?.kind {
            K::Assign {
                op: Some(hir::BinOp::Add),
                target,
                value,
            } => match &target.kind {
                K::Local(n) if *n == name => self.interval_of(value)?,
                _ => return None,
            },
            _ => return None,
        };
        if step_iv.lo != step_iv.hi || step_iv.lo <= 0 {
            return None;
        }
        let stepv = step_iv.lo;
        // Empty or reversed range: the body is dead; keep intervals
        // well-formed by declining the proof.
        if start > hi {
            return None;
        }
        // The counter reaches at most `hi` inside the body, then the
        // step computes `hi + STEP`; that must not overflow the type,
        // or the counter could wrap to a value below `start` on a later
        // iteration and violate the lower bound.
        let tr = int_type_range(&ty)?;
        if start < tr.lo || hi.checked_add(stepv)? > tr.hi {
            return None;
        }
        // The counter must not be reassigned in the body (the step is
        // the only permitted mutation), or it is no longer monotonic.
        if stmts_assign_to(body, &name) {
            return None;
        }
        Some((name, Interval { lo: start, hi }))
    }

    fn iconst(&mut self, t: types::Type, v: i64) -> Value {
        let v = if t == types::I32 {
            i64::from(v as i32)
        } else if t == types::I16 {
            i64::from(v as i16)
        } else if t == types::I8 {
            i64::from(v as i8)
        } else {
            v
        };
        self.b.ins().iconst(t, v)
    }

    fn zero_of(&mut self, t: types::Type) -> Value {
        if t == types::F32 {
            self.b.ins().f32const(0.0f32)
        } else if t == types::F64 {
            self.b.ins().f64const(0.0f64)
        } else {
            self.iconst(t, 0)
        }
    }

    fn addr_off(&mut self, addr: Value, off: i64) -> Value {
        if off == 0 {
            addr
        } else {
            self.b.ins().iadd_imm(addr, off)
        }
    }

    fn temp_slot(&mut self, size: u32, align: u32) -> Value {
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size.max(1),
            align_shift(align.max(1)),
        ));
        self.b.ins().stack_addr(types::I64, slot, 0)
    }

    fn copy_bytes(&mut self, dest: Value, src: Value, size: u32, align: u32) {
        let config = self.ml.module.isa().frontend_config();
        // Plain flags: the helper widens its accesses beyond the
        // aggregate's alignment and sets the aligned flag itself only
        // when that is provably safe.
        self.b.emit_small_memory_copy(
            config,
            dest,
            src,
            u64::from(size),
            align.max(1) as u8,
            align.max(1) as u8,
            true,
            MemFlags::new(),
        );
    }

    fn zero_bytes(&mut self, dest: Value, size: u32, align: u32) {
        let config = self.ml.module.isa().frontend_config();
        self.b.emit_small_memset(
            config,
            dest,
            0,
            u64::from(size),
            align.max(1) as u8,
            MemFlags::new(),
        );
    }

    /// Loads a value of `ty` from `addr + off`.
    fn load_val(&mut self, ty: &Type, addr: Value, off: i32) -> Result<RV, String> {
        Ok(match self.ml.layouts.repr(ty)? {
            Repr::None => RV::None,
            Repr::Scalar(t) => RV::S(self.b.ins().load(t, flags(), addr, off)),
            Repr::Pair => {
                let code = self.b.ins().load(types::I64, flags(), addr, off);
                let env = self.b.ins().load(types::I64, flags(), addr, off + 8);
                RV::P(code, env)
            }
            Repr::Agg { .. } => RV::A(self.addr_off(addr, i64::from(off))),
        })
    }

    /// Stores `rv` (of `ty`) to `addr + off` (copy semantics for
    /// aggregates, C2).
    fn store_val(&mut self, ty: &Type, addr: Value, off: i32, rv: RV) -> Result<(), String> {
        match (self.ml.layouts.repr(ty)?, rv) {
            (Repr::None, _) => Ok(()),
            (Repr::Scalar(_), RV::S(v)) => {
                self.b.ins().store(flags(), v, addr, off);
                Ok(())
            }
            (Repr::Pair, RV::P(code, env)) => {
                self.b.ins().store(flags(), code, addr, off);
                self.b.ins().store(flags(), env, addr, off + 8);
                Ok(())
            }
            (Repr::Agg { size, align }, RV::A(src)) => {
                let dest = self.addr_off(addr, i64::from(off));
                self.copy_bytes(dest, src, size, align);
                Ok(())
            }
            // Chain-slot address-of (Q13's single implicit address-of): a
            // value struct written into a `Struct | null` boundary pointer
            // slot stores the *address* of the struct's storage. Confined
            // to this boundary-only slot type (C7); ordinary code never
            // reaches it. The struct must outlive the holder (lifetime
            // rule): here its storage is a live caller local/stack slot.
            (Repr::Scalar(_), RV::A(src)) if self.is_boundary_struct_ptr(ty) => {
                self.b.ins().store(flags(), src, addr, off);
                Ok(())
            }
            (r, v) => Err(internal(format!("store mismatch {r:?} vs {v:?}"))),
        }
    }

    fn expect_s(&self, rv: RV) -> Result<Value, String> {
        match rv {
            RV::S(v) => Ok(v),
            other => Err(internal(format!("expected scalar, got {other:?}"))),
        }
    }

    fn expect_a(&self, rv: RV) -> Result<Value, String> {
        match rv {
            RV::A(v) => Ok(v),
            other => Err(internal(format!("expected aggregate, got {other:?}"))),
        }
    }

    fn expect_p(&self, rv: RV) -> Result<(Value, Value), String> {
        match rv {
            RV::P(a, b) => Ok((a, b)),
            other => Err(internal(format!("expected function value, got {other:?}"))),
        }
    }

    // ----- traps and unwinding -----

    fn unwind_block(&mut self) -> Block {
        if let Some(u) = self.unwind {
            return u;
        }
        let u = self.b.create_block();
        self.unwind = Some(u);
        u
    }

    /// Emits the pending-trap check after a call that can fault.
    fn trap_check(&mut self) {
        let flag = self.b.ins().load(types::I32, flags(), self.ctx_v, 0);
        let cont = self.b.create_block();
        let uw = self.unwind_block();
        self.b.ins().brif(flag, uw, &[], cont, &[]);
        self.b.switch_to_block(cont);
    }

    /// Branches to a trap (kind, pos) when `ok` is false.
    fn guard(&mut self, ok: Value, kind: TrapKind, pos: &Pos) -> Result<(), String> {
        let cont = self.b.create_block();
        let bad = self.b.create_block();
        self.b.ins().brif(ok, cont, &[], bad, &[]);
        self.b.switch_to_block(bad);
        let kind_v = self.iconst(types::I32, i64::from(kind as u32));
        let pid = self.pos_id(pos);
        let pos_v = self.iconst(types::I32, pid);
        self.call_rt(self.ml.rt.trap, &[self.ctx_v, kind_v, pos_v], false)?;
        let uw = self.unwind_block();
        self.b.ins().jump(uw, &[]);
        self.b.switch_to_block(cont);
        Ok(())
    }

    /// Use-after-delete check on a reference-class / frame pointer
    /// (dev tier, Q6): the allocation header state must be LIVE.
    fn live_check(&mut self, ptr: Value, pos: &Pos) -> Result<(), String> {
        let state = self
            .b
            .ins()
            .load(types::I64, flags(), ptr, rtc::STATE_OFFSET);
        let ok = self
            .b
            .ins()
            .icmp_imm(IntCC::Equal, state, rtc::LIVE_STATE as i64);
        self.guard(ok, TrapKind::UseAfterDelete, pos)
    }

    /// Stale-coroutine check on `.next()` (`LowerOptions::reload`):
    /// the frame's creation epoch must still be the Context's epoch.
    /// A hot reload replaces every function body and bumps the epoch,
    /// so a coroutine suspended across a swap traps here, at the
    /// resume position.
    fn reload_epoch_check(&mut self, frame: Value, pos: &Pos) -> Result<(), String> {
        if !self.ml.opts.reload {
            return Ok(());
        }
        let epoch_off = ctx_off(rtc::Context::reload_epoch_offset())?;
        let now = self
            .b
            .ins()
            .load(types::I32, flags(), self.ctx_v, epoch_off);
        let made = self.b.ins().load(types::I32, flags(), frame, GEN_EPOCH_OFF);
        let ok = self.b.ins().icmp(IntCC::Equal, now, made);
        self.guard(ok, TrapKind::StaleCoroutine, pos)
    }

    // ----- calls -----

    fn call_rt(
        &mut self,
        f: cranelift_module::FuncId,
        args: &[Value],
        checked: bool,
    ) -> Result<Option<Value>, String> {
        let fref = self.ml.module.declare_func_in_func(f, self.b.func);
        let inst = self.b.ins().call(fref, args);
        let res = self.b.inst_results(inst).first().copied();
        if checked {
            self.trap_check();
        }
        Ok(res)
    }

    fn call_script(
        &mut self,
        key: &FnKey,
        args: &[Value],
    ) -> Result<Vec<Value>, String> {
        let inst = if self.ml.opts.reload {
            let (code, sigref) = indirect_target(self.ml, &mut self.b, self.ctx_v, key)?;
            self.b.ins().call_indirect(sigref, code, args)
        } else {
            let id = self.ml.func_id(key)?;
            let fref = self.ml.module.declare_func_in_func(id, self.b.func);
            self.b.ins().call(fref, args)
        };
        let res = self.b.inst_results(inst).to_vec();
        self.trap_check();
        Ok(res)
    }

    // ----- bindings -----

    fn shadow_addr(&mut self, idx: u32) -> Result<Value, String> {
        let base = self
            .shadow_base
            .ok_or_else(|| internal("shadow slot without shadow frame"))?;
        Ok(self.addr_off(base, i64::from(idx) * 8))
    }

    fn read_binding(&mut self, binding: &Binding) -> Result<RV, String> {
        match binding.storage {
            Storage::Var(v) => Ok(RV::S(self.b.use_var(v))),
            Storage::Pair(a, b) => {
                let code = self.b.use_var(a);
                let env = self.b.use_var(b);
                Ok(RV::P(code, env))
            }
            Storage::Addr(a) => self.load_val(&binding.ty.clone(), a, 0),
            Storage::Shadow(i) => {
                let addr = self.shadow_addr(i)?;
                Ok(RV::S(self.b.ins().load(types::I64, flags(), addr, 0)))
            }
            Storage::Frame(off) => {
                let frame = self
                    .genc
                    .as_ref()
                    .map(|g| g.frame)
                    .ok_or_else(|| internal("frame storage outside a generator"))?;
                self.load_val(&binding.ty.clone(), frame, off as i32)
            }
        }
    }

    fn place_of_binding(&mut self, binding: &Binding) -> Result<Place, String> {
        Ok(match binding.storage {
            Storage::Var(v) => Place::Var(v),
            Storage::Pair(a, b) => Place::Pair(a, b),
            Storage::Addr(a) => Place::Mem(a, 0),
            Storage::Shadow(i) => {
                let addr = self.shadow_addr(i)?;
                Place::Mem(addr, 0)
            }
            Storage::Frame(off) => {
                let frame = self
                    .genc
                    .as_ref()
                    .map(|g| g.frame)
                    .ok_or_else(|| internal("frame storage outside a generator"))?;
                Place::Mem(frame, off as i32)
            }
        })
    }

    /// Resolves a dynamic-array element place to its current address
    /// (bounds-checked; called at access time so growth between place
    /// computation and access cannot leave a dangling pointer).
    fn resolve_array_elem(
        &mut self,
        handle: Value,
        index: Value,
        pos_id: i64,
    ) -> Result<Value, String> {
        let pos_v = self.iconst(types::I32, pos_id);
        let r = self.call_rt(
            self.ml.rt.array_ptr,
            &[self.ctx_v, handle, index, pos_v],
            true,
        )?;
        r.ok_or_else(|| internal("array_ptr result"))
    }

    fn read_place(&mut self, p: Place, ty: &Type) -> Result<RV, String> {
        match p {
            Place::Var(v) => Ok(RV::S(self.b.use_var(v))),
            Place::Pair(a, b) => {
                let code = self.b.use_var(a);
                let env = self.b.use_var(b);
                Ok(RV::P(code, env))
            }
            Place::Mem(addr, off) => self.load_val(ty, addr, off),
            Place::ArrayElem {
                handle,
                index,
                pos_id,
            } => {
                let addr = self.resolve_array_elem(handle, index, pos_id)?;
                self.load_val(ty, addr, 0)
            }
        }
    }

    fn write_place(&mut self, p: Place, ty: &Type, rv: RV) -> Result<(), String> {
        match p {
            Place::Var(v) => {
                let x = self.expect_s(rv)?;
                self.b.def_var(v, x);
                Ok(())
            }
            Place::Pair(a, b) => {
                let (code, env) = self.expect_p(rv)?;
                self.b.def_var(a, code);
                self.b.def_var(b, env);
                Ok(())
            }
            Place::Mem(addr, off) => self.store_val(ty, addr, off, rv),
            Place::ArrayElem {
                handle,
                index,
                pos_id,
            } => {
                let addr = self.resolve_array_elem(handle, index, pos_id)?;
                self.store_val(ty, addr, 0, rv)
            }
        }
    }

    /// Allocates the storage for a new local *without* writing it.
    /// Managed scalars get a shadow slot; aggregates whose interior
    /// holds managed handles (e.g. `FixedArray` of references,
    /// `IterResult<string>`) live *inside* the shadow frame so the
    /// collector's conservative word scan sees every handle stored in
    /// them (M1). Splitting allocation from the write lets an aggregate
    /// initializer be built directly into the storage (§10.2), eliding
    /// the temporary a construct-then-copy would use.
    fn alloc_storage(&mut self, ty: &Type) -> Result<Storage, String> {
        Ok(if self.genc.is_some() {
            let g = self
                .genc
                .as_mut()
                .ok_or_else(|| internal("generator context"))?;
            let off = *g
                .let_offsets
                .get(g.next_let)
                .ok_or_else(|| internal("frame offset table exhausted"))?;
            g.next_let += 1;
            Storage::Frame(off)
        } else if is_managed(&self.ml.layouts, ty)? {
            let idx = self.next_shadow;
            self.next_shadow += 1;
            Storage::Shadow(idx)
        } else {
            match self.ml.layouts.repr(ty)? {
                Repr::Agg { size, align } => {
                    if has_managed_interior(&self.ml.layouts, ty)? {
                        let words = managed_words(&self.ml.layouts, ty)?;
                        let idx = self.next_shadow;
                        self.next_shadow += words;
                        let addr = self.shadow_addr(idx)?;
                        Storage::Addr(addr)
                    } else {
                        Storage::Addr(self.temp_slot(size, align))
                    }
                }
                Repr::Pair => {
                    let a = self.b.declare_var(types::I64);
                    let c = self.b.declare_var(types::I64);
                    Storage::Pair(a, c)
                }
                Repr::Scalar(t) => Storage::Var(self.b.declare_var(t)),
                Repr::None => Storage::Var(self.b.declare_var(types::I8)),
            }
        })
    }

    /// Declares a local from its initializer *expression*. When the
    /// local is an aggregate stored in memory, the initializer is built
    /// straight into that storage (§10.2 copy elision); otherwise the
    /// initializer is evaluated and written. The name is bound only
    /// after the initializer runs, so it cannot reference itself.
    fn declare_local(&mut self, name: &str, ty: &Type, init: &hir::Expr) -> Result<(), String> {
        let storage = self.alloc_storage(ty)?;
        let binding = Binding {
            ty: ty.clone(),
            storage,
            range: None,
        };
        let place = self.place_of_binding(&binding)?;
        match place {
            Place::Mem(addr, off) if matches!(self.ml.layouts.repr(ty)?, Repr::Agg { .. }) => {
                let dest = self.addr_off(addr, i64::from(off));
                self.eval_agg_into(init, dest, ty)?;
            }
            _ => {
                let rv = self.eval(init)?;
                self.write_place(place, ty, rv)?;
            }
        }
        self.bind(name, binding);
        Ok(())
    }

    // ----- expressions -----

    fn eval(&mut self, e: &hir::Expr) -> Result<RV, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Int(v) => {
                let t = match self.ml.layouts.repr(&e.ty)? {
                    Repr::Scalar(t) => t,
                    _ => types::I32,
                };
                Ok(RV::S(self.iconst(t, *v)))
            }
            K::Float(v) => {
                if e.ty == Type::F16 {
                    let wide = self.b.ins().f64const(*v);
                    let raw = self
                        .call_rt(self.ml.rt.f16_from_f64, &[wide], false)?
                        .ok_or_else(|| internal("f16 narrowing result"))?;
                    Ok(RV::S(raw))
                } else if e.ty == Type::F32 {
                    Ok(RV::S(self.b.ins().f32const(*v as f32)))
                } else {
                    Ok(RV::S(self.b.ins().f64const(*v)))
                }
            }
            K::Bool(v) => Ok(RV::S(self.iconst(types::I8, i64::from(*v)))),
            K::Str(s) => {
                let h = self.string_literal(s.as_bytes(), &e.pos)?;
                Ok(RV::S(h))
            }
            K::Null => Ok(RV::S(self.iconst(types::I64, 0))),
            K::This => {
                let (ptr, cid) = self
                    .this_v
                    .ok_or_else(|| internal("`this` outside a method"))?;
                if self.ml.layouts.class(cid)?.is_value {
                    Ok(RV::A(ptr))
                } else {
                    Ok(RV::S(ptr))
                }
            }
            K::Local(name) => {
                let binding = self.lookup(name)?;
                self.read_binding(&binding)
            }
            K::Global(name) => {
                let (addr, ty) = self.global_slot(name)?;
                self.load_val(&ty, addr, 0)
            }
            K::FuncRef(name) => {
                let id = wrapper_for(self.ml, name)?;
                let fref = self.ml.module.declare_func_in_func(id, self.b.func);
                let code = self.b.ins().func_addr(types::I64, fref);
                let env = self.iconst(types::I64, 0);
                Ok(RV::P(code, env))
            }
            K::EnumMember { value, .. } => Ok(RV::S(self.iconst(types::I32, *value))),
            K::Unary { op, operand } => {
                let v = self.eval(operand)?;
                let v = self.expect_s(v)?;
                Ok(RV::S(match op {
                    hir::UnOp::Neg => {
                        if operand.ty.is_float() {
                            self.b.ins().fneg(v)
                        } else {
                            self.b.ins().ineg(v)
                        }
                    }
                    hir::UnOp::Not => self.b.ins().bxor_imm(v, 1),
                    hir::UnOp::BitNot => self.b.ins().bnot(v),
                    _ => return Err(internal("unknown unary operator")),
                }))
            }
            K::Binary { op, left, right } => self.eval_binary(*op, left, right, &e.pos),
            K::Assign { op, target, value } => self.eval_assign(*op, target, value, &e.pos),
            K::Cast(inner) => {
                let v = self.eval(inner)?;
                self.eval_cast(v, &inner.ty, &e.ty, &e.pos)
            }
            K::Call { callee, args } => self.eval_call(callee, args, &e.ty, &e.pos, None),
            K::New { class, args } => self.eval_new(class.0, args, &e.pos, None),
            K::Field { obj, name } => {
                let (addr, off, fty) = self.field_addr(obj, name)?;
                self.load_val(&fty, addr, off)
            }
            K::Length(obj) => {
                let rv = self.eval(obj)?;
                match &obj.ty {
                    Type::Str => {
                        let h = self.expect_s(rv)?;
                        let r = self.call_rt(self.ml.rt.str_len, &[self.ctx_v, h], false)?;
                        r.map(RV::S).ok_or_else(|| internal("str_len result"))
                    }
                    Type::Array(_) => {
                        let h = self.expect_s(rv)?;
                        let r = self.call_rt(self.ml.rt.array_len, &[self.ctx_v, h], false)?;
                        r.map(RV::S).ok_or_else(|| internal("array_len result"))
                    }
                    Type::FixedArray(_, n) => Ok(RV::S(self.iconst(types::I32, i64::from(*n)))),
                    other => Err(internal(format!("length of {other:?}"))),
                }
            }
            K::Index { obj, index } => {
                let (addr, elem_ty) = self.index_addr(obj, index, &e.pos)?;
                self.load_val(&elem_ty, addr, 0)
            }
            K::ArrayLit(elems) => self.eval_array_lit(&e.ty, elems, &e.pos),
            K::Template(parts) => self.eval_template(parts, &e.pos),
            K::Lambda {
                params,
                ret,
                body,
                captures,
            } => self.eval_lambda(params, ret, body, captures, &e.pos),
            K::Yield(arg) => self.eval_yield(arg.as_deref(), &e.pos),
            K::Cond { cond, then, els } => {
                let c = self.eval(cond)?;
                let c = self.expect_s(c)?;
                let then_blk = self.b.create_block();
                let else_blk = self.b.create_block();
                let merge = self.b.create_block();
                let repr = self.ml.layouts.repr(&e.ty)?;
                let n_params = match repr {
                    Repr::None => 0,
                    Repr::Scalar(t) => {
                        self.b.append_block_param(merge, t);
                        1
                    }
                    Repr::Pair => {
                        self.b.append_block_param(merge, types::I64);
                        self.b.append_block_param(merge, types::I64);
                        2
                    }
                    Repr::Agg { .. } => {
                        self.b.append_block_param(merge, types::I64);
                        1
                    }
                };
                self.b.ins().brif(c, then_blk, &[], else_blk, &[]);
                for (blk, side) in [(then_blk, then), (else_blk, els)] {
                    self.b.switch_to_block(blk);
                    let rv = self.eval(side)?;
                    let args: Vec<BlockArg> = match rv {
                        RV::None => vec![],
                        RV::S(v) | RV::A(v) => vec![BlockArg::Value(v)],
                        RV::P(a, b) => vec![BlockArg::Value(a), BlockArg::Value(b)],
                    };
                    self.b.ins().jump(merge, args.iter());
                }
                self.b.switch_to_block(merge);
                let params = self.b.block_params(merge).to_vec();
                Ok(match (n_params, repr) {
                    (0, _) => RV::None,
                    (1, Repr::Agg { .. }) => RV::A(params[0]),
                    (1, _) => RV::S(params[0]),
                    (_, _) => RV::P(params[0], params[1]),
                })
            }
            other => Err(internal(format!("expression kind {other:?}"))),
        }
    }

    fn string_literal(&mut self, bytes: &[u8], pos: &Pos) -> Result<Value, String> {
        let data = self.ml.literal_data(bytes)?;
        let gv = self.ml.module.declare_data_in_func(data, self.b.func);
        let addr = self.b.ins().symbol_value(types::I64, gv);
        let len = self.iconst(types::I64, bytes.len() as i64);
        let pid = self.pos_id(pos);
        let pos_v = self.iconst(types::I32, pid);
        let r = self.call_rt(self.ml.rt.str_lit, &[self.ctx_v, addr, len, pos_v], true)?;
        r.ok_or_else(|| internal("str_lit result"))
    }

    fn global_slot(&mut self, name: &str) -> Result<(Value, Type), String> {
        let (slot, ty) = self
            .ml
            .globals
            .get(name)
            .cloned()
            .ok_or_else(|| internal(format!("unknown global `{name}`")))?;
        let addr = match slot {
            GlobalSlot::Data(data) => {
                let gv = self.ml.module.declare_data_in_func(data, self.b.func);
                self.b.ins().symbol_value(types::I64, gv)
            }
            GlobalSlot::Offset(off) => {
                let base_off = ctx_off(rtc::Context::globals_offset())?;
                let base = self.b.ins().load(types::I64, flags(), self.ctx_v, base_off);
                self.addr_off(base, i64::from(off))
            }
        };
        Ok((addr, ty))
    }

    fn eval_binary(
        &mut self,
        op: hir::BinOp,
        left: &hir::Expr,
        right: &hir::Expr,
        pos: &Pos,
    ) -> Result<RV, String> {
        use hir::BinOp as B;
        // Short-circuit boolean operators.
        if matches!(op, B::And | B::Or) {
            let l = self.eval(left)?;
            let l = self.expect_s(l)?;
            let rhs_blk = self.b.create_block();
            let merge = self.b.create_block();
            self.b.append_block_param(merge, types::I8);
            let l_arg = [BlockArg::Value(l)];
            if op == B::And {
                self.b.ins().brif(l, rhs_blk, &[], merge, l_arg.iter());
            } else {
                self.b.ins().brif(l, merge, l_arg.iter(), rhs_blk, &[]);
            }
            self.b.switch_to_block(rhs_blk);
            let r = self.eval(right)?;
            let r = self.expect_s(r)?;
            let r_arg = [BlockArg::Value(r)];
            self.b.ins().jump(merge, r_arg.iter());
            self.b.switch_to_block(merge);
            return Ok(RV::S(self.b.block_params(merge)[0]));
        }

        let operand_ty = if left.ty == Type::Null {
            right.ty.clone()
        } else {
            left.ty.clone()
        };

        // String operations.
        if operand_ty == Type::Str {
            let l = self.eval(left)?;
            let l = self.expect_s(l)?;
            let r = self.eval(right)?;
            let r = self.expect_s(r)?;
            return match op {
                B::Add => {
                    let pid = self.pos_id(pos);
                    let pos_v = self.iconst(types::I32, pid);
                    let res = self
                        .call_rt(self.ml.rt.str_concat, &[self.ctx_v, l, r, pos_v], true)?;
                    res.map(RV::S).ok_or_else(|| internal("concat result"))
                }
                B::Eq | B::Ne => {
                    let res = self.call_rt(self.ml.rt.str_eq, &[self.ctx_v, l, r], false)?;
                    let res = res.ok_or_else(|| internal("str_eq result"))?;
                    let cmp = if op == B::Eq {
                        self.b.ins().icmp_imm(IntCC::NotEqual, res, 0)
                    } else {
                        self.b.ins().icmp_imm(IntCC::Equal, res, 0)
                    };
                    Ok(RV::S(cmp))
                }
                _ => Err(internal("string operator")),
            };
        }

        let l = self.eval(left)?;
        let l = self.expect_s(l)?;
        let r = self.eval(right)?;
        let r = self.expect_s(r)?;
        if operand_ty == Type::F16 {
            let lw = self
                .call_rt(self.ml.rt.f16_to_f64, &[l], false)?
                .ok_or_else(|| internal("f16 left widening result"))?;
            let rw = self
                .call_rt(self.ml.rt.f16_to_f64, &[r], false)?
                .ok_or_else(|| internal("f16 right widening result"))?;
            let cc = match op {
                B::Eq => FloatCC::Equal,
                B::Ne => FloatCC::NotEqual,
                B::Lt => FloatCC::LessThan,
                B::Le => FloatCC::LessThanOrEqual,
                B::Gt => FloatCC::GreaterThan,
                B::Ge => FloatCC::GreaterThanOrEqual,
                other => return Err(internal(format!("f16 operator {other:?}"))),
            };
            return Ok(RV::S(self.b.ins().fcmp(cc, lw, rw)));
        }
        let float = operand_ty.is_float();
        let unsigned = is_unsigned(&operand_ty);
        let r = if matches!(op, B::Shl | B::Shr | B::UShr) {
            self.b.ins().band_imm(r, shift_mask(&operand_ty)?)
        } else {
            r
        };
        let out = match op {
            B::Add => {
                if float {
                    self.b.ins().fadd(l, r)
                } else {
                    self.b.ins().iadd(l, r)
                }
            }
            B::Sub => {
                if float {
                    self.b.ins().fsub(l, r)
                } else {
                    self.b.ins().isub(l, r)
                }
            }
            B::Mul => {
                if float {
                    self.b.ins().fmul(l, r)
                } else {
                    self.b.ins().imul(l, r)
                }
            }
            B::Div | B::Rem => {
                if float {
                    if op == B::Div {
                        self.b.ins().fdiv(l, r)
                    } else {
                        // f64/f32 remainder is not exercised by the
                        // corpus surface; integer-only per C3.
                        return Err(internal("float remainder"));
                    }
                } else {
                    return self.int_divrem(op == B::Div, l, r, unsigned, pos).map(RV::S);
                }
            }
            B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge => {
                let v = if float {
                    let cc = match op {
                        B::Eq => FloatCC::Equal,
                        B::Ne => FloatCC::NotEqual,
                        B::Lt => FloatCC::LessThan,
                        B::Le => FloatCC::LessThanOrEqual,
                        B::Gt => FloatCC::GreaterThan,
                        _ => FloatCC::GreaterThanOrEqual,
                    };
                    self.b.ins().fcmp(cc, l, r)
                } else {
                    let cc = match (op, unsigned) {
                        (B::Eq, _) => IntCC::Equal,
                        (B::Ne, _) => IntCC::NotEqual,
                        (B::Lt, false) => IntCC::SignedLessThan,
                        (B::Le, false) => IntCC::SignedLessThanOrEqual,
                        (B::Gt, false) => IntCC::SignedGreaterThan,
                        (B::Ge, false) => IntCC::SignedGreaterThanOrEqual,
                        (B::Lt, true) => IntCC::UnsignedLessThan,
                        (B::Le, true) => IntCC::UnsignedLessThanOrEqual,
                        (B::Gt, true) => IntCC::UnsignedGreaterThan,
                        _ => IntCC::UnsignedGreaterThanOrEqual,
                    };
                    self.b.ins().icmp(cc, l, r)
                };
                v
            }
            B::BitAnd => self.b.ins().band(l, r),
            B::BitOr => self.b.ins().bor(l, r),
            B::BitXor => self.b.ins().bxor(l, r),
            B::Shl => self.b.ins().ishl(l, r),
            B::Shr => {
                if unsigned {
                    self.b.ins().ushr(l, r)
                } else {
                    self.b.ins().sshr(l, r)
                }
            }
            B::UShr => self.b.ins().ushr(l, r),
            _ => return Err(internal("unknown binary operator")),
        };
        Ok(RV::S(out))
    }

    /// Integer division/remainder with explicit checks so faults trap
    /// through the runtime instead of the hardware: divide-by-zero
    /// traps; `MIN / -1` wraps to `MIN` (two's complement), `MIN % -1`
    /// is 0.
    fn int_divrem(
        &mut self,
        is_div: bool,
        l: Value,
        r: Value,
        unsigned: bool,
        pos: &Pos,
    ) -> Result<Value, String> {
        let nonzero = self.b.ins().icmp_imm(IntCC::NotEqual, r, 0);
        self.guard(nonzero, TrapKind::DivisionByZero, pos)?;
        if unsigned {
            return Ok(if is_div {
                self.b.ins().udiv(l, r)
            } else {
                self.b.ins().urem(l, r)
            });
        }
        let ty = self.b.func.dfg.value_type(l);
        let is_m1 = self.b.ins().icmp_imm(IntCC::Equal, r, -1);
        let m1_blk = self.b.create_block();
        let div_blk = self.b.create_block();
        let merge = self.b.create_block();
        self.b.append_block_param(merge, ty);
        self.b.ins().brif(is_m1, m1_blk, &[], div_blk, &[]);
        self.b.switch_to_block(m1_blk);
        let m1_res = if is_div {
            self.b.ins().ineg(l)
        } else {
            self.zero_of(ty)
        };
        let m1_arg = [BlockArg::Value(m1_res)];
        self.b.ins().jump(merge, m1_arg.iter());
        self.b.switch_to_block(div_blk);
        let d_res = if is_div {
            self.b.ins().sdiv(l, r)
        } else {
            self.b.ins().srem(l, r)
        };
        let d_arg = [BlockArg::Value(d_res)];
        self.b.ins().jump(merge, d_arg.iter());
        self.b.switch_to_block(merge);
        Ok(self.b.block_params(merge)[0])
    }

    fn eval_cast(&mut self, rv: RV, from: &Type, to: &Type, pos: &Pos) -> Result<RV, String> {
        // Reference narrowing: object / object|null -> reference class.
        if let Type::Class(cid) = to {
            if matches!(from, Type::Object)
                || matches!(from, Type::Nullable(inner) if **inner == Type::Object)
            {
                let ptr = self.expect_s(rv)?;
                let nonnull = self.b.ins().icmp_imm(IntCC::NotEqual, ptr, 0);
                self.guard(nonnull, TrapKind::NullNarrowing, pos)?;
                self.live_check(ptr, pos)?;
                let class_id = self
                    .b
                    .ins()
                    .load(types::I32, flags(), ptr, rtc::CLASS_ID_OFFSET);
                let ok = self
                    .b
                    .ins()
                    .icmp_imm(IntCC::Equal, class_id, i64::from(cid.0 as u32));
                self.guard(ok, TrapKind::ClassMismatch, pos)?;
                return Ok(RV::S(ptr));
            }
        }
        let v = self.expect_s(rv)?;
        let (from, to) = (from.clone(), to.clone());
        // Enum source behaves as i32.
        let from = if matches!(from, Type::Enum(_)) {
            Type::I32
        } else {
            from
        };
        if from == to {
            return Ok(RV::S(v));
        }
        if to == Type::F16 {
            let wide = match from {
                Type::F32 => self.b.ins().fpromote(types::F64, v),
                Type::F64 => v,
                other => return Err(internal(format!("cast {other:?} -> f16"))),
            };
            let raw = self
                .call_rt(self.ml.rt.f16_from_f64, &[wide], false)?
                .ok_or_else(|| internal("f16 narrowing result"))?;
            return Ok(RV::S(raw));
        }
        if from == Type::F16 {
            let wide = self
                .call_rt(self.ml.rt.f16_to_f64, &[v], false)?
                .ok_or_else(|| internal("f16 widening result"))?;
            return Ok(RV::S(match to {
                Type::F32 => self.b.ins().fdemote(types::F32, wide),
                Type::F64 => wide,
                other => return Err(internal(format!("cast f16 -> {other:?}"))),
            }));
        }
        if from.is_integer() && to.is_integer() {
            let from_ty = match self.ml.layouts.repr(&from)? {
                Repr::Scalar(t) => t,
                other => return Err(internal(format!("integer source repr {other:?}"))),
            };
            let to_ty = match self.ml.layouts.repr(&to)? {
                Repr::Scalar(t) => t,
                other => return Err(internal(format!("integer target repr {other:?}"))),
            };
            let out = if from_ty == to_ty {
                v
            } else if from_ty.bits() < to_ty.bits() {
                if is_unsigned(&from) {
                    self.b.ins().uextend(to_ty, v)
                } else {
                    self.b.ins().sextend(to_ty, v)
                }
            } else {
                self.b.ins().ireduce(to_ty, v)
            };
            return Ok(RV::S(out));
        }
        if from.is_integer() && matches!(to, Type::F32 | Type::F64) {
            let target = if to == Type::F32 { types::F32 } else { types::F64 };
            let out = if is_unsigned(&from) {
                self.b.ins().fcvt_from_uint(target, v)
            } else {
                self.b.ins().fcvt_from_sint(target, v)
            };
            return Ok(RV::S(out));
        }
        if matches!(from, Type::F32 | Type::F64) && to.is_integer() {
            let target = match self.ml.layouts.repr(&to)? {
                Repr::Scalar(t) => t,
                other => return Err(internal(format!("integer target repr {other:?}"))),
            };
            let out = if is_unsigned(&to) {
                self.b.ins().fcvt_to_uint_sat(target, v)
            } else {
                self.b.ins().fcvt_to_sint_sat(target, v)
            };
            return Ok(RV::S(out));
        }
        let out = match (&from, &to) {
            // float -> float
            (Type::F32, Type::F64) => self.b.ins().fpromote(types::F64, v),
            (Type::F64, Type::F32) => self.b.ins().fdemote(types::F32, v),
            (a, b) => return Err(internal(format!("cast {a:?} -> {b:?}"))),
        };
        Ok(RV::S(out))
    }

    /// Field address resolution shared by reads and writes. Returns
    /// `(addr, offset, field type)`; emits the use-after-delete check
    /// for reference-class receivers.
    fn field_addr(
        &mut self,
        obj: &hir::Expr,
        name: &str,
    ) -> Result<(Value, i32, Type), String> {
        match &obj.ty {
            Type::IterResult(v) => {
                let rv = self.eval(obj)?;
                let base = self.expect_a(rv)?;
                match name {
                    "done" => Ok((base, 0, Type::Bool)),
                    "value" => {
                        let off = self.ml.layouts.iter_result_value_offset(v)?;
                        Ok((base, off as i32, (**v).clone()))
                    }
                    _ => Err(internal(format!("IterResult member `{name}`"))),
                }
            }
            Type::Class(cid) => {
                let hirm = self.ml.hir;
                let class = hirm
                    .classes
                    .get(cid.0)
                    .ok_or_else(|| internal("class id out of range"))?;
                let idx = class
                    .fields
                    .iter()
                    .position(|f| f.name == name)
                    .ok_or_else(|| internal(format!("no field `{name}`")))?;
                let fty = class.fields[idx].ty.clone();
                let layout = self.ml.layouts.class(cid.0)?;
                let off = *layout
                    .field_offsets
                    .get(idx)
                    .ok_or_else(|| internal("field offset out of range"))?
                    as i32;
                let is_value = layout.is_value;
                let rv = self.eval(obj)?;
                if is_value {
                    let base = self.expect_a(rv)?;
                    Ok((base, off, fty))
                } else {
                    let ptr = self.expect_s(rv)?;
                    self.live_check(ptr, &obj.pos)?;
                    Ok((ptr, off, fty))
                }
            }
            other => Err(internal(format!("field access on {other:?}"))),
        }
    }

    /// Element address for indexing; bounds-checked. Returns
    /// `(addr, element type)`. Evaluation order: object, then index.
    fn index_addr(
        &mut self,
        obj: &hir::Expr,
        index: &hir::Expr,
        pos: &Pos,
    ) -> Result<(Value, Type), String> {
        match &obj.ty {
            Type::FixedArray(elem, n) => {
                let rv = self.eval(obj)?;
                let base = self.expect_a(rv)?;
                let idx_rv = self.eval(index)?;
                let idx = self.expect_s(idx_rv)?;
                // Proof-based elision (§10.1): emit the check only when
                // the index is not proven in `[0, n)`. A proven index
                // has no reachable trap, so removing it changes no
                // observable behaviour; an unproven one keeps the
                // unsigned compare, which rejects negatives and `>= n`
                // at once. The branch, not just the compare, is what
                // forecloses vectorization of the inner loop.
                if !self.index_in_bounds(index, *n) {
                    let ok = self
                        .b
                        .ins()
                        .icmp_imm(IntCC::UnsignedLessThan, idx, i64::from(*n));
                    self.guard(ok, TrapKind::IndexOutOfBounds, pos)?;
                }
                let stride = self.ml.layouts.stride(elem)?;
                let idx64 = self.b.ins().uextend(types::I64, idx);
                let scaled = self.b.ins().imul_imm(idx64, i64::from(stride));
                let addr = self.b.ins().iadd(base, scaled);
                Ok((addr, (**elem).clone()))
            }
            Type::Array(elem) => {
                let rv = self.eval(obj)?;
                let h = self.expect_s(rv)?;
                let idx_rv = self.eval(index)?;
                let idx = self.expect_s(idx_rv)?;
                let pid = self.pos_id(pos);
                let addr = self.resolve_array_elem(h, idx, pid)?;
                Ok((addr, (**elem).clone()))
            }
            other => Err(internal(format!("index on {other:?}"))),
        }
    }

    fn place(&mut self, e: &hir::Expr) -> Result<(Place, Type), String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Local(name) => {
                let binding = self.lookup(name)?;
                let p = self.place_of_binding(&binding)?;
                Ok((p, binding.ty))
            }
            K::Global(name) => {
                let (addr, ty) = self.global_slot(name)?;
                Ok((Place::Mem(addr, 0), ty))
            }
            K::Field { obj, name } => {
                let (addr, off, fty) = self.field_addr(obj, name)?;
                Ok((Place::Mem(addr, off), fty))
            }
            K::Index { obj, index } => {
                if let Type::Array(elem) = &obj.ty {
                    // Deferred: the element address is resolved at the
                    // moment of the access, after the assigned value
                    // has been evaluated (growth-safe).
                    let rv = self.eval(obj)?;
                    let handle = self.expect_s(rv)?;
                    let idx_rv = self.eval(index)?;
                    let idx = self.expect_s(idx_rv)?;
                    let pid = self.pos_id(&e.pos);
                    return Ok((
                        Place::ArrayElem {
                            handle,
                            index: idx,
                            pos_id: pid,
                        },
                        (**elem).clone(),
                    ));
                }
                let (addr, elem_ty) = self.index_addr(obj, index, &e.pos)?;
                Ok((Place::Mem(addr, 0), elem_ty))
            }
            other => Err(internal(format!("assignment target {other:?}"))),
        }
    }

    fn eval_assign(
        &mut self,
        op: Option<hir::BinOp>,
        target: &hir::Expr,
        value: &hir::Expr,
        pos: &Pos,
    ) -> Result<RV, String> {
        let (place, ty) = self.place(target)?;
        match op {
            None => {
                // §10.2: when the target is an aggregate at a stable
                // address (a local, global, field, or in-place
                // `FixedArray` element — never a growth-relocatable
                // dynamic-array element), build the RHS straight into
                // it, eliding the construct-then-copy temporary. C2's
                // observable copy semantics are unchanged: a plain
                // `b = a` still copies (the fallback path), and only a
                // freshly produced aggregate is written in place.
                if matches!(self.ml.layouts.repr(&ty)?, Repr::Agg { .. }) {
                    if let Place::Mem(addr, off) = place {
                        let dest = self.addr_off(addr, i64::from(off));
                        self.eval_agg_into(value, dest, &ty)?;
                        return Ok(RV::A(dest));
                    }
                }
                let rv = self.eval(value)?;
                // Copy semantics for aggregates: the write below copies
                // bytes into the target's own storage (C2).
                self.write_place(place, &ty, rv)?;
                Ok(rv)
            }
            Some(bin) => {
                let cur = self.read_place(place, &ty)?;
                let cur_v = self.expect_s(cur)?;
                let rhs = self.eval(value)?;
                let rhs_v = self.expect_s(rhs)?;
                let combined = self.apply_binop(bin, &ty, cur_v, rhs_v, pos)?;
                self.write_place(place, &ty, RV::S(combined))?;
                Ok(RV::S(combined))
            }
        }
    }

    /// Scalar compound-assignment operator on already-evaluated
    /// operands of type `ty`.
    fn apply_binop(
        &mut self,
        op: hir::BinOp,
        ty: &Type,
        l: Value,
        r: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        use hir::BinOp as B;
        let float = ty.is_float();
        let unsigned = is_unsigned(ty);
        let r = if matches!(op, B::Shl | B::Shr | B::UShr) {
            self.b.ins().band_imm(r, shift_mask(ty)?)
        } else {
            r
        };
        Ok(match op {
            B::Add => {
                if *ty == Type::Str {
                    let pid = self.pos_id(pos);
                    let pos_v = self.iconst(types::I32, pid);
                    let res =
                        self.call_rt(self.ml.rt.str_concat, &[self.ctx_v, l, r, pos_v], true)?;
                    res.ok_or_else(|| internal("concat result"))?
                } else if float {
                    self.b.ins().fadd(l, r)
                } else {
                    self.b.ins().iadd(l, r)
                }
            }
            B::Sub => {
                if float {
                    self.b.ins().fsub(l, r)
                } else {
                    self.b.ins().isub(l, r)
                }
            }
            B::Mul => {
                if float {
                    self.b.ins().fmul(l, r)
                } else {
                    self.b.ins().imul(l, r)
                }
            }
            B::Div => {
                if float {
                    self.b.ins().fdiv(l, r)
                } else {
                    self.int_divrem(true, l, r, unsigned, pos)?
                }
            }
            B::Rem => self.int_divrem(false, l, r, unsigned, pos)?,
            B::BitAnd => self.b.ins().band(l, r),
            B::BitOr => self.b.ins().bor(l, r),
            B::BitXor => self.b.ins().bxor(l, r),
            B::Shl => self.b.ins().ishl(l, r),
            B::Shr => {
                if unsigned {
                    self.b.ins().ushr(l, r)
                } else {
                    self.b.ins().sshr(l, r)
                }
            }
            B::UShr => self.b.ins().ushr(l, r),
            other => return Err(internal(format!("compound operator {other:?}"))),
        })
    }

    /// Materializes a value into memory and returns its address (for
    /// runtime calls that copy from a source pointer).
    fn materialize(&mut self, rv: RV, ty: &Type) -> Result<Value, String> {
        match rv {
            RV::A(ptr) => Ok(ptr),
            RV::S(v) => {
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let slot = self.temp_slot(size.max(8), align.max(8));
                self.b.ins().store(flags(), v, slot, 0);
                Ok(slot)
            }
            RV::P(code, env) => {
                let slot = self.temp_slot(16, 8);
                self.b.ins().store(flags(), code, slot, 0);
                self.b.ins().store(flags(), env, slot, 8);
                Ok(slot)
            }
            RV::None => Err(internal("materialize of void")),
        }
    }

    /// Copies an aggregate into a fresh caller-owned temp (C2
    /// copy-on-pass) and returns the temp's address.
    fn copy_to_temp(&mut self, src: Value, ty: &Type) -> Result<Value, String> {
        let (size, align) = self.ml.layouts.size_align(ty)?;
        let slot = self.temp_slot(size, align);
        self.copy_bytes(slot, src, size, align);
        Ok(slot)
    }

    /// Appends one evaluated argument as ABI values (aggregates are
    /// copied into a fresh caller-owned temp — C2 copy-on-pass).
    fn push_one_arg(&mut self, out: &mut Vec<Value>, ty: &Type, rv: RV) -> Result<(), String> {
        match rv {
            RV::S(v) => out.push(v),
            RV::P(a, b) => {
                out.push(a);
                out.push(b);
            }
            RV::A(ptr) => {
                let copy = self.copy_to_temp(ptr, ty)?;
                out.push(copy);
            }
            RV::None => return Err(internal("void argument")),
        }
        Ok(())
    }

    /// Evaluates call arguments (filling defaults from the callee's
    /// declaration) into ABI values.
    fn push_args(
        &mut self,
        out: &mut Vec<Value>,
        params: &[hir::Param],
        args: &[hir::Expr],
    ) -> Result<(), String> {
        for (i, p) in params.iter().enumerate() {
            let rv = if let Some(a) = args.get(i) {
                self.eval(a)?
            } else {
                let d = p
                    .default
                    .as_ref()
                    .ok_or_else(|| internal(format!("missing argument `{}`", p.name)))?;
                self.eval(d)?
            };
            self.push_one_arg(out, &p.ty, rv)?;
        }
        Ok(())
    }

    /// Shapes a call's results according to the return type. `sret`
    /// is the temp the caller allocated when the return is aggregate.
    fn shape_results(
        &self,
        ret: &Type,
        results: &[Value],
        sret: Option<Value>,
    ) -> Result<RV, String> {
        Ok(match self.ml.layouts.repr(ret)? {
            Repr::None => RV::None,
            Repr::Agg { .. } => sret.map(RV::A).unwrap_or(RV::None),
            Repr::Pair => {
                let (a, b) = (results.first(), results.get(1));
                match (a, b) {
                    (Some(&a), Some(&b)) => RV::P(a, b),
                    _ => return Err(internal("missing pair results")),
                }
            }
            Repr::Scalar(_) => results.first().copied().map(RV::S).unwrap_or(RV::None),
        })
    }

    /// Allocates the by-value return slot (`sret`) for a call: the
    /// caller-supplied `dest` when the aggregate result is wanted in a
    /// known place (§10.2), otherwise a fresh temporary. `None` for a
    /// non-aggregate return.
    fn sret_slot(&mut self, ret: &Type, dest: Option<Value>) -> Result<Option<Value>, String> {
        Ok(match self.ml.layouts.repr(ret)? {
            Repr::Agg { size, align } => Some(match dest {
                Some(d) => d,
                None => self.temp_slot(size, align),
            }),
            _ => None,
        })
    }

    fn eval_call(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
        dest: Option<Value>,
    ) -> Result<RV, String> {
        match callee {
            hir::Callee::Func(name) => {
                let f = self.ml.hir_fn(name)?;
                if f.is_generator {
                    // Creator call: allocates and initializes the frame.
                    let mut argv = vec![self.ctx_v];
                    self.push_args(&mut argv, &f.params, args)?;
                    let res = self.call_script(&FnKey::Free(name.clone()), &argv)?;
                    return Ok(RV::S(
                        *res.first().ok_or_else(|| internal("creator result"))?,
                    ));
                }
                let mut argv = vec![self.ctx_v];
                let sret = self.sret_slot(&f.ret, dest)?;
                if let Some(s) = sret {
                    argv.push(s);
                }
                self.push_args(&mut argv, &f.params, args)?;
                let ret = f.ret.clone();
                let res = self.call_script(&FnKey::Free(name.clone()), &argv)?;
                self.shape_results(&ret, &res, sret)
            }
            hir::Callee::Ambient(a) => self.eval_ambient(*a, args, pos),
            hir::Callee::Math(f) => self.eval_math(*f, args),
            hir::Callee::Num(f) => self.eval_num(*f, args, pos),
            hir::Callee::Date(f) => self.eval_date(*f, args, pos),
            hir::Callee::Str(f) => self.eval_str(*f, args, pos),
            hir::Callee::Arr(f) => self.eval_arr(*f, args, ret_ty, pos),
            hir::Callee::Map(f) => self.eval_map(*f, args, ret_ty, pos),
            hir::Callee::Set(f) => self.eval_set(*f, args, ret_ty, pos),
            hir::Callee::Value(v) => {
                let ft = match &v.ty {
                    Type::Func(ft) => (**ft).clone(),
                    other => return Err(internal(format!("call of {other:?}"))),
                };
                let rv = self.eval(v)?;
                let (code, env) = self.expect_p(rv)?;
                let mut argv = vec![self.ctx_v, env];
                let sret = self.sret_slot(&ft.ret, dest)?;
                if let Some(s) = sret {
                    argv.push(s);
                }
                // Function-typed values have no defaults: arity is the
                // full parameter list.
                if args.len() != ft.params.len() {
                    return Err(internal(format!("indirect call arity at {pos}")));
                }
                for (t, a) in ft.params.iter().zip(args) {
                    let rv = self.eval(a)?;
                    self.push_one_arg(&mut argv, t, rv)?;
                }
                let sig = self.ml.make_sig(&ft.params, &ft.ret, true, false)?;
                let sigref = self.b.import_signature(sig);
                let inst = self.b.ins().call_indirect(sigref, code, &argv);
                let res = self.b.inst_results(inst).to_vec();
                self.trap_check();
                self.shape_results(&ft.ret, &res, sret)
            }
            hir::Callee::Method { recv, name } => {
                self.eval_method(recv, name, args, ret_ty, pos, dest)
            }
            hir::Callee::Foreign(name) => self.eval_foreign_call(name, args, pos),
            other => Err(internal(format!("callee {other:?}"))),
        }
    }

    /// Lowers a foreign C-ABI call (`Callee::Foreign`, P5.2b) to a direct
    /// call of the header symbol. The signature is built from the mirror's
    /// boundary types by marshaling each argument per Q13; the symbol is
    /// imported (`Linkage::Import`) exactly as the `sub_rt_*` runtime is,
    /// and resolved by the JIT's symbol registration / the ship-C link.
    fn eval_foreign_call(
        &mut self,
        name: &str,
        args: &[hir::Expr],
        pos: &Pos,
    ) -> Result<RV, String> {
        let ff = self.ml.foreign_fn(name)?;
        let params: Vec<Type> = ff.params.iter().map(|p| p.ty.clone()).collect();
        let ret = ff.ret.clone();
        if args.len() != params.len() {
            return Err(internal(format!("foreign call `{name}` arity at {pos}")));
        }
        let mut sig = Signature::new(self.ml.call_conv);
        // A by-value boundary-struct return (§14.2): plan the C-ABI return
        // before the arguments, since a large (sret) return prepends a
        // hidden result-pointer argument.
        let ret_repr = self.ml.layouts.repr(&ret)?;
        let struct_ret = if let Repr::Agg { size, align } = ret_repr {
            Some(self.plan_foreign_struct_return(&ret, size, align, &mut sig, pos)?)
        } else {
            None
        };
        let mut argv: Vec<Value> = Vec::new();
        if let Some(StructRet::Sret(slot)) = struct_ret {
            argv.push(slot);
        }
        for (ty, a) in params.iter().zip(args) {
            let rv = self.eval(a)?;
            self.marshal_foreign_arg(ty, rv, &mut sig, &mut argv)?;
        }
        match ret_repr {
            Repr::None | Repr::Agg { .. } => {}
            Repr::Scalar(t) => sig.returns.push(AbiParam::new(t)),
            other => {
                return Err(internal(format!(
                    "foreign return {other:?} is not a boundary form at {pos}"
                )))
            }
        }
        let id = if let Some(&id) = self.ml.foreign_ids.get(name) {
            id
        } else {
            let id = self
                .ml
                .module
                .declare_function(name, Linkage::Import, &sig)
                .map_err(|e| internal(format!("declare foreign {name}: {e}")))?;
            self.ml.foreign_ids.insert(name.to_string(), id);
            id
        };
        let fref = self.ml.module.declare_func_in_func(id, self.b.func);
        let inst = self.b.ins().call(fref, &argv);
        let res = self.b.inst_results(inst).to_vec();
        // A foreign call may set the Context trap flag — directly, or via
        // a callback that trapped inside the trampoline — so check it.
        self.trap_check();
        Ok(match ret_repr {
            Repr::Scalar(_) => RV::S(*res.first().ok_or_else(|| internal("foreign result"))?),
            Repr::Agg { .. } => {
                // Reconstruct the returned by-value struct into a language
                // slot (§14.2): sret already wrote it; a register return is
                // stored into the slot chunk-by-chunk.
                let sr = struct_ret.ok_or_else(|| internal("struct-return plan missing"))?;
                RV::A(self.finish_foreign_struct_return(sr, &res))
            }
            _ => RV::None,
        })
    }

    /// Plans the C-ABI return of a by-value boundary struct (§14.2),
    /// arch-gated by §12.3a exactly as by-value struct *arguments* are: the
    /// by-value aggregate ABI is target-specific, so only AAPCS64 and Win64
    /// are honored; any other host fails loud rather than silently
    /// mis-marshal (dev-JIT ≠ ship-C otherwise). A small struct is returned
    /// in registers (declared in `sig.returns`); a large one via `sret` (a
    /// hidden result pointer to a caller slot).
    ///
    /// A pure Homogeneous Floating-point Aggregate (all-`f32`/all-`f64`,
    /// 1–4 members) is returned in SIMD registers on both ABIs and is
    /// **rejected loud** here — the general-register paths below would read
    /// the wrong registers (a silent mismatch). Non-HFA returns:
    ///
    /// - **AAPCS64**: ≤ 16 bytes → general registers as `ceil(size/8)`
    ///   eightbyte integer chunks; larger → `sret`.
    /// - **Win64**: exactly 1/2/4/8 bytes → one integer register of that
    ///   width; every other size → `sret`.
    fn plan_foreign_struct_return(
        &mut self,
        ret: &Type,
        size: u32,
        align: u32,
        sig: &mut Signature,
        pos: &Pos,
    ) -> Result<StructRet, String> {
        let triple = self.ml.module.isa().triple().clone();
        if !crate::lower::boundary_struct_by_value_supported(&triple) {
            return Err(internal(format!(
                "foreign call returning a boundary struct by value is only supported \
                 on aarch64 (AAPCS64) and x86-64 Windows (Win64) in the dev JIT \
                 (compiler.md §12.3a); target {triple} is unsupported (at {pos})"
            )));
        }
        // A returned struct's fields must be plain data (scalars / nested
        // value structs). Callback and descriptor-embedded-array fields are
        // input-only marshaling idioms; a foreign function does not return
        // them by value.
        self.assert_returnable_struct(ret)?;
        // Pure Homogeneous Floating-point Aggregates (1–4 members all of the
        // same fundamental float type — all f32 or all f64) are returned in
        // SIMD registers (AAPCS64 v0–v3; Win64 XMM0), NOT the general
        // registers the paths below model. Marshaling them as integer
        // eightbytes would read the wrong registers — a silent dev-JIT ≠
        // ship-C mismatch. Both ABIs fail loud here (§12.3a: never a silent
        // mis-marshal); HFA returns are unsupported until the return path
        // models the SIMD registers. Non-HFA returns — all-integer, mixed
        // integer+float (returned in general registers), and non-homogeneous
        // or >4-member aggregates (returned via sret) — are unaffected.
        let leaves = self.return_leaf_clifs(ret)?;
        if is_pure_hfa_leaves(&leaves) {
            return Err(internal(format!(
                "foreign call returning a homogeneous floating-point aggregate \
                 (all-{} struct) by value is not supported in the dev JIT: AAPCS64/\
                 Win64 return it in SIMD registers, which the register-return path \
                 does not yet model (compiler.md §12.3a — fail loud, never a silent \
                 mis-marshal) (at {pos})",
                if leaves.first() == Some(&types::F32) { "f32" } else { "f64" }
            )));
        }
        if self.is_win64() {
            let width = match size {
                1 => Some(types::I8),
                2 => Some(types::I16),
                4 => Some(types::I32),
                8 => Some(types::I64),
                _ => None,
            };
            if let Some(w) = width {
                sig.returns.push(AbiParam::new(w));
                let slot = self.temp_slot(size, align);
                return Ok(StructRet::Reg {
                    slot,
                    chunks: vec![(0, w)],
                });
            }
            let slot = self.temp_slot(size, align);
            sig.params
                .push(AbiParam::special(types::I64, ArgumentPurpose::StructReturn));
            return Ok(StructRet::Sret(slot));
        }
        // AAPCS64.
        if size <= 16 {
            let mut chunks = Vec::new();
            let mut off = 0u32;
            while off < size {
                sig.returns.push(AbiParam::new(types::I64));
                chunks.push((off, types::I64));
                off += 8;
            }
            // The register image is up to `chunks.len() * 8` bytes; the slot
            // is sized to hold whole eightbyte stores without overrunning
            // (only the struct's own `size` bytes are ever read back).
            let slot = self.temp_slot(chunks.len() as u32 * 8, align.max(8));
            Ok(StructRet::Reg { slot, chunks })
        } else {
            let slot = self.temp_slot(size, align);
            sig.params
                .push(AbiParam::special(types::I64, ArgumentPurpose::StructReturn));
            Ok(StructRet::Sret(slot))
        }
    }

    /// Fails loud if a struct returned by value from a foreign call carries
    /// a field that is not plain data (callback pair or descriptor-embedded
    /// array — input-only marshaling idioms).
    fn assert_returnable_struct(&self, ret: &Type) -> Result<(), String> {
        let Type::Class(id) = ret else {
            return Err(internal("returnable-struct check on a non-class"));
        };
        let class = self
            .ml
            .hir
            .classes
            .get(id.0)
            .ok_or_else(|| internal("return struct class id out of range"))?;
        for f in &class.fields {
            match &f.ty {
                Type::Func(_) | Type::Array(_) => {
                    return Err(internal(format!(
                        "foreign return struct field `{}` is a {:?}, not plain data; \
                         callback/array fields are input-only boundary idioms",
                        f.name, f.ty
                    )))
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Flattens a return type into the CLIF types of its leaf scalars, in
    /// order (nested value structs and fixed arrays expanded), for the pure-
    /// HFA test. A non-scalar leaf (never reached for a returnable struct —
    /// callback/array fields are already rejected) contributes a non-float
    /// sentinel so the aggregate is not misread as an HFA.
    fn return_leaf_clifs(&self, ty: &Type) -> Result<Vec<types::Type>, String> {
        let mut out = Vec::new();
        self.collect_leaf_clifs(ty, &mut out)?;
        Ok(out)
    }

    fn collect_leaf_clifs(&self, ty: &Type, out: &mut Vec<types::Type>) -> Result<(), String> {
        match ty {
            Type::Class(id) if self.is_value_class_ty(ty) => {
                let class = self
                    .ml
                    .hir
                    .classes
                    .get(id.0)
                    .ok_or_else(|| internal("hfa leaf class id out of range"))?;
                for f in &class.fields {
                    self.collect_leaf_clifs(&f.ty, out)?;
                }
            }
            Type::FixedArray(elem, n) => {
                for _ in 0..*n {
                    self.collect_leaf_clifs(elem, out)?;
                }
            }
            other => out.push(match self.ml.layouts.repr(other)? {
                Repr::Scalar(t) => t,
                // A non-scalar leaf is not a float; the I64 sentinel makes
                // the aggregate non-homogeneous-float (not an HFA).
                _ => types::I64,
            }),
        }
        Ok(())
    }

    /// Materializes a planned struct return into its language slot and
    /// returns the slot address. `sret` already wrote the slot; a register
    /// return stores each returned chunk at its offset.
    fn finish_foreign_struct_return(&mut self, sr: StructRet, res: &[Value]) -> Value {
        match sr {
            StructRet::Sret(slot) => slot,
            StructRet::Reg { slot, chunks } => {
                for ((off, _ty), v) in chunks.iter().zip(res) {
                    self.b.ins().store(flags(), *v, slot, *off as i32);
                }
                slot
            }
        }
    }

    /// Appends one ABI value (type + value) to a foreign call's signature
    /// and argument list, keeping the two in lockstep.
    fn push_abi(&self, sig: &mut Signature, argv: &mut Vec<Value>, t: types::Type, v: Value) {
        sig.params.push(AbiParam::new(t));
        argv.push(v);
    }

    /// True when `ty` is a value class (a boundary struct / `@CStruct`).
    fn is_value_class_ty(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id)
            if self.ml.layouts.class(id.0).map(|l| l.is_value).unwrap_or(false))
    }

    /// True for a `Struct | null` boundary pointer slot (a nullable value
    /// class): the one place the language takes a value's address
    /// implicitly (Q13's chain-slot address-of).
    fn is_boundary_struct_ptr(&self, ty: &Type) -> bool {
        matches!(ty, Type::Nullable(inner) if self.is_value_class_ty(inner))
    }

    /// The pointer form of a `Struct | null` value: the address of a
    /// struct's storage (an aggregate), or the already-pointer scalar
    /// (`null` is 0). The struct must outlive the call — its storage is
    /// the caller's stack/local, live across the synchronous foreign call
    /// (the userdata/borrow lifetime rule).
    fn boundary_ptr(&self, rv: RV) -> Result<Value, String> {
        match rv {
            RV::A(addr) => Ok(addr),
            RV::S(v) => Ok(v),
            other => Err(internal(format!("boundary pointer from {other:?}"))),
        }
    }

    /// Marshals one evaluated argument to a foreign call's C-ABI values
    /// per Q13: `string` → `(const char*, size_t)`; `T[]` →
    /// `(const T*, size_t)`; a by-value boundary struct → its fields as
    /// eightbytes (with the callback trampoline for a function-pointer
    /// field); `Struct | null` → a nullable struct pointer; handles,
    /// `object | null`, and scalars → one value.
    fn marshal_foreign_arg(
        &mut self,
        ty: &Type,
        rv: RV,
        sig: &mut Signature,
        argv: &mut Vec<Value>,
    ) -> Result<(), String> {
        match ty {
            Type::Str => {
                // A length-carrying string view is the C aggregate
                // `{ const char *data; size_t len; }` (16 bytes, align 8),
                // passed BY VALUE — so its ABI is target-specific exactly
                // like any boundary struct (compiler.md §12.3a): AAPCS64
                // packs it into two registers, Win64 passes it by reference.
                let h = self.expect_s(rv)?;
                let data = self
                    .call_rt(self.ml.rt.str_data, &[self.ctx_v, h], false)?
                    .ok_or_else(|| internal("str_data result"))?;
                let len32 = self
                    .call_rt(self.ml.rt.str_len, &[self.ctx_v, h], false)?
                    .ok_or_else(|| internal("str_len result"))?;
                let len = self.b.ins().uextend(types::I64, len32);
                let comps = [(0u32, types::I64, data), (8u32, types::I64, len)];
                self.push_aggregate_abi(sig, argv, &comps, 16, 8);
                Ok(())
            }
            Type::Array(_) => {
                // A (pointer, count) descriptor is the C aggregate
                // `{ const T *items; size_t count; }` (16 bytes, align 8),
                // passed BY VALUE — target-specific ABI as above (§12.3a).
                let h = self.expect_s(rv)?;
                let data = self
                    .call_rt(self.ml.rt.array_data, &[self.ctx_v, h], false)?
                    .ok_or_else(|| internal("array_data result"))?;
                let len32 = self
                    .call_rt(self.ml.rt.array_len, &[self.ctx_v, h], false)?
                    .ok_or_else(|| internal("array_len result"))?;
                let count = self.b.ins().uextend(types::I64, len32);
                let comps = [(0u32, types::I64, data), (8u32, types::I64, count)];
                self.push_aggregate_abi(sig, argv, &comps, 16, 8);
                Ok(())
            }
            Type::Class(id) if self.is_value_class_ty(ty) => {
                let addr = self.expect_a(rv)?;
                self.marshal_boundary_struct(id.0, addr, sig, argv)
            }
            _ if self.is_boundary_struct_ptr(ty) => {
                let v = self.boundary_ptr(rv)?;
                self.push_abi(sig, argv, types::I64, v);
                Ok(())
            }
            _ => match self.ml.layouts.repr(ty)? {
                Repr::Scalar(t) => {
                    let v = self.expect_s(rv)?;
                    self.push_abi(sig, argv, t, v);
                    Ok(())
                }
                other => Err(internal(format!("foreign argument repr {other:?}"))),
            },
        }
    }

    /// The C-ABI size and alignment of one boundary-struct field: a
    /// function-pointer field is a single pointer (8) in the C struct,
    /// unlike the language `(code, env)` pair (16); scalars/enums keep
    /// their C size; pointers/`object`/`Struct | null` are 8.
    fn boundary_c_field(&self, ty: &Type) -> Result<(u32, u32), String> {
        Ok(match ty {
            Type::Func(_) | Type::Object | Type::Nullable(_) => (8, 8),
            // A descriptor-embedded `(count, pointer)` array field (§13.2)
            // is the C pair `size_t count; const T* ptr;` — 16 bytes, align 8.
            Type::Array(_) => (16, 8),
            Type::I8 | Type::U8 => (1, 1),
            Type::I16 | Type::U16 | Type::F16 => (2, 2),
            Type::I64 | Type::U64 | Type::F64 => (8, 8),
            Type::I32 | Type::U32 | Type::F32 | Type::Enum(_) => (4, 4),
            Type::Bool => (1, 1),
            Type::Class(id) if self.is_value_class_ty(ty) => {
                let l = self.ml.layouts.class(id.0)?;
                (l.size, l.align)
            }
            other => return Err(internal(format!("boundary C field type {other:?}"))),
        })
    }

    /// Marshals a by-value boundary struct to the C ABI. It builds the
    /// C-layout components (each pointer/scalar field a value; a
    /// function-pointer field → the generic trampoline plus a binding
    /// built from the following `userdata` slot — the callback-info idiom),
    /// then passes them the way the platform C ABI passes the struct.
    /// AAPCS64: a composite of ≤ 16 bytes goes in registers (the components
    /// as arguments); a larger one is passed by reference (built into a
    /// stack slot, its address passed — AAPCS64 B.4). Win64: a 1/2/4/8-byte
    /// aggregate goes in one integer register as its raw bytes; any other
    /// size is passed by reference. Both match how the C compiler passes it
    /// on the ship tier.
    fn marshal_boundary_struct(
        &mut self,
        cid: usize,
        addr: Value,
        sig: &mut Signature,
        argv: &mut Vec<Value>,
    ) -> Result<(), String> {
        // A boundary struct passed BY VALUE has a target-specific C ABI;
        // AAPCS64 (aarch64) and Win64 (x86-64/Windows) are implemented and
        // verified (compiler.md §12.3a). On any other dev host this must
        // fail loudly rather than silently mis-marshal (dev-JIT ≠ ship-C).
        // Genuinely scalar/single-pointer boundary args are target-neutral
        // and reach here through other paths; a (ptr,len) descriptor is a
        // 16-byte by-value aggregate and reaches the ABI-specific path here.
        let triple = self.ml.module.isa().triple().clone();
        if !crate::lower::boundary_struct_by_value_supported(&triple) {
            return Err(internal(format!(
                "foreign call passing a boundary struct by value is only supported \
                 on aarch64 (AAPCS64) and x86-64 Windows (Win64) in the dev JIT \
                 (compiler.md §12.3a); target {triple} is unsupported"
            )));
        }
        let class = self
            .ml
            .hir
            .classes
            .get(cid)
            .ok_or_else(|| internal("boundary struct class id out of range"))?;
        let layout = self.ml.layouts.class(cid)?.clone();
        // C-layout components: (byte offset in the C struct, CLIF type,
        // value). Offsets follow C struct rules over the C field sizes.
        let mut comps: Vec<(u32, types::Type, Value)> = Vec::new();
        let mut coff = 0u32;
        let mut struct_align = 1u32;
        let mut i = 0;
        while i < class.fields.len() {
            let field = &class.fields[i];
            let lang_off = *layout
                .field_offsets
                .get(i)
                .ok_or_else(|| internal("boundary field offset"))? as i32;
            let (cs, ca) = self.boundary_c_field(&field.ty)?;
            coff = round_up(coff, ca);
            struct_align = struct_align.max(ca);
            match &field.ty {
                Type::Func(_) => {
                    let code = self.b.ins().load(types::I64, flags(), addr, lang_off);
                    let env = self.b.ins().load(types::I64, flags(), addr, lang_off + 8);
                    let tref = self
                        .ml
                        .module
                        .declare_func_in_func(self.ml.rt.cb_trampoline, self.b.func);
                    let tramp = self.b.ins().func_addr(types::I64, tref);
                    comps.push((coff, types::I64, tramp));
                    coff += cs;
                    // The callback field is followed by one or two userdata
                    // slots (§14.4). The first is required; the second is
                    // present in a two-userdata callback-info. Both are bound
                    // into one binding record the trampoline reads; the C
                    // struct's first userdata slot carries the binding and any
                    // second slot carries null (the binding is authoritative).
                    let ud1_field = class
                        .fields
                        .get(i + 1)
                        .ok_or_else(|| internal("a callback field needs a following userdata slot"))?;
                    let ud1_lang = *layout
                        .field_offsets
                        .get(i + 1)
                        .ok_or_else(|| internal("userdata field offset"))?
                        as i32;
                    let ud1 = self.b.ins().load(types::I64, flags(), addr, ud1_lang);
                    let has_ud2 = class
                        .fields
                        .get(i + 2)
                        .map(|f| is_userdata_slot(&f.ty))
                        .unwrap_or(false);
                    let ud2 = if has_ud2 {
                        let ud2_lang = *layout
                            .field_offsets
                            .get(i + 2)
                            .ok_or_else(|| internal("second userdata field offset"))?
                            as i32;
                        self.b.ins().load(types::I64, flags(), addr, ud2_lang)
                    } else {
                        self.iconst(types::I64, 0)
                    };
                    let record = self
                        .call_rt(self.ml.rt.cb_bind, &[self.ctx_v, code, env, ud1, ud2], false)?
                        .ok_or_else(|| internal("cb_bind result"))?;
                    // First userdata C slot → the binding.
                    let (uds1, uda1) = self.boundary_c_field(&ud1_field.ty)?;
                    coff = round_up(coff, uda1);
                    struct_align = struct_align.max(uda1);
                    comps.push((coff, types::I64, record));
                    coff += uds1;
                    if has_ud2 {
                        // Second userdata C slot → null (the binding carries
                        // the real second userdata).
                        let ud2_field = &class.fields[i + 2];
                        let (uds2, uda2) = self.boundary_c_field(&ud2_field.ty)?;
                        coff = round_up(coff, uda2);
                        struct_align = struct_align.max(uda2);
                        let nullv = self.iconst(types::I64, 0);
                        comps.push((coff, types::I64, nullv));
                        coff += uds2;
                        i += 3;
                    } else {
                        i += 2;
                    }
                }
                Type::Array(_) => {
                    // Descriptor-embedded (count, pointer) array field
                    // (§13.2): the language struct holds one array handle at
                    // `lang_off`; the C struct wants (size_t count, const T*
                    // ptr) reconstructed count-first, both from the array's
                    // own backing store (zero-copy). `cs` is 16 (boundary_c_field).
                    let handle = self.b.ins().load(types::I64, flags(), addr, lang_off);
                    let data = self
                        .call_rt(self.ml.rt.array_data, &[self.ctx_v, handle], false)?
                        .ok_or_else(|| internal("array_data result"))?;
                    let len32 = self
                        .call_rt(self.ml.rt.array_len, &[self.ctx_v, handle], false)?
                        .ok_or_else(|| internal("array_len result"))?;
                    let count = self.b.ins().uextend(types::I64, len32);
                    comps.push((coff, types::I64, count));
                    comps.push((coff + 8, types::I64, data));
                    coff += cs;
                    i += 1;
                }
                other => {
                    let ty = other.clone();
                    let rv = self.load_val(&ty, addr, lang_off)?;
                    let clif = match self.ml.layouts.repr(&ty)? {
                        Repr::Scalar(t) => t,
                        other => {
                            return Err(internal(format!("boundary field repr {other:?}")))
                        }
                    };
                    let v = self.expect_s(rv)?;
                    comps.push((coff, clif, v));
                    coff += cs;
                    i += 1;
                }
            }
        }
        let total = round_up(coff, struct_align.max(1));
        self.push_aggregate_abi(sig, argv, &comps, total, struct_align.max(1));
        Ok(())
    }

    /// True when the JIT host targets the Win64 ABI (`x86_64` + Windows).
    fn is_win64(&self) -> bool {
        let t = self.ml.module.isa().triple();
        matches!(t.architecture, target_lexicon::Architecture::X86_64)
            && matches!(t.operating_system, target_lexicon::OperatingSystem::Windows)
    }

    /// Passes an aggregate of `total` bytes to a foreign call the way the
    /// target C ABI passes it (`specs/blocks/compiler.md` §12.3a). `comps`
    /// are its C-layout components — `(byte offset, CLIF type, value)`.
    ///
    /// - **AAPCS64**: ≤ 16 bytes → its components in registers (as
    ///   arguments); larger → by reference to a caller copy (B.4).
    /// - **Win64**: exactly 1/2/4/8 bytes → one integer register holding
    ///   the struct's raw bytes as a same-width integer (no HFA, no
    ///   multi-register packing); every other size → by reference.
    ///
    /// The non-Win64 branch reproduces the AAPCS64 rule, which is also the
    /// pre-existing behavior for `(ptr,len)` pairs on every non-Win64 host —
    /// a 16-byte by-value aggregate that occupies two registers on
    /// AAPCS64/SysV; only Win64 diverges (by reference at 16 bytes).
    fn push_aggregate_abi(
        &mut self,
        sig: &mut Signature,
        argv: &mut Vec<Value>,
        comps: &[(u32, types::Type, Value)],
        total: u32,
        align: u32,
    ) {
        if self.is_win64() {
            let int_ty = match total {
                1 => Some(types::I8),
                2 => Some(types::I16),
                4 => Some(types::I32),
                8 => Some(types::I64),
                _ => None,
            };
            if let Some(width) = int_ty {
                // Store the components at their offsets, then load the whole
                // slot back as one integer: the stored bytes are the Win64
                // register image for any field mix (including float fields).
                let slot = self.temp_slot(total, align.max(total));
                for (off, _, v) in comps {
                    self.b.ins().store(flags(), *v, slot, *off as i32);
                }
                let word = self.b.ins().load(width, flags(), slot, 0);
                self.push_abi(sig, argv, width, word);
            } else {
                // By reference: caller copy, its address passed.
                let slot = self.temp_slot(total, align);
                for (off, _, v) in comps {
                    self.b.ins().store(flags(), *v, slot, *off as i32);
                }
                self.push_abi(sig, argv, types::I64, slot);
            }
        } else if total <= 16 {
            // AAPCS64 small composite: passed in registers as its components.
            for (_, clif, v) in comps {
                self.push_abi(sig, argv, *clif, *v);
            }
        } else {
            // AAPCS64 large composite: passed by reference to a caller copy.
            let slot = self.temp_slot(total, align);
            for (off, _, v) in comps {
                self.b.ins().store(flags(), *v, slot, *off as i32);
            }
            self.push_abi(sig, argv, types::I64, slot);
        }
    }

    /// Lowers a `Math.<fn>` intrinsic (stdlib.md §1) to its opaque
    /// `sub_rt_math_*` runtime call. `clz32` is `(ctx, u32) -> i32`;
    /// all others use `f64`. No trap check follows: the runtime entries
    /// never trap (pure, or a PRNG state advance). Constants never reach
    /// here — they folded to literals at check time.
    fn eval_math(&mut self, f: hir::MathFn, args: &[hir::Expr]) -> Result<RV, String> {
        if args.len() != f.arity() {
            return Err(internal(format!("Math.{} arity", f.name())));
        }
        let mut argv = vec![self.ctx_v];
        for a in args {
            let rv = self.eval(a)?;
            argv.push(self.expect_s(rv)?);
        }
        let res = self.call_rt(self.ml.rt.math[f as usize], &argv, false)?;
        res.map(RV::S)
            .ok_or_else(|| internal(format!("Math.{} result", f.name())))
    }

    /// Lowers a Q25/Q26 Number or parser intrinsic to its opaque runtime
    /// symbol. The checker fixes every arity, normalizes optional
    /// `toExponential` digits, and widens `f32` receivers where required.
    fn eval_num(
        &mut self,
        f: hir::NumFn,
        args: &[hir::Expr],
        pos: &Pos,
    ) -> Result<RV, String> {
        use hir::NumFn as N;
        let expected = match f {
            N::IsNaN | N::IsFinite | N::IsInteger | N::IsSafeInteger | N::ParseFloat => 1,
            N::ParseInt
            | N::ToFixed
            | N::ToStringF32
            | N::ToStringF64
            | N::ToExponential
            | N::ToPrecision => 2,
            other => return Err(internal(format!("unknown NumFn {other:?}"))),
        };
        if args.len() != expected {
            return Err(internal(format!("{} arity", f.name())));
        }
        let mut argv = vec![self.ctx_v];
        for arg in args {
            let value = self.eval(arg)?;
            argv.push(self.expect_s(value)?);
        }
        if f.takes_pos_id() {
            let pid = self.pos_id(pos);
            argv.push(self.iconst(types::I32, pid));
        }
        let traps = f.takes_pos_id();
        let result = self
            .call_rt(self.ml.rt.num[f as usize], &argv, traps)?
            .ok_or_else(|| internal(format!("{} result", f.name())))?;
        Ok(RV::S(if f.returns_bool() {
            self.b.ins().icmp_imm(IntCC::NotEqual, result, 0)
        } else {
            result
        }))
    }

    /// Lowers a `Date` intrinsic (stdlib.md §3) to its opaque
    /// `sub_rt_date_*` runtime call. A Date value is its `i64`
    /// millisecond representation (`Type::Date` reprs as I64); the
    /// range-checked operations (`new`, `UTC`, `toISOString`) are
    /// fault-capable and followed by a trap check, the accessors and
    /// `now` are not. `getTime` never reaches here — it folded to the
    /// receiver at check time.
    fn eval_date(
        &mut self,
        f: hir::DateFn,
        args: &[hir::Expr],
        pos: &Pos,
    ) -> Result<RV, String> {
        use hir::DateFn as D;
        let scalar_arg = |this: &mut Self, e: &hir::Expr| -> Result<Value, String> {
            let rv = this.eval(e)?;
            this.expect_s(rv)
        };
        match f {
            D::New => {
                let ms = scalar_arg(self, args.first().ok_or_else(|| internal("Date arity"))?)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(self.ml.rt.date_new, &[self.ctx_v, ms, pos_v], true)?;
                res.map(RV::S).ok_or_else(|| internal("Date result"))
            }
            D::Utc => {
                if args.len() != 7 {
                    return Err(internal("Date.UTC arity (checker normalizes to 7)"));
                }
                let mut argv = vec![self.ctx_v];
                for a in args {
                    argv.push(scalar_arg(self, a)?);
                }
                let pid = self.pos_id(pos);
                argv.push(self.iconst(types::I32, pid));
                let res = self.call_rt(self.ml.rt.date_utc, &argv, true)?;
                res.map(RV::S).ok_or_else(|| internal("Date.UTC result"))
            }
            D::Now => {
                let res = self.call_rt(self.ml.rt.date_now, &[self.ctx_v], false)?;
                res.map(RV::S).ok_or_else(|| internal("Date.now result"))
            }
            D::ToIso => {
                let ms = scalar_arg(
                    self,
                    args.first().ok_or_else(|| internal("toISOString receiver"))?,
                )?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res =
                    self.call_rt(self.ml.rt.date_to_iso, &[self.ctx_v, ms, pos_v], true)?;
                res.map(RV::S).ok_or_else(|| internal("toISOString result"))
            }
            accessor => {
                let code = accessor
                    .field_code()
                    .ok_or_else(|| internal(format!("Date intrinsic {accessor:?}")))?;
                let ms = scalar_arg(
                    self,
                    args.first().ok_or_else(|| internal("Date accessor receiver"))?,
                )?;
                let field = self.iconst(types::I32, i64::from(code));
                let res =
                    self.call_rt(self.ml.rt.date_get, &[self.ctx_v, ms, field], false)?;
                res.map(RV::S)
                    .ok_or_else(|| internal(format!("Date accessor {} result", accessor.name())))
            }
        }
    }

    /// Lowers a `String` method intrinsic (stdlib.md §8) to its opaque
    /// `sub_rt_str_*` runtime call. The receiver is the first HIR
    /// argument and every value is a scalar (string handles and `i32`
    /// byte measures); a trailing `pos_id` and a trap check follow
    /// exactly when the symbol is fault-capable
    /// ([`hir::StrFn::takes_pos_id`]). A `boolean` result arrives as
    /// `i32` 0/1 and is narrowed here.
    fn eval_str(&mut self, f: hir::StrFn, args: &[hir::Expr], pos: &Pos) -> Result<RV, String> {
        if args.len() != 1 + f.params().len() {
            return Err(internal(format!("{} arity (checker normalizes)", f.name())));
        }
        let mut argv = vec![self.ctx_v];
        for a in args {
            let rv = self.eval(a)?;
            argv.push(self.expect_s(rv)?);
        }
        let traps = f.takes_pos_id();
        if traps {
            let pid = self.pos_id(pos);
            argv.push(self.iconst(types::I32, pid));
        }
        let res = self.call_rt(self.ml.rt.str_ops[f as usize], &argv, traps)?;
        let res = res.ok_or_else(|| internal(format!("{} result", f.name())))?;
        Ok(RV::S(match f.ret() {
            hir::StrRet::Bool => self.b.ins().icmp_imm(IntCC::NotEqual, res, 0),
            _ => res,
        }))
    }

    /// Lowers an `Array` method intrinsic (stdlib.md §9) to its opaque
    /// `sub_rt_arr_*` runtime call. The receiver handle is the first
    /// HIR argument; element values the runtime receives are
    /// materialized and passed by pointer; a callback is evaluated to
    /// its `(code, env)` pair; kind tags come from the shared compiler
    /// mapping ([`crate::layout::arr_elem_kind`]). Calls that can leave
    /// the Context trapped (allocation, or any callback) are followed
    /// by the standing trap check.
    fn eval_arr(
        &mut self,
        f: hir::ArrFn,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
    ) -> Result<RV, String> {
        use hir::ArrFn as A;
        let recv = args.first().ok_or_else(|| internal("array method receiver"))?;
        let elem = match &recv.ty {
            Type::Array(e) => (**e).clone(),
            other => return Err(internal(format!("array method on {other:?}"))),
        };
        let rv = self.eval(recv)?;
        let h = self.expect_s(rv)?;
        let rt = self.ml.rt.arr_ops[f as usize];
        let arg_at = |i: usize| -> Result<&hir::Expr, String> {
            args.get(i)
                .ok_or_else(|| internal(format!("{} arity (checker normalizes)", f.name())))
        };
        let checked = f.can_trap();
        match f {
            A::IndexOf | A::LastIndexOf | A::Includes => {
                let kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let x = self.eval(arg_at(1)?)?;
                let ptr = self.materialize(x, &elem)?;
                let kv = self.iconst(types::I32, i64::from(kind.code()));
                let res = self.call_rt(rt, &[self.ctx_v, h, ptr, kv], checked)?;
                let res = res.ok_or_else(|| internal(format!("{} result", f.name())))?;
                Ok(RV::S(if f == A::Includes {
                    self.b.ins().icmp_imm(IntCC::NotEqual, res, 0)
                } else {
                    res
                }))
            }
            A::Join => {
                let kind = crate::layout::arr_fmt_kind(&elem)?;
                let sep = self.eval(arg_at(1)?)?;
                let sep = self.expect_s(sep)?;
                let kv = self.iconst(types::I32, i64::from(kind.code()));
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(rt, &[self.ctx_v, h, sep, kv, pos_v], checked)?;
                res.map(RV::S).ok_or_else(|| internal("join result"))
            }
            A::Slice => {
                let start = self.eval(arg_at(1)?)?;
                let start = self.expect_s(start)?;
                let end = self.eval(arg_at(2)?)?;
                let end = self.expect_s(end)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(rt, &[self.ctx_v, h, start, end, pos_v], checked)?;
                res.map(RV::S).ok_or_else(|| internal("slice result"))
            }
            A::Fill => {
                let x = self.eval(arg_at(1)?)?;
                let ptr = self.materialize(x, &elem)?;
                let start = self.eval(arg_at(2)?)?;
                let start = self.expect_s(start)?;
                let end = self.eval(arg_at(3)?)?;
                let end = self.expect_s(end)?;
                self.call_rt(rt, &[self.ctx_v, h, ptr, start, end], checked)?;
                // In place: the expression's value is the receiver.
                Ok(RV::S(h))
            }
            A::Reverse => {
                self.call_rt(rt, &[self.ctx_v, h], checked)?;
                Ok(RV::S(h))
            }
            A::Concat => {
                let other = self.eval(arg_at(1)?)?;
                let other = self.expect_s(other)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(rt, &[self.ctx_v, h, other, pos_v], checked)?;
                res.map(RV::S).ok_or_else(|| internal("concat result"))
            }
            A::ForEach | A::Filter | A::Some | A::Every | A::FindIndex | A::Sort => {
                let kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let cb = self.eval(arg_at(1)?)?;
                let (code, env) = self.expect_p(cb)?;
                let kv = self.iconst(types::I32, i64::from(kind.code()));
                let mut argv = vec![self.ctx_v, h, code, env, kv];
                if f == A::Filter {
                    let pid = self.pos_id(pos);
                    argv.push(self.iconst(types::I32, pid));
                }
                let res = self.call_rt(rt, &argv, checked)?;
                Ok(match f {
                    A::ForEach => RV::None,
                    A::Sort => RV::S(h),
                    A::Some | A::Every => {
                        let r = res.ok_or_else(|| internal("predicate result"))?;
                        RV::S(self.b.ins().icmp_imm(IntCC::NotEqual, r, 0))
                    }
                    _ => RV::S(res.ok_or_else(|| internal(format!("{} result", f.name())))?),
                })
            }
            A::Map => {
                let elem_kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let ret_elem = match ret_ty {
                    Type::Array(u) => (**u).clone(),
                    other => return Err(internal(format!("map result {other:?}"))),
                };
                let ret_kind = crate::layout::arr_elem_kind(self.ml.hir, &ret_elem)?;
                let ret_stride = self.ml.layouts.stride(&ret_elem)?;
                let cb = self.eval(arg_at(1)?)?;
                let (code, env) = self.expect_p(cb)?;
                let ekv = self.iconst(types::I32, i64::from(elem_kind.code()));
                let rkv = self.iconst(types::I32, i64::from(ret_kind.code()));
                let size_v = self.iconst(types::I64, i64::from(ret_stride));
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(
                    rt,
                    &[self.ctx_v, h, code, env, ekv, rkv, size_v, pos_v],
                    checked,
                )?;
                res.map(RV::S).ok_or_else(|| internal("map result"))
            }
            A::Reduce => {
                let elem_kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let acc_kind = crate::layout::arr_elem_kind(self.ml.hir, ret_ty)?;
                let acc_stride = self.ml.layouts.stride(ret_ty)?;
                let cb = self.eval(arg_at(1)?)?;
                let (code, env) = self.expect_p(cb)?;
                // The accumulator travels in/out through a caller slot.
                let init = self.eval(arg_at(2)?)?;
                let slot = self.materialize(init, ret_ty)?;
                let ekv = self.iconst(types::I32, i64::from(elem_kind.code()));
                let akv = self.iconst(types::I32, i64::from(acc_kind.code()));
                let size_v = self.iconst(types::I64, i64::from(acc_stride));
                self.call_rt(
                    rt,
                    &[self.ctx_v, h, code, env, ekv, akv, size_v, slot],
                    checked,
                )?;
                self.load_val(ret_ty, slot, 0)
            }
            other => Err(internal(format!("unknown ArrFn {other:?}"))),
        }
    }

    /// Lowers one monomorphized `Map<K, V>` operation (Q24). Concrete
    /// widths and key kind reach the runtime on construction; ordinary
    /// values cross the opaque ABI by pointer.
    fn eval_map(
        &mut self,
        f: hir::MapFn,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
    ) -> Result<RV, String> {
        use hir::MapFn as F;
        let (key, value) = match f {
            F::New => match ret_ty {
                Type::Map(key, value) => ((**key).clone(), (**value).clone()),
                other => return Err(internal(format!("Map constructor result {other:?}"))),
            },
            _ => match args.first().map(|arg| &arg.ty) {
                Some(Type::Map(key, value)) => ((**key).clone(), (**value).clone()),
                other => return Err(internal(format!("Map receiver {other:?}"))),
            },
        };
        let rt = self.ml.rt.map_ops[f as usize];
        let checked = f.can_trap();
        if f == F::New {
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let (value_size, _) = self.ml.layouts.size_align(&value)?;
            let kind = crate::layout::assoc_key_kind(self.ml.hir, &key)?;
            let pos_id = self.pos_id(pos);
            let argv = [
                self.ctx_v,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I64, i64::from(value_size)),
                self.iconst(types::I32, i64::from(kind.code())),
                self.iconst(types::I32, i64::from(pos_id)),
            ];
            let result = self.call_rt(rt, &argv, checked)?;
            return result
                .map(RV::S)
                .ok_or_else(|| internal("Map constructor result"));
        }
        let recv = args
            .first()
            .ok_or_else(|| internal("Map operation receiver"))?;
        let recv_rv = self.eval(recv)?;
        let handle = self.expect_s(recv_rv)?;
        self.live_check(handle, pos)?;
        let arg = |index: usize| {
            args.get(index)
                .ok_or_else(|| internal(format!("Map.{} arity", f.name())))
        };
        match f {
            F::Size => {
                let result = self.call_rt(rt, &[self.ctx_v, handle], false)?;
                result.map(RV::S).ok_or_else(|| internal("Map.size result"))
            }
            F::Get => {
                let key_rv = self.eval(arg(1)?)?;
                let key_ptr = self.materialize(key_rv, &key)?;
                let (size, align) = self.ml.layouts.size_align(&value)?;
                let out = self.temp_slot(size.max(8), align.max(8));
                self.zero_bytes(out, size.max(8), align.max(8));
                self.call_rt(rt, &[self.ctx_v, handle, key_ptr, out], false)?;
                self.load_val(&value, out, 0)
            }
            F::GetOr => {
                let key_rv = self.eval(arg(1)?)?;
                let key_ptr = self.materialize(key_rv, &key)?;
                let fallback_rv = self.eval(arg(2)?)?;
                let fallback = self.materialize(fallback_rv, &value)?;
                let (size, align) = self.ml.layouts.size_align(&value)?;
                let out = self.temp_slot(size.max(8), align.max(8));
                // Keep the result total even if an earlier allocation
                // failure supplied a null receiver and the pending trap
                // makes the runtime return without writing `out`.
                let slot_size = size.max(8);
                let access_align = 1u32 << slot_size.trailing_zeros();
                self.zero_bytes(out, slot_size, align.max(8).min(access_align));
                self.call_rt(
                    rt,
                    &[self.ctx_v, handle, key_ptr, fallback, out],
                    false,
                )?;
                self.load_val(&value, out, 0)
            }
            F::Set => {
                let key_rv = self.eval(arg(1)?)?;
                let key_ptr = self.materialize(key_rv, &key)?;
                let value_rv = self.eval(arg(2)?)?;
                let value_ptr = self.materialize(value_rv, &value)?;
                let pos_id = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, i64::from(pos_id));
                self.call_rt(
                    rt,
                    &[self.ctx_v, handle, key_ptr, value_ptr, pos_v],
                    checked,
                )?;
                Ok(RV::S(handle))
            }
            F::Has | F::Delete => {
                let key_rv = self.eval(arg(1)?)?;
                let key_ptr = self.materialize(key_rv, &key)?;
                let result =
                    self.call_rt(rt, &[self.ctx_v, handle, key_ptr], checked)?;
                let result =
                    result.ok_or_else(|| internal(format!("Map.{} result", f.name())))?;
                Ok(RV::S(
                    self.b.ins().icmp_imm(IntCC::NotEqual, result, 0),
                ))
            }
            F::Clear => {
                self.call_rt(rt, &[self.ctx_v, handle], false)?;
                Ok(RV::None)
            }
            F::ForEach => {
                let callback = self.eval(arg(1)?)?;
                let (code, env) = self.expect_p(callback)?;
                let bridge_id = define_assoc_bridge(self.ml, &key, Some(&value))?;
                let bridge_ref = self
                    .ml
                    .module
                    .declare_func_in_func(bridge_id, self.b.func);
                let bridge = self.b.ins().func_addr(types::I64, bridge_ref);
                self.call_rt(
                    rt,
                    &[self.ctx_v, handle, code, env, bridge],
                    checked,
                )?;
                Ok(RV::None)
            }
            F::New => Err(internal("Map.New reached receiver lowering")),
            other => Err(internal(format!("unknown MapFn {other:?}"))),
        }
    }

    /// Lowers one monomorphized `Set<K>` operation (Q24).
    fn eval_set(
        &mut self,
        f: hir::SetFn,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
    ) -> Result<RV, String> {
        use hir::SetFn as F;
        let key = match f {
            F::New => match ret_ty {
                Type::Set(key) => (**key).clone(),
                other => return Err(internal(format!("Set constructor result {other:?}"))),
            },
            _ => match args.first().map(|arg| &arg.ty) {
                Some(Type::Set(key)) => (**key).clone(),
                other => return Err(internal(format!("Set receiver {other:?}"))),
            },
        };
        let rt = self.ml.rt.set_ops[f as usize];
        let checked = f.can_trap();
        if f == F::New {
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let kind = crate::layout::assoc_key_kind(self.ml.hir, &key)?;
            let pos_id = self.pos_id(pos);
            let argv = [
                self.ctx_v,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I32, i64::from(kind.code())),
                self.iconst(types::I32, i64::from(pos_id)),
            ];
            let result = self.call_rt(rt, &argv, checked)?;
            return result
                .map(RV::S)
                .ok_or_else(|| internal("Set constructor result"));
        }
        let recv = args
            .first()
            .ok_or_else(|| internal("Set operation receiver"))?;
        let recv_rv = self.eval(recv)?;
        let handle = self.expect_s(recv_rv)?;
        self.live_check(handle, pos)?;
        let arg = |index: usize| {
            args.get(index)
                .ok_or_else(|| internal(format!("Set.{} arity", f.name())))
        };
        match f {
            F::Size => {
                let result = self.call_rt(rt, &[self.ctx_v, handle], false)?;
                result.map(RV::S).ok_or_else(|| internal("Set.size result"))
            }
            F::Add => {
                let key_rv = self.eval(arg(1)?)?;
                let key_ptr = self.materialize(key_rv, &key)?;
                let pos_id = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, i64::from(pos_id));
                self.call_rt(rt, &[self.ctx_v, handle, key_ptr, pos_v], checked)?;
                Ok(RV::S(handle))
            }
            F::Has | F::Delete => {
                let key_rv = self.eval(arg(1)?)?;
                let key_ptr = self.materialize(key_rv, &key)?;
                let result =
                    self.call_rt(rt, &[self.ctx_v, handle, key_ptr], checked)?;
                let result =
                    result.ok_or_else(|| internal(format!("Set.{} result", f.name())))?;
                Ok(RV::S(
                    self.b.ins().icmp_imm(IntCC::NotEqual, result, 0),
                ))
            }
            F::Clear => {
                self.call_rt(rt, &[self.ctx_v, handle], false)?;
                Ok(RV::None)
            }
            F::ForEach => {
                let callback = self.eval(arg(1)?)?;
                let (code, env) = self.expect_p(callback)?;
                let bridge_id = define_assoc_bridge(self.ml, &key, None)?;
                let bridge_ref = self
                    .ml
                    .module
                    .declare_func_in_func(bridge_id, self.b.func);
                let bridge = self.b.ins().func_addr(types::I64, bridge_ref);
                self.call_rt(
                    rt,
                    &[self.ctx_v, handle, code, env, bridge],
                    checked,
                )?;
                Ok(RV::None)
            }
            F::New => Err(internal("Set.New reached receiver lowering")),
            other => Err(internal(format!("unknown SetFn {other:?}"))),
        }
    }

    fn eval_ambient(
        &mut self,
        a: hir::AmbientFn,
        args: &[hir::Expr],
        pos: &Pos,
    ) -> Result<RV, String> {
        match a {
            hir::AmbientFn::Print => {
                let arg = args.first().ok_or_else(|| internal("print arity"))?;
                let rv = self.eval(arg)?;
                let h = self.expect_s(rv)?;
                self.call_rt(self.ml.rt.print, &[self.ctx_v, h], false)?;
                Ok(RV::None)
            }
            hir::AmbientFn::Collect => {
                self.call_rt(self.ml.rt.collect, &[self.ctx_v], false)?;
                Ok(RV::None)
            }
            hir::AmbientFn::UnsafeDelete => {
                let arg = args.first().ok_or_else(|| internal("unsafeDelete arity"))?;
                let rv = self.eval(arg)?;
                let ptr = self.expect_s(rv)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                self.call_rt(self.ml.rt.delete, &[self.ctx_v, ptr, pos_v], true)?;
                Ok(RV::None)
            }
            _ => Err(internal("unknown ambient function")),
        }
    }

    fn eval_method(
        &mut self,
        recv: &hir::Expr,
        name: &str,
        args: &[hir::Expr],
        _ret_ty: &Type,
        pos: &Pos,
        dest: Option<Value>,
    ) -> Result<RV, String> {
        match recv.ty.clone() {
            Type::Array(elem) => {
                let rv = self.eval(recv)?;
                let h = self.expect_s(rv)?;
                match name {
                    "push" => {
                        let arg = args.first().ok_or_else(|| internal("push arity"))?;
                        let v = self.eval(arg)?;
                        let src = self.materialize(v, &elem)?;
                        let pid = self.pos_id(pos);
                        let pos_v = self.iconst(types::I32, pid);
                        let res = self.call_rt(
                            self.ml.rt.array_push,
                            &[self.ctx_v, h, src, pos_v],
                            true,
                        )?;
                        res.map(RV::S).ok_or_else(|| internal("push result"))
                    }
                    "pop" => {
                        let (size, align) = self.ml.layouts.size_align(&elem)?;
                        let dst = self.temp_slot(size.max(8), align.max(8));
                        let pid = self.pos_id(pos);
                        let pos_v = self.iconst(types::I32, pid);
                        self.call_rt(
                            self.ml.rt.array_pop,
                            &[self.ctx_v, h, dst, pos_v],
                            true,
                        )?;
                        self.load_val(&elem, dst, 0)
                    }
                    other => Err(internal(format!("array method `{other}`"))),
                }
            }
            Type::Str => {
                let rv = self.eval(recv)?;
                let h = self.expect_s(rv)?;
                if name != "slice" {
                    return Err(internal(format!("string method `{name}`")));
                }
                let a0 = self.eval(args.first().ok_or_else(|| internal("slice arity"))?)?;
                let a0 = self.expect_s(a0)?;
                let a1 = self.eval(args.get(1).ok_or_else(|| internal("slice arity"))?)?;
                let a1 = self.expect_s(a1)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(
                    self.ml.rt.str_slice,
                    &[self.ctx_v, h, a0, a1, pos_v],
                    true,
                )?;
                res.map(RV::S).ok_or_else(|| internal("slice result"))
            }
            Type::Generator(y) => {
                if name != "next" {
                    return Err(internal(format!("generator method `{name}`")));
                }
                let rv = self.eval(recv)?;
                let frame = self.expect_s(rv)?;
                self.live_check(frame, pos)?;
                self.reload_epoch_check(frame, pos)?;
                let step_ty = Type::IterResult(y.clone());
                let (size, align) = self.ml.layouts.size_align(&step_ty)?;
                let slot = self.temp_slot(size, align);
                // C8: `value` zero-initialized when done.
                self.zero_bytes(slot, size, align);
                let value_off = self.ml.layouts.iter_result_value_offset(&y)?;
                let out = self.addr_off(slot, i64::from(value_off));
                let resume = self
                    .b
                    .ins()
                    .load(types::I64, flags(), frame, GEN_RESUME_OFF);
                let sig = self.ml.resume_sig();
                let sigref = self.b.import_signature(sig);
                let inst = self
                    .b
                    .ins()
                    .call_indirect(sigref, resume, &[self.ctx_v, frame, out]);
                let done = self.b.inst_results(inst)[0];
                self.trap_check();
                self.b.ins().store(flags(), done, slot, 0);
                Ok(RV::A(slot))
            }
            Type::Class(cid) => {
                let rv = self.eval(recv)?;
                let this = match rv {
                    RV::A(ptr) => ptr,
                    RV::S(ptr) => {
                        self.live_check(ptr, &recv.pos)?;
                        ptr
                    }
                    other => return Err(internal(format!("receiver {other:?}"))),
                };
                let m = self.ml.hir_method(cid.0, name)?;
                let mut argv = vec![self.ctx_v];
                let sret = self.sret_slot(&m.ret, dest)?;
                if let Some(s) = sret {
                    argv.push(s);
                }
                argv.push(this);
                self.push_args(&mut argv, &m.params, args)?;
                let ret = m.ret.clone();
                let res = self.call_script(&FnKey::Method(cid.0, name.to_string()), &argv)?;
                self.shape_results(&ret, &res, sret)
            }
            other => Err(internal(format!("method on {other:?}"))),
        }
    }

    /// Constructs `new C(...)`. For a value class, `dest` (when given)
    /// is the storage the instance is built into directly (§10.2),
    /// eliding the temporary the caller would otherwise copy from;
    /// `dest` is ignored for a reference class, whose value is a handle.
    fn eval_new(
        &mut self,
        cid: usize,
        args: &[hir::Expr],
        pos: &Pos,
        dest: Option<Value>,
    ) -> Result<RV, String> {
        let hirm = self.ml.hir;
        let class = hirm
            .classes
            .get(cid)
            .ok_or_else(|| internal("class id out of range"))?;
        let layout = self.ml.layouts.class(cid)?.clone();
        let this = if layout.is_value {
            let slot = match dest {
                Some(d) => d,
                None => self.temp_slot(layout.size, layout.align),
            };
            self.zero_bytes(slot, layout.size, layout.align);
            slot
        } else {
            let size = self.iconst(types::I64, i64::from(layout.size));
            let class_v = self.iconst(types::I32, i64::from(cid as u32));
            let pid = self.pos_id(pos);
            let pos_v = self.iconst(types::I32, pid);
            let res = self.call_rt(
                self.ml.rt.alloc,
                &[self.ctx_v, size, class_v, pos_v],
                true,
            )?;
            res.ok_or_else(|| internal("alloc result"))?
        };
        // Declared field initializers, in declaration order.
        for (i, field) in class.fields.iter().enumerate() {
            if let Some(init) = &field.init {
                let rv = self.eval(init)?;
                let off = layout.field_offsets[i] as i32;
                self.store_val(&field.ty, this, off, rv)?;
            }
        }
        // A mirror boundary struct has no in-language constructor body: its
        // `new` stores the arguments positionally into the fields (arg `i`
        // → field `i`), each through the boundary coercion (chain-slot
        // address-of for a `Struct | null` field).
        if class.is_boundary {
            if args.len() != class.fields.len() {
                return Err(internal(format!(
                    "boundary struct `{}` expects {} field arguments, got {}",
                    class.name,
                    class.fields.len(),
                    args.len()
                )));
            }
            for (i, field) in class.fields.iter().enumerate() {
                let off = layout.field_offsets[i] as i32;
                let fty = field.ty.clone();
                let rv = self.eval(&args[i])?;
                self.store_val(&fty, this, off, rv)?;
            }
        }
        if let Some(ctor) = &class.ctor {
            let mut argv = vec![self.ctx_v, this];
            self.push_args(&mut argv, &ctor.params, args)?;
            self.call_script(&FnKey::Ctor(cid), &argv)?;
        }
        Ok(if layout.is_value {
            RV::A(this)
        } else {
            RV::S(this)
        })
    }

    fn eval_array_lit(
        &mut self,
        ty: &Type,
        elems: &[hir::Expr],
        pos: &Pos,
    ) -> Result<RV, String> {
        match ty {
            Type::Array(elem) => {
                let stride = self.ml.layouts.stride(elem)?;
                let stride_v = self.iconst(types::I64, i64::from(stride));
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(
                    self.ml.rt.array_new,
                    &[self.ctx_v, stride_v, pos_v],
                    true,
                )?;
                let h = res.ok_or_else(|| internal("array_new result"))?;
                for e in elems {
                    let rv = self.eval(e)?;
                    let src = self.materialize(rv, elem)?;
                    let pid = self.pos_id(&e.pos);
                    let pos_v = self.iconst(types::I32, pid);
                    self.call_rt(
                        self.ml.rt.array_push,
                        &[self.ctx_v, h, src, pos_v],
                        true,
                    )?;
                }
                Ok(RV::S(h))
            }
            Type::FixedArray(..) => {
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let slot = self.temp_slot(size, align);
                self.array_lit_into(ty, elems, slot)?;
                Ok(RV::A(slot))
            }
            other => Err(internal(format!("array literal of {other:?}"))),
        }
    }

    /// Stores a `FixedArray` literal's elements straight into `dest`
    /// (§10.2): the destination is a stable in-place address, so the
    /// literal never needs an intermediate the caller would copy from.
    fn array_lit_into(
        &mut self,
        ty: &Type,
        elems: &[hir::Expr],
        dest: Value,
    ) -> Result<(), String> {
        let elem = match ty {
            Type::FixedArray(elem, _) => elem,
            other => return Err(internal(format!("fixed-array literal into {other:?}"))),
        };
        let stride = self.ml.layouts.stride(elem)?;
        for (i, e) in elems.iter().enumerate() {
            let rv = self.eval(e)?;
            self.store_val(elem, dest, (i as u32 * stride) as i32, rv)?;
        }
        Ok(())
    }

    /// Evaluates an aggregate expression, writing its bytes directly to
    /// `dest` (§10.2 copy elision). Construct-like forms — `new` of a
    /// value class, a call whose aggregate result becomes `dest`'s
    /// `sret`, a `FixedArray` literal — build in place; any other
    /// aggregate is evaluated and copied, as before.
    ///
    /// The caller guarantees `dest` is a stable address (a local's
    /// storage, a field, an in-place `FixedArray` element, or an `sret`
    /// slot), never a dynamic-array element whose bounds-checked address
    /// must be resolved *after* the value (growth-safe, N3). C2's
    /// observable copy semantics are unchanged: elision only removes a
    /// temporary between a freshly produced aggregate and its final
    /// home, which no alias can observe.
    fn eval_agg_into(&mut self, e: &hir::Expr, dest: Value, ty: &Type) -> Result<(), String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::New { class, args } => {
                self.eval_new(class.0, args, &e.pos, Some(dest))?;
                Ok(())
            }
            K::Call { callee, args } => {
                let rv = self.eval_call(callee, args, &e.ty, &e.pos, Some(dest))?;
                match rv {
                    RV::A(addr) => {
                        // Calls that take an `sret` wrote straight into
                        // `dest` (address identical). Built-in methods
                        // that do not — generator `.next()`, array
                        // `.pop()` — return their own slot, so copy from
                        // it, preserving the value.
                        if addr != dest {
                            let (size, align) = self.ml.layouts.size_align(ty)?;
                            self.copy_bytes(dest, addr, size, align);
                        }
                        Ok(())
                    }
                    RV::None => Ok(()),
                    other => Err(internal(format!("aggregate call yielded {other:?}"))),
                }
            }
            K::ArrayLit(elems) if matches!(ty, Type::FixedArray(..)) => {
                self.array_lit_into(ty, elems, dest)
            }
            _ => {
                let rv = self.eval(e)?;
                let src = self.expect_a(rv)?;
                let (size, align) = self.ml.layouts.size_align(ty)?;
                self.copy_bytes(dest, src, size, align);
                Ok(())
            }
        }
    }

    fn eval_template(&mut self, parts: &[hir::TplPart], pos: &Pos) -> Result<RV, String> {
        let mut acc: Option<Value> = None;
        for part in parts {
            let h = match part {
                hir::TplPart::Text(t) => self.string_literal(t.as_bytes(), pos)?,
                hir::TplPart::Expr(e) => {
                    let rv = self.eval(e)?;
                    self.format_value(rv, &e.ty, &e.pos)?
                }
                other => return Err(internal(format!("template part {other:?}"))),
            };
            acc = Some(match acc {
                None => h,
                Some(prev) => {
                    let pid = self.pos_id(pos);
                    let pos_v = self.iconst(types::I32, pid);
                    let res = self.call_rt(
                        self.ml.rt.str_concat,
                        &[self.ctx_v, prev, h, pos_v],
                        true,
                    )?;
                    res.ok_or_else(|| internal("concat result"))?
                }
            });
        }
        match acc {
            Some(h) => Ok(RV::S(h)),
            None => Ok(RV::S(self.string_literal(b"", pos)?)),
        }
    }

    /// Q14 formatting of one interpolated value into a string handle.
    fn format_value(&mut self, rv: RV, ty: &Type, pos: &Pos) -> Result<Value, String> {
        let v = self.expect_s(rv)?;
        let pid = self.pos_id(pos);
        let pos_v = self.iconst(types::I32, pid);
        let (f, arg) = match ty {
            Type::Str => return Ok(v),
            Type::I8 | Type::I16 => {
                let wide = self.b.ins().sextend(types::I32, v);
                (self.ml.rt.fmt_i32, wide)
            }
            Type::U8 | Type::U16 => {
                let wide = self.b.ins().uextend(types::I32, v);
                (self.ml.rt.fmt_u32, wide)
            }
            Type::I32 | Type::Enum(_) => (self.ml.rt.fmt_i32, v),
            Type::U32 => (self.ml.rt.fmt_u32, v),
            Type::I64 => (self.ml.rt.fmt_i64, v),
            Type::U64 => (self.ml.rt.fmt_u64, v),
            Type::F32 => (self.ml.rt.fmt_f32, v),
            Type::F64 => (self.ml.rt.fmt_f64, v),
            Type::F16 => {
                let wide = self
                    .call_rt(self.ml.rt.f16_to_f64, &[v], false)?
                    .ok_or_else(|| internal("f16 formatting widening result"))?;
                (self.ml.rt.fmt_f64, wide)
            }
            Type::Bool => {
                let wide = self.b.ins().uextend(types::I32, v);
                (self.ml.rt.fmt_bool, wide)
            }
            other => return Err(internal(format!("interpolation of {other:?}"))),
        };
        let res = self.call_rt(f, &[self.ctx_v, arg, pos_v], true)?;
        res.ok_or_else(|| internal("fmt result"))
    }

    fn eval_lambda(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[String],
        pos: &Pos,
    ) -> Result<RV, String> {
        // Environment layout: captured values in capture order,
        // naturally aligned, copied by value at creation (C5).
        let mut cap_info: Vec<(String, Type, u32)> = Vec::new();
        let mut off = 0u32;
        let mut env_align = 1u32;
        for name in captures {
            let binding = self.lookup(name)?;
            let (s, a) = self.ml.layouts.size_align(&binding.ty)?;
            off = round_up(off, a);
            cap_info.push((name.clone(), binding.ty.clone(), off));
            off += s;
            env_align = env_align.max(a);
        }
        let env = if captures.is_empty() {
            self.iconst(types::I64, 0)
        } else {
            let slot = self.temp_slot(round_up(off.max(1), env_align), env_align);
            for (name, ty, at) in &cap_info {
                let binding = self.lookup(name)?;
                let rv = self.read_binding(&binding)?;
                self.store_val(ty, slot, *at as i32, rv)?;
            }
            slot
        };
        let id = define_lambda(self.ml, params, ret, body, &cap_info, pos)?;
        let fref = self.ml.module.declare_func_in_func(id, self.b.func);
        let code = self.b.ins().func_addr(types::I64, fref);
        Ok(RV::P(code, env))
    }

    fn eval_yield(&mut self, arg: Option<&hir::Expr>, _pos: &Pos) -> Result<RV, String> {
        let (out, frame, yield_ty, state) = {
            let g = self
                .genc
                .as_mut()
                .ok_or_else(|| internal("yield outside a generator"))?;
            let state = (g.next_resume + 1) as i64;
            (g.out, g.frame, g.yield_ty.clone(), state)
        };
        if let Some(a) = arg {
            let rv = self.eval(a)?;
            self.store_val(&yield_ty, out, 0, rv)?;
        }
        let state_v = self.iconst(types::I32, state);
        self.b.ins().store(flags(), state_v, frame, 0);
        let zero = self.iconst(types::I8, 0);
        self.b.ins().return_(&[zero]);
        // Continuation: the block dispatch jumps to on the next resume.
        let g = self
            .genc
            .as_mut()
            .ok_or_else(|| internal("generator context"))?;
        let resume = *g
            .resume_blocks
            .get(g.next_resume)
            .ok_or_else(|| internal("resume block table exhausted"))?;
        g.next_resume += 1;
        self.b.switch_to_block(resume);
        Ok(RV::None)
    }

    // ----- statements -----

    fn lower_stmts(&mut self, stmts: &[hir::Stmt]) -> Result<(), String> {
        for (i, s) in stmts.iter().enumerate() {
            if self.term {
                // Statically unreachable code after a terminator; the
                // checker allows it, the lowering skips it. The
                // generator pre-passes assigned frame offsets and
                // resume blocks over *all* `let`s and `yield`s, so
                // both cursors must account for the skipped ones: the
                // offset cursor advances, and each skipped yield's
                // resume block (still referenced by the dispatch
                // chain, but unreachable) is filled with a plain
                // `done` return so the function stays well-formed.
                if self.genc.is_some() {
                    let mut rest: Vec<&Type> = Vec::new();
                    walk_lets(&stmts[i..], &mut rest);
                    let skipped_yields = count_yields(&stmts[i..]);
                    if let Some(g) = self.genc.as_mut() {
                        g.next_let += rest.len();
                    }
                    for _ in 0..skipped_yields {
                        let blk = {
                            let g = self
                                .genc
                                .as_mut()
                                .ok_or_else(|| internal("generator context"))?;
                            let blk = *g
                                .resume_blocks
                                .get(g.next_resume)
                                .ok_or_else(|| internal("resume block table exhausted"))?;
                            g.next_resume += 1;
                            blk
                        };
                        // The current block is terminated (that is why
                        // we are skipping), so switching away is legal.
                        self.b.switch_to_block(blk);
                        let one = self.iconst(types::I8, 1);
                        self.b.ins().return_(&[one]);
                    }
                }
                break;
            }
            self.lower_stmt(s)?;
        }
        Ok(())
    }

    fn lower_stmt(&mut self, s: &hir::Stmt) -> Result<(), String> {
        match s {
            hir::Stmt::Let { name, ty, init, .. } => {
                self.declare_local(name, ty, init)
            }
            hir::Stmt::Expr(e) => {
                self.eval(e)?;
                Ok(())
            }
            hir::Stmt::Return { value, .. } => {
                // §10.2 NRVO: an aggregate return builds directly into
                // the caller-provided `sret`, so `return new M(...)` or
                // `return f(...)` produces the result in the caller's
                // slot with no intermediate copy. (Resume functions
                // return the `done` flag, never an aggregate.)
                let ret_ty = self.ret_ty.clone();
                if !self.is_resume {
                    if let (Some(v), Repr::Agg { .. }) =
                        (value, self.ml.layouts.repr(&ret_ty)?)
                    {
                        let sret = self.sret_v.ok_or_else(|| internal("missing sret"))?;
                        self.eval_agg_into(v, sret, &ret_ty)?;
                        self.emit_shadow_pop()?;
                        self.b.ins().return_(&[]);
                        self.term = true;
                        return Ok(());
                    }
                }
                let rv = match value {
                    Some(v) => Some((self.eval(v)?, v.ty.clone())),
                    None => None,
                };
                self.emit_return(rv)
            }
            hir::Stmt::If { cond, then, els, .. } => {
                let c = self.eval(cond)?;
                let c = self.expect_s(c)?;
                let then_blk = self.b.create_block();
                let merge = self.b.create_block();
                let else_blk = if els.is_some() {
                    self.b.create_block()
                } else {
                    merge
                };
                self.b.ins().brif(c, then_blk, &[], else_blk, &[]);
                self.b.switch_to_block(then_blk);
                self.scope_push();
                self.lower_stmts(then)?;
                self.scope_pop();
                if !self.term {
                    self.b.ins().jump(merge, &[]);
                }
                self.term = false;
                if let Some(e) = els {
                    self.b.switch_to_block(else_blk);
                    self.scope_push();
                    self.lower_stmts(e)?;
                    self.scope_pop();
                    if !self.term {
                        self.b.ins().jump(merge, &[]);
                    }
                    self.term = false;
                }
                self.b.switch_to_block(merge);
                Ok(())
            }
            hir::Stmt::While { cond, body, .. } => {
                let hdr = self.b.create_block();
                let body_blk = self.b.create_block();
                let exit = self.b.create_block();
                self.b.ins().jump(hdr, &[]);
                self.b.switch_to_block(hdr);
                let c = self.eval(cond)?;
                let c = self.expect_s(c)?;
                self.b.ins().brif(c, body_blk, &[], exit, &[]);
                self.b.switch_to_block(body_blk);
                self.loops.push(LoopCtx {
                    brk: exit,
                    cont: Some(hdr),
                });
                self.scope_push();
                self.lower_stmts(body)?;
                self.scope_pop();
                self.loops.pop();
                if !self.term {
                    self.b.ins().jump(hdr, &[]);
                }
                self.term = false;
                self.b.switch_to_block(exit);
                Ok(())
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                self.scope_push();
                if let Some(i) = init {
                    self.lower_stmt(i)?;
                }
                // §10.1: if this is a counted loop over a constant range
                // whose counter the body never reassigns, publish the
                // counter's proven interval so indexing inside the body
                // can drop the bounds check.
                if let Some((name, range)) = self.induction_interval(
                    init.as_deref(),
                    cond.as_ref(),
                    step.as_ref(),
                    body,
                ) {
                    self.set_range(&name, range);
                }
                let hdr = self.b.create_block();
                let body_blk = self.b.create_block();
                let step_blk = self.b.create_block();
                let exit = self.b.create_block();
                self.b.ins().jump(hdr, &[]);
                self.b.switch_to_block(hdr);
                let c = match cond {
                    Some(c) => {
                        let v = self.eval(c)?;
                        self.expect_s(v)?
                    }
                    None => self.iconst(types::I8, 1),
                };
                self.b.ins().brif(c, body_blk, &[], exit, &[]);
                self.b.switch_to_block(body_blk);
                self.loops.push(LoopCtx {
                    brk: exit,
                    cont: Some(step_blk),
                });
                self.scope_push();
                self.lower_stmts(body)?;
                self.scope_pop();
                self.loops.pop();
                if !self.term {
                    self.b.ins().jump(step_blk, &[]);
                }
                self.term = false;
                self.b.switch_to_block(step_blk);
                if let Some(st) = step {
                    self.eval(st)?;
                }
                self.b.ins().jump(hdr, &[]);
                self.b.switch_to_block(exit);
                self.scope_pop();
                Ok(())
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                let d = self.eval(disc)?;
                let d = self.expect_s(d)?;
                let exit = self.b.create_block();
                let body_blocks: Vec<Block> =
                    cases.iter().map(|_| self.b.create_block()).collect();
                let default_idx = cases.iter().position(|c| c.test.is_none());
                // Test chain in source order (tests are evaluated in
                // order; `default` is skipped by the chain and entered
                // only by fallthrough or chain exhaustion).
                for (i, case) in cases.iter().enumerate() {
                    if let Some(test) = &case.test {
                        let t = self.eval(test)?;
                        let t = self.expect_s(t)?;
                        let eq = self.b.ins().icmp(IntCC::Equal, d, t);
                        let next = self.b.create_block();
                        self.b.ins().brif(eq, body_blocks[i], &[], next, &[]);
                        self.b.switch_to_block(next);
                    }
                }
                match default_idx {
                    Some(i) => self.b.ins().jump(body_blocks[i], &[]),
                    None => self.b.ins().jump(exit, &[]),
                };
                self.loops.push(LoopCtx {
                    brk: exit,
                    cont: None,
                });
                for (i, case) in cases.iter().enumerate() {
                    self.b.switch_to_block(body_blocks[i]);
                    self.scope_push();
                    self.lower_stmts(&case.body)?;
                    self.scope_pop();
                    if !self.term {
                        // Fallthrough to the next arm, or out.
                        let target = body_blocks.get(i + 1).copied().unwrap_or(exit);
                        self.b.ins().jump(target, &[]);
                    }
                    self.term = false;
                }
                self.loops.pop();
                self.b.switch_to_block(exit);
                Ok(())
            }
            hir::Stmt::Break(_) => {
                let target = self
                    .loops
                    .last()
                    .map(|l| l.brk)
                    .ok_or_else(|| internal("break outside a loop"))?;
                self.b.ins().jump(target, &[]);
                self.term = true;
                Ok(())
            }
            hir::Stmt::Continue(_) => {
                let target = self
                    .loops
                    .iter()
                    .rev()
                    .find_map(|l| l.cont)
                    .ok_or_else(|| internal("continue outside a loop"))?;
                self.b.ins().jump(target, &[]);
                self.term = true;
                Ok(())
            }
            hir::Stmt::Block(stmts) => {
                self.scope_push();
                self.lower_stmts(stmts)?;
                self.scope_pop();
                Ok(())
            }
            other => Err(internal(format!("statement {other:?}"))),
        }
    }

    fn emit_shadow_pop(&mut self) -> Result<(), String> {
        if self.shadow_base.is_some() {
            self.call_rt(self.ml.rt.shadow_pop, &[self.ctx_v], false)?;
        }
        Ok(())
    }

    fn emit_return(&mut self, value: Option<(RV, Type)>) -> Result<(), String> {
        if self.is_resume {
            // Generator completion: terminal state, done = 1.
            let g = self
                .genc
                .as_ref()
                .ok_or_else(|| internal("resume without generator context"))?;
            let frame = g.frame;
            let done_state = self.iconst(types::I32, GEN_DONE);
            self.b.ins().store(flags(), done_state, frame, 0);
            let one = self.iconst(types::I8, 1);
            self.b.ins().return_(&[one]);
            self.term = true;
            return Ok(());
        }
        self.emit_shadow_pop()?;
        match (self.ml.layouts.repr(&self.ret_ty.clone())?, value) {
            (Repr::None, _) => {
                self.b.ins().return_(&[]);
            }
            (Repr::Agg { size, align }, Some((rv, _))) => {
                let src = self.expect_a(rv)?;
                let sret = self.sret_v.ok_or_else(|| internal("missing sret"))?;
                self.copy_bytes(sret, src, size, align);
                self.b.ins().return_(&[]);
            }
            (Repr::Scalar(_), Some((rv, _))) => {
                let v = self.expect_s(rv)?;
                self.b.ins().return_(&[v]);
            }
            (Repr::Pair, Some((rv, _))) => {
                let (a, b) = self.expect_p(rv)?;
                self.b.ins().return_(&[a, b]);
            }
            (r, None) => return Err(internal(format!("missing return value for {r:?}"))),
        }
        self.term = true;
        Ok(())
    }

    /// Terminates the entry-to-exit path (implicit end of body) and
    /// fills the unwind block, then seals and finalizes.
    fn finish(mut self) -> Result<(), String> {
        if !self.term {
            if self.is_resume || matches!(self.ml.layouts.repr(&self.ret_ty)?, Repr::None) {
                self.emit_return(None)?;
            } else {
                // Unreachable (the checker proved all paths return);
                // emit a zeroed return to keep the block well-formed.
                let zeros = self.zero_return_values()?;
                self.emit_shadow_pop()?;
                self.b.ins().return_(&zeros);
            }
        }
        if let Some(u) = self.unwind {
            self.b.switch_to_block(u);
            self.emit_shadow_pop()?;
            let vals = if self.is_resume {
                // A trapped coroutine is finished: store the terminal
                // state so a (hypothetical) later resume stays done
                // instead of re-entering the body.
                let g = self
                    .genc
                    .as_ref()
                    .ok_or_else(|| internal("resume without generator context"))?;
                let frame = g.frame;
                let done_state = self.iconst(types::I32, GEN_DONE);
                self.b.ins().store(flags(), done_state, frame, 0);
                vec![self.iconst(types::I8, 1)]
            } else {
                self.zero_return_values()?
            };
            self.b.ins().return_(&vals);
        }
        self.b.seal_all_blocks();
        self.b.finalize();
        Ok(())
    }

    fn zero_return_values(&mut self) -> Result<Vec<Value>, String> {
        Ok(match self.ml.layouts.repr(&self.ret_ty.clone())? {
            Repr::None | Repr::Agg { .. } => vec![],
            Repr::Scalar(t) => vec![self.zero_of(t)],
            Repr::Pair => {
                let a = self.iconst(types::I64, 0);
                let b = self.iconst(types::I64, 0);
                vec![a, b]
            }
        })
    }
}

// ----- function drivers -----

struct Prologue {
    ctx_v: Value,
    env_v: Option<Value>,
    sret_v: Option<Value>,
    this_v: Option<Value>,
    param_vals: Vec<Value>,
}

/// Splits the entry block's parameters per the calling convention.
fn split_params<M: Module>(
    ml: &ModLower<M>,
    b: &mut FunctionBuilder,
    entry: Block,
    ret: &Type,
    has_env: bool,
    has_this: bool,
    params: &[hir::Param],
) -> Result<Prologue, String> {
    let vals = b.block_params(entry).to_vec();
    let mut i = 0usize;
    let mut take = |what: &str| -> Result<Value, String> {
        let v = vals
            .get(i)
            .copied()
            .ok_or_else(|| internal(format!("missing ABI param {what}")))?;
        i += 1;
        Ok(v)
    };
    let ctx_v = take("ctx")?;
    let env_v = if has_env { Some(take("env")?) } else { None };
    let sret_v = if matches!(ml.layouts.repr(ret)?, Repr::Agg { .. }) {
        Some(take("sret")?)
    } else {
        None
    };
    let this_v = if has_this { Some(take("this")?) } else { None };
    let mut param_vals = Vec::new();
    for p in params {
        match ml.layouts.repr(&p.ty)? {
            Repr::None => {}
            Repr::Pair => {
                param_vals.push(take("param")?);
                param_vals.push(take("param")?);
            }
            _ => param_vals.push(take("param")?),
        }
    }
    Ok(Prologue {
        ctx_v,
        env_v,
        sret_v,
        this_v,
        param_vals,
    })
}

/// Shadow-frame size in 8-byte words: managed params and locals plus
/// aggregate params/locals whose interior holds managed handles (M1:
/// the collector word-scans the whole frame, so aggregates stored in
/// it are covered).
fn shadow_words<M: Module>(
    ml: &ModLower<M>,
    params: &[hir::Param],
    body: &[hir::Stmt],
) -> Result<u32, String> {
    let mut lets: Vec<&Type> = Vec::new();
    walk_lets(body, &mut lets);
    let mut n = 0u32;
    for p in params {
        n += managed_words(&ml.layouts, &p.ty)?;
    }
    for t in lets {
        n += managed_words(&ml.layouts, t)?;
    }
    Ok(n)
}

/// Emits the shadow-frame prologue; returns the base address.
fn shadow_prologue<M: Module>(
    body: &mut Body<M>,
    slots: u32,
) -> Result<(), String> {
    if slots == 0 {
        return Ok(());
    }
    let base = body.temp_slot(slots * 8, 8);
    body.zero_bytes(base, slots * 8, 8);
    let n = body.iconst(types::I64, i64::from(slots));
    body.call_rt(body.ml.rt.shadow_push, &[body.ctx_v, base, n], false)?;
    body.shadow_base = Some(base);
    Ok(())
}

/// Binds declared parameters into the body's scope.
fn bind_params<M: Module>(
    body: &mut Body<M>,
    params: &[hir::Param],
    vals: &[Value],
) -> Result<(), String> {
    let mut vi = 0usize;
    for p in params {
        let repr = body.ml.layouts.repr(&p.ty)?;
        let storage = match repr {
            Repr::None => continue,
            Repr::Pair => {
                let a = body.b.declare_var(types::I64);
                let c = body.b.declare_var(types::I64);
                body.b.def_var(a, vals[vi]);
                body.b.def_var(c, vals[vi + 1]);
                vi += 2;
                Storage::Pair(a, c)
            }
            Repr::Agg { size, align } => {
                // Pointer to the caller-owned copy (C2 copy-on-pass):
                // the callee owns that copy for the duration of the
                // call, so it doubles as the parameter's storage —
                // unless it contains managed handles, in which case it
                // is copied into the callee's shadow frame so the
                // collector sees it (the caller's temp is not a root).
                let v = vals[vi];
                vi += 1;
                if has_managed_interior(&body.ml.layouts, &p.ty)? {
                    let words = managed_words(&body.ml.layouts, &p.ty)?;
                    let idx = body.next_shadow;
                    body.next_shadow += words;
                    let addr = body.shadow_addr(idx)?;
                    body.copy_bytes(addr, v, size, align);
                    Storage::Addr(addr)
                } else {
                    Storage::Addr(v)
                }
            }
            Repr::Scalar(t) => {
                let v = vals[vi];
                vi += 1;
                if is_managed(&body.ml.layouts, &p.ty)? {
                    let idx = body.next_shadow;
                    body.next_shadow += 1;
                    let addr = body.shadow_addr(idx)?;
                    body.b.ins().store(flags(), v, addr, 0);
                    Storage::Shadow(idx)
                } else {
                    let var = body.b.declare_var(t);
                    body.b.def_var(var, v);
                    Storage::Var(var)
                }
            }
        };
        body.bind(
            &p.name,
            Binding {
                ty: p.ty.clone(),
                storage,
                range: None,
            },
        );
    }
    Ok(())
}

/// Defines a plain function, constructor, or method body.
pub(crate) fn define_function<M: Module>(
    ml: &mut ModLower<M>,
    key: FnKey,
    f: &hir::Function,
    class: Option<usize>,
) -> Result<(), String> {
    let params_ty: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
    let ret = if matches!(key, FnKey::Ctor(_)) {
        Type::Void
    } else {
        f.ret.clone()
    };
    let sig = ml.make_sig(&params_ty, &ret, false, class.is_some())?;
    let id = ml.func_id(&key)?;
    let mut cctx = ml.module.make_context();
    cctx.func.signature = sig;
    let mut fbx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let pro = split_params(ml, &mut b, entry, &ret, false, class.is_some(), &f.params)?;
        let slots = shadow_words(ml, &f.params, &f.body)?;
        let mut body = Body {
            ml,
            b,
            ctx_v: pro.ctx_v,
            env_v: pro.env_v,
            sret_v: pro.sret_v,
            this_v: pro.this_v.zip(class),
            ret_ty: ret,
            is_resume: false,
            scopes: vec![Vec::new()],
            loops: Vec::new(),
            unwind: None,
            shadow_base: None,
            next_shadow: 0,
            genc: None,
            term: false,
        };
        shadow_prologue(&mut body, slots)?;
        bind_params(&mut body, &f.params, &pro.param_vals)?;
        body.lower_stmts(&f.body)?;
        body.finish()?;
    }
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define {key:?}: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(())
}

/// Defines a lambda function `(ctx, env, [sret], params...)`.
fn define_lambda<M: Module>(
    ml: &mut ModLower<M>,
    params: &[hir::Param],
    ret: &Type,
    stmts: &[hir::Stmt],
    captures: &[(String, Type, u32)],
    pos: &Pos,
) -> Result<cranelift_module::FuncId, String> {
    let params_ty: Vec<Type> = params.iter().map(|p| p.ty.clone()).collect();
    let sig = ml.make_sig(&params_ty, ret, true, false)?;
    let name = format!("ss_lambda{}", ml.lambda_count);
    ml.lambda_count += 1;
    let id = ml
        .module
        .declare_function(&name, cranelift_module::Linkage::Local, &sig)
        .map_err(|e| internal(format!("declare {name}: {e}")))?;
    let mut cctx = ml.module.make_context();
    cctx.func.signature = sig;
    let mut fbx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let pro = split_params(ml, &mut b, entry, ret, true, false, params)?;
        // Shadow words: managed params + managed lets of the lambda
        // body (captures are const copies owned by the enclosing
        // frame's environment slot; the originals stay rooted there).
        let slots = shadow_words(ml, params, stmts)?;
        let mut body = Body {
            ml,
            b,
            ctx_v: pro.ctx_v,
            env_v: pro.env_v,
            sret_v: pro.sret_v,
            this_v: None,
            ret_ty: ret.clone(),
            is_resume: false,
            scopes: vec![Vec::new()],
            loops: Vec::new(),
            unwind: None,
            shadow_base: None,
            next_shadow: 0,
            genc: None,
            term: false,
        };
        shadow_prologue(&mut body, slots)?;
        bind_params(&mut body, params, &pro.param_vals)?;
        // Captures: read-only copies inside the environment.
        let env = body
            .env_v
            .ok_or_else(|| internal(format!("lambda without env at {pos}")))?;
        for (name, ty, off) in captures {
            let addr = body.addr_off(env, i64::from(*off));
            body.bind(
                name,
                Binding {
                    ty: ty.clone(),
                    storage: Storage::Addr(addr),
                    range: None,
                },
            );
        }
        body.lower_stmts(stmts)?;
        body.finish()?;
    }
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define {name}: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(id)
}

/// Defines a fixed-ABI runtime→script bridge for one Map/Set forEach
/// call. The runtime always supplies pointers to stored bytes; this
/// bridge loads the concrete monomorphized values and invokes the actual
/// `(ctx, env, value, key)` / `(ctx, env, key)` script callback.
fn define_assoc_bridge<M: Module>(
    ml: &mut ModLower<M>,
    key: &Type,
    value: Option<&Type>,
) -> Result<cranelift_module::FuncId, String> {
    let mut bridge_sig = Signature::new(ml.call_conv);
    let fixed_params = if value.is_some() { 5 } else { 4 };
    for _ in 0..fixed_params {
        bridge_sig.params.push(AbiParam::new(types::I64));
    }
    let name = format!("ss_assoc_bridge{}", ml.lambda_count);
    ml.lambda_count += 1;
    let id = ml
        .module
        .declare_function(&name, cranelift_module::Linkage::Local, &bridge_sig)
        .map_err(|e| internal(format!("declare {name}: {e}")))?;
    let script_params = match value {
        Some(value) => vec![value.clone(), key.clone()],
        None => vec![key.clone()],
    };
    let script_sig = ml.make_sig(&script_params, &Type::Void, true, false)?;
    let mut cctx = ml.module.make_context();
    cctx.func.signature = bridge_sig;
    let mut fbx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let fixed = b.block_params(entry).to_vec();
        let ctx = fixed[0];
        let code = fixed[1];
        let env = fixed[2];
        let pointers: Vec<Value> = if value.is_some() {
            vec![fixed[3], fixed[4]]
        } else {
            vec![fixed[3]]
        };
        let mut argv = vec![ctx, env];
        for (ty, pointer) in script_params.iter().zip(pointers) {
            match ml.layouts.repr(ty)? {
                Repr::None => {}
                Repr::Scalar(repr) => {
                    argv.push(b.ins().load(repr, flags(), pointer, 0));
                }
                Repr::Pair => {
                    argv.push(b.ins().load(types::I64, flags(), pointer, 0));
                    argv.push(b.ins().load(types::I64, flags(), pointer, 8));
                }
                Repr::Agg { size, align } => {
                    // C2: the runtime pointer addresses the container's
                    // live inline entry. The callback must receive a
                    // caller-owned copy, exactly like an ordinary script
                    // call and the C bridge's by-value struct load.
                    let slot = b.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        size.max(1),
                        align_shift(align.max(1)),
                    ));
                    let copy = b.ins().stack_addr(types::I64, slot, 0);
                    let config = ml.module.isa().frontend_config();
                    let access_align = 1u32 << size.max(1).trailing_zeros();
                    let copy_align = align.max(1).min(access_align);
                    b.emit_small_memory_copy(
                        config,
                        copy,
                        pointer,
                        u64::from(size),
                        copy_align as u8,
                        copy_align as u8,
                        true,
                        MemFlags::new(),
                    );
                    argv.push(copy);
                }
            }
        }
        let sigref = b.import_signature(script_sig);
        b.ins().call_indirect(sigref, code, &argv);
        b.ins().return_(&[]);
        b.seal_all_blocks();
        b.finalize();
    }
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define {name}: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(id)
}

/// On-demand env-taking wrapper for a named function used as a value
/// (a13): `(ctx, env, args...) -> target(ctx, args...)`.
pub(crate) fn wrapper_for<M: Module>(
    ml: &mut ModLower<M>,
    name: &str,
) -> Result<cranelift_module::FuncId, String> {
    let key = FnKey::Wrapper(name.to_string());
    if let Some(&id) = ml.fns.get(&key) {
        return Ok(id);
    }
    let f = ml.hir_fn(name)?;
    if f.is_generator {
        return Err(internal("generators are not function values"));
    }
    let params_ty: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
    let sig = ml.make_sig(&params_ty, &f.ret, true, false)?;
    let sym = format!("ss_wrap_{}", ml.fns.len());
    let id = ml
        .module
        .declare_function(&sym, cranelift_module::Linkage::Local, &sig)
        .map_err(|e| internal(format!("declare {sym}: {e}")))?;
    ml.bind_slot(&key, id);
    ml.fns.insert(key, id);

    let target = FnKey::Free(name.to_string());
    let target_id = ml.func_id(&target)?;
    let mut cctx = ml.module.make_context();
    cctx.func.signature = sig;
    let mut fbx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let vals = b.block_params(entry).to_vec();
        // Drop the env parameter (vals[1]); forward the rest.
        let mut argv = vec![vals[0]];
        argv.extend_from_slice(&vals[2..]);
        // The wrapper forwards through the indirection table in reload
        // mode, so a function value taken before a swap still reaches
        // the post-swap body (the wrapper itself is pure forwarding and
        // its shape is fixed by the signature, which a swap cannot
        // change).
        let inst = if ml.opts.reload {
            let (code, sigref) = indirect_target(ml, &mut b, vals[0], &target)?;
            b.ins().call_indirect(sigref, code, &argv)
        } else {
            let fref = ml.module.declare_func_in_func(target_id, b.func);
            b.ins().call(fref, &argv)
        };
        let results = b.inst_results(inst).to_vec();
        b.ins().return_(&results);
        b.seal_all_blocks();
        b.finalize();
    }
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define {sym}: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(id)
}

/// Frame layout of a generator: `(param offsets, let offsets, size)`.
fn generator_frame<M: Module>(
    ml: &ModLower<M>,
    f: &hir::Function,
) -> Result<(Vec<u32>, Vec<u32>, u32), String> {
    let mut off = GEN_PAYLOAD_OFF;
    let mut param_offsets = Vec::new();
    for p in &f.params {
        let (s, a) = ml.layouts.size_align(&p.ty)?;
        off = round_up(off, a.max(1));
        param_offsets.push(off);
        off += s.max(1);
    }
    let mut lets: Vec<&Type> = Vec::new();
    walk_lets(&f.body, &mut lets);
    let mut let_offsets = Vec::new();
    for t in lets {
        let (s, a) = ml.layouts.size_align(t)?;
        off = round_up(off, a.max(1));
        let_offsets.push(off);
        off += s.max(1);
    }
    Ok((param_offsets, let_offsets, round_up(off, 8)))
}

/// Defines the creator and resume functions of a `function*` (C8).
pub(crate) fn define_generator<M: Module>(
    ml: &mut ModLower<M>,
    f: &hir::Function,
) -> Result<(), String> {
    let (param_offsets, let_offsets, frame_size) = generator_frame(ml, f)?;
    let yield_ty = match &f.ret {
        Type::Generator(y) => (**y).clone(),
        other => return Err(internal(format!("generator return {other:?}"))),
    };
    let creator_id = ml.func_id(&FnKey::Free(f.name.clone()))?;
    let resume_id = ml.func_id(&FnKey::Resume(f.name.clone()))?;

    // --- creator ---
    {
        let params_ty: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
        let sig =
            ml.make_sig(&params_ty, &Type::Generator(Box::new(Type::Void)), false, false)?;
        let mut cctx = ml.module.make_context();
        cctx.func.signature = sig;
        let mut fbx = FunctionBuilderContext::new();
        {
            let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let pro = split_params(ml, &mut b, entry, &Type::Void, false, false, &f.params)?;
            let mut body = Body {
                ml,
                b,
                ctx_v: pro.ctx_v,
                env_v: None,
                sret_v: None,
                this_v: None,
                ret_ty: Type::Generator(Box::new(yield_ty.clone())),
                is_resume: false,
                scopes: vec![Vec::new()],
                loops: Vec::new(),
                unwind: None,
                shadow_base: None,
                next_shadow: 0,
                genc: None,
                term: false,
            };
            let size_v = body.iconst(types::I64, i64::from(frame_size));
            let class_v = body.iconst(types::I32, i64::from(rtc::CLASS_GENERATOR));
            let pid = body.pos_id(&f.pos);
            let pos_v = body.iconst(types::I32, pid);
            let res = body.call_rt(
                body.ml.rt.alloc,
                &[body.ctx_v, size_v, class_v, pos_v],
                true,
            )?;
            let frame = res.ok_or_else(|| internal("frame alloc result"))?;
            // state = 0 (fresh allocation is zeroed); resume pointer:
            let rref = body.ml.module.declare_func_in_func(resume_id, body.b.func);
            let raddr = body.b.ins().func_addr(types::I64, rref);
            body.b.ins().store(flags(), raddr, frame, GEN_RESUME_OFF);
            if body.ml.opts.reload {
                // Stamp the creation epoch so a resume after a swap
                // traps instead of re-entering a replaced body.
                let epoch_off = ctx_off(rtc::Context::reload_epoch_offset())?;
                let epoch = body
                    .b
                    .ins()
                    .load(types::I32, flags(), body.ctx_v, epoch_off);
                body.b.ins().store(flags(), epoch, frame, GEN_EPOCH_OFF);
            }
            // Parameters into the frame.
            let mut vi = 0usize;
            for (p, off) in f.params.iter().zip(&param_offsets) {
                match body.ml.layouts.repr(&p.ty)? {
                    Repr::None => {}
                    Repr::Pair => {
                        let rv = RV::P(pro.param_vals[vi], pro.param_vals[vi + 1]);
                        vi += 2;
                        body.store_val(&p.ty, frame, *off as i32, rv)?;
                    }
                    Repr::Agg { .. } => {
                        let rv = RV::A(pro.param_vals[vi]);
                        vi += 1;
                        body.store_val(&p.ty, frame, *off as i32, rv)?;
                    }
                    Repr::Scalar(_) => {
                        let rv = RV::S(pro.param_vals[vi]);
                        vi += 1;
                        body.store_val(&p.ty, frame, *off as i32, rv)?;
                    }
                }
            }
            body.b.ins().return_(&[frame]);
            body.term = true;
            body.finish()?;
        }
        ml.module
            .define_function(creator_id, &mut cctx)
            .map_err(|e| internal(format!("define creator: {e}")))?;
        ml.module.clear_context(&mut cctx);
    }

    // --- resume: the state machine ---
    {
        let sig = ml.resume_sig();
        let mut cctx = ml.module.make_context();
        cctx.func.signature = sig;
        let mut fbx = FunctionBuilderContext::new();
        {
            let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let vals = b.block_params(entry).to_vec();
            let (ctx_v, frame, out) = (vals[0], vals[1], vals[2]);
            let n_yields = count_yields(&f.body);
            let start = b.create_block();
            let done_blk = b.create_block();
            let resume_blocks: Vec<Block> = (0..n_yields).map(|_| b.create_block()).collect();
            // Dispatch on the state word.
            let state = b.ins().load(types::I32, flags(), frame, 0);
            let mut cur = entry;
            for (i, &blk) in std::iter::once(&start).chain(&resume_blocks).enumerate() {
                let _ = cur;
                let eq = b.ins().icmp_imm(IntCC::Equal, state, i as i64);
                let next = b.create_block();
                b.ins().brif(eq, blk, &[], next, &[]);
                b.switch_to_block(next);
                cur = next;
            }
            b.ins().jump(done_blk, &[]);
            // Already-finished coroutine: stays done (C8), value slot
            // was zero-filled by the caller.
            b.switch_to_block(done_blk);
            let one = b.ins().iconst(types::I8, 1);
            b.ins().return_(&[one]);

            b.switch_to_block(start);
            let mut body = Body {
                ml,
                b,
                ctx_v,
                env_v: None,
                sret_v: None,
                this_v: None,
                ret_ty: Type::Void,
                is_resume: true,
                scopes: vec![Vec::new()],
                loops: Vec::new(),
                unwind: None,
                shadow_base: None,
                next_shadow: 0,
                genc: Some(GenCtx {
                    frame,
                    out,
                    yield_ty: yield_ty.clone(),
                    resume_blocks,
                    next_resume: 0,
                    let_offsets,
                    next_let: 0,
                }),
                term: false,
            };
            // Parameters live in the frame.
            for (p, off) in f.params.iter().zip(&param_offsets) {
                body.bind(
                    &p.name,
                    Binding {
                        ty: p.ty.clone(),
                        storage: Storage::Frame(*off),
                        range: None,
                    },
                );
            }
            body.lower_stmts(&f.body)?;
            body.finish()?;
        }
        ml.module
            .define_function(resume_id, &mut cctx)
            .map_err(|e| internal(format!("define resume: {e}")))?;
        ml.module.clear_context(&mut cctx);
    }
    Ok(())
}

/// Defines the synthesized `ss_init` function: evaluates every
/// module-global initializer in declaration order and registers
/// managed globals as collection roots.
pub(crate) fn define_init<M: Module>(ml: &mut ModLower<M>) -> Result<(), String> {
    let id = ml.func_id(&FnKey::Init)?;
    let sig = ml.make_sig(&[], &Type::Void, false, false)?;
    let mut cctx = ml.module.make_context();
    cctx.func.signature = sig;
    let mut fbx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let ctx_v = b.block_params(entry)[0];
        let mut body = Body {
            ml,
            b,
            ctx_v,
            env_v: None,
            sret_v: None,
            this_v: None,
            ret_ty: Type::Void,
            is_resume: false,
            scopes: vec![Vec::new()],
            loops: Vec::new(),
            unwind: None,
            shadow_base: None,
            next_shadow: 0,
            genc: None,
            term: false,
        };
        let globals: Vec<hir::Global> = body.ml.hir.globals.to_vec();
        for g in &globals {
            let rv = body.eval(&g.init)?;
            let (addr, ty) = body.global_slot(&g.name)?;
            body.store_val(&ty, addr, 0, rv)?;
            // Root registration: one word for a managed scalar, the
            // whole (word-scanned) range for an aggregate global with
            // managed interior (M1).
            let words = managed_words(&body.ml.layouts, &ty)?;
            if words > 0 {
                let words_v = body.iconst(types::I64, i64::from(words));
                body.call_rt(
                    body.ml.rt.root_add,
                    &[body.ctx_v, addr, words_v],
                    false,
                )?;
            }
        }
        body.finish()?;
    }
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define init: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(())
}

#[cfg(test)]
mod hfa_tests {
    use super::is_pure_hfa_leaves;
    use cranelift_codegen::ir::types;

    #[test]
    fn pure_float_aggregates_are_hfas() {
        // 1–4 members, all the same fundamental float type.
        assert!(is_pure_hfa_leaves(&[types::F32]));
        assert!(is_pure_hfa_leaves(&[types::F64]));
        assert!(is_pure_hfa_leaves(&[types::F32, types::F32]));
        assert!(is_pure_hfa_leaves(&[types::F64, types::F64]));
        assert!(is_pure_hfa_leaves(&[
            types::F32,
            types::F32,
            types::F32,
            types::F32
        ]));
    }

    #[test]
    fn non_hfa_returns_are_not_rejected() {
        // All-integer (a37's shapes), mixed integer+float, mixed float
        // widths, empty, and >4 members are NOT pure HFAs — the register/
        // sret integer path handles them and must keep working.
        assert!(!is_pure_hfa_leaves(&[types::I64])); // {u64}
        assert!(!is_pure_hfa_leaves(&[types::I64, types::I64])); // {u64,u64}
        assert!(!is_pure_hfa_leaves(&[types::I64, types::F64])); // mixed {u64,double}
        assert!(!is_pure_hfa_leaves(&[types::F32, types::F64])); // mixed float widths
        assert!(!is_pure_hfa_leaves(&[])); // no leaves
        assert!(!is_pure_hfa_leaves(&[
            types::F32,
            types::F32,
            types::F32,
            types::F32,
            types::F32
        ])); // 5 floats — not an HFA (>4 members)
    }
}
