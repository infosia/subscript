//! HIR-to-CLIF lowering, tier-neutral (`specs/blocks/compiler.md` §1).
//!
//! The lowering targets the [`cranelift_module::Module`] trait, never
//! a concrete backend: the dev JIT instantiates it with `JITModule`
//! and the ship tier instantiates it with `ObjectModule`. Nothing in
//! here embeds runtime addresses or other JIT-only shortcuts — string
//! literals are module data, globals are module data, and the runtime
//! is reached through imported `extern "C"` symbols.
//!
//! [`LowerOptions`] is the lowering's only parameter. Both tiers use
//! its default on the differential-gate path; the one non-default form
//! is the dev tier's hot-reload mode, which exists solely so a live
//! program's bodies can be replaced.
//!
//! # Calling convention of generated functions
//!
//! Every script function takes the Context pointer first. Optional
//! parameters follow in a fixed order:
//! `ctx, [env], [sret], [this], params...`
//! - `env`: lambda/function-value environment pointer (only functions
//!   callable through a function value).
//! - `sret`: caller-allocated result storage when the return type is
//!   a by-value aggregate.
//! - `this`: receiver (constructors, methods).
//! Value-class parameters are passed as pointers to caller-owned
//! copies (C2 copy-on-pass); function-typed parameters are
//! `(code, env)` pairs.
//!
//! # Traps
//!
//! Runtime faults set the Context trap flag (offset 0). After every
//! call that can fault — and after each emitted check via
//! `sub_rt_trap` — generated code branches to a per-function unwind
//! block that pops its shadow frame and returns a zeroed value, so
//! the whole stack returns to the driver without signals or
//! unwinding. Each trap site carries an index into the position
//! table returned in [`Lowered::positions`].

mod func;

use std::collections::HashMap;

use cranelift_codegen::ir::{types, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::Configurable;
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use subscript_compiler::{hir, Pos, Type};

use crate::layout::{Layouts, Repr};

pub(crate) use func::define_function;

/// Identity of a lowered function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FnKey {
    /// Free function by HIR name (generator creators included).
    Free(String),
    /// Generator resume function, by generator name.
    Resume(String),
    /// Constructor of class `usize`.
    Ctor(usize),
    /// Method `String` of class `usize`.
    Method(usize, String),
    /// Env-taking wrapper for a named function used as a value.
    Wrapper(String),
    /// The synthesized global-initializer entry.
    Init,
}

/// Imported runtime entry points.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub(crate) struct RtFns {
    pub print: FuncId,
    pub collect: FuncId,
    pub alloc: FuncId,
    pub delete: FuncId,
    pub trap: FuncId,
    pub root_add: FuncId,
    pub shadow_push: FuncId,
    pub shadow_pop: FuncId,
    pub str_lit: FuncId,
    pub str_len: FuncId,
    pub str_concat: FuncId,
    pub str_eq: FuncId,
    pub fmt_i32: FuncId,
    pub fmt_u32: FuncId,
    pub fmt_i64: FuncId,
    pub fmt_u64: FuncId,
    pub fmt_f32: FuncId,
    pub fmt_f64: FuncId,
    pub fmt_bool: FuncId,
    /// `f64` to raw IEEE binary16 bits (Q23).
    pub f16_from_f64: FuncId,
    /// Raw IEEE binary16 bits to exact `f64` (Q23).
    pub f16_to_f64: FuncId,
    pub array_new: FuncId,
    pub array_len: FuncId,
    pub array_push: FuncId,
    pub array_pop: FuncId,
    pub array_ptr: FuncId,
    pub str_data: FuncId,
    pub array_data: FuncId,
    pub cb_bind: FuncId,
    pub cb_trampoline: FuncId,
    /// `sub_rt_math_*` imports (stdlib.md §1), indexed by
    /// `hir::MathFn as usize` (the [`hir::MathFn::ALL`] order).
    pub math: [FuncId; hir::MathFn::ALL.len()],
    /// `sub_rt_num_*` imports (stdlib.md §11, Q25/Q26), indexed by
    /// [`hir::NumFn::ALL`] discriminant order.
    pub num: [FuncId; hir::NumFn::ALL.len()],
    /// `sub_rt_date_utc` (stdlib.md §3): 7 `i32` components + pos id → i64.
    pub date_utc: FuncId,
    /// `sub_rt_date_new`: ms + pos id → range-checked ms.
    pub date_new: FuncId,
    /// `sub_rt_date_now`: Context clock → i64 ms.
    pub date_now: FuncId,
    /// `sub_rt_date_get`: (ms, field code) → i32 UTC accessor.
    pub date_get: FuncId,
    /// `sub_rt_date_to_iso`: (ms, pos id) → string handle.
    pub date_to_iso: FuncId,
    /// Checker-generated JSON serializer leaves (stdlib.md §13), indexed
    /// by [`hir::JsonFn::ALL`] discriminant order.
    pub json: [FuncId; hir::JsonFn::ALL.len()],
    /// `sub_rt_str_*` method imports (stdlib.md §8), indexed by
    /// `hir::StrFn as usize` (the [`hir::StrFn::ALL`] order). Each
    /// signature is `(ctx, recv, params…[, pos_id])` per
    /// [`hir::StrFn::params`] / [`hir::StrFn::takes_pos_id`].
    pub str_ops: [FuncId; hir::StrFn::ALL.len()],
    /// `sub_rt_arr_*` method imports (stdlib.md §9), indexed by
    /// `hir::ArrFn as usize` (the [`hir::ArrFn::ALL`] order). Each
    /// signature starts `(ctx, recv, …)`; element values travel by
    /// pointer, callbacks as `(code, env)`, kind tags as `u32`.
    pub arr_ops: [FuncId; hir::ArrFn::ALL.len()],
    /// Q27 `FixedArray<T, N>` callback-family imports. Unsupported
    /// `ArrFn` variants have no fixed-buffer entry.
    pub fixed_arr_ops: [Option<FuncId>; hir::ArrFn::ALL.len()],
    /// `sub_rt_map_*` imports (stdlib.md §10), indexed by
    /// [`hir::MapFn::ALL`] discriminant order.
    pub map_ops: [FuncId; hir::MapFn::ALL.len()],
    /// `sub_rt_set_*` imports (stdlib.md §10), indexed by
    /// [`hir::SetFn::ALL`] discriminant order.
    pub set_ops: [FuncId; hir::SetFn::ALL.len()],
}

