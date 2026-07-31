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
use subscript_compiler::types::{
    CRANELIFT_FRAME_ALIGNMENT, MAX_FRAME_BYTES,
};
use subscript_runtime::context as rtc;
use subscript_runtime::TrapKind;

use crate::layout::{has_managed_interior, is_managed, is_unsigned, managed_words, Repr};
use crate::lower::{
    checked_layout_add, checked_layout_mul, internal, round_up_layout, FnKey, GlobalSlot, ModLower,
};
use crate::trap_sites::{lower_trap_sites, TrapSiteConsumer};

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

/// Backend values already materialized for one HIR trap site.
///
/// Keeping this separate from `hir::Expr` is intentional: the HIR site owns
/// the operand role, while this value supplies the one SSA value produced by
/// evaluation. No guard can ask the lowering to evaluate an expression a
/// second time.
enum TrapOperand {
    Pending,
    Value(Value),
    Condition(Value),
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
}

/// An assignable location.
///
/// Dynamic-array elements stay *unresolved* (`ArrayElem`) until the
/// moment of the access: resolving the element address early would
/// dangle if the value expression grows the same array (its storage
/// is reallocated on growth).
#[derive(Debug, Clone)]
enum Place {
    Var(Variable),
    Pair(Variable, Variable),
    Mem(Value, i32),
    ArrayElem {
        handle: Value,
        index: Value,
        read_site: Option<hir::TrapSite>,
        write_site: hir::TrapSite,
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
    child_offsets: Vec<u32>,
    next_child: usize,
    kind: FrameKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Generator,
    Async,
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
            hir::Stmt::ForOf { ty, body, .. } => {
                out.push(ty);
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

/// Verifies the exact explicit-slot layout before handing a function to
/// Cranelift. The checker owns source diagnostics for this bound; this
/// second line of defense covers HIR constructed by another route and
/// makes the invariant safe in release builds too.
fn ensure_explicit_frame_supported(
    function: &cranelift_codegen::ir::Function,
    context: &str,
) -> Result<(), String> {
    let align_up = |value: u64, align: u64| {
        let mask = align.checked_sub(1)?;
        value.checked_add(mask).map(|sum| sum & !mask)
    };
    let mut end = 0u64;
    for data in function.sized_stack_slots.values() {
        let requested = 1u32
            .checked_shl(u32::from(data.align_shift))
            .ok_or_else(|| internal(format!("{context} has invalid stack-slot alignment")))?;
        let align = u64::from(requested.max(8));
        end = align_up(end, align)
            .and_then(|start| start.checked_add(u64::from(data.size)))
            .ok_or_else(|| internal(format!("{context} stack-frame size overflows u64")))?;
    }
    let final_size = align_up(end, u64::from(CRANELIFT_FRAME_ALIGNMENT))
        .ok_or_else(|| internal(format!("{context} stack-frame size overflows u64")))?;
    if final_size > u64::from(MAX_FRAME_BYTES) {
        return Err(internal(format!(
            "{context} explicit stack frame is {final_size} bytes after ABI alignment; \
             maximum supported frame size is {MAX_FRAME_BYTES} bytes"
        )));
    }
    Ok(())
}

fn count_yields_expr(e: &hir::Expr) -> usize {
    use hir::ExprKind as K;
    match &e.kind {
        K::Yield(arg) => 1 + arg.as_deref().map_or(0, count_yields_expr),
        K::AsyncSuspend => 1,
        K::AsyncCall { args, .. } => {
            1 + args.iter().map(count_yields_expr).sum::<usize>()
        }
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
        K::DescriptorLit { fields, .. } => fields
            .iter()
            .flatten()
            .map(count_yields_expr)
            .sum(),
        K::Field { obj, .. } | K::JsonResultValue(obj) => count_yields_expr(obj),
        K::Length(obj) => count_yields_expr(obj),
        K::Index { obj, index, .. } => count_yields_expr(obj) + count_yields_expr(index),
        K::ArrayLit(elems) => elems.iter().map(count_yields_expr).sum(),
        K::ArraySpreadLit(elems) => {
            elems.iter().map(|elem| count_yields_expr(&elem.expr)).sum()
        }
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

fn walk_async_calls_expr(e: &hir::Expr, out: &mut usize) {
    use hir::ExprKind as K;
    match &e.kind {
        K::AsyncCall { args, .. } => {
            for arg in args {
                walk_async_calls_expr(arg, out);
            }
            *out += 1;
        }
        K::Unary { operand, .. } | K::Cast(operand) | K::Length(operand) => {
            walk_async_calls_expr(operand, out)
        }
        K::Binary { left, right, .. } => {
            walk_async_calls_expr(left, out);
            walk_async_calls_expr(right, out);
        }
        K::Assign { target, value, .. } => {
            walk_async_calls_expr(target, out);
            walk_async_calls_expr(value, out);
        }
        K::Call { callee, args } => {
            match callee {
                hir::Callee::Value(value) => walk_async_calls_expr(value, out),
                hir::Callee::Method { recv, .. } => walk_async_calls_expr(recv, out),
                _ => {}
            }
            for arg in args {
                walk_async_calls_expr(arg, out);
            }
        }
        K::New { args, .. } | K::ArrayLit(args) => {
            for arg in args {
                walk_async_calls_expr(arg, out);
            }
        }
        K::DescriptorLit { fields, .. } => {
            for value in fields.iter().flatten() {
                walk_async_calls_expr(value, out);
            }
        }
        K::Field { obj, .. } | K::JsonResultValue(obj) => walk_async_calls_expr(obj, out),
        K::Index { obj, index, .. } => {
            walk_async_calls_expr(obj, out);
            walk_async_calls_expr(index, out);
        }
        K::ArraySpreadLit(elems) => {
            for elem in elems {
                walk_async_calls_expr(&elem.expr, out);
            }
        }
        K::Template(parts) => {
            for part in parts {
                if let hir::TplPart::Expr(value) = part {
                    walk_async_calls_expr(value, out);
                }
            }
        }
        K::Yield(Some(value)) => walk_async_calls_expr(value, out),
        K::Cond { cond, then, els } => {
            walk_async_calls_expr(cond, out);
            walk_async_calls_expr(then, out);
            walk_async_calls_expr(els, out);
        }
        _ => {}
    }
}

fn count_async_calls(stmts: &[hir::Stmt]) -> usize {
    fn walk(stmts: &[hir::Stmt], out: &mut usize) {
        for stmt in stmts {
            match stmt {
                hir::Stmt::Let { init, .. } | hir::Stmt::Expr(init) => {
                    walk_async_calls_expr(init, out)
                }
                hir::Stmt::Return { value, .. } => {
                    if let Some(value) = value {
                        walk_async_calls_expr(value, out);
                    }
                }
                hir::Stmt::If { cond, then, els, .. } => {
                    walk_async_calls_expr(cond, out);
                    walk(then, out);
                    if let Some(els) = els {
                        walk(els, out);
                    }
                }
                hir::Stmt::While { cond, body, .. } => {
                    walk_async_calls_expr(cond, out);
                    walk(body, out);
                }
                hir::Stmt::For { init, cond, step, body, .. } => {
                    if let Some(init) = init {
                        walk(std::slice::from_ref(init), out);
                    }
                    if let Some(cond) = cond {
                        walk_async_calls_expr(cond, out);
                    }
                    if let Some(step) = step {
                        walk_async_calls_expr(step, out);
                    }
                    walk(body, out);
                }
                hir::Stmt::ForOf { subject, body, .. } => {
                    walk_async_calls_expr(subject, out);
                    walk(body, out);
                }
                hir::Stmt::Switch { disc, cases, .. } => {
                    walk_async_calls_expr(disc, out);
                    for case in cases {
                        if let Some(test) = &case.test {
                            walk_async_calls_expr(test, out);
                        }
                        walk(&case.body, out);
                    }
                }
                hir::Stmt::Block(body) => walk(body, out),
                hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
                _ => {}
            }
        }
    }
    let mut count = 0;
    walk(stmts, &mut count);
    count
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
            hir::Stmt::ForOf { subject, body, .. } => {
                n += count_yields_expr(subject) + count_yields(body);
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
    /// Map/Set traversals currently active at this lowering point.
    /// Explicit returns close them before leaving the function.
    assoc_iters: Vec<Value>,
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

    /// Lowers one explicit HIR trap site.
    ///
    /// This match must stay exhaustive. Adding a `TrapSite` variant without
    /// teaching this lowering what it means is therefore a compile error.
    fn emit_trap_site(
        &mut self,
        site: &hir::TrapSite,
        operand: TrapOperand,
    ) -> Result<(), String> {
        let value = |operand: TrapOperand, what: &str| match operand {
            TrapOperand::Value(value) | TrapOperand::Condition(value) => Ok(value),
            TrapOperand::Pending => Err(internal(format!("{what} trap operand"))),
        };
        match site {
            hir::TrapSite::Allocation { .. } | hir::TrapSite::Call { .. } => {
                if !matches!(operand, TrapOperand::Pending) {
                    return Err(internal("pending-trap site received a value"));
                }
                self.trap_check();
                Ok(())
            }
            hir::TrapSite::DivisionByZero { pos } => {
                let divisor = value(operand, "division")?;
                let nonzero = self
                    .b
                    .ins()
                    .icmp_imm(IntCC::NotEqual, divisor, 0);
                self.guard(nonzero, TrapKind::DivisionByZero, pos)
            }
            hir::TrapSite::IndexRead { pos } | hir::TrapSite::IndexWrite { pos } => {
                match operand {
                    TrapOperand::Pending => {
                        self.trap_check();
                        Ok(())
                    }
                    TrapOperand::Value(condition) | TrapOperand::Condition(condition) => {
                        self.guard(condition, TrapKind::IndexOutOfBounds, pos)
                    }
                }
            }
            hir::TrapSite::JsonResultValue { pos } => {
                let condition = value(operand, "JsonResult.value")?;
                self.guard(condition, TrapKind::JsonResultValue, pos)
            }
            hir::TrapSite::NullNarrowing { pos } => {
                let pointer = value(operand, "null narrowing")?;
                let nonnull = self
                    .b
                    .ins()
                    .icmp_imm(IntCC::NotEqual, pointer, 0);
                self.guard(nonnull, TrapKind::NullNarrowing, pos)
            }
            hir::TrapSite::ClassMismatch { class, pos } => {
                let pointer = value(operand, "class narrowing")?;
                let class_id = self.b.ins().load(
                    types::I32,
                    flags(),
                    pointer,
                    rtc::CLASS_ID_OFFSET,
                );
                let matches = self.b.ins().icmp_imm(
                    IntCC::Equal,
                    class_id,
                    i64::from(class.0 as u32),
                );
                self.guard(matches, TrapKind::ClassMismatch, pos)
            }
            hir::TrapSite::DevOnlyLifetime { pos } => {
                match operand {
                    TrapOperand::Pending => {
                        self.trap_check();
                        Ok(())
                    }
                    TrapOperand::Value(pointer) | TrapOperand::Condition(pointer) => {
                        self.live_check(pointer, pos)
                    }
                }
            }
            hir::TrapSite::DevReloadOnlyStaleCoroutine { pos } => {
                let frame = value(operand, "stale coroutine")?;
                self.reload_epoch_check(frame, pos)
            }
        }
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
        checked: bool,
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
        if checked {
            self.trap_check();
        }
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
        site: &hir::TrapSite,
    ) -> Result<Value, String> {
        let pos_id = self.pos_id(site.pos());
        let pos_v = self.iconst(types::I32, pos_id);
        let r = self.call_rt(
            self.ml.rt.array_ptr,
            &[self.ctx_v, handle, index, pos_v],
            false,
        )?;
        self.emit_trap_site(site, TrapOperand::Pending)?;
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
                read_site,
                ..
            } => {
                let site = read_site
                    .as_ref()
                    .ok_or_else(|| internal("array place read has no HIR site"))?;
                let addr = self.resolve_array_elem(handle, index, site)?;
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
                write_site,
                ..
            } => {
                let addr = self.resolve_array_elem(handle, index, &write_site)?;
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

    /// Declares storage for a synthesized loop binding. The fused
    /// lowering writes it once per active visit.
    fn declare_loop_local(&mut self, name: &str, ty: &Type) -> Result<Binding, String> {
        let storage = self.alloc_storage(ty)?;
        let binding = Binding {
            ty: ty.clone(),
            storage,
        };
        self.bind(name, binding.clone());
        Ok(binding)
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
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "string literal", |sites| {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        internal("string literal has no HIR allocation site"),
                    )?;
                    let h = self.string_literal(s.as_bytes(), site)?;
                    Ok(RV::S(h))
                })
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
            K::Binary { op, left, right } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "binary expression", |sites| {
                    self.eval_binary(*op, left, right, &e.pos, sites)
                })
            }
            K::Assign { op, target, value } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "assignment", |sites| {
                    self.eval_assign(*op, target, value, &e.pos, sites)
                })
            }
            K::Cast(inner) => {
                let v = self.eval(inner)?;
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "cast", |sites| {
                    self.eval_cast(v, &inner.ty, &e.ty, sites)
                })
            }
            K::Call { callee, args } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "call", |sites| {
                    self.eval_call(callee, args, &e.ty, &e.pos, sites, None)
                })
            }
            K::New { class, args } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "new expression", |sites| {
                    self.eval_new(class.0, args, &e.pos, sites, None)
                })
            }
            K::DescriptorLit { class, fields } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "descriptor literal", |sites| {
                    self.eval_descriptor_lit(class.0, fields, &e.pos, sites)
                })
            }
            K::Zero => Ok(match self.ml.layouts.repr(&e.ty)? {
                Repr::None => RV::None,
                Repr::Scalar(ty) => RV::S(self.zero_of(ty)),
                Repr::Pair => RV::P(self.iconst(types::I64, 0), self.iconst(types::I64, 0)),
                Repr::Agg { size, align } => {
                    let slot = self.temp_slot(size, align);
                    self.zero_bytes(slot, size, align);
                    RV::A(slot)
                }
            }),
            K::RawNew { class } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "RawNew", |sites| {
                    self.eval_raw_new(class.0, sites)
                })
            }
            K::Field { obj, name } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "field read", |sites| {
                    let (addr, off, fty) = self.field_addr(obj, name, sites)?;
                    self.load_val(&fty, addr, off)
                })
            }
            K::JsonResultValue(obj) => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "JsonResult.value", |sites| {
                    self.eval_json_result_value(obj, sites)
                })
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
            K::Index {
                obj,
                index,
                ..
            } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "index read", |sites| {
                    let (addr, elem_ty) = self.index_addr(obj, index, sites)?;
                    self.load_val(&elem_ty, addr, 0)
                })
            }
            K::ArrayLit(elems) => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "array literal", |sites| {
                    self.eval_array_lit(&e.ty, elems, sites)
                })
            }
            K::ArraySpreadLit(elems) => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "array spread literal", |sites| {
                    self.eval_array_spread_lit(&e.ty, elems, sites)
                })
            }
            K::Template(parts) => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "template", |sites| {
                    self.eval_template(parts, sites)
                })
            }
            K::Lambda {
                params,
                ret,
                body,
                captures,
            } => self.eval_lambda(params, ret, body, captures, &e.pos),
            K::Yield(arg) => self.eval_yield(arg.as_deref(), &e.pos),
            K::AsyncSuspend => self.eval_async_suspend(&e.pos),
            K::AsyncCall { function, args } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "async call", |sites| {
                    self.eval_async_call(function, args, &e.ty, &e.pos, sites)
                })
            }
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

    fn string_literal(
        &mut self,
        bytes: &[u8],
        site: &hir::TrapSite,
    ) -> Result<Value, String> {
        let hir::TrapSite::Allocation { pos } = site else {
            return Err(internal("string literal has a non-allocation HIR site"));
        };
        let data = self.ml.literal_data(bytes)?;
        let gv = self.ml.module.declare_data_in_func(data, self.b.func);
        let addr = self.b.ins().symbol_value(types::I64, gv);
        let len = self.iconst(types::I64, bytes.len() as i64);
        let pid = self.pos_id(pos);
        let pos_v = self.iconst(types::I32, pid);
        let r = self.call_rt(
            self.ml.rt.str_lit,
            &[self.ctx_v, addr, len, pos_v],
            false,
        )?;
        self.emit_trap_site(site, TrapOperand::Pending)?;
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
        sites: &mut TrapSiteConsumer<'_>,
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
                    let site = sites
                        .take_required(
                            |site| matches!(site, hir::TrapSite::Allocation { .. }),
                            internal("string addition has no HIR allocation site"),
                        )?;
                    let pid = self.pos_id(pos);
                    let pos_v = self.iconst(types::I32, pid);
                    let res = self
                        .call_rt(self.ml.rt.str_concat, &[self.ctx_v, l, r, pos_v], false)?;
                    self.emit_trap_site(site, TrapOperand::Pending)?;
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
                    let site = sites
                        .take_required(
                            |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                            internal("integer div/rem has no HIR trap site"),
                        )?;
                    return self
                        .int_divrem(op == B::Div, l, r, unsigned, site)
                        .map(RV::S);
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
        site: &hir::TrapSite,
    ) -> Result<Value, String> {
        self.emit_trap_site(site, TrapOperand::Value(r))?;
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

    fn eval_cast(
        &mut self,
        rv: RV,
        from: &Type,
        to: &Type,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        // Reference narrowing: object / object|null -> reference class.
        if let Type::Class(_) = to {
            if matches!(from, Type::Object)
                || matches!(from, Type::Nullable(inner) if **inner == Type::Object)
            {
                let ptr = self.expect_s(rv)?;
                while let Some(site) = sites.take(|_| true) {
                    self.emit_trap_site(site, TrapOperand::Value(ptr))?;
                }
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
            // The x64 backend only converts from 32/64-bit integers; a
            // narrow source (I8/I16) is `unreachable!` there. Widen it to
            // I32 first, matching its signedness, before the float convert.
            // This is a no-op on every result: the extended value is the
            // same number, so goldens stay byte-exact on all architectures.
            let src_ty = match self.ml.layouts.repr(&from)? {
                Repr::Scalar(t) => t,
                other => return Err(internal(format!("integer source repr {other:?}"))),
            };
            let v = if src_ty.bits() < 32 {
                if is_unsigned(&from) {
                    self.b.ins().uextend(types::I32, v)
                } else {
                    self.b.ins().sextend(types::I32, v)
                }
            } else {
                v
            };
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
            // The x64 backend only produces 32/64-bit integer results from
            // the saturating float->int conversions; a narrow target
            // (I8/I16) is `unreachable!` there. For a narrow target,
            // saturate into I32, clamp to the narrow width's range, then
            // reduce. The clamp reproduces the narrow saturation of
            // `fcvt_to_{s,u}int_sat(<narrow>)` bit-for-bit for every input
            // (in-range, overflow both ways, NaN->0), so the CLIF stays
            // arch-independent and the tiers and goldens keep agreeing.
            let out = if target.bits() >= 32 {
                if is_unsigned(&to) {
                    self.b.ins().fcvt_to_uint_sat(target, v)
                } else {
                    self.b.ins().fcvt_to_sint_sat(target, v)
                }
            } else if is_unsigned(&to) {
                let wide = self.b.ins().fcvt_to_uint_sat(types::I32, v);
                let hi = self.iconst(types::I32, (1i64 << target.bits()) - 1);
                let clamped = self.b.ins().umin(wide, hi);
                self.b.ins().ireduce(target, clamped)
            } else {
                let wide = self.b.ins().fcvt_to_sint_sat(types::I32, v);
                let lo = self.iconst(types::I32, -(1i64 << (target.bits() - 1)));
                let hi = self.iconst(types::I32, (1i64 << (target.bits() - 1)) - 1);
                let low_clamped = self.b.ins().smax(wide, lo);
                let clamped = self.b.ins().smin(low_clamped, hi);
                self.b.ins().ireduce(target, clamped)
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
        sites: &mut TrapSiteConsumer<'_>,
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
                    let site = sites
                        .take_required(
                            |site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }),
                            internal("reference field has no HIR lifetime site"),
                        )?;
                    self.emit_trap_site(site, TrapOperand::Value(ptr))?;
                    Ok((ptr, off, fty))
                }
            }
            other => Err(internal(format!("field access on {other:?}"))),
        }
    }

    /// Reads a `JsonResult<T>` payload after checking its sibling `ok`
    /// field. The checker emits this HIR form only for the exact
    /// monomorphized two-field result class.
    fn eval_json_result_value(
        &mut self,
        obj: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        let Type::Class(cid) = &obj.ty else {
            return Err(internal("JsonResult value receiver is not a class"));
        };
        let class = self
            .ml
            .hir
            .classes
            .get(cid.0)
            .ok_or_else(|| internal("JsonResult class id out of range"))?;
        let ok_index = class
            .fields
            .iter()
            .position(|field| field.name == "ok" && field.ty == Type::Bool)
            .ok_or_else(|| internal("JsonResult is missing its boolean ok field"))?;
        let value_index = class
            .fields
            .iter()
            .position(|field| field.name == "value")
            .ok_or_else(|| internal("JsonResult is missing its value field"))?;
        let value_ty = class.fields[value_index].ty.clone();
        let layout = self.ml.layouts.class(cid.0)?;
        if layout.is_value {
            return Err(internal("JsonResult unexpectedly has value-class layout"));
        }
        let ok_offset = *layout
            .field_offsets
            .get(ok_index)
            .ok_or_else(|| internal("JsonResult ok field offset out of range"))?
            as i32;
        let value_offset = *layout
            .field_offsets
            .get(value_index)
            .ok_or_else(|| internal("JsonResult value field offset out of range"))?
            as i32;

        let rv = self.eval(obj)?;
        let ptr = self.expect_s(rv)?;
        while let Some(site) =
            sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
        {
            self.emit_trap_site(site, TrapOperand::Value(ptr))?;
        }
        let ok = self.load_val(&Type::Bool, ptr, ok_offset)?;
        let ok = self.expect_s(ok)?;
        while let Some(site) =
            sites.take(|site| matches!(site, hir::TrapSite::JsonResultValue { .. }))
        {
            self.emit_trap_site(site, TrapOperand::Condition(ok))?;
        }
        self.load_val(&value_ty, ptr, value_offset)
    }

    /// Element address for indexing; bounds-checked. Returns
    /// `(addr, element type)`. Evaluation order: object, then index.
    fn index_addr(
        &mut self,
        obj: &hir::Expr,
        index: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<(Value, Type), String> {
        match &obj.ty {
            Type::FixedArray(elem, n) => {
                let rv = self.eval(obj)?;
                let base = self.expect_a(rv)?;
                let idx_rv = self.eval(index)?;
                let idx = self.expect_s(idx_rv)?;
                // HIR already made the proof-based elision decision.
                if let Some(site) =
                    sites.take(|site| {
                        matches!(
                            site,
                            hir::TrapSite::IndexRead { .. }
                                | hir::TrapSite::IndexWrite { .. }
                        )
                    })
                {
                    let ok = self
                        .b
                        .ins()
                        .icmp_imm(IntCC::UnsignedLessThan, idx, i64::from(*n));
                    self.emit_trap_site(site, TrapOperand::Condition(ok))?;
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
                while let Some(site) =
                    sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                {
                    self.emit_trap_site(site, TrapOperand::Value(h))?;
                }
                let idx_rv = self.eval(index)?;
                let idx = self.expect_s(idx_rv)?;
                let site = sites
                    .take_required(
                        |site| matches!(site, hir::TrapSite::IndexRead { .. }),
                        internal("dynamic index has no HIR read site"),
                    )?;
                let addr = self.resolve_array_elem(h, idx, site)?;
                Ok((addr, (**elem).clone()))
            }
            other => Err(internal(format!("index on {other:?}"))),
        }
    }

    fn place(
        &mut self,
        e: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<(Place, Type), String> {
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
                let (addr, off, fty) = self.field_addr(obj, name, sites)?;
                Ok((Place::Mem(addr, off), fty))
            }
            K::Index {
                obj,
                index,
                ..
            } => {
                if let Type::Array(elem) = &obj.ty {
                    // Deferred: the element address is resolved at the
                    // moment of the access, after the assigned value
                    // has been evaluated (growth-safe).
                    let rv = self.eval(obj)?;
                    let handle = self.expect_s(rv)?;
                    while let Some(site) =
                        sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                    {
                        self.emit_trap_site(site, TrapOperand::Value(handle))?;
                    }
                    let idx_rv = self.eval(index)?;
                    let idx = self.expect_s(idx_rv)?;
                    let read_site = sites
                        .take(|site| matches!(site, hir::TrapSite::IndexRead { .. }))
                        .cloned();
                    let write_site = sites
                        .take(|site| matches!(site, hir::TrapSite::IndexWrite { .. }))
                        .cloned()
                        .ok_or_else(|| internal("array assignment has no HIR write site"))?;
                    return Ok((
                        Place::ArrayElem {
                            handle,
                            index: idx,
                            read_site,
                            write_site,
                        },
                        (**elem).clone(),
                    ));
                }
                let (addr, elem_ty) = self.index_addr(obj, index, sites)?;
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
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        let (place, ty) = self.place(target, sites)?;
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
                let cur = self.read_place(place.clone(), &ty)?;
                let cur_v = self.expect_s(cur)?;
                let rhs = self.eval(value)?;
                let rhs_v = self.expect_s(rhs)?;
                let combined = self.apply_binop(bin, &ty, cur_v, rhs_v, pos, sites)?;
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
        sites: &mut TrapSiteConsumer<'_>,
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
                    let site = sites
                        .take_required(
                            |site| matches!(site, hir::TrapSite::Allocation { .. }),
                            internal(
                                "string compound assignment has no HIR allocation site",
                            ),
                        )?;
                    let pid = self.pos_id(pos);
                    let pos_v = self.iconst(types::I32, pid);
                    let res = self.call_rt(
                        self.ml.rt.str_concat,
                        &[self.ctx_v, l, r, pos_v],
                        false,
                    )?;
                    self.emit_trap_site(site, TrapOperand::Pending)?;
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
                    let site = sites
                        .take_required(
                            |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                            internal("integer compound div has no HIR trap site"),
                        )?;
                    self.int_divrem(true, l, r, unsigned, site)?
                }
            }
            B::Rem => {
                let site = sites
                    .take_required(
                        |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                        internal("integer compound rem has no HIR trap site"),
                    )?;
                self.int_divrem(false, l, r, unsigned, site)?
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
        sites: &mut TrapSiteConsumer<'_>,
        dest: Option<Value>,
    ) -> Result<RV, String> {
        let checked = sites
            .take(|site| matches!(site, hir::TrapSite::Call { .. }))
            .is_some();
        match callee {
            hir::Callee::Func(name) => {
                let f = self.ml.hir_fn(name)?;
                if f.is_generator {
                    // Creator call: allocates and initializes the frame.
                    let mut argv = vec![self.ctx_v];
                    self.push_args(&mut argv, &f.params, args)?;
                    let res =
                        self.call_script(&FnKey::Free(name.clone()), &argv, checked)?;
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
                let res =
                    self.call_script(&FnKey::Free(name.clone()), &argv, checked)?;
                self.shape_results(&ret, &res, sret)
            }
            hir::Callee::Ambient(a) => {
                self.eval_ambient(*a, args, pos, sites, checked)
            }
            hir::Callee::Math(f) => self.eval_math(*f, args, checked),
            hir::Callee::Num(f) => self.eval_num(*f, args, pos, checked),
            hir::Callee::Date(f) => self.eval_date(*f, args, pos, checked),
            hir::Callee::Json(f) => self.eval_json(*f, args, pos, checked),
            hir::Callee::Str(f) => self.eval_str(*f, args, pos, checked),
            hir::Callee::Regex(f) => self.eval_regex(*f, args, pos, checked),
            hir::Callee::Arr(f) => self.eval_arr(*f, args, ret_ty, pos, checked),
            hir::Callee::Map(f) => self.eval_map(*f, args, ret_ty, pos, checked),
            hir::Callee::Set(f) => self.eval_set(*f, args, ret_ty, pos, checked),
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
                if checked {
                    self.trap_check();
                }
                self.shape_results(&ft.ret, &res, sret)
            }
            hir::Callee::Method { recv, name } => {
                self.eval_method(recv, name, args, ret_ty, pos, sites, dest, checked)
            }
            hir::Callee::Foreign(name) => {
                self.eval_foreign_call(name, args, pos, checked)
            }
            other => Err(internal(format!("callee {other:?}"))),
        }
    }

    /// Lowers a foreign C-ABI call (`Callee::Foreign`, P5.2b) to a direct
    /// call of the header symbol. The signature is built from the mirror's
    /// boundary types by marshaling each argument per Q13; the symbol is
    /// imported (`Linkage::Import`) exactly as the `subscript_rt_*` runtime is,
    /// and resolved by the JIT's symbol registration / the ship-C link.
    fn eval_foreign_call(
        &mut self,
        name: &str,
        args: &[hir::Expr],
        pos: &Pos,
        checked: bool,
    ) -> Result<RV, String> {
        let ff = self.ml.foreign_fn(name)?;
        let params = ff.params.clone();
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
        for (parameter, a) in params.iter().zip(args) {
            let rv = self.eval(a)?;
            self.marshal_foreign_arg(parameter, rv, &mut sig, &mut argv)?;
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
            self.ml.foreign_symbols.push(name.to_string());
            id
        };
        let fref = self.ml.module.declare_func_in_func(id, self.b.func);
        let inst = self.b.ins().call(fref, &argv);
        let res = self.b.inst_results(inst).to_vec();
        // A foreign call may set the Context trap flag — directly, or via
        // a callback that trapped inside the trampoline — so check it.
        if checked {
            self.trap_check();
        }
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
            let chunk_count = u32::try_from(chunks.len())
                .map_err(|_| internal("struct-return chunk count does not fit in u32"))?;
            let slot_size = checked_layout_mul(chunk_count, 8, "struct-return register image")?;
            let slot = self.temp_slot(slot_size, align.max(8));
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
    /// per Q13/§27: `string` → a by-value string-view aggregate; `T[]` →
    /// either a by-value `(pointer,count)` descriptor or the two scalar-pair
    /// ABI arguments `(count,pointer)`, according to typed provenance; a
    /// by-value boundary struct → its fields as eightbytes (with the callback
    /// trampoline for a function-pointer field); `Struct | null` → a nullable
    /// struct pointer; handles, `object | null`, and scalars → one value.
    fn marshal_foreign_arg(
        &mut self,
        parameter: &hir::Param,
        rv: RV,
        sig: &mut Signature,
        argv: &mut Vec<Value>,
    ) -> Result<(), String> {
        let ty = &parameter.ty;
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
                let h = self.expect_s(rv)?;
                let data = self
                    .call_rt(self.ml.rt.array_data, &[self.ctx_v, h], false)?
                    .ok_or_else(|| internal("array_data result"))?;
                let len32 = self
                    .call_rt(self.ml.rt.array_len, &[self.ctx_v, h], false)?
                    .ok_or_else(|| internal("array_len result"))?;
                let count = self.b.ins().uextend(types::I64, len32);
                match &parameter.foreign_provenance {
                    Some(hir::ForeignTypeProvenance::Descriptor { .. }) => {
                        // A descriptor is the C aggregate `{ T *items;
                        // size_t count; }` (16 bytes, align 8), passed BY
                        // VALUE — target-specific ABI as above (§12.3a).
                        let comps = [
                            (0u32, types::I64, data),
                            (8u32, types::I64, count),
                        ];
                        self.push_aggregate_abi(sig, argv, &comps, 16, 8);
                        Ok(())
                    }
                    Some(hir::ForeignTypeProvenance::ScalarPair { .. }) => {
                        // §27 is not an aggregate: the original C function
                        // has two adjacent parameters, count first and
                        // pointer second. Both come from the same language
                        // array handle, so mutable writes land directly in
                        // the caller's backing storage.
                        self.push_abi(sig, argv, types::I64, count);
                        self.push_abi(sig, argv, types::I64, data);
                        Ok(())
                    }
                    None => Err(internal(format!(
                        "foreign array parameter `{}` lacks boundary provenance",
                        parameter.name
                    ))),
                    Some(other) => Err(internal(format!(
                        "foreign array parameter `{}` has incompatible provenance {other:?}",
                        parameter.name
                    ))),
                }
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
            coff = round_up_layout(coff, ca, "boundary C struct layout")?;
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
                    coff = checked_layout_add(coff, cs, "boundary C struct layout")?;
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
                    coff =
                        round_up_layout(coff, uda1, "boundary callback userdata layout")?;
                    struct_align = struct_align.max(uda1);
                    comps.push((coff, types::I64, record));
                    coff =
                        checked_layout_add(coff, uds1, "boundary callback userdata layout")?;
                    if has_ud2 {
                        // Second userdata C slot → null (the binding carries
                        // the real second userdata).
                        let ud2_field = &class.fields[i + 2];
                        let (uds2, uda2) = self.boundary_c_field(&ud2_field.ty)?;
                        coff =
                            round_up_layout(coff, uda2, "boundary callback userdata layout")?;
                        struct_align = struct_align.max(uda2);
                        let nullv = self.iconst(types::I64, 0);
                        comps.push((coff, types::I64, nullv));
                        coff =
                            checked_layout_add(coff, uds2, "boundary callback userdata layout")?;
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
                    let data_offset =
                        checked_layout_add(coff, 8, "boundary array descriptor layout")?;
                    comps.push((data_offset, types::I64, data));
                    coff = checked_layout_add(coff, cs, "boundary C struct layout")?;
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
                    coff = checked_layout_add(coff, cs, "boundary C struct layout")?;
                    i += 1;
                }
            }
        }
        let total = round_up_layout(
            coff,
            struct_align.max(1),
            "final boundary C struct layout",
        )?;
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
    /// `subscript_rt_math_*` runtime call. `clz32` is `(ctx, u32) -> i32`;
    /// all others use `f64`. No trap check follows: the runtime entries
    /// never trap (pure, or a PRNG state advance). Constants never reach
    /// here — they folded to literals at check time.
    fn eval_math(
        &mut self,
        f: hir::MathFn,
        args: &[hir::Expr],
        checked: bool,
    ) -> Result<RV, String> {
        if args.len() != f.arity() {
            return Err(internal(format!("Math.{} arity", f.name())));
        }
        let mut argv = vec![self.ctx_v];
        for a in args {
            let rv = self.eval(a)?;
            argv.push(self.expect_s(rv)?);
        }
        let res = self.call_rt(self.ml.rt.math[f as usize], &argv, checked)?;
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
        checked: bool,
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
        let result = self
            .call_rt(self.ml.rt.num[f as usize], &argv, checked)?
            .ok_or_else(|| internal(format!("{} result", f.name())))?;
        Ok(RV::S(if f.returns_bool() {
            self.b.ins().icmp_imm(IntCC::NotEqual, result, 0)
        } else {
            result
        }))
    }

    /// Lowers one leaf of the checker-generated `JSON.stringify<T>`
    /// serializer graph. The graph itself is ordinary typed HIR; both
    /// tiers share these opaque builder, formatter, escaping, and
    /// active-reference operations in the runtime.
    fn eval_json(
        &mut self,
        f: hir::JsonFn,
        args: &[hir::Expr],
        pos: &Pos,
        checked: bool,
    ) -> Result<RV, String> {
        use hir::JsonFn as J;
        let expected = match f {
            J::Begin | J::BeginTracked => 0,
            J::Finish | J::Null | J::ParseBegin | J::ParseEnd | J::ParseRoot => 1,
            J::Raw
            | J::Str
            | J::I32
            | J::U32
            | J::I64
            | J::U64
            | J::F32
            | J::F64
            | J::Bool
            | J::Date
            | J::Visit
            | J::Leave
            | J::ParseNumber
            | J::ParseBool
            | J::ParseString
            | J::ParseArrayLen => 2,
            J::ParseIsKind
            | J::ParseNumberFits
            | J::ParseInteger
            | J::ParseArrayGet
            | J::ParseObjectGet => 3,
            other => return Err(internal(format!("unknown JsonFn {other:?}"))),
        };
        if args.len() != expected {
            return Err(internal(format!("JSON {} arity", f.symbol())));
        }
        let mut argv = vec![self.ctx_v];
        for arg in args {
            let value = self.eval(arg)?;
            argv.push(self.expect_s(value)?);
        }
        let pid = self.pos_id(pos);
        argv.push(self.iconst(types::I32, pid));
        let result = self.call_rt(self.ml.rt.json[f as usize], &argv, checked)?;
        Ok(match f {
            J::Begin
            | J::BeginTracked
            | J::Finish
            | J::ParseBegin
            | J::ParseRoot
            | J::ParseNumber
            | J::ParseInteger
            | J::ParseString
            | J::ParseArrayLen
            | J::ParseArrayGet
            | J::ParseObjectGet => {
                RV::S(result.ok_or_else(|| internal(format!("{} result", f.symbol())))?)
            }
            J::Visit | J::ParseIsKind | J::ParseNumberFits | J::ParseBool => {
                let value = result.ok_or_else(|| internal("subscript_rt_json_visit result"))?;
                RV::S(self.b.ins().icmp_imm(IntCC::NotEqual, value, 0))
            }
            _ => RV::None,
        })
    }

    /// Lowers a `Date` intrinsic (stdlib.md §3) to its opaque
    /// `subscript_rt_date_*` runtime call. A Date value is its `i64`
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
        checked: bool,
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
                let res =
                    self.call_rt(self.ml.rt.date_new, &[self.ctx_v, ms, pos_v], checked)?;
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
                let res = self.call_rt(self.ml.rt.date_utc, &argv, checked)?;
                res.map(RV::S).ok_or_else(|| internal("Date.UTC result"))
            }
            D::Now => {
                let res = self.call_rt(self.ml.rt.date_now, &[self.ctx_v], checked)?;
                res.map(RV::S).ok_or_else(|| internal("Date.now result"))
            }
            D::ToIso => {
                let ms = scalar_arg(
                    self,
                    args.first().ok_or_else(|| internal("toISOString receiver"))?,
                )?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(
                    self.ml.rt.date_to_iso,
                    &[self.ctx_v, ms, pos_v],
                    checked,
                )?;
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
                let res = self.call_rt(
                    self.ml.rt.date_get,
                    &[self.ctx_v, ms, field],
                    checked,
                )?;
                res.map(RV::S)
                    .ok_or_else(|| internal(format!("Date accessor {} result", accessor.name())))
            }
        }
    }

    /// Lowers a `String` method intrinsic (stdlib.md §8) to its opaque
    /// `subscript_rt_str_*` runtime call. The receiver is the first HIR
    /// argument and every value is a scalar (string handles and `i32`
    /// byte measures); a trailing `pos_id` and a trap check follow
    /// exactly when the symbol is fault-capable
    /// ([`hir::StrFn::takes_pos_id`]). A `boolean` result arrives as
    /// `i32` 0/1 and is narrowed here.
    fn eval_str(
        &mut self,
        f: hir::StrFn,
        args: &[hir::Expr],
        pos: &Pos,
        checked: bool,
    ) -> Result<RV, String> {
        if args.len() != 1 + f.params().len() {
            return Err(internal(format!("{} arity (checker normalizes)", f.name())));
        }
        let mut argv = vec![self.ctx_v];
        for a in args {
            let rv = self.eval(a)?;
            argv.push(self.expect_s(rv)?);
        }
        if f.takes_pos_id() {
            let pid = self.pos_id(pos);
            argv.push(self.iconst(types::I32, pid));
        }
        let res =
            self.call_rt(self.ml.rt.str_ops[f as usize], &argv, checked)?;
        let res = res.ok_or_else(|| internal(format!("{} result", f.name())))?;
        Ok(RV::S(match f.ret() {
            hir::StrRet::Bool => self.b.ins().icmp_imm(IntCC::NotEqual, res, 0),
            _ => res,
        }))
    }

    fn eval_regex(
        &mut self,
        function: hir::RegexFn,
        args: &[hir::Expr],
        pos: &Pos,
        checked: bool,
    ) -> Result<RV, String> {
        use hir::RegexFn as R;
        let expected = match function {
            R::New | R::Test | R::Search | R::Split => 2,
            R::Source | R::Flags => 1,
            R::Replace | R::ReplaceAll => 3,
            R::MatchStart | R::MatchEnd => 2,
            other => return Err(internal(format!("unknown RegexFn {other:?}"))),
        };
        if args.len() != expected {
            return Err(internal(format!(
                "{} arity (expected {expected}, got {})",
                function.symbol(),
                args.len()
            )));
        }
        let mut argv = vec![self.ctx_v];
        for arg in args {
            let value = self.eval(arg)?;
            argv.push(self.expect_s(value)?);
        }
        if function.can_trap() {
            let pos_id = self.pos_id(pos);
            argv.push(self.iconst(types::I32, pos_id));
        }
        let result = self
            .call_rt(self.ml.rt.regex_ops[function as usize], &argv, checked)?
            .ok_or_else(|| internal(format!("{} result", function.symbol())))?;
        Ok(RV::S(if function == R::Test {
            self.b.ins().icmp_imm(IntCC::NotEqual, result, 0)
        } else {
            result
        }))
    }

    /// Lowers an `Array` method intrinsic (stdlib.md §9) to its opaque
    /// `subscript_rt_arr_*` runtime call. The receiver handle is the first
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
        checked: bool,
    ) -> Result<RV, String> {
        use hir::ArrFn as A;
        let recv = args.first().ok_or_else(|| internal("array method receiver"))?;
        let (elem, fixed_len) = match &recv.ty {
            Type::Array(e) => ((**e).clone(), None),
            Type::FixedArray(e, n) => ((**e).clone(), Some(*n)),
            other => return Err(internal(format!("array method on {other:?}"))),
        };
        let rv = self.eval(recv)?;
        let h = if fixed_len.is_some() {
            self.expect_a(rv)?
        } else {
            self.expect_s(rv)?
        };
        let rt = if fixed_len.is_some() {
            self.ml.rt.fixed_arr_ops[f as usize]
                .ok_or_else(|| internal(format!("{} is not a FixedArray method", f.name())))?
        } else {
            self.ml.rt.arr_ops[f as usize]
        };
        let arg_at = |i: usize| -> Result<&hir::Expr, String> {
            args.get(i)
                .ok_or_else(|| internal(format!("{} arity (checker normalizes)", f.name())))
        };
        let callback_indexed = |callback: &hir::Expr| -> Result<bool, String> {
            let indexed_arity = f.callback_index_arity().ok_or_else(|| {
                internal(format!("{} has no indexed callback shape", f.name()))
            })?;
            let Type::Func(ft) = &callback.ty else {
                return Err(internal(format!("{} callback is not a function", f.name())));
            };
            match ft.params.len() {
                arity if arity + 1 == indexed_arity => Ok(false),
                arity if arity == indexed_arity => Ok(true),
                arity => Err(internal(format!(
                    "{} callback arity {arity} escaped the checker",
                    f.name()
                ))),
            }
        };
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
            A::Splice => {
                let start = self.eval(arg_at(1)?)?;
                let start = self.expect_s(start)?;
                let delete_count = self.eval(arg_at(2)?)?;
                let delete_count = self.expect_s(delete_count)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(
                    rt,
                    &[self.ctx_v, h, start, delete_count, pos_v],
                    checked,
                )?;
                res.map(RV::S).ok_or_else(|| internal("splice result"))
            }
            A::Shift => {
                let (size, align) = self.ml.layouts.size_align(&elem)?;
                let dst = self.temp_slot(size.max(8), align.max(8));
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                self.call_rt(rt, &[self.ctx_v, h, dst, pos_v], checked)?;
                self.load_val(&elem, dst, 0)
            }
            A::Unshift => {
                let x = self.eval(arg_at(1)?)?;
                let ptr = self.materialize(x, &elem)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(rt, &[self.ctx_v, h, ptr, pos_v], checked)?;
                res.map(RV::S).ok_or_else(|| internal("unshift result"))
            }
            A::CopyWithin => {
                let target = self.eval(arg_at(1)?)?;
                let target = self.expect_s(target)?;
                let start = self.eval(arg_at(2)?)?;
                let start = self.expect_s(start)?;
                let end = self.eval(arg_at(3)?)?;
                let end = self.expect_s(end)?;
                self.call_rt(rt, &[self.ctx_v, h, target, start, end], checked)?;
                Ok(RV::S(h))
            }
            A::ForEach | A::Filter | A::Some | A::Every | A::FindIndex => {
                let kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let callback = arg_at(1)?;
                let indexed = callback_indexed(callback)?;
                let cb = self.eval(callback)?;
                let (code, env) = self.expect_p(cb)?;
                let kv = self.iconst(types::I32, i64::from(kind.code()));
                let mut argv = vec![self.ctx_v, h];
                if let Some(n) = fixed_len {
                    let stride = self.ml.layouts.stride(&elem)?;
                    argv.push(self.iconst(types::I64, i64::from(n)));
                    argv.push(self.iconst(types::I64, i64::from(stride)));
                }
                argv.extend([code, env, kv]);
                if f == A::Filter {
                    let pid = self.pos_id(pos);
                    argv.push(self.iconst(types::I32, pid));
                }
                argv.push(self.iconst(types::I32, i64::from(indexed)));
                let res = self.call_rt(rt, &argv, checked)?;
                Ok(match f {
                    A::ForEach => RV::None,
                    A::Some | A::Every => {
                        let r = res.ok_or_else(|| internal("predicate result"))?;
                        RV::S(self.b.ins().icmp_imm(IntCC::NotEqual, r, 0))
                    }
                    _ => RV::S(res.ok_or_else(|| internal(format!("{} result", f.name())))?),
                })
            }
            A::Sort => {
                let kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let cb = self.eval(arg_at(1)?)?;
                let (code, env) = self.expect_p(cb)?;
                let kv = self.iconst(types::I32, i64::from(kind.code()));
                self.call_rt(rt, &[self.ctx_v, h, code, env, kv], checked)?;
                Ok(RV::S(h))
            }
            A::Map => {
                let elem_kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let ret_elem = match ret_ty {
                    Type::Array(u) => (**u).clone(),
                    other => return Err(internal(format!("map result {other:?}"))),
                };
                let ret_kind = crate::layout::arr_elem_kind(self.ml.hir, &ret_elem)?;
                let ret_stride = self.ml.layouts.stride(&ret_elem)?;
                let callback = arg_at(1)?;
                let indexed = callback_indexed(callback)?;
                let cb = self.eval(callback)?;
                let (code, env) = self.expect_p(cb)?;
                let ekv = self.iconst(types::I32, i64::from(elem_kind.code()));
                let rkv = self.iconst(types::I32, i64::from(ret_kind.code()));
                let size_v = self.iconst(types::I64, i64::from(ret_stride));
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let indexed_v = self.iconst(types::I32, i64::from(indexed));
                let mut argv = vec![self.ctx_v, h];
                if let Some(n) = fixed_len {
                    let elem_stride = self.ml.layouts.stride(&elem)?;
                    argv.push(self.iconst(types::I64, i64::from(n)));
                    argv.push(self.iconst(types::I64, i64::from(elem_stride)));
                }
                argv.extend([
                    code, env, ekv, rkv, size_v, pos_v, indexed_v,
                ]);
                let res = self.call_rt(
                    rt,
                    &argv,
                    checked,
                )?;
                res.map(RV::S).ok_or_else(|| internal("map result"))
            }
            A::Reduce | A::ReduceRight => {
                let elem_kind = crate::layout::arr_elem_kind(self.ml.hir, &elem)?;
                let acc_kind = crate::layout::arr_elem_kind(self.ml.hir, ret_ty)?;
                let acc_stride = self.ml.layouts.stride(ret_ty)?;
                let callback = arg_at(1)?;
                let indexed = callback_indexed(callback)?;
                let cb = self.eval(callback)?;
                let (code, env) = self.expect_p(cb)?;
                // The accumulator travels in/out through a caller slot.
                let init = self.eval(arg_at(2)?)?;
                let slot = self.materialize(init, ret_ty)?;
                let ekv = self.iconst(types::I32, i64::from(elem_kind.code()));
                let akv = self.iconst(types::I32, i64::from(acc_kind.code()));
                let size_v = self.iconst(types::I64, i64::from(acc_stride));
                let indexed_v = self.iconst(types::I32, i64::from(indexed));
                let mut argv = vec![self.ctx_v, h];
                if let Some(n) = fixed_len {
                    let elem_stride = self.ml.layouts.stride(&elem)?;
                    argv.push(self.iconst(types::I64, i64::from(n)));
                    argv.push(self.iconst(types::I64, i64::from(elem_stride)));
                }
                argv.extend([
                    code, env, ekv, akv, size_v, slot, indexed_v,
                ]);
                self.call_rt(
                    rt,
                    &argv,
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
        checked: bool,
    ) -> Result<RV, String> {
        use hir::MapFn as F;
        if f == F::GroupBy {
            let (key, elem) = match (ret_ty, args.first().map(|arg| &arg.ty)) {
                (Type::Map(key, value), Some(Type::Array(elem))) => match &**value {
                    Type::Array(group_elem) if **group_elem == **elem => {
                        ((**key).clone(), (**elem).clone())
                    }
                    other => {
                        return Err(internal(format!(
                            "Map.groupBy result value {other:?}"
                        )))
                    }
                },
                other => return Err(internal(format!("Map.groupBy shape {other:?}"))),
            };
            let items_expr = args
                .first()
                .ok_or_else(|| internal("Map.groupBy items"))?;
            let items_rv = self.eval(items_expr)?;
            let items = self.expect_s(items_rv)?;
            self.live_check(items, pos)?;
            let callback = self.eval(
                args.get(1)
                    .ok_or_else(|| internal("Map.groupBy callback"))?,
            )?;
            let (code, env) = self.expect_p(callback)?;
            let bridge_id = define_group_bridge(self.ml, &elem, &key)?;
            let bridge_ref = self
                .ml
                .module
                .declare_func_in_func(bridge_id, self.b.func);
            let bridge = self.b.ins().func_addr(types::I64, bridge_ref);
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let kind = crate::layout::assoc_key_kind(self.ml.hir, &key)?;
            let pos_id = self.pos_id(pos);
            let argv = [
                self.ctx_v,
                items,
                code,
                env,
                bridge,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I32, i64::from(kind.code())),
                self.iconst(types::I32, i64::from(pos_id)),
            ];
            let result =
                self.call_rt(self.ml.rt.map_ops[f as usize], &argv, checked)?;
            return result
                .map(RV::S)
                .ok_or_else(|| internal("Map.groupBy result"));
        }
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
            F::GroupBy => Err(internal("Map.GroupBy reached receiver lowering")),
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
        checked: bool,
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
            F::Union | F::Intersection | F::Difference | F::SymmetricDifference => {
                let other_rv = self.eval(arg(1)?)?;
                let other = self.expect_s(other_rv)?;
                self.live_check(other, pos)?;
                let pos_id = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, i64::from(pos_id));
                let result =
                    self.call_rt(rt, &[self.ctx_v, handle, other, pos_v], checked)?;
                result
                    .map(RV::S)
                    .ok_or_else(|| internal(format!("Set.{} result", f.name())))
            }
            F::IsSubsetOf | F::IsSupersetOf | F::IsDisjointFrom => {
                let other_rv = self.eval(arg(1)?)?;
                let other = self.expect_s(other_rv)?;
                self.live_check(other, pos)?;
                let result =
                    self.call_rt(rt, &[self.ctx_v, handle, other], false)?;
                let result =
                    result.ok_or_else(|| internal(format!("Set.{} result", f.name())))?;
                Ok(RV::S(
                    self.b.ins().icmp_imm(IntCC::NotEqual, result, 0),
                ))
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
        sites: &mut TrapSiteConsumer<'_>,
        checked: bool,
    ) -> Result<RV, String> {
        match a {
            hir::AmbientFn::Print => {
                let arg = args.first().ok_or_else(|| internal("print arity"))?;
                let rv = self.eval(arg)?;
                let h = self.expect_s(rv)?;
                self.call_rt(self.ml.rt.print, &[self.ctx_v, h], checked)?;
                Ok(RV::None)
            }
            hir::AmbientFn::Collect => {
                self.call_rt(self.ml.rt.collect, &[self.ctx_v], checked)?;
                Ok(RV::None)
            }
            hir::AmbientFn::UnsafeDelete => {
                let arg = args.first().ok_or_else(|| internal("Context.free arity"))?;
                let rv = self.eval(arg)?;
                let ptr = self.expect_s(rv)?;
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                self.call_rt(
                    self.ml.rt.delete,
                    &[self.ctx_v, ptr, pos_v],
                    false,
                )?;
                let site = sites
                    .take_required(
                        |site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }),
                        internal("Context.free has no HIR lifetime site"),
                    )?;
                self.emit_trap_site(site, TrapOperand::Pending)?;
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
        sites: &mut TrapSiteConsumer<'_>,
        dest: Option<Value>,
        checked: bool,
    ) -> Result<RV, String> {
        match recv.ty.clone() {
            Type::Array(elem) => {
                let rv = self.eval(recv)?;
                let h = self.expect_s(rv)?;
                while let Some(site) =
                    sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                {
                    self.emit_trap_site(site, TrapOperand::Value(h))?;
                }
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
                            checked,
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
                            checked,
                        )?;
                        self.load_val(&elem, dst, 0)
                    }
                    other => Err(internal(format!("array method `{other}`"))),
                }
            }
            Type::Str => Err(internal(format!("string method `{name}`"))),
            Type::Generator(y) => {
                if name != "next" {
                    return Err(internal(format!("generator method `{name}`")));
                }
                let rv = self.eval(recv)?;
                let frame = self.expect_s(rv)?;
                while let Some(site) = sites.take(|site| {
                    matches!(
                        site,
                        hir::TrapSite::DevOnlyLifetime { .. }
                            | hir::TrapSite::DevReloadOnlyStaleCoroutine { .. }
                    )
                }) {
                    self.emit_trap_site(site, TrapOperand::Value(frame))?;
                }
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
                if checked {
                    self.trap_check();
                }
                self.b.ins().store(flags(), done, slot, 0);
                Ok(RV::A(slot))
            }
            Type::Class(cid) => {
                let rv = self.eval(recv)?;
                let this = match rv {
                    RV::A(ptr) => ptr,
                    RV::S(ptr) => {
                        while let Some(site) = sites
                            .take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                        {
                            self.emit_trap_site(site, TrapOperand::Value(ptr))?;
                        }
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
                let res = self.call_script(
                    &FnKey::Method(cid.0, name.to_string()),
                    &argv,
                    checked,
                )?;
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
        sites: &mut TrapSiteConsumer<'_>,
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
            let site = sites
                .take_required(
                    |site| matches!(site, hir::TrapSite::Allocation { .. }),
                    internal("reference new has no HIR allocation site"),
                )?;
            let size = self.iconst(types::I64, i64::from(layout.size));
            let class_v = self.iconst(types::I32, i64::from(cid as u32));
            let pid = self.pos_id(pos);
            let pos_v = self.iconst(types::I32, pid);
            let res = self.call_rt(
                self.ml.rt.alloc,
                &[self.ctx_v, size, class_v, pos_v],
                false,
            )?;
            self.emit_trap_site(site, TrapOperand::Pending)?;
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
            let site = sites
                .take_required(
                    |site| matches!(site, hir::TrapSite::Call { .. }),
                    internal("constructor call has no HIR call site"),
                )?;
            let mut argv = vec![self.ctx_v, this];
            self.push_args(&mut argv, &ctor.params, args)?;
            self.call_script(&FnKey::Ctor(cid), &argv, false)?;
            self.emit_trap_site(site, TrapOperand::Pending)?;
        }
        Ok(if layout.is_value {
            RV::A(this)
        } else {
            RV::S(this)
        })
    }

    /// Constructs a Q33 descriptor literal as an ordinary reference-class
    /// allocation followed by declaration-ordered member stores. Omitted
    /// members evaluate their checked defaults here, once per construction.
    fn eval_descriptor_lit(
        &mut self,
        cid: usize,
        fields: &[Option<hir::Expr>],
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        let class = self
            .ml
            .hir
            .classes
            .get(cid)
            .cloned()
            .ok_or_else(|| internal("descriptor class id out of range"))?;
        let layout = self.ml.layouts.class(cid)?.clone();
        if !class.is_descriptor || layout.is_value {
            return Err(internal("DescriptorLit does not name a descriptor reference class"));
        }
        if fields.len() != class.fields.len() {
            return Err(internal(format!(
                "descriptor `{}` has {} fields but its literal has {} slots",
                class.name,
                class.fields.len(),
                fields.len()
            )));
        }
        let site = sites.take_required(
            |site| matches!(site, hir::TrapSite::Allocation { .. }),
            internal("descriptor literal has no HIR allocation site"),
        )?;
        let size = self.iconst(types::I64, i64::from(layout.size));
        let class_v = self.iconst(types::I32, i64::from(cid as u32));
        let pid = self.pos_id(pos);
        let pos_v = self.iconst(types::I32, pid);
        let result = self.call_rt(
            self.ml.rt.alloc,
            &[self.ctx_v, size, class_v, pos_v],
            false,
        )?;
        self.emit_trap_site(site, TrapOperand::Pending)?;
        let this = result.ok_or_else(|| internal("descriptor allocation result"))?;

        for (index, (slot, field)) in fields.iter().zip(&class.fields).enumerate() {
            let value = match slot {
                Some(value) => self.eval(value)?,
                None => {
                    if !field.is_defaulted {
                        return Err(internal(format!(
                            "required descriptor member `{}` has no literal value",
                            field.name
                        )));
                    }
                    let default = field.init.as_ref().ok_or_else(|| {
                        internal(format!(
                            "defaulted descriptor member `{}` has no checked default",
                            field.name
                        ))
                    })?;
                    let saved_this = self.this_v;
                    self.this_v = Some((this, cid));
                    let evaluated = self.eval(default);
                    self.this_v = saved_this;
                    evaluated?
                }
            };
            let offset = layout.field_offsets[index] as i32;
            self.store_val(&field.ty, this, offset, value)?;
        }
        Ok(RV::S(this))
    }

    /// Allocates a zeroed reference-class payload without running source
    /// field initializers or its constructor. Only checker-generated
    /// JSON.parse construction uses this path.
    fn eval_raw_new(
        &mut self,
        cid: usize,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        let site = sites
            .take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                internal("RawNew has no HIR allocation site"),
            )?;
        let hir::TrapSite::Allocation { pos } = site else {
            return Err(internal("RawNew has a non-allocation HIR site"));
        };
        let layout = self.ml.layouts.class(cid)?.clone();
        if layout.is_value {
            return Err(internal("raw allocation requested for a value class"));
        }
        let size = self.iconst(types::I64, i64::from(layout.size));
        let class_v = self.iconst(types::I32, i64::from(cid as u32));
        let pos_id = self.pos_id(pos);
        let pos_v = self.iconst(types::I32, pos_id);
        let result = self.call_rt(
            self.ml.rt.alloc,
            &[self.ctx_v, size, class_v, pos_v],
            false,
        )?;
        self.emit_trap_site(site, TrapOperand::Pending)?;
        Ok(RV::S(result.ok_or_else(|| {
            internal("raw JSON object allocation result")
        })?))
    }

    fn eval_array_lit(
        &mut self,
        ty: &Type,
        elems: &[hir::Expr],
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        match ty {
            Type::Array(elem) => {
                let site = sites
                    .take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        internal("array literal has no HIR allocation site"),
                    )?;
                let hir::TrapSite::Allocation { pos } = site else {
                    return Err(internal("array literal has a non-allocation HIR site"));
                };
                let stride = self.ml.layouts.stride(elem)?;
                let stride_v = self.iconst(types::I64, i64::from(stride));
                let pid = self.pos_id(pos);
                let pos_v = self.iconst(types::I32, pid);
                let res = self.call_rt(
                    self.ml.rt.array_new,
                    &[self.ctx_v, stride_v, pos_v],
                    false,
                )?;
                self.emit_trap_site(site, TrapOperand::Pending)?;
                let h = res.ok_or_else(|| internal("array_new result"))?;
                for e in elems {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        internal("array literal element has no HIR allocation site"),
                    )?;
                    let hir::TrapSite::Allocation { pos } = site else {
                        return Err(internal(
                            "array literal element has a non-allocation HIR site",
                        ));
                    };
                    let rv = self.eval(e)?;
                    let src = self.materialize(rv, elem)?;
                    let pid = self.pos_id(pos);
                    let pos_v = self.iconst(types::I32, pid);
                    self.call_rt(
                        self.ml.rt.array_push,
                        &[self.ctx_v, h, src, pos_v],
                        false,
                    )?;
                    self.emit_trap_site(site, TrapOperand::Pending)?;
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

    fn eval_array_spread_lit(
        &mut self,
        ty: &Type,
        elems: &[hir::ArrayLitElem],
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        let Type::Array(elem_ty) = ty else {
            return Err(internal("spread literal is not a dynamic array"));
        };
        let initial = sites
            .take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                internal("array spread literal has no allocation site"),
            )?;
        let hir::TrapSite::Allocation { pos } = initial else {
            return Err(internal("array spread literal allocation site kind"));
        };
        let stride = self.ml.layouts.stride(elem_ty)?;
        let stride_v = self.iconst(types::I64, i64::from(stride));
        let pos_id = self.pos_id(pos);
        let pid = self.iconst(types::I32, pos_id);
        let handle = self
            .call_rt(
                self.ml.rt.array_new,
                &[self.ctx_v, stride_v, pid],
                false,
            )?
            .ok_or_else(|| internal("array spread literal handle"))?;
        self.emit_trap_site(initial, TrapOperand::Pending)?;

        for elem in elems {
            let site = sites
                .take_required(
                    |site| matches!(site, hir::TrapSite::Allocation { .. }),
                    internal("array spread element has no allocation site"),
                )?;
            let hir::TrapSite::Allocation { pos } = site else {
                return Err(internal("array spread element site kind"));
            };
            let pos_id = self.pos_id(pos);
            let pid = self.iconst(types::I32, pos_id);
            match elem.spread {
                None => {
                    let value = self.eval(&elem.expr)?;
                    let src = self.materialize(value, elem_ty)?;
                    self.call_rt(
                        self.ml.rt.array_push,
                        &[self.ctx_v, handle, src, pid],
                        false,
                    )?;
                }
                Some(hir::SpreadKind::Array) => {
                    let value = self.eval(&elem.expr)?;
                    let source = self.expect_s(value)?;
                    self.call_rt(
                        self.ml.rt.array_spread_array,
                        &[self.ctx_v, handle, source, pid],
                        false,
                    )?;
                }
                Some(hir::SpreadKind::FixedArray) => {
                    let value = self.eval(&elem.expr)?;
                    let source = self.expect_a(value)?;
                    let Type::FixedArray(_, count) = &elem.expr.ty else {
                        return Err(internal("fixed spread source type"));
                    };
                    let count = self.iconst(types::I64, i64::from(*count));
                    self.call_rt(
                        self.ml.rt.array_spread_fixed,
                        &[self.ctx_v, handle, source, count, pid],
                        false,
                    )?;
                }
                Some(hir::SpreadKind::MapKeys | hir::SpreadKind::SetValues) => {
                    let value = self.eval(&elem.expr)?;
                    let source = self.expect_s(value)?;
                    self.call_rt(
                        self.ml.rt.array_spread_assoc,
                        &[self.ctx_v, handle, source, pid],
                        false,
                    )?;
                }
                Some(hir::SpreadKind::StringCodePoints) => {
                    let value = self.eval(&elem.expr)?;
                    let source = self.expect_s(value)?;
                    self.call_rt(
                        self.ml.rt.array_spread_string,
                        &[self.ctx_v, handle, source, pid],
                        false,
                    )?;
                }
                Some(other) => {
                    return Err(internal(format!("unknown SpreadKind {other:?}")));
                }
            }
            self.emit_trap_site(site, TrapOperand::Pending)?;
        }
        Ok(RV::S(handle))
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
            let index = u32::try_from(i)
                .map_err(|_| internal("FixedArray literal index does not fit in u32"))?;
            let offset = checked_layout_mul(index, stride, "FixedArray literal offset")?;
            let offset = i32::try_from(offset)
                .map_err(|_| internal("FixedArray literal offset does not fit in i32"))?;
            self.store_val(elem, dest, offset, rv)?;
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
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "new expression", |sites| {
                    self.eval_new(class.0, args, &e.pos, sites, Some(dest))?;
                    Ok(())
                })
            }
            K::DescriptorLit { .. } => {
                Err(internal("descriptor reference cannot build into aggregate storage"))
            }
            K::Call { callee, args } => {
                let sites = e.trap_sites(self.ml.hir);
                lower_trap_sites(&sites, "call", |sites| {
                    let rv =
                        self.eval_call(callee, args, &e.ty, &e.pos, sites, Some(dest))?;
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
                })
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

    fn eval_template(
        &mut self,
        parts: &[hir::TplPart],
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        let mut acc: Option<Value> = None;
        for part in parts {
            let h = match part {
                hir::TplPart::Text(t) => {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        internal("template text has no HIR allocation site"),
                    )?;
                    self.string_literal(t.as_bytes(), site)?
                }
                hir::TplPart::Expr(e) => {
                    let rv = self.eval(e)?;
                    let site = if e.ty == Type::Str {
                        None
                    } else {
                        Some(sites.take_required(
                            |site| matches!(site, hir::TrapSite::Allocation { .. }),
                            internal("template formatting has no HIR allocation site"),
                        )?)
                    };
                    self.format_value(rv, &e.ty, site)?
                }
                other => return Err(internal(format!("template part {other:?}"))),
            };
            acc = Some(match acc {
                None => h,
                Some(prev) => {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        internal("template concat has no HIR allocation site"),
                    )?;
                    let hir::TrapSite::Allocation { pos } = site else {
                        return Err(internal("template concat has a non-allocation HIR site"));
                    };
                    let pid = self.pos_id(pos);
                    let pos_v = self.iconst(types::I32, pid);
                    let res = self.call_rt(
                        self.ml.rt.str_concat,
                        &[self.ctx_v, prev, h, pos_v],
                        false,
                    )?;
                    self.emit_trap_site(site, TrapOperand::Pending)?;
                    res.ok_or_else(|| internal("concat result"))?
                }
            });
        }
        let result = match acc {
            Some(h) => h,
            None => {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Allocation { .. }),
                    internal("empty template has no HIR allocation site"),
                )?;
                self.string_literal(b"", site)?
            }
        };
        Ok(RV::S(result))
    }

    /// Q14 formatting of one interpolated value into a string handle.
    fn format_value(
        &mut self,
        rv: RV,
        ty: &Type,
        site: Option<&hir::TrapSite>,
    ) -> Result<Value, String> {
        let v = self.expect_s(rv)?;
        if *ty == Type::Str {
            if site.is_some() {
                return Err(internal("string interpolation has an allocation site"));
            }
            return Ok(v);
        }
        let site = site.ok_or_else(|| internal("formatting has no HIR allocation site"))?;
        let hir::TrapSite::Allocation { pos } = site else {
            return Err(internal("formatting has a non-allocation HIR site"));
        };
        let pid = self.pos_id(pos);
        let pos_v = self.iconst(types::I32, pid);
        if let Type::StringAlias(id) = ty {
            let table = self.ml.string_alias_table_data(*id)?;
            let gv = self.ml.module.declare_data_in_func(table, self.b.func);
            let base = self.b.ins().symbol_value(types::I64, gv);
            let index = self.b.ins().uextend(types::I64, v);
            let offset = self.b.ins().ishl_imm(index, 4);
            let entry = self.b.ins().iadd(base, offset);
            let data = self.b.ins().load(types::I64, flags(), entry, 0);
            let len = self.b.ins().load(types::I64, flags(), entry, 8);
            let result = self.call_rt(
                self.ml.rt.str_lit,
                &[self.ctx_v, data, len, pos_v],
                false,
            )?;
            self.emit_trap_site(site, TrapOperand::Pending)?;
            return result.ok_or_else(|| internal("string alias formatting result"));
        }
        let (f, arg) = match ty {
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
        let res = self.call_rt(f, &[self.ctx_v, arg, pos_v], false)?;
        self.emit_trap_site(site, TrapOperand::Pending)?;
        res.ok_or_else(|| internal("fmt result"))
    }

    fn eval_lambda(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[hir::Capture],
        pos: &Pos,
    ) -> Result<RV, String> {
        // Environment layout: captured values in capture order,
        // naturally aligned, copied by value at creation (C5).
        let mut cap_info: Vec<(String, Type, u32)> = Vec::new();
        let mut off = 0u32;
        let mut env_align = 1u32;
        for capture in captures {
            let binding = self.lookup(&capture.name)?;
            debug_assert_eq!(binding.ty, capture.ty);
            let (s, a) = self.ml.layouts.size_align(&capture.ty)?;
            off = round_up_layout(off, a, "closure environment layout")?;
            cap_info.push((capture.name.clone(), capture.ty.clone(), off));
            off = checked_layout_add(off, s, "closure environment layout")?;
            env_align = env_align.max(a);
        }
        let env = if captures.is_empty() {
            self.iconst(types::I64, 0)
        } else {
            let size =
                round_up_layout(off.max(1), env_align, "final closure environment layout")?;
            let slot = self.temp_slot(size, env_align);
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
            if g.kind != FrameKind::Generator {
                return Err(internal("yield inside an async frame"));
            }
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

    fn next_suspend_site(&mut self) -> Result<(Value, Value, Block, i64), String> {
        let g = self
            .genc
            .as_mut()
            .ok_or_else(|| internal("suspension outside a coroutine frame"))?;
        let state = (g.next_resume + 1) as i64;
        let resume = *g
            .resume_blocks
            .get(g.next_resume)
            .ok_or_else(|| internal("resume block table exhausted"))?;
        g.next_resume += 1;
        Ok((g.frame, g.out, resume, state))
    }

    fn eval_async_suspend(&mut self, pos: &Pos) -> Result<RV, String> {
        if self.genc.as_ref().map(|g| g.kind) != Some(FrameKind::Async) {
            return Err(internal("Context.suspend outside an async frame"));
        }
        let (frame, _, resume, state) = self.next_suspend_site()?;
        let state_v = self.iconst(types::I32, state);
        self.b.ins().store(flags(), state_v, frame, 0);
        let zero = self.iconst(types::I8, 0);
        self.b.ins().return_(&[zero]);
        self.b.switch_to_block(resume);
        self.reload_epoch_check(frame, pos)?;
        Ok(RV::None)
    }

    fn eval_async_call(
        &mut self,
        function: &str,
        args: &[hir::Expr],
        ret: &Type,
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<RV, String> {
        if self.genc.as_ref().map(|g| g.kind) != Some(FrameKind::Async) {
            return Err(internal("async call outside an async frame"));
        }
        let callee = self.ml.hir_fn(function)?;
        if !callee.is_async {
            return Err(internal(format!("awaited non-async function `{function}`")));
        }
        let child_off = {
            let g = self.genc.as_mut().ok_or_else(|| internal("async context"))?;
            let off = *g
                .child_offsets
                .get(g.next_child)
                .ok_or_else(|| internal("async child-frame offset table exhausted"))?;
            g.next_child += 1;
            off
        };
        let (parent, _, resume_block, state) = self.next_suspend_site()?;

        let mut argv = vec![self.ctx_v];
        self.push_args(&mut argv, &callee.params, args)?;
        let checked = sites
            .take(|site| matches!(site, hir::TrapSite::Call { .. }))
            .is_some();
        let created = self.call_script(&FnKey::Free(function.to_string()), &argv, checked)?;
        let child = *created
            .first()
            .ok_or_else(|| internal("async creator result"))?;
        self.b
            .ins()
            .store(flags(), child, parent, child_off as i32);

        let (size, align) = self.ml.layouts.size_align(ret)?;
        let out_slot = if *ret == Type::Void {
            None
        } else {
            Some(self.b.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size.max(1),
                align_shift(align.max(1)),
            )))
        };
        let attempt = self.b.create_block();
        self.b.append_block_param(attempt, types::I64);
        let suspended = self.b.create_block();
        let completed = self.b.create_block();
        self.b.ins().jump(attempt, &[BlockArg::Value(child)]);

        self.b.switch_to_block(resume_block);
        self.reload_epoch_check(parent, pos)?;
        let resumed_child = self
            .b
            .ins()
            .load(types::I64, flags(), parent, child_off as i32);
        self.b
            .ins()
            .jump(attempt, &[BlockArg::Value(resumed_child)]);

        self.b.switch_to_block(attempt);
        let child = self.b.block_params(attempt)[0];
        let out = match out_slot {
            None => self.iconst(types::I64, 0),
            Some(slot) => self.b.ins().stack_addr(types::I64, slot, 0),
        };
        let results = self.call_script(
            &FnKey::Resume(function.to_string()),
            &[self.ctx_v, child, out],
            false,
        )?;
        self.trap_check();
        let done = *results
            .first()
            .ok_or_else(|| internal("async resume result"))?;
        self.b.ins().brif(done, completed, &[], suspended, &[]);

        self.b.switch_to_block(suspended);
        let state_v = self.iconst(types::I32, state);
        self.b.ins().store(flags(), state_v, parent, 0);
        let zero = self.iconst(types::I8, 0);
        self.b.ins().return_(&[zero]);

        self.b.switch_to_block(completed);
        if *ret == Type::Void {
            Ok(RV::None)
        } else {
            self.load_val(ret, out, 0)
        }
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
                        self.end_assoc_iters()?;
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
            hir::Stmt::ForOf {
                name,
                ty,
                subject,
                kind,
                body,
                pos,
            } => self.lower_for_of(name, ty, subject, *kind, body, pos),
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

    fn lower_for_of(
        &mut self,
        name: &str,
        ty: &Type,
        subject: &hir::Expr,
        kind: hir::ForOfKind,
        body: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), String> {
        use hir::ForOfKind as K;

        self.scope_push();
        let subject_rv = self.eval(subject)?;
        let binding = self.declare_loop_local(name, ty)?;
        let index_var = self.b.declare_var(types::I64);
        let zero = self.iconst(types::I64, 0);
        self.b.def_var(index_var, zero);

        let mut assoc_handle = None;
        let (subject_scalar, subject_addr, bound) = match kind {
            K::ArrayValues | K::ArrayKeys => {
                let handle = self.expect_s(subject_rv)?;
                let n = self
                    .call_rt(self.ml.rt.array_len, &[self.ctx_v, handle], false)?
                    .ok_or_else(|| internal("array for-of length"))?;
                let n = self.b.ins().uextend(types::I64, n);
                (Some(handle), None, n)
            }
            K::FixedArrayValues => {
                let addr = self.expect_a(subject_rv)?;
                let Type::FixedArray(_, n) = &subject.ty else {
                    return Err(internal("fixed-array for-of subject type"));
                };
                let n = self.iconst(types::I64, i64::from(*n));
                (None, Some(addr), n)
            }
            K::MapKeys | K::MapValues | K::SetValues => {
                let handle = self.expect_s(subject_rv)?;
                let pos_id = self.pos_id(pos);
                let pid = self.iconst(types::I32, pos_id);
                let n = self
                    .call_rt(
                        self.ml.rt.assoc_iter_begin,
                        &[self.ctx_v, handle, pid],
                        true,
                    )?
                    .ok_or_else(|| internal("associative for-of bound"))?;
                assoc_handle = Some(handle);
                self.assoc_iters.push(handle);
                (Some(handle), None, n)
            }
            K::StringCodePoints => {
                let handle = self.expect_s(subject_rv)?;
                let n = self
                    .call_rt(self.ml.rt.str_len, &[self.ctx_v, handle], false)?
                    .ok_or_else(|| internal("string for-of byte length"))?;
                let n = self.b.ins().uextend(types::I64, n);
                (Some(handle), None, n)
            }
            other => return Err(internal(format!("unknown ForOfKind {other:?}"))),
        };

        let hdr = self.b.create_block();
        let visit = self.b.create_block();
        let body_blk = self.b.create_block();
        let step_blk = self.b.create_block();
        let exit = self.b.create_block();
        self.b.ins().jump(hdr, &[]);
        self.b.switch_to_block(hdr);
        let index = self.b.use_var(index_var);
        let below_snapshot = self
            .b
            .ins()
            .icmp(IntCC::UnsignedLessThan, index, bound);
        let condition = if matches!(kind, K::ArrayValues | K::ArrayKeys) {
            let handle = subject_scalar.ok_or_else(|| internal("array for-of handle"))?;
            let current = self
                .call_rt(self.ml.rt.array_len, &[self.ctx_v, handle], false)?
                .ok_or_else(|| internal("array for-of current length"))?;
            let current = self.b.ins().uextend(types::I64, current);
            let below_current =
                self.b
                    .ins()
                    .icmp(IntCC::UnsignedLessThan, index, current);
            self.b.ins().band(below_snapshot, below_current)
        } else {
            below_snapshot
        };
        self.b.ins().brif(condition, visit, &[], exit, &[]);

        self.b.switch_to_block(visit);
        let mut next_index = None;
        let value = match kind {
            K::ArrayKeys => {
                let narrowed = self.b.ins().ireduce(types::I32, index);
                Some(RV::S(narrowed))
            }
            K::ArrayValues => {
                let handle = subject_scalar.ok_or_else(|| internal("array for-of handle"))?;
                let data = self
                    .call_rt(self.ml.rt.array_data, &[self.ctx_v, handle], false)?
                    .ok_or_else(|| internal("array for-of data"))?;
                let stride = self.ml.layouts.stride(ty)?;
                let offset = self.b.ins().imul_imm(index, i64::from(stride));
                let addr = self.b.ins().iadd(data, offset);
                Some(self.load_val(ty, addr, 0)?)
            }
            K::FixedArrayValues => {
                let base = subject_addr.ok_or_else(|| internal("fixed-array for-of base"))?;
                let stride = self.ml.layouts.stride(ty)?;
                let offset = self.b.ins().imul_imm(index, i64::from(stride));
                let addr = self.b.ins().iadd(base, offset);
                Some(self.load_val(ty, addr, 0)?)
            }
            K::MapKeys | K::MapValues | K::SetValues => {
                let handle = subject_scalar.ok_or_else(|| internal("assoc for-of handle"))?;
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let slot = self.temp_slot(size.max(1), align.max(1));
                let select_value = self.iconst(types::I32, i64::from(kind == K::MapValues));
                let pos_id = self.pos_id(pos);
                let pid = self.iconst(types::I32, pos_id);
                let active = self
                    .call_rt(
                        self.ml.rt.assoc_iter_copy,
                        &[self.ctx_v, handle, index, select_value, slot, pid],
                        true,
                    )?
                    .ok_or_else(|| internal("assoc for-of active flag"))?;
                let loaded = self.load_val(ty, slot, 0)?;
                let active = self.b.ins().icmp_imm(IntCC::NotEqual, active, 0);
                self.b.ins().brif(active, body_blk, &[], step_blk, &[]);
                Some(loaded)
            }
            K::StringCodePoints => {
                let handle = subject_scalar.ok_or_else(|| internal("string for-of handle"))?;
                let index32 = self.b.ins().ireduce(types::I32, index);
                let next_slot = self.temp_slot(4, 4);
                let pos_id = self.pos_id(pos);
                let pid = self.iconst(types::I32, pos_id);
                let value = self
                    .call_rt(
                        self.ml.rt.str_iter_code_point,
                        &[self.ctx_v, handle, index32, next_slot, pid],
                        true,
                    )?
                    .ok_or_else(|| internal("string for-of code point"))?;
                let next32 = self.b.ins().load(types::I32, flags(), next_slot, 0);
                next_index = Some(self.b.ins().uextend(types::I64, next32));
                Some(RV::S(value))
            }
            other => return Err(internal(format!("unknown ForOfKind {other:?}"))),
        };
        if !matches!(kind, K::MapKeys | K::MapValues | K::SetValues) {
            self.b.ins().jump(body_blk, &[]);
        }

        self.b.switch_to_block(body_blk);
        let value = value.ok_or_else(|| internal("for-of visit value"))?;
        let place = self.place_of_binding(&binding)?;
        self.write_place(place, ty, value)?;
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
        let next = next_index.unwrap_or_else(|| self.b.ins().iadd_imm(index, 1));
        self.b.def_var(index_var, next);
        self.b.ins().jump(hdr, &[]);

        self.b.switch_to_block(exit);
        if let Some(handle) = assoc_handle {
            self.call_rt(
                self.ml.rt.assoc_iter_end,
                &[self.ctx_v, handle],
                false,
            )?;
            self.assoc_iters.pop();
        }
        self.scope_pop();
        Ok(())
    }

    fn emit_shadow_pop(&mut self) -> Result<(), String> {
        if self.shadow_base.is_some() {
            self.call_rt(self.ml.rt.shadow_pop, &[self.ctx_v], false)?;
        }
        Ok(())
    }

    fn end_assoc_iters(&mut self) -> Result<(), String> {
        let handles: Vec<Value> = self.assoc_iters.iter().rev().copied().collect();
        for handle in handles {
            self.call_rt(
                self.ml.rt.assoc_iter_end,
                &[self.ctx_v, handle],
                false,
            )?;
        }
        Ok(())
    }

    fn emit_return(&mut self, value: Option<(RV, Type)>) -> Result<(), String> {
        self.end_assoc_iters()?;
        if self.is_resume {
            // Coroutine completion: an async frame first writes its
            // fulfilled value through `out`; both frame kinds then store
            // the terminal state and return done = 1.
            let (frame, out, kind) = self
                .genc
                .as_ref()
                .map(|g| (g.frame, g.out, g.kind))
                .ok_or_else(|| internal("resume without coroutine context"))?;
            if kind == FrameKind::Async {
                match value {
                    Some((rv, ty)) => self.store_val(&ty, out, 0, rv)?,
                    None if self.ret_ty != Type::Void => {
                        return Err(internal("missing async fulfilled value"));
                    }
                    None => {}
                }
            }
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
                let (frame, kind) = self
                    .genc
                    .as_ref()
                    .map(|g| (g.frame, g.kind))
                    .ok_or_else(|| internal("resume without coroutine context"))?;
                if kind == FrameKind::Generator {
                    // A trapped explicitly-driven generator stays done.
                    let done_state = self.iconst(types::I32, GEN_DONE);
                    self.b.ins().store(flags(), done_state, frame, 0);
                    vec![self.iconst(types::I8, 1)]
                } else {
                    // The runtime retains a trapping async root. Keeping
                    // its suspension state makes a cleared stale-frame trap
                    // recur on the next explicit step (§8.2/Q34).
                    vec![self.iconst(types::I8, 0)]
                }
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
        n = checked_layout_add(n, managed_words(&ml.layouts, &p.ty)?, "shadow word count")?;
    }
    for t in lets {
        n = checked_layout_add(n, managed_words(&ml.layouts, t)?, "shadow word count")?;
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
    let bytes = checked_layout_mul(slots, 8, "shadow frame byte size")?;
    let base = body.temp_slot(bytes, 8);
    body.zero_bytes(base, bytes, 8);
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
            assoc_iters: Vec::new(),
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
    ensure_explicit_frame_supported(&cctx.func, &format!("{key:?}"))?;
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
    let name = format!("subscript_lambda{}", ml.lambda_count);
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
            assoc_iters: Vec::new(),
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
                },
            );
        }
        body.lower_stmts(stmts)?;
        body.finish()?;
    }
    ensure_explicit_frame_supported(&cctx.func, &name)?;
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
    let name = format!("subscript_assoc_bridge{}", ml.lambda_count);
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
    ensure_explicit_frame_supported(&cctx.func, &name)?;
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define {name}: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(id)
}

