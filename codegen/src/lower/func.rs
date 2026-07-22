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
    types, Block, BlockArg, InstBuilder, MemFlags, StackSlotData, StackSlotKind, Value,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::Module;
use subscript_compiler::{hir, Pos, Type};
use subscript_runtime::context as rtc;
use subscript_runtime::TrapKind;

use crate::layout::{has_managed_interior, is_managed, is_unsigned, managed_words, Repr};
use crate::lower::{internal, FnKey, ModLower};

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

fn align_shift(align: u32) -> u8 {
    align.max(1).trailing_zeros() as u8
}

fn round_up(v: u32, a: u32) -> u32 {
    (v + a - 1) & !(a - 1)
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

    fn iconst(&mut self, t: types::Type, v: i64) -> Value {
        let v = if t == types::I32 {
            i64::from(v as i32)
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
        let id = self.ml.func_id(key)?;
        let fref = self.ml.module.declare_func_in_func(id, self.b.func);
        let inst = self.b.ins().call(fref, args);
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

    /// Creates the storage for a new local and writes its initial
    /// value. Managed scalars get a shadow slot; aggregates whose
    /// interior holds managed handles (e.g. `FixedArray` of
    /// references, `IterResult<string>`) live *inside* the shadow
    /// frame so the collector's conservative word scan sees every
    /// handle stored in them (M1).
    fn declare_local(&mut self, name: &str, ty: &Type, init: RV) -> Result<(), String> {
        let storage = if self.genc.is_some() {
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
        };
        let binding = Binding {
            ty: ty.clone(),
            storage,
        };
        let place = self.place_of_binding(&binding)?;
        self.write_place(place, ty, init)?;
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
            K::Float(v) => Ok(RV::S(if e.ty == Type::F32 {
                self.b.ins().f32const(*v as f32)
            } else {
                self.b.ins().f64const(*v)
            })),
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
            K::Call { callee, args } => self.eval_call(callee, args, &e.ty, &e.pos),
            K::New { class, args } => self.eval_new(class.0, args, &e.pos),
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
        let (data, ty) = self
            .ml
            .globals
            .get(name)
            .cloned()
            .ok_or_else(|| internal(format!("unknown global `{name}`")))?;
        let gv = self.ml.module.declare_data_in_func(data, self.b.func);
        let addr = self.b.ins().symbol_value(types::I64, gv);
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
        let float = operand_ty.is_float();
        let unsigned = is_unsigned(&operand_ty);
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
        let out = match (&from, &to) {
            (a, b) if a == b => v,
            // int -> int
            (Type::I32 | Type::U32, Type::I32 | Type::U32) => v,
            (Type::I64 | Type::U64, Type::I64 | Type::U64) => v,
            (Type::I32, Type::I64 | Type::U64) => self.b.ins().sextend(types::I64, v),
            (Type::U32, Type::I64 | Type::U64) => self.b.ins().uextend(types::I64, v),
            (Type::I64 | Type::U64, Type::I32 | Type::U32) => {
                self.b.ins().ireduce(types::I32, v)
            }
            // int -> float (by source signedness)
            (Type::I32 | Type::I64, Type::F32) => self.b.ins().fcvt_from_sint(types::F32, v),
            (Type::I32 | Type::I64, Type::F64) => self.b.ins().fcvt_from_sint(types::F64, v),
            (Type::U32 | Type::U64, Type::F32) => self.b.ins().fcvt_from_uint(types::F32, v),
            (Type::U32 | Type::U64, Type::F64) => self.b.ins().fcvt_from_uint(types::F64, v),
            // float -> int (saturating, by target signedness; C leaves
            // out-of-range conversion undefined — saturation is this
            // dev tier's defined choice, and it cannot hardware-trap)
            (Type::F32 | Type::F64, Type::I32) => self.b.ins().fcvt_to_sint_sat(types::I32, v),
            (Type::F32 | Type::F64, Type::I64) => self.b.ins().fcvt_to_sint_sat(types::I64, v),
            (Type::F32 | Type::F64, Type::U32) => self.b.ins().fcvt_to_uint_sat(types::I32, v),
            (Type::F32 | Type::F64, Type::U64) => self.b.ins().fcvt_to_uint_sat(types::I64, v),
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
                // Unsigned compare rejects negatives and >= n at once.
                let ok = self
                    .b
                    .ins()
                    .icmp_imm(IntCC::UnsignedLessThan, idx, i64::from(*n));
                self.guard(ok, TrapKind::IndexOutOfBounds, pos)?;
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

    fn eval_call(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
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
                let sret = match self.ml.layouts.repr(&f.ret)? {
                    Repr::Agg { size, align } => {
                        let s = self.temp_slot(size, align);
                        argv.push(s);
                        Some(s)
                    }
                    _ => None,
                };
                self.push_args(&mut argv, &f.params, args)?;
                let ret = f.ret.clone();
                let res = self.call_script(&FnKey::Free(name.clone()), &argv)?;
                self.shape_results(&ret, &res, sret)
            }
            hir::Callee::Ambient(a) => self.eval_ambient(*a, args, pos),
            hir::Callee::Value(v) => {
                let ft = match &v.ty {
                    Type::Func(ft) => (**ft).clone(),
                    other => return Err(internal(format!("call of {other:?}"))),
                };
                let rv = self.eval(v)?;
                let (code, env) = self.expect_p(rv)?;
                let mut argv = vec![self.ctx_v, env];
                let sret = match self.ml.layouts.repr(&ft.ret)? {
                    Repr::Agg { size, align } => {
                        let s = self.temp_slot(size, align);
                        argv.push(s);
                        Some(s)
                    }
                    _ => None,
                };
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
                self.eval_method(recv, name, args, ret_ty, pos)
            }
            other => Err(internal(format!("callee {other:?}"))),
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
                let sret = match self.ml.layouts.repr(&m.ret)? {
                    Repr::Agg { size, align } => {
                        let s = self.temp_slot(size, align);
                        argv.push(s);
                        Some(s)
                    }
                    _ => None,
                };
                argv.push(this);
                self.push_args(&mut argv, &m.params, args)?;
                let ret = m.ret.clone();
                let res = self.call_script(&FnKey::Method(cid.0, name.to_string()), &argv)?;
                self.shape_results(&ret, &res, sret)
            }
            other => Err(internal(format!("method on {other:?}"))),
        }
    }

    fn eval_new(&mut self, cid: usize, args: &[hir::Expr], pos: &Pos) -> Result<RV, String> {
        let hirm = self.ml.hir;
        let class = hirm
            .classes
            .get(cid)
            .ok_or_else(|| internal("class id out of range"))?;
        let layout = self.ml.layouts.class(cid)?.clone();
        let this = if layout.is_value {
            let slot = self.temp_slot(layout.size, layout.align);
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
            Type::FixedArray(elem, _) => {
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let stride = self.ml.layouts.stride(elem)?;
                let slot = self.temp_slot(size, align);
                for (i, e) in elems.iter().enumerate() {
                    let rv = self.eval(e)?;
                    self.store_val(elem, slot, (i as u32 * stride) as i32, rv)?;
                }
                Ok(RV::A(slot))
            }
            other => Err(internal(format!("array literal of {other:?}"))),
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
            Type::I32 | Type::Enum(_) => (self.ml.rt.fmt_i32, v),
            Type::U32 => (self.ml.rt.fmt_u32, v),
            Type::I64 => (self.ml.rt.fmt_i64, v),
            Type::U64 => (self.ml.rt.fmt_u64, v),
            Type::F32 => (self.ml.rt.fmt_f32, v),
            Type::F64 => (self.ml.rt.fmt_f64, v),
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
                let rv = self.eval(init)?;
                self.declare_local(name, ty, rv)
            }
            hir::Stmt::Expr(e) => {
                self.eval(e)?;
                Ok(())
            }
            hir::Stmt::Return { value, .. } => {
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
    ml.fns.insert(key, id);

    let target = ml.func_id(&FnKey::Free(name.to_string()))?;
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
        let fref = ml.module.declare_func_in_func(target, b.func);
        let inst = b.ins().call(fref, &argv);
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