/// Parameters of the shared lowering.
///
/// One lowering serves both tiers (`specs/blocks/compiler.md` §1/§8.1);
/// the single parameter below selects the *dev-tier hot-reload* form,
/// which is a mode of the dev tier, not a second lowering. Both tiers
/// run with [`LowerOptions::default`] on the differential-gate path, so
/// dev-JIT and AOT code is generated by identical settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LowerOptions {
    /// Hot-reload mode (`specs/blocks/compiler.md` §8.2). It changes
    /// three things, all of which exist only to make a body swap
    /// possible:
    /// - script calls dispatch through the Context's per-function
    ///   indirection table instead of a direct call,
    /// - module globals live in a host-owned block reached through the
    ///   Context instead of in this module's data (module data dies
    ///   with the module; Context state must survive a swap),
    /// - coroutine frames record the reload epoch and `.next()` checks
    ///   it, so a resume across a swap traps.
    ///
    /// The AOT tier never sets it: a shipped binary has no reload.
    pub reload: bool,
}

/// One host-callable entry point: an exported `f(): void`.
pub(crate) struct EntryPoint {
    /// Script-level name.
    pub name: String,
    /// Lowered function.
    pub id: FuncId,
}

/// Result of lowering a whole program.
pub(crate) struct Lowered {
    /// The exported `main(): void` entry.
    pub main: FuncId,
    /// The synthesized global initializer; run before `main`.
    pub init: FuncId,
    /// Trap position table: `pos_id` -> TS position.
    pub positions: Vec<Pos>,
    /// Every exported zero-argument `void` function, in declaration
    /// order (the host-entry surface; `main` is one of them).
    pub entries: Vec<EntryPoint>,
    /// Function-slot table: slot index -> lowered function, `None` for
    /// a slot whose function was never materialized (an env wrapper
    /// for a function never used as a value). Slot numbering is a
    /// function of the *declarations* alone, so it is identical across
    /// recompiles that a hot reload accepts.
    pub slots: Vec<Option<FuncId>>,
    /// Size in bytes of the module-global block
    /// ([`LowerOptions::reload`] only; 0 otherwise).
    pub globals_size: u32,
    /// Alignment of the module-global block
    /// ([`LowerOptions::reload`] only; 1 otherwise).
    pub globals_align: u32,
}

/// Where a module-level variable's storage lives.
#[derive(Debug, Clone, Copy)]
pub(crate) enum GlobalSlot {
    /// Writable module data (default lowering).
    Data(DataId),
    /// Byte offset into the host-owned block the Context points at
    /// ([`LowerOptions::reload`]).
    Offset(u32),
}

/// Shared lowering state across all functions of one module.
pub(crate) struct ModLower<'a, M: Module> {
    pub module: &'a mut M,
    pub hir: &'a hir::Module,
    pub layouts: Layouts,
    pub rt: RtFns,
    pub opts: LowerOptions,
    pub fns: HashMap<FnKey, FuncId>,
    pub fn_slot: HashMap<FnKey, u32>,
    pub slots: Vec<Option<FuncId>>,
    pub fn_index: HashMap<String, usize>,
    pub str_data: HashMap<Vec<u8>, DataId>,
    pub globals: HashMap<String, (GlobalSlot, Type)>,
    /// Imported foreign C symbols, declared on first use (P5.2b).
    pub foreign_ids: HashMap<String, FuncId>,
    pub positions: Vec<Pos>,
    pub lambda_count: u32,
    pub str_count: u32,
    pub call_conv: CallConv,
}

/// Internal-error constructor (an invariant the checker should have
/// guaranteed does not hold; never a user-facing diagnostic).
pub(crate) fn internal(msg: impl Into<String>) -> String {
    format!("internal lowering error: {}", msg.into())
}

/// Whether the dev-JIT boundary marshaler may pass a **boundary struct by
/// value** to a foreign call on `triple` (`specs/blocks/compiler.md`
/// §12.3a). A by-value aggregate's C ABI differs by target, so only the
/// implemented-and-verified ABIs are permitted; any other dev host must
/// fail loudly rather than silently mis-marshal (dev-JIT ≠ ship-C).
/// Supported: aarch64 (AAPCS64, any OS) and x86-64 on Windows (Win64).
/// x86-64 on non-Windows (SysV) is **not** implemented and stays
/// unsupported. Only genuinely scalar / single-pointer boundary arguments
/// are target-neutral and are **not** gated by this; a `(ptr,len)` /
/// string-view descriptor is a 16-byte by-value aggregate whose C ABI is
/// target-specific (e.g. by reference on Win64) and is handled on the
/// by-value struct path this gates.
pub(crate) fn boundary_struct_by_value_supported(triple: &target_lexicon::Triple) -> bool {
    use target_lexicon::{Architecture, OperatingSystem};
    matches!(triple.architecture, Architecture::Aarch64(_))
        || (matches!(triple.architecture, Architecture::X86_64)
            && matches!(triple.operating_system, OperatingSystem::Windows))
}