/// Defines the fixed runtime→script bridge used by `Map.groupBy`.
/// The runtime supplies a copied element and an output slot for the
/// callback-produced key, so neither side exposes live container
/// storage across the callback.
fn define_group_bridge<M: Module>(
    ml: &mut ModLower<M>,
    elem: &Type,
    key: &Type,
) -> Result<cranelift_module::FuncId, String> {
    let Repr::Scalar(key_repr) = ml.layouts.repr(key)? else {
        return Err(internal(format!("Map.groupBy key representation {key:?}")));
    };
    let mut bridge_sig = Signature::new(ml.call_conv);
    for _ in 0..5 {
        bridge_sig.params.push(AbiParam::new(types::I64));
    }
    let name = format!("subscript_group_bridge{}", ml.lambda_count);
    ml.lambda_count += 1;
    let id = ml
        .module
        .declare_function(&name, cranelift_module::Linkage::Local, &bridge_sig)
        .map_err(|e| internal(format!("declare {name}: {e}")))?;
    let script_sig = ml.make_sig(std::slice::from_ref(elem), key, true, false)?;
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
        let element = fixed[3];
        let key_out = fixed[4];
        let mut argv = vec![ctx, env];
        match ml.layouts.repr(elem)? {
            Repr::None => {}
            Repr::Scalar(repr) => {
                argv.push(b.ins().load(repr, flags(), element, 0));
            }
            Repr::Pair => {
                argv.push(b.ins().load(types::I64, flags(), element, 0));
                argv.push(b.ins().load(types::I64, flags(), element, 8));
            }
            Repr::Agg { size, align } => {
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
                    element,
                    u64::from(size),
                    copy_align as u8,
                    copy_align as u8,
                    true,
                    MemFlags::new(),
                );
                argv.push(copy);
            }
        }
        let sigref = b.import_signature(script_sig);
        let inst = b.ins().call_indirect(sigref, code, &argv);
        let result = b
            .inst_results(inst)
            .first()
            .copied()
            .ok_or_else(|| internal("Map.groupBy callback result"))?;
        b.ins().store(flags(), result, key_out, 0);
        debug_assert_eq!(b.func.dfg.value_type(result), key_repr);
        b.ins().return_(&[]);
        b.seal_all_blocks();
        b.finalize();
    }
    ensure_explicit_frame_supported(&cctx.func, &name)?;
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
    if f.is_generator || f.is_async {
        return Err(internal("coroutines are not function values"));
    }
    let params_ty: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
    let sig = ml.make_sig(&params_ty, &f.ret, true, false)?;
    let sym = format!("subscript_wrap_{}", ml.fns.len());
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
    ensure_explicit_frame_supported(&cctx.func, &sym)?;
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define {sym}: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(id)
}

/// Frame layout shared by generators and async functions: parameter/local
/// storage followed by one child-frame pointer for each direct awaited call.
fn generator_frame<M: Module>(
    ml: &ModLower<M>,
    f: &hir::Function,
) -> Result<(Vec<u32>, Vec<u32>, Vec<u32>, u32), String> {
    let mut off = GEN_PAYLOAD_OFF;
    let mut param_offsets = Vec::new();
    for p in &f.params {
        let (s, a) = ml.layouts.size_align(&p.ty)?;
        off = round_up_layout(off, a.max(1), "generator parameter layout")?;
        param_offsets.push(off);
        off = checked_layout_add(off, s.max(1), "generator parameter layout")?;
    }
    let mut lets: Vec<&Type> = Vec::new();
    walk_lets(&f.body, &mut lets);
    let mut let_offsets = Vec::new();
    for t in lets {
        let (s, a) = ml.layouts.size_align(t)?;
        off = round_up_layout(off, a.max(1), "generator local layout")?;
        let_offsets.push(off);
        off = checked_layout_add(off, s.max(1), "generator local layout")?;
    }
    let mut child_offsets = Vec::new();
    for _ in 0..count_async_calls(&f.body) {
        off = round_up_layout(off, 8, "async child-frame layout")?;
        child_offsets.push(off);
        off = checked_layout_add(off, 8, "async child-frame layout")?;
    }
    let size = round_up_layout(off, 8, "final generator frame layout")?;
    Ok((param_offsets, let_offsets, child_offsets, size))
}