impl<'a, M: Module> ModLower<'a, M> {
    /// Allocates a position-table entry.
    pub fn pos_id(&mut self, pos: &Pos) -> u32 {
        self.positions.push(pos.clone());
        (self.positions.len() - 1) as u32
    }

    /// Builds the signature for a script function.
    pub fn make_sig(
        &self,
        params: &[Type],
        ret: &Type,
        has_env: bool,
        has_this: bool,
    ) -> Result<Signature, String> {
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ctx
        if has_env {
            sig.params.push(AbiParam::new(types::I64));
        }
        let ret_repr = self.layouts.repr(ret)?;
        if matches!(ret_repr, Repr::Agg { .. }) {
            sig.params.push(AbiParam::new(types::I64)); // sret
        }
        if has_this {
            sig.params.push(AbiParam::new(types::I64));
        }
        for p in params {
            match self.layouts.repr(p)? {
                Repr::None => {}
                Repr::Scalar(t) => sig.params.push(AbiParam::new(t)),
                Repr::Pair => {
                    sig.params.push(AbiParam::new(types::I64));
                    sig.params.push(AbiParam::new(types::I64));
                }
                Repr::Agg { .. } => sig.params.push(AbiParam::new(types::I64)),
            }
        }
        match ret_repr {
            Repr::None | Repr::Agg { .. } => {}
            Repr::Scalar(t) => sig.returns.push(AbiParam::new(t)),
            Repr::Pair => {
                sig.returns.push(AbiParam::new(types::I64));
                sig.returns.push(AbiParam::new(types::I64));
            }
        }
        Ok(sig)
    }

    /// Signature of a generator resume function:
    /// `(ctx, frame, out) -> done`.
    pub fn resume_sig(&self) -> Signature {
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I8));
        sig
    }

    /// Interns a string literal as module data; returns its id.
    pub fn literal_data(&mut self, bytes: &[u8]) -> Result<DataId, String> {
        if let Some(&id) = self.str_data.get(bytes) {
            return Ok(id);
        }
        let name = format!("ss_str{}", self.str_count);
        self.str_count += 1;
        let id = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(|e| internal(format!("declare data: {e}")))?;
        let mut desc = DataDescription::new();
        // Literal data may be empty ("" literals); pad to one byte so
        // every literal has a distinct, valid address.
        let stored: Box<[u8]> = if bytes.is_empty() {
            Box::new([0u8])
        } else {
            bytes.to_vec().into_boxed_slice()
        };
        desc.define(stored);
        self.module
            .define_data(id, &desc)
            .map_err(|e| internal(format!("define data: {e}")))?;
        self.str_data.insert(bytes.to_vec(), id);
        Ok(id)
    }

    /// The foreign (C-ABI) function named `name` (P5.2b).
    pub fn foreign_fn(&self, name: &str) -> Result<&'a hir::ForeignFn, String> {
        self.hir
            .foreign_fns
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| internal(format!("unknown foreign function `{name}`")))
    }

    /// The HIR function named `name`.
    pub fn hir_fn(&self, name: &str) -> Result<&'a hir::Function, String> {
        self.fn_index
            .get(name)
            .map(|&i| &self.hir.functions[i])
            .ok_or_else(|| internal(format!("unknown function `{name}`")))
    }

    /// The HIR method `name` of class `cid`.
    pub fn hir_method(&self, cid: usize, name: &str) -> Result<&'a hir::Function, String> {
        self.hir
            .classes
            .get(cid)
            .and_then(|c| c.methods.iter().find(|m| m.name == name))
            .ok_or_else(|| internal(format!("unknown method `{name}` on class {cid}")))
    }

    /// FuncId for a key.
    pub fn func_id(&self, key: &FnKey) -> Result<FuncId, String> {
        self.fns
            .get(key)
            .copied()
            .ok_or_else(|| internal(format!("undeclared function {key:?}")))
    }

    /// Indirection-table slot for a key.
    pub fn slot_of(&self, key: &FnKey) -> Result<u32, String> {
        self.fn_slot
            .get(key)
            .copied()
            .ok_or_else(|| internal(format!("function {key:?} has no reload slot")))
    }

    /// Reserves the next slot for `key` (declaration order only, so
    /// slot numbers stay identical across an accepted reload).
    fn reserve_slot(&mut self, key: FnKey) {
        let n = self.slots.len() as u32;
        self.slots.push(None);
        self.fn_slot.insert(key, n);
    }

    /// Records the lowered function behind `key`'s slot.
    pub fn bind_slot(&mut self, key: &FnKey, id: FuncId) {
        if let Some(&s) = self.fn_slot.get(key) {
            if let Some(entry) = self.slots.get_mut(s as usize) {
                *entry = Some(id);
            }
        }
    }

    /// The declared signature of a lowered function.
    pub fn signature_of(&self, id: FuncId) -> Signature {
        self.module
            .declarations()
            .get_function_decl(id)
            .signature
            .clone()
    }
}