/// Defines the creator and resume functions of a `function*` (C8).
pub(crate) fn define_generator<M: Module>(
    ml: &mut ModLower<M>,
    f: &hir::Function,
) -> Result<(), String> {
    let (param_offsets, let_offsets, child_offsets, frame_size) = generator_frame(ml, f)?;
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
                assoc_iters: Vec::new(),
                unwind: None,
                shadow_base: None,
                next_shadow: 0,
                genc: None,
                term: false,
            };
            let size_v = body.iconst(types::I64, i64::from(frame_size));
            let class_v = body.iconst(types::I32, i64::from(rtc::CLASS_GENERATOR));
            let sites = f.trap_sites();
            let frame = lower_trap_sites(&sites, "generator frame creation", |sites| {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Allocation { .. }),
                    internal("generator has no HIR allocation site"),
                )?;
                let hir::TrapSite::Allocation { pos } = site else {
                    return Err(internal("generator has a non-allocation HIR site"));
                };
                let pid = body.pos_id(pos);
                let pos_v = body.iconst(types::I32, pid);
                let res = body.call_rt(
                    body.ml.rt.alloc,
                    &[body.ctx_v, size_v, class_v, pos_v],
                    false,
                )?;
                body.emit_trap_site(site, TrapOperand::Pending)?;
                res.ok_or_else(|| internal("frame alloc result"))
            })?;
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
        ensure_explicit_frame_supported(&cctx.func, "generator creator")?;
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
                assoc_iters: Vec::new(),
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
                    child_offsets,
                    next_child: 0,
                    kind: FrameKind::Generator,
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
                    },
                );
            }
            body.lower_stmts(&f.body)?;
            body.finish()?;
        }
        ensure_explicit_frame_supported(&cctx.func, "generator resume")?;
        ml.module
            .define_function(resume_id, &mut cctx)
            .map_err(|e| internal(format!("define resume: {e}")))?;
        ml.module.clear_context(&mut cctx);
    }
    Ok(())
}