fn declare_rt<M: Module>(
    module: &mut M,
    call_conv: CallConv,
) -> Result<RtFns, String> {
    let mut mk = |name: &str, params: &[types::Type], ret: Option<types::Type>| {
        let mut sig = Signature::new(call_conv);
        for &p in params {
            sig.params.push(AbiParam::new(p));
        }
        if let Some(r) = ret {
            sig.returns.push(AbiParam::new(r));
        }
        module
            .declare_function(name, Linkage::Import, &sig)
            .map_err(|e| internal(format!("declare {name}: {e}")))
    };
    use types::{F32, F64, I16, I32, I64, I8};
    // Math intrinsic imports (stdlib.md §1): one opaque symbol per
    // accepted function, in MathFn::ALL order so `f as usize` indexes
    // the table. `clz32` is `(ctx, u32) -> i32`, `imul` is
    // `(ctx, i32, i32) -> i32`; all other arguments and results are
    // f64.
    let mut math_ids: Vec<FuncId> = Vec::with_capacity(hir::MathFn::ALL.len());
    for f in hir::MathFn::ALL {
        let (params, ret) = match f {
            hir::MathFn::Clz32 => (vec![I64, I32], I32),
            hir::MathFn::Imul => (vec![I64, I32, I32], I32),
            _ => {
                let mut params = vec![I64];
                params.extend(std::iter::repeat_n(F64, f.arity()));
                (params, F64)
            }
        };
        math_ids.push(mk(&format!("sub_rt_math_{}", f.name()), &params, Some(ret))?);
    }
    let math: [FuncId; hir::MathFn::ALL.len()] = math_ids
        .try_into()
        .map_err(|_| internal("math import table size"))?;
    // Number and parsing intrinsics (stdlib.md §11, Q25/Q26).
    // Every import starts with Context and is opaque to both tiers.
    let mut num_ids: Vec<FuncId> = Vec::with_capacity(hir::NumFn::ALL.len());
    for f in hir::NumFn::ALL {
        use hir::NumFn as N;
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            N::IsNaN | N::IsFinite | N::IsInteger | N::IsSafeInteger => {
                (&[I64, F64], Some(I32))
            }
            N::ParseInt => (&[I64, I64, I32, I32], Some(F64)),
            N::ParseFloat => (&[I64, I64, I32], Some(F64)),
            N::ToFixed => (&[I64, F64, I32, I32], Some(I64)),
            N::ToStringF32 => (&[I64, F32, I32, I32], Some(I64)),
            N::ToStringF64 | N::ToExponential | N::ToPrecision => {
                (&[I64, F64, I32, I32], Some(I64))
            }
            other => return Err(internal(format!("unknown NumFn {other:?}"))),
        };
        num_ids.push(mk(f.symbol(), params, ret)?);
    }
    let num: [FuncId; hir::NumFn::ALL.len()] = num_ids
        .try_into()
        .map_err(|_| internal("Number import table size"))?;
    // JSON builder leaves (stdlib.md §13). The checker emits a typed
    // serializer graph; these are its only runtime-specific operations.
    let mut json_ids: Vec<FuncId> = Vec::with_capacity(hir::JsonFn::ALL.len());
    for f in hir::JsonFn::ALL {
        use hir::JsonFn as J;
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            J::Begin | J::BeginTracked => (&[I64, I32], Some(I64)),
            J::Finish => (&[I64, I64, I32], Some(I64)),
            J::Raw | J::Str => (&[I64, I64, I64, I32], None),
            J::I32 | J::U32 => (&[I64, I64, I32, I32], None),
            J::I64 | J::U64 | J::Date => (&[I64, I64, I64, I32], None),
            J::F32 => (&[I64, I64, F32, I32], None),
            J::F64 => (&[I64, I64, F64, I32], None),
            J::Bool => (&[I64, I64, I8, I32], None),
            J::Null => (&[I64, I64, I32], None),
            J::Visit => (&[I64, I64, I64, I32], Some(I32)),
            J::Leave => (&[I64, I64, I64, I32], None),
            other => return Err(internal(format!("unknown JsonFn {other:?}"))),
        };
        json_ids.push(mk(f.symbol(), params, ret)?);
    }
    let json: [FuncId; hir::JsonFn::ALL.len()] = json_ids
        .try_into()
        .map_err(|_| internal("JSON import table size"))?;
    // String method imports (stdlib.md §8): one opaque symbol per
    // accepted method, `(ctx, recv, params…[, pos_id])`, in StrFn::ALL
    // order so `f as usize` indexes the table. The signature is built
    // from the StrFn tables the checker normalized against, so the two
    // sides cannot drift independently.
    let mut str_ids: Vec<FuncId> = Vec::with_capacity(hir::StrFn::ALL.len());
    for f in hir::StrFn::ALL {
        let mut params = vec![I64, I64]; // ctx, receiver handle
        for p in f.params() {
            params.push(match p {
                hir::StrParam::Str => I64,
                hir::StrParam::I32 => I32,
                // `StrParam` is #[non_exhaustive]; a variant this crate
                // does not know is a compiler/codegen version skew.
                other => return Err(internal(format!("unknown StrParam {other:?}"))),
            });
        }
        if f.takes_pos_id() {
            params.push(I32);
        }
        let ret = match f.ret() {
            hir::StrRet::I32 | hir::StrRet::Bool => I32,
            hir::StrRet::Str | hir::StrRet::StrArray => I64,
            // See the StrParam arm above.
            other => return Err(internal(format!("unknown StrRet {other:?}"))),
        };
        str_ids.push(mk(f.symbol(), &params, Some(ret))?);
    }
    let str_ops: [FuncId; hir::StrFn::ALL.len()] = str_ids
        .try_into()
        .map_err(|_| internal("string import table size"))?;
    // Array method imports (stdlib.md §9): one opaque symbol per
    // accepted method, in ArrFn::ALL order so `f as usize` indexes the
    // table. Signatures per the §9 marshaling contract: element values
    // by pointer (I64), callbacks as `(code, env)` (I64, I64), kind
    // tags and pos ids as u32 (I32), sizes as u64 (I64).
    let mut arr_ids: Vec<FuncId> = Vec::with_capacity(hir::ArrFn::ALL.len());
    for f in hir::ArrFn::ALL {
        use hir::ArrFn as A;
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            // (ctx, recv, x_ptr, kind) -> i32
            A::IndexOf | A::LastIndexOf | A::Includes => (&[I64, I64, I64, I32], Some(I32)),
            // (ctx, recv, sep, fmt_kind, pos_id) -> str handle
            A::Join => (&[I64, I64, I64, I32, I32], Some(I64)),
            // (ctx, recv, start, end, pos_id) -> array handle
            A::Slice => (&[I64, I64, I32, I32, I32], Some(I64)),
            // (ctx, recv, x_ptr, start, end)
            A::Fill => (&[I64, I64, I64, I32, I32], None),
            // (ctx, recv)
            A::Reverse => (&[I64, I64], None),
            // (ctx, recv, other, pos_id) -> array handle
            A::Concat => (&[I64, I64, I64, I32], Some(I64)),
            // (ctx, recv, code, env, kind, indexed)
            A::ForEach => (&[I64, I64, I64, I64, I32, I32], None),
            // (ctx, recv, code, env, elem_kind, ret_kind, ret_size,
            // pos_id, indexed) -> array handle
            A::Map => (
                &[I64, I64, I64, I64, I32, I32, I64, I32, I32],
                Some(I64),
            ),
            // (ctx, recv, code, env, kind, pos_id, indexed) -> array handle
            A::Filter => (&[I64, I64, I64, I64, I32, I32, I32], Some(I64)),
            // (ctx, recv, code, env, elem_kind, acc_kind, acc_size,
            // acc_ptr, indexed)
            A::Reduce | A::ReduceRight => {
                (
                    &[I64, I64, I64, I64, I32, I32, I64, I64, I32],
                    None,
                )
            }
            // (ctx, recv, code, env, kind, indexed) -> i32
            A::Some | A::Every | A::FindIndex => {
                (&[I64, I64, I64, I64, I32, I32], Some(I32))
            }
            // (ctx, recv, code, env, kind)
            A::Sort => (&[I64, I64, I64, I64, I32], None),
            // (ctx, recv, start, delete_count, pos_id) -> array handle
            A::Splice => (&[I64, I64, I32, I32, I32], Some(I64)),
            // (ctx, recv, out_ptr, pos_id)
            A::Shift => (&[I64, I64, I64, I32], None),
            // (ctx, recv, x_ptr, pos_id) -> new length
            A::Unshift => (&[I64, I64, I64, I32], Some(I32)),
            // (ctx, recv, target, start, end)
            A::CopyWithin => (&[I64, I64, I32, I32, I32], None),
            // `ArrFn` is #[non_exhaustive]; a variant this crate does
            // not know is a compiler/codegen version skew.
            other => return Err(internal(format!("unknown ArrFn {other:?}"))),
        };
        arr_ids.push(mk(f.symbol(), params, ret)?);
    }
    let arr_ops: [FuncId; hir::ArrFn::ALL.len()] = arr_ids
        .try_into()
        .map_err(|_| internal("array import table size"))?;
    let mut fixed_arr_ids: Vec<Option<FuncId>> =
        Vec::with_capacity(hir::ArrFn::ALL.len());
    for f in hir::ArrFn::ALL {
        use hir::ArrFn as A;
        let Some(symbol) = f.fixed_symbol() else {
            fixed_arr_ids.push(None);
            continue;
        };
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            // (ctx, data, len, elem_size, code, env, kind, indexed)
            A::ForEach => (&[I64, I64, I64, I64, I64, I64, I32, I32], None),
            // Plus result kind/size and allocation position.
            A::Map => (
                &[
                    I64, I64, I64, I64, I64, I64, I32, I32, I64, I32, I32,
                ],
                Some(I64),
            ),
            A::Filter => (
                &[I64, I64, I64, I64, I64, I64, I32, I32, I32],
                Some(I64),
            ),
            A::Reduce | A::ReduceRight => (
                &[
                    I64, I64, I64, I64, I64, I64, I32, I32, I64, I64, I32,
                ],
                None,
            ),
            A::Some | A::Every | A::FindIndex => (
                &[I64, I64, I64, I64, I64, I64, I32, I32],
                Some(I32),
            ),
            other => {
                return Err(internal(format!(
                    "FixedArray symbol on unsupported ArrFn {other:?}"
                )))
            }
        };
        fixed_arr_ids.push(Some(mk(symbol, params, ret)?));
    }
    let fixed_arr_ops: [Option<FuncId>; hir::ArrFn::ALL.len()] = fixed_arr_ids
        .try_into()
        .map_err(|_| internal("FixedArray callback import table size"))?;
    // Map/Set operations (stdlib.md §10). Keys, values, and fallbacks
    // travel by pointer; new receives the concrete monomorphized widths
    // and key-kind tag. forEach receives a generated fixed-ABI bridge.
    let mut map_ids: Vec<FuncId> = Vec::with_capacity(hir::MapFn::ALL.len());
    for f in hir::MapFn::ALL {
        use hir::MapFn as F;
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            F::New => (&[I64, I64, I64, I32, I32], Some(I64)),
            F::Size => (&[I64, I64], Some(I32)),
            F::Get => (&[I64, I64, I64, I64], Some(I32)),
            F::GetOr => (&[I64, I64, I64, I64, I64], None),
            F::Set => (&[I64, I64, I64, I64, I32], Some(I64)),
            F::Has | F::Delete => (&[I64, I64, I64], Some(I32)),
            F::Clear => (&[I64, I64], None),
            F::ForEach => (&[I64, I64, I64, I64, I64], None),
            F::GroupBy => (&[I64, I64, I64, I64, I64, I64, I32, I32], Some(I64)),
            other => return Err(internal(format!("unknown MapFn {other:?}"))),
        };
        map_ids.push(mk(f.symbol(), params, ret)?);
    }
    let map_ops: [FuncId; hir::MapFn::ALL.len()] = map_ids
        .try_into()
        .map_err(|_| internal("Map import table size"))?;
    let mut set_ids: Vec<FuncId> = Vec::with_capacity(hir::SetFn::ALL.len());
    for f in hir::SetFn::ALL {
        use hir::SetFn as F;
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            F::New => (&[I64, I64, I32, I32], Some(I64)),
            F::Size => (&[I64, I64], Some(I32)),
            F::Add => (&[I64, I64, I64, I32], Some(I64)),
            F::Has | F::Delete => (&[I64, I64, I64], Some(I32)),
            F::Clear => (&[I64, I64], None),
            F::ForEach => (&[I64, I64, I64, I64, I64], None),
            F::Union | F::Intersection | F::Difference | F::SymmetricDifference => {
                (&[I64, I64, I64, I32], Some(I64))
            }
            F::IsSubsetOf | F::IsSupersetOf | F::IsDisjointFrom => {
                (&[I64, I64, I64], Some(I32))
            }
            other => return Err(internal(format!("unknown SetFn {other:?}"))),
        };
        set_ids.push(mk(f.symbol(), params, ret)?);
    }
    let set_ops: [FuncId; hir::SetFn::ALL.len()] = set_ids
        .try_into()
        .map_err(|_| internal("Set import table size"))?;
    Ok(RtFns {
        print: mk("sub_rt_print", &[I64, I64], None)?,
        collect: mk("sub_rt_collect", &[I64], None)?,
        alloc: mk("sub_rt_alloc", &[I64, I64, I32, I32], Some(I64))?,
        delete: mk("sub_rt_delete", &[I64, I64, I32], None)?,
        trap: mk("sub_rt_trap", &[I64, I32, I32], None)?,
        root_add: mk("sub_rt_root_add", &[I64, I64, I64], None)?,
        shadow_push: mk("sub_rt_shadow_push", &[I64, I64, I64], None)?,
        shadow_pop: mk("sub_rt_shadow_pop", &[I64], None)?,
        str_lit: mk("sub_rt_str_lit", &[I64, I64, I64, I32], Some(I64))?,
        str_len: mk("sub_rt_str_len", &[I64, I64], Some(I32))?,
        str_concat: mk("sub_rt_str_concat", &[I64, I64, I64, I32], Some(I64))?,
        str_eq: mk("sub_rt_str_eq", &[I64, I64, I64], Some(I32))?,
        fmt_i32: mk("sub_rt_fmt_i32", &[I64, I32, I32], Some(I64))?,
        fmt_u32: mk("sub_rt_fmt_u32", &[I64, I32, I32], Some(I64))?,
        fmt_i64: mk("sub_rt_fmt_i64", &[I64, I64, I32], Some(I64))?,
        fmt_u64: mk("sub_rt_fmt_u64", &[I64, I64, I32], Some(I64))?,
        fmt_f32: mk("sub_rt_fmt_f32", &[I64, F32, I32], Some(I64))?,
        fmt_f64: mk("sub_rt_fmt_f64", &[I64, F64, I32], Some(I64))?,
        fmt_bool: mk("sub_rt_fmt_bool", &[I64, I32, I32], Some(I64))?,
        f16_from_f64: mk("sub_rt_f16_from_f64", &[F64], Some(I16))?,
        f16_to_f64: mk("sub_rt_f16_to_f64", &[I16], Some(F64))?,
        array_new: mk("sub_rt_array_new", &[I64, I64, I32], Some(I64))?,
        array_len: mk("sub_rt_array_len", &[I64, I64], Some(I32))?,
        array_push: mk("sub_rt_array_push", &[I64, I64, I64, I32], Some(I32))?,
        array_pop: mk("sub_rt_array_pop", &[I64, I64, I64, I32], None)?,
        array_ptr: mk("sub_rt_array_ptr", &[I64, I64, I32, I32], Some(I64))?,
        str_data: mk("sub_rt_str_data", &[I64, I64], Some(I64))?,
        array_data: mk("sub_rt_array_data", &[I64, I64], Some(I64))?,
        // (ctx, code, env, userdata1, userdata2) → binding pointer (§14.4:
        // two userdata slots).
        cb_bind: mk("sub_rt_cb_bind", &[I64, I64, I64, I64, I64], Some(I64))?,
        // The generic C-ABI callback trampoline (P5.2b, §14.4). Generated
        // code never calls it — a foreign C API does — so it is imported
        // only to take its address (`func_addr`) for a callback-info
        // struct's function-pointer slot; the declared signature (message
        // as two words, then the two userdata slots) is unused.
        cb_trampoline: mk("sub_rt_cb_trampoline", &[I64, I64, I64, I64], None)?,
        math,
        num,
        // Date intrinsics (stdlib.md §3): opaque symbols on both tiers.
        date_utc: mk(
            "sub_rt_date_utc",
            &[I64, I32, I32, I32, I32, I32, I32, I32, I32],
            Some(I64),
        )?,
        date_new: mk("sub_rt_date_new", &[I64, I64, I32], Some(I64))?,
        date_now: mk("sub_rt_date_now", &[I64], Some(I64))?,
        date_get: mk("sub_rt_date_get", &[I64, I64, I32], Some(I32))?,
        date_to_iso: mk("sub_rt_date_to_iso", &[I64, I64, I32], Some(I64))?,
        json,
        str_ops,
        arr_ops,
        fixed_arr_ops,
        map_ops,
        set_ops,
    })
}