/// Defines the creator and resume functions of a Q34 async declaration.
/// The frame/state ABI is deliberately the generator ABI: allocation,
/// parameter/local storage, reload epoch, and CPS dispatch are shared; only
/// suspension and completion behavior differ.
pub(crate) fn define_async<M: Module>(
    ml: &mut ModLower<M>,
    f: &hir::Function,
) -> Result<(), String> {
    let (param_offsets, let_offsets, child_offsets, frame_size) = generator_frame(ml, f)?;
    let creator_id = ml.func_id(&FnKey::Free(f.name.clone()))?;
    let resume_id = ml.func_id(&FnKey::Resume(f.name.clone()))?;

    // Creator: allocate and initialize, but do not execute. An await site
    // or exported host wrapper performs the first resume immediately.
    {
        let params_ty: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
        let sig = ml.make_sig(
            &params_ty,
            &Type::Generator(Box::new(Type::Void)),
            false,
            false,
        )?;
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
                ret_ty: Type::Generator(Box::new(Type::Void)),
                is_resume: false,
                scopes: vec![Vec::new()],
                loops: Vec::new(),
                assoc_iters: Vec::new(),
                unwind: None,
                shadow_base: None,
                next_shadow: 0,
                genc: None,
                term: false,
            };
            let size_v = body.iconst(types::I64, i64::from(frame_size));
            let class_v = body.iconst(types::I32, i64::from(rtc::CLASS_GENERATOR));
            let sites = f.trap_sites();
            let frame = lower_trap_sites(&sites, "async frame creation", |sites| {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Allocation { .. }),
                    internal("async function has no HIR allocation site"),
                )?;
                let hir::TrapSite::Allocation { pos } = site else {
                    return Err(internal("async function has a non-allocation HIR site"));
                };
                let pid = body.pos_id(pos);
                let pos_v = body.iconst(types::I32, pid);
                let result = body.call_rt(
                    body.ml.rt.alloc,
                    &[body.ctx_v, size_v, class_v, pos_v],
                    false,
                )?;
                body.emit_trap_site(site, TrapOperand::Pending)?;
                result.ok_or_else(|| internal("async frame alloc result"))
            })?;
            let resume_ref = body
                .ml
                .module
                .declare_func_in_func(resume_id, body.b.func);
            let resume_addr = body.b.ins().func_addr(types::I64, resume_ref);
            body.b
                .ins()
                .store(flags(), resume_addr, frame, GEN_RESUME_OFF);
            if body.ml.opts.reload {
                let epoch_off = ctx_off(rtc::Context::reload_epoch_offset())?;
                let epoch = body
                    .b
                    .ins()
                    .load(types::I32, flags(), body.ctx_v, epoch_off);
                body.b.ins().store(flags(), epoch, frame, GEN_EPOCH_OFF);
            }
            let mut value_index = 0usize;
            for (param, off) in f.params.iter().zip(&param_offsets) {
                let value = match body.ml.layouts.repr(&param.ty)? {
                    Repr::None => continue,
                    Repr::Pair => {
                        let value = RV::P(
                            pro.param_vals[value_index],
                            pro.param_vals[value_index + 1],
                        );
                        value_index += 2;
                        value
                    }
                    Repr::Agg { .. } => {
                        let value = RV::A(pro.param_vals[value_index]);
                        value_index += 1;
                        value
                    }
                    Repr::Scalar(_) => {
                        let value = RV::S(pro.param_vals[value_index]);
                        value_index += 1;
                        value
                    }
                };
                body.store_val(&param.ty, frame, *off as i32, value)?;
            }
            body.b.ins().return_(&[frame]);
            body.term = true;
            body.finish()?;
        }
        ensure_explicit_frame_supported(&cctx.func, "async creator")?;
        ml.module
            .define_function(creator_id, &mut cctx)
            .map_err(|error| internal(format!("define async creator: {error}")))?;
        ml.module.clear_context(&mut cctx);
    }

    // Resume state machine.
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
            let values = b.block_params(entry).to_vec();
            let (ctx_v, frame, out) = (values[0], values[1], values[2]);
            let suspension_count = count_yields(&f.body);
            let start = b.create_block();
            let done_block = b.create_block();
            let resume_blocks: Vec<Block> = (0..suspension_count)
                .map(|_| b.create_block())
                .collect();
            let state = b.ins().load(types::I32, flags(), frame, 0);
            for (index, &block) in std::iter::once(&start).chain(&resume_blocks).enumerate() {
                let equal = b.ins().icmp_imm(IntCC::Equal, state, index as i64);
                let next = b.create_block();
                b.ins().brif(equal, block, &[], next, &[]);
                b.switch_to_block(next);
            }
            b.ins().jump(done_block, &[]);
            b.switch_to_block(done_block);
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
                ret_ty: f.ret.clone(),
                is_resume: true,
                scopes: vec![Vec::new()],
                loops: Vec::new(),
                assoc_iters: Vec::new(),
                unwind: None,
                shadow_base: None,
                next_shadow: 0,
                genc: Some(GenCtx {
                    frame,
                    out,
                    yield_ty: f.ret.clone(),
                    resume_blocks,
                    next_resume: 0,
                    let_offsets,
                    next_let: 0,
                    child_offsets,
                    next_child: 0,
                    kind: FrameKind::Async,
                }),
                term: false,
            };
            for (param, off) in f.params.iter().zip(&param_offsets) {
                body.bind(
                    &param.name,
                    Binding {
                        ty: param.ty.clone(),
                        storage: Storage::Frame(*off),
                    },
                );
            }
            body.lower_stmts(&f.body)?;
            body.finish()?;
        }
        ensure_explicit_frame_supported(&cctx.func, "async resume")?;
        ml.module
            .define_function(resume_id, &mut cctx)
            .map_err(|error| internal(format!("define async resume: {error:?}")))?;
        ml.module.clear_context(&mut cctx);
    }
    Ok(())
}