/// Cranelift settings for the dev tier: no position-independent code
/// (the JIT resolves everything by absolute address in-process).
pub(crate) fn dev_flags() -> Result<cranelift_codegen::settings::Flags, String> {
    let mut fb = cranelift_codegen::settings::builder();
    fb.set("opt_level", "speed")
        .and_then(|()| fb.set("is_pic", "false"))
        .map_err(|e| internal(format!("settings: {e}")))?;
    Ok(cranelift_codegen::settings::Flags::new(fb))
}

/// Cranelift settings for the ship tier: position-independent code, as
/// the P0.5 spike used, because the object is linked into a PIE on
/// every target platform this project ships to.
pub(crate) fn aot_flags() -> Result<cranelift_codegen::settings::Flags, String> {
    let mut fb = cranelift_codegen::settings::builder();
    fb.set("opt_level", "speed")
        .and_then(|()| fb.set("is_pic", "true"))
        .map_err(|e| internal(format!("settings: {e}")))?;
    Ok(cranelift_codegen::settings::Flags::new(fb))
}

/// Assigns an indirection-table slot to every function the module
/// declares, in declaration order and *only* from declarations, so
/// that a recompile with an unchanged declaration hash produces the
/// same slot for the same function (§8.2). Slots are reserved for env
/// wrappers too, whether or not the program uses the function as a
/// value: wrapper creation is body-driven and must not shift the
/// numbering.
fn reserve_slots<M: Module>(ml: &mut ModLower<'_, M>) {
    for f in &ml.hir.functions {
        ml.reserve_slot(FnKey::Free(f.name.clone()));
        if f.is_generator {
            ml.reserve_slot(FnKey::Resume(f.name.clone()));
        } else {
            ml.reserve_slot(FnKey::Wrapper(f.name.clone()));
        }
    }
    for (ci, c) in ml.hir.classes.iter().enumerate() {
        if c.ctor.is_some() {
            ml.reserve_slot(FnKey::Ctor(ci));
        }
        for m in &c.methods {
            ml.reserve_slot(FnKey::Method(ci, m.name.clone()));
        }
    }
    ml.reserve_slot(FnKey::Init);
}

/// Lowers a checked program into `module`.
pub(crate) fn lower_module_with<M: Module>(
    module: &mut M,
    hirm: &hir::Module,
    opts: LowerOptions,
) -> Result<Lowered, String> {
    if module.isa().pointer_type() != types::I64 {
        return Err(internal(
            "only 64-bit targets are supported: the runtime ABI assumes 8-byte handles",
        ));
    }
    let call_conv = module.isa().default_call_conv();
    let rt = declare_rt(module, call_conv)?;
    let layouts = Layouts::build(hirm)?;

    let mut ml = ModLower {
        module,
        hir: hirm,
        layouts,
        rt,
        opts,
        fns: HashMap::new(),
        fn_slot: HashMap::new(),
        slots: Vec::new(),
        fn_index: HashMap::new(),
        str_data: HashMap::new(),
        globals: HashMap::new(),
        foreign_ids: HashMap::new(),
        positions: Vec::new(),
        lambda_count: 0,
        str_count: 0,
        call_conv,
    };

    for (i, f) in hirm.functions.iter().enumerate() {
        ml.fn_index.insert(f.name.clone(), i);
    }
    reserve_slots(&mut ml);

    // Globals: zero-initialized writable module data, filled by the
    // synthesized init function, which also registers managed ones as
    // collection roots. In reload mode the same layout is computed
    // into a host-owned block instead, because module data dies with
    // the module and Context state must outlive a swap (§8.2).
    let mut globals_size = 0u32;
    let mut globals_align = 1u32;
    for (gi, g) in hirm.globals.iter().enumerate() {
        let (size, align) = ml.layouts.size_align(&g.ty)?;
        let (size, align) = (size.max(1), align.max(1));
        let slot = if opts.reload {
            globals_align = globals_align.max(align);
            globals_size = globals_size.next_multiple_of(align);
            let at = globals_size;
            globals_size += size;
            GlobalSlot::Offset(at)
        } else {
            let name = format!("ss_g{gi}");
            let id = ml
                .module
                .declare_data(&name, Linkage::Local, true, false)
                .map_err(|e| internal(format!("declare global: {e}")))?;
            let mut desc = DataDescription::new();
            desc.define_zeroinit(size as usize);
            desc.set_align(u64::from(align));
            ml.module
                .define_data(id, &desc)
                .map_err(|e| internal(format!("define global: {e}")))?;
            GlobalSlot::Data(id)
        };
        ml.globals.insert(g.name.clone(), (slot, g.ty.clone()));
    }
    globals_size = globals_size.next_multiple_of(globals_align);

    // Declare every script function up front so bodies can call in any
    // order. Symbol names are index-based (stable and linker-clean for
    // the AOT tier; HIR names may contain `<...>` from monomorphization).
    // Exported functions get a stable `ss_export_<name>` symbol with
    // external linkage: they are the host-entry surface (Q12) and the
    // AOT entry program resolves them at link time.
    let decl = |ml: &mut ModLower<M>, key: FnKey, sym: String, sig: &Signature, export: bool| {
        let linkage = if export { Linkage::Export } else { Linkage::Local };
        let id = ml
            .module
            .declare_function(&sym, linkage, sig)
            .map_err(|e| internal(format!("declare {sym}: {e}")))?;
        ml.bind_slot(&key, id);
        ml.fns.insert(key, id);
        Ok::<(), String>(())
    };
    for (i, f) in hirm.functions.iter().enumerate() {
        let params: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
        let export = f.exported && !f.is_generator;
        let sym = if export {
            format!("ss_export_{}", f.name)
        } else {
            format!("ss_f{i}")
        };
        if f.is_generator {
            let sig =
                ml.make_sig(&params, &Type::Generator(Box::new(Type::Void)), false, false)?;
            decl(&mut ml, FnKey::Free(f.name.clone()), sym, &sig, false)?;
            let rsig = ml.resume_sig();
            decl(
                &mut ml,
                FnKey::Resume(f.name.clone()),
                format!("ss_f{i}_resume"),
                &rsig,
                false,
            )?;
        } else {
            let sig = ml.make_sig(&params, &f.ret, false, false)?;
            decl(&mut ml, FnKey::Free(f.name.clone()), sym, &sig, export)?;
        }
    }
    for (ci, c) in hirm.classes.iter().enumerate() {
        if let Some(ctor) = &c.ctor {
            let params: Vec<Type> = ctor.params.iter().map(|p| p.ty.clone()).collect();
            let sig = ml.make_sig(&params, &Type::Void, false, true)?;
            decl(&mut ml, FnKey::Ctor(ci), format!("ss_ctor{ci}"), &sig, false)?;
        }
        for (mi, m) in c.methods.iter().enumerate() {
            let params: Vec<Type> = m.params.iter().map(|p| p.ty.clone()).collect();
            let sig = ml.make_sig(&params, &m.ret, false, true)?;
            decl(
                &mut ml,
                FnKey::Method(ci, m.name.clone()),
                format!("ss_m{ci}_{mi}"),
                &sig,
                false,
            )?;
        }
    }
    {
        let sig = ml.make_sig(&[], &Type::Void, false, false)?;
        decl(&mut ml, FnKey::Init, "ss_init".to_string(), &sig, true)?;
    }

    // Define bodies.
    for f in &hirm.functions {
        if f.is_generator {
            func::define_generator(&mut ml, f)?;
        } else {
            define_function(&mut ml, FnKey::Free(f.name.clone()), f, None)?;
        }
    }
    for (ci, c) in hirm.classes.iter().enumerate() {
        if let Some(ctor) = &c.ctor {
            define_function(&mut ml, FnKey::Ctor(ci), ctor, Some(ci))?;
        }
        for m in &c.methods {
            define_function(&mut ml, FnKey::Method(ci, m.name.clone()), m, Some(ci))?;
        }
    }
    func::define_init(&mut ml)?;

    let main = ml
        .fn_index
        .get("main")
        .and_then(|&i| {
            let f = &hirm.functions[i];
            (f.exported && !f.is_generator && f.params.is_empty()).then_some(())
        })
        .ok_or_else(|| internal("no exported `main(): void` entry point"))
        .and_then(|()| ml.func_id(&FnKey::Free("main".to_string())))?;
    let init = ml.func_id(&FnKey::Init)?;
    let mut entries = Vec::new();
    for f in &hirm.functions {
        if f.exported && !f.is_generator && f.params.is_empty() && f.ret == Type::Void {
            entries.push(EntryPoint {
                name: f.name.clone(),
                id: ml.func_id(&FnKey::Free(f.name.clone()))?,
            });
        }
    }
    let positions = std::mem::take(&mut ml.positions);
    let slots = std::mem::take(&mut ml.slots);
    Ok(Lowered {
        main,
        init,
        positions,
        entries,
        slots,
        globals_size,
        globals_align,
    })
}

#[cfg(test)]
mod tests {
    use super::boundary_struct_by_value_supported;
    use std::str::FromStr;
    use target_lexicon::Triple;

    /// The dev-JIT boundary-struct-by-value marshaler implements AAPCS64
    /// (aarch64, any OS) and Win64 (x86-64 on Windows) only (compiler.md
    /// §12.3a): the classifier must accept those and reject x86-64 SysV
    /// (unimplemented), so an unsupported target fails loudly at codegen
    /// instead of silently mis-marshaling (dev-JIT ≠ ship-C).
    #[test]
    fn boundary_struct_by_value_is_aapcs64_and_win64() {
        for t in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let triple = Triple::from_str(t).expect("triple");
            assert!(
                boundary_struct_by_value_supported(&triple),
                "{t} must be supported by the by-value struct path"
            );
        }
        let sysv = Triple::from_str("x86_64-unknown-linux-gnu").expect("triple");
        assert!(
            !boundary_struct_by_value_supported(&sysv),
            "x86_64-unknown-linux-gnu (SysV) must be unsupported by the by-value struct path"
        );
    }
}