/// Defines the zero-argument void host wrapper for an exported async
/// function: create its frame, then let the runtime perform the initial
/// resume and pending-root registration.
pub(crate) fn define_async_export<M: Module>(
    ml: &mut ModLower<M>,
    f: &hir::Function,
) -> Result<(), String> {
    if !f.params.is_empty() || f.ret != Type::Void {
        return Err(internal(format!(
            "exported async function `{}` is not zero-argument Promise<void>",
            f.name
        )));
    }
    let id = ml.func_id(&FnKey::AsyncExport(f.name.clone()))?;
    let sig = ml.make_sig(&[], &Type::Void, false, false)?;
    let mut cctx = ml.module.make_context();
    cctx.func.signature = sig;
    let mut fbx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let ctx = b.block_params(entry)[0];
        let creator = ml.func_id(&FnKey::Free(f.name.clone()))?;
        let creator_ref = ml.module.declare_func_in_func(creator, b.func);
        let call = b.ins().call(creator_ref, &[ctx]);
        let frame = b.inst_results(call)[0];
        let resume = ml.func_id(&FnKey::Resume(f.name.clone()))?;
        let resume_ref = ml.module.declare_func_in_func(resume, b.func);
        let resume_addr = b.ins().func_addr(types::I64, resume_ref);
        let kick_ref = ml.module.declare_func_in_func(ml.rt.async_kick, b.func);
        b.ins().call(kick_ref, &[ctx, frame, resume_addr]);
        b.ins().return_(&[]);
        b.seal_all_blocks();
        b.finalize();
    }
    ensure_explicit_frame_supported(&cctx.func, "async export wrapper")?;
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|error| internal(format!("define async export: {error}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(())
}

/// Defines the standard AOT-runner helper: after `main`, kick every other
/// exported async function in declaration order. The generic AOT entry then
/// pumps the Context to quiescence.
pub(crate) fn define_async_runner<M: Module>(ml: &mut ModLower<M>) -> Result<(), String> {
    let id = ml.func_id(&FnKey::AsyncRunner)?;
    let sig = ml.make_sig(&[], &Type::Void, false, false)?;
    let async_exports: Vec<String> = ml
        .hir
        .functions
        .iter()
        .filter(|function| function.exported && function.is_async && function.name != "main")
        .map(|function| function.name.clone())
        .collect();
    let mut cctx = ml.module.make_context();
    cctx.func.signature = sig;
    let mut fbx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut cctx.func, &mut fbx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let ctx = b.block_params(entry)[0];
        let done = b.create_block();
        for name in async_exports {
            let export = ml.func_id(&FnKey::AsyncExport(name))?;
            let export_ref = ml.module.declare_func_in_func(export, b.func);
            b.ins().call(export_ref, &[ctx]);
            let trap = b.ins().load(types::I32, flags(), ctx, 0);
            let clear = b.ins().icmp_imm(IntCC::Equal, trap, 0);
            let next = b.create_block();
            b.ins().brif(clear, next, &[], done, &[]);
            b.switch_to_block(next);
        }
        b.ins().jump(done, &[]);
        b.switch_to_block(done);
        b.ins().return_(&[]);
        b.seal_all_blocks();
        b.finalize();
    }
    ensure_explicit_frame_supported(&cctx.func, "async standard-runner helper")?;
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|error| internal(format!("define async runner: {error}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(())
}

/// Defines the synthesized `subscript_init` function: evaluates every
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
            assoc_iters: Vec::new(),
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
    ensure_explicit_frame_supported(&cctx.func, "module initializer")?;
    ml.module
        .define_function(id, &mut cctx)
        .map_err(|e| internal(format!("define init: {e}")))?;
    ml.module.clear_context(&mut cctx);
    Ok(())
}

#[cfg(test)]
mod hfa_tests {
    use super::{
        ensure_explicit_frame_supported, is_pure_hfa_leaves,
    };
    use cranelift_codegen::ir::{
        types, Function, StackSlotData, StackSlotKind,
    };
    use subscript_compiler::types::MAX_FRAME_BYTES;

    #[test]
    fn explicit_frame_guard_pins_the_aarch64_boundary() {
        let mut supported = Function::new();
        supported.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            MAX_FRAME_BYTES,
            0,
        ));
        ensure_explicit_frame_supported(&supported, "boundary")
            .expect("greatest supported aligned frame");

        let mut rejected = Function::new();
        rejected.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            MAX_FRAME_BYTES + 1,
            0,
        ));
        let error = ensure_explicit_frame_supported(&rejected, "boundary")
            .expect_err("next byte rounds the frame to 2^31");
        assert!(error.contains("2147483632"), "{error}");
    }

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
