//! HIR-to-CLIF lowering for the development JIT.
//!
//! The lowering targets the [`cranelift_module::Module`] trait. The dev
//! JIT instantiates it with `JITModule`. The ship tier uses the separate
//! LIR-to-C lowering.
//!
//! [`LowerOptions`] is the lowering's only parameter. Hot-reload mode
//! permits an entry-less module because the host drives its exports.
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
//! `subscript_rt_trap` — generated code branches to a per-function unwind
//! block that pops its shadow frame and returns a zeroed value, so
//! the whole stack returns to the driver without signals or
//! unwinding. Each trap site carries an index into the position
//! table returned in [`Lowered::positions`].

mod func;

use std::collections::HashMap;

use cranelift_codegen::ir::{
    condcodes::IntCC, types, AbiParam, Endianness, InstBuilder, MemFlags, Signature,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::Configurable;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use subscript_compiler::hir as front_end;
use subscript_compiler::{lir, ClassId, Pos, StringAliasId, Type};

use front_end::{
    ArrFn, JsonFn, MapFn, MathFn, Module as HirModule, NumFn, RegexFn, SetFn, StrFn, StrParam,
    StrRet,
};

use crate::layout::{
    checked_add_size as checked_layout_add, checked_mul_size as checked_layout_mul,
    round_up_layout, Layouts, Repr,
};

pub(crate) use func::define_function;

/// Identity of a lowered function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FnKey {
    /// Free function by HIR name (legacy module orchestration only).
    Free(String),
    /// Generator resume function by source name (legacy module orchestration only).
    Resume(String),
    /// Host ABI wrapper for an exported async function.
    AsyncExport(String),
    /// Reload-only adapter for one parameterized host export.
    ReloadExport(String),
    /// Constructor of a class.
    Ctor(usize),
    /// Method of a class.
    Method(usize, String),
    /// Async method resume function.
    MethodResume(usize, String),
    /// Generated standard-runner helper that kicks non-main async exports.
    AsyncRunner,
    /// Env-taking wrapper for a named function used as a value.
    Wrapper(String),
    /// Declared LIR function (coroutine creators included).
    LirFunction(lir::FunctionId),
    /// LIR coroutine resume function.
    LirResume(lir::FunctionId),
    /// Host ABI wrapper for an exported LIR async function.
    LirAsyncExport(lir::FunctionId),
    /// Env-taking wrapper for a direct LIR function value.
    LirWrapper(lir::FunctionId),
    /// The synthesized global-initializer entry.
    Init,
    /// Generated initializer adapter used for fresh worker Contexts.
    WorkerInit,
    /// Runtime-C-ABI adapter for one monomorphized Q35 worker entry.
    WorkerEntry(usize),
}

/// Imported runtime entry points.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub(crate) struct RtFns {
    pub print: FuncId,
    pub collect: FuncId,
    pub alloc: FuncId,
    pub globals_init: FuncId,
    pub root_add: FuncId,
    pub boundary_scratch_mark: FuncId,
    pub boundary_scratch_alloc: FuncId,
    pub boundary_scratch_release: FuncId,
    pub delete: FuncId,
    pub trap: FuncId,
    pub trap_index_out_of_bounds: FuncId,
    pub trap_wire_enum: FuncId,
    pub shadow_push: FuncId,
    pub shadow_pop: FuncId,
    pub async_kick: FuncId,
    pub async_register: FuncId,
    pub async_retain: FuncId,
    pub async_release: FuncId,
    pub async_retain_array: FuncId,
    pub async_release_array: FuncId,
    pub async_is_stale: FuncId,
    pub async_complete: FuncId,
    pub async_result: FuncId,
    pub str_lit: FuncId,
    pub str_from_view: FuncId,
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
    /// IEEE floating remainder shared by both code-generation tiers.
    pub fmod: FuncId,
    pub array_new: FuncId,
    pub array_from_bytes: FuncId,
    pub array_byte_range: FuncId,
    pub array_len: FuncId,
    pub array_push: FuncId,
    pub array_pop: FuncId,
    pub str_data: FuncId,
    pub array_data: FuncId,
    pub assoc_iter_begin: FuncId,
    pub assoc_iter_copy: FuncId,
    pub str_iter_code_point: FuncId,
    pub array_spread_array: FuncId,
    pub array_spread_fixed: FuncId,
    pub array_spread_assoc: FuncId,
    pub array_spread_string: FuncId,
    pub cb_bind: FuncId,
    pub cb_trampoline: FuncId,
    /// `subscript_rt_math_*` imports (stdlib.md §1), indexed by
    /// `MathFn as usize` (the [`MathFn::ALL`] order).
    pub math: [FuncId; MathFn::ALL.len()],
    /// `subscript_rt_num_*` imports (stdlib.md §11, Q25/Q26), indexed by
    /// [`NumFn::ALL`] discriminant order.
    pub num: [FuncId; NumFn::ALL.len()],
    /// `subscript_rt_date_utc` (stdlib.md §3): 7 `i32` components + pos id → i64.
    pub date_utc: FuncId,
    /// `subscript_rt_date_new`: ms + pos id → range-checked ms.
    pub date_new: FuncId,
    /// `subscript_rt_date_now`: Context clock → i64 ms.
    pub date_now: FuncId,
    /// `subscript_rt_date_get`: (ms, field code) → i32 UTC accessor.
    pub date_get: FuncId,
    /// `subscript_rt_date_to_iso`: (ms, pos id) → string handle.
    pub date_to_iso: FuncId,
    /// Checker-generated JSON serializer leaves (stdlib.md §13), indexed
    /// by [`JsonFn::ALL`] discriminant order.
    pub json: [FuncId; JsonFn::ALL.len()],
    /// `subscript_rt_str_*` method imports (stdlib.md §8), indexed by
    /// `StrFn as usize` (the [`StrFn::ALL`] order). Each
    /// signature is `(ctx, recv, params…[, pos_id])` per
    /// [`StrFn::params`] / [`StrFn::takes_pos_id`].
    pub str_ops: [FuncId; StrFn::ALL.len()],
    /// `subscript_rt_regex_*` imports (stdlib.md §15).
    pub regex_ops: [FuncId; RegexFn::ALL.len()],
    /// `subscript_rt_arr_*` method imports (stdlib.md §9), indexed by
    /// `ArrFn as usize` (the [`ArrFn::ALL`] order). Each
    /// signature starts `(ctx, recv, …)`; element values travel by
    /// pointer, callbacks as `(code, env)`, kind tags as `u32`.
    pub arr_ops: [FuncId; ArrFn::ALL.len()],
    /// Q27 `FixedArray<T, N>` callback-family imports. Unsupported
    /// `ArrFn` variants have no fixed-buffer entry.
    pub fixed_arr_ops: [Option<FuncId>; ArrFn::ALL.len()],
    /// `subscript_rt_map_*` imports (stdlib.md §10), indexed by
    /// [`MapFn::ALL`] discriminant order.
    pub map_ops: [FuncId; MapFn::ALL.len()],
    /// `subscript_rt_set_*` imports (stdlib.md §10), indexed by
    /// [`SetFn::ALL`] discriminant order.
    pub set_ops: [FuncId; SetFn::ALL.len()],
    pub worker_spawn: FuncId,
    pub worker_post: FuncId,
    pub worker_poll: FuncId,
    pub worker_close: FuncId,
    pub worker_join: FuncId,
    pub worker_inbox_wait: FuncId,
    pub worker_inbox_poll: FuncId,
    pub worker_outbox_post: FuncId,
}

/// Parameters of the development-tier Cranelift lowering.
///
/// These parameters select the hot-reload form. The normal dev-JIT path
/// uses [`LowerOptions::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// The normal dev-JIT run path never sets it.
    pub reload: bool,
    /// Whether lowering requires an exported `main(): void` entry.
    /// Program run paths keep this enabled. A reload session disables
    /// it because the host calls named exports.
    pub require_main: bool,
}

impl Default for LowerOptions {
    fn default() -> Self {
        Self {
            reload: false,
            require_main: true,
        }
    }
}

/// One host-callable entry point.
pub(crate) struct EntryPoint {
    /// Script-level name.
    pub name: String,
    /// Lowered function.
    pub id: FuncId,
    /// Parameter types in declaration order.
    pub params: Vec<Type>,
    /// Reload-only uniform argument adapter for a parameterized entry.
    pub reload_adapter: Option<FuncId>,
    /// Whether this entry is an async root wrapper (Q34).
    pub is_async: bool,
}

/// Result of lowering a whole program.
pub(crate) struct Lowered {
    /// The exported `main(): void` entry, when the selected lowering
    /// mode permits it to be absent.
    pub main: Option<FuncId>,
    /// The synthesized global initializer; run before `main`.
    pub init: FuncId,
    /// Trap position table: `pos_id` -> TS position.
    pub positions: Vec<Pos>,
    /// Every host-callable export in declaration order.
    pub entries: Vec<EntryPoint>,
    /// Function-slot table: slot index -> lowered function, `None` for
    /// a slot whose function was never materialized (an env wrapper
    /// for a function never used as a value). Slot numbering is a
    /// function of the *declarations* alone, so it is identical across
    /// recompiles that a hot reload accepts.
    pub slots: Vec<Option<FuncId>>,
    /// Size in bytes of the Context-owned module-global block (reload and
    /// Q35 worker-bearing modules; 0 for ordinary non-reload JIT modules).
    pub globals_size: u32,
    /// Alignment of the Context-owned module-global block.
    pub globals_align: u32,
    /// Foreign C symbols imported by lowered call sites, in first-use
    /// order. This is the complete set because the sole
    /// `Linkage::Import` path for `Callee::Foreign` records here when it
    /// declares the import.
    pub foreign_symbols: Vec<String>,
}

const NO_MAIN_DIAGNOSTIC: &str = "no exported `main(): void` entry point";

impl Lowered {
    /// Returns the program entry or the strict run-path diagnostic.
    pub(crate) fn main_id(&self) -> Result<FuncId, String> {
        self.main.ok_or_else(|| internal(NO_MAIN_DIAGNOSTIC))
    }
}

/// Where a module-level variable's storage lives.
#[derive(Debug, Clone, Copy)]
pub(crate) enum GlobalSlot {
    /// Writable module data (default lowering).
    Data(DataId),
    /// Byte offset into the module-global block the Context points at
    /// (hot reload and Q35 worker-bearing modules).
    Offset(u32),
}

/// Shared lowering state across all functions of one module.
pub(crate) struct ModLower<'a, M: Module> {
    pub module: &'a mut M,
    pub lir: &'a lir::Module,
    pub layouts: Layouts,
    pub rt: RtFns,
    pub opts: LowerOptions,
    pub fns: HashMap<FnKey, FuncId>,
    pub fn_slot: HashMap<FnKey, u32>,
    pub slots: Vec<Option<FuncId>>,
    pub str_data: HashMap<Vec<u8>, DataId>,
    /// Per-Q32-alias tables of `(member bytes pointer, byte length)`.
    pub string_alias_tables: HashMap<StringAliasId, DataId>,
    /// Per-message-class runtime descriptors in program-image data.
    pub worker_message_descriptors: HashMap<ClassId, DataId>,
    pub globals: HashMap<String, (GlobalSlot, Type)>,
    /// Imported foreign C symbols, declared on first use (P5.2b).
    pub foreign_ids: HashMap<String, FuncId>,
    /// Foreign imports in deterministic first-use order.
    pub foreign_symbols: Vec<String>,
    pub positions: Vec<Pos>,
    pub lambda_count: u32,
    pub str_count: u32,
    pub call_conv: CallConv,
    /// Whether this lowering reaches module globals through the Context.
    pub context_globals: bool,
    /// Layout passed to the runtime when a fresh non-host-owned block is
    /// required (ordinary worker Contexts and non-reload parents).
    pub globals_size: u32,
    pub globals_align: u32,
}

/// Internal-error constructor (an invariant the checker should have
/// guaranteed does not hold; never a user-facing diagnostic).
pub(crate) fn internal(msg: impl Into<String>) -> String {
    format!("internal lowering error: {}", msg.into())
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
        let name = format!("subscript_str{}", self.str_count);
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

    /// Defines the declaration-ordered static formatting table for one
    /// Q32 alias and returns its module-data id.
    pub fn string_alias_table_data(&mut self, alias_id: StringAliasId) -> Result<DataId, String> {
        if let Some(&id) = self.string_alias_tables.get(&alias_id) {
            return Ok(id);
        }
        let members = self
            .lir
            .string_aliases
            .get(alias_id.0)
            .ok_or_else(|| internal(format!("string alias id {} is out of range", alias_id.0)))?
            .members
            .clone();
        let size = members
            .len()
            .checked_mul(16)
            .ok_or_else(|| internal("string alias table size overflows usize"))?;
        let mut contents = vec![0u8; size];
        let mut member_data = Vec::with_capacity(members.len());
        let endian = self.module.isa().endianness();
        for (index, member) in members.iter().enumerate() {
            member_data.push(self.literal_data(member.as_bytes())?);
            let len = u64::try_from(member.len())
                .map_err(|_| internal("string alias member length does not fit u64"))?;
            let encoded = match endian {
                Endianness::Little => len.to_le_bytes(),
                Endianness::Big => len.to_be_bytes(),
            };
            let at = index
                .checked_mul(16)
                .and_then(|offset| offset.checked_add(8))
                .ok_or_else(|| internal("string alias table offset overflows usize"))?;
            let end = at
                .checked_add(8)
                .ok_or_else(|| internal("string alias length slot overflows usize"))?;
            let slot = contents
                .get_mut(at..end)
                .ok_or_else(|| internal("string alias length slot is out of range"))?;
            slot.copy_from_slice(&encoded);
        }
        let name = format!("subscript_string_alias{}", alias_id.0);
        let id = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(|error| internal(format!("declare string alias table: {error}")))?;
        let mut desc = DataDescription::new();
        desc.define(contents.into_boxed_slice());
        desc.set_align(8);
        for (index, member_id) in member_data.into_iter().enumerate() {
            let member = self.module.declare_data_in_data(member_id, &mut desc);
            let offset = u32::try_from(
                index
                    .checked_mul(16)
                    .ok_or_else(|| internal("string alias relocation offset overflows usize"))?,
            )
            .map_err(|_| internal("string alias relocation offset does not fit u32"))?;
            desc.write_data_addr(offset, member, 0);
        }
        self.module
            .define_data(id, &desc)
            .map_err(|error| internal(format!("define string alias table: {error}")))?;
        self.string_alias_tables.insert(alias_id, id);
        Ok(id)
    }

    /// Defines one worker message descriptor and returns its module-data id.
    pub fn worker_message_descriptor_data(&mut self, class: ClassId) -> Result<DataId, String> {
        if let Some(&id) = self.worker_message_descriptors.get(&class) {
            return Ok(id);
        }
        let payload_size = u64::from(self.layouts.class(class.0)?.size);
        let offsets = self.layouts.worker_message_string_slot_offsets(class.0)?;
        let endian = self.module.isa().endianness();
        let encode = |value: u64| match endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        };

        let offsets_id = if offsets.is_empty() {
            None
        } else {
            let mut contents = Vec::with_capacity(offsets.len() * 8);
            for &offset in &offsets {
                contents.extend_from_slice(&encode(offset));
            }
            let name = format!("subscript_worker_string_offsets{}", class.0);
            let id = self
                .module
                .declare_data(&name, Linkage::Local, false, false)
                .map_err(|error| internal(format!("declare worker string offsets: {error}")))?;
            let mut description = DataDescription::new();
            description.define(contents.into_boxed_slice());
            description.set_align(8);
            self.module
                .define_data(id, &description)
                .map_err(|error| internal(format!("define worker string offsets: {error}")))?;
            Some(id)
        };

        // The runtime test `generated_worker_message_descriptor_matches_the_rust_c_layout`
        // pins these descriptor ABI constants.
        const DESCRIPTOR_SIZE: usize = 24;
        const STRING_OFFSETS_POINTER_OFFSET: u32 = 16;
        let mut contents = vec![0u8; DESCRIPTOR_SIZE];
        contents[0..8].copy_from_slice(&encode(payload_size));
        contents[8..16].copy_from_slice(&encode(offsets.len() as u64));
        let name = format!("subscript_worker_message_descriptor{}", class.0);
        let id = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(|error| internal(format!("declare worker message descriptor: {error}")))?;
        let mut description = DataDescription::new();
        description.define(contents.into_boxed_slice());
        description.set_align(8);
        if let Some(offsets_id) = offsets_id {
            let offsets_ref = self
                .module
                .declare_data_in_data(offsets_id, &mut description);
            description.write_data_addr(STRING_OFFSETS_POINTER_OFFSET, offsets_ref, 0);
        }
        self.module
            .define_data(id, &description)
            .map_err(|error| internal(format!("define worker message descriptor: {error}")))?;
        self.worker_message_descriptors.insert(class, id);
        Ok(id)
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

    /// Binds an additional identity to an already declared function and slot.
    pub fn alias_function(&mut self, alias: FnKey, target: &FnKey) -> Result<(), String> {
        let id = self.func_id(target)?;
        self.fns.insert(alias.clone(), id);
        if let Some(slot) = self.fn_slot.get(target).copied() {
            self.fn_slot.insert(alias, slot);
        }
        Ok(())
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

fn declare_rt<M: Module>(module: &mut M, call_conv: CallConv) -> Result<RtFns, String> {
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
    // the table. Each match arm supplies the sized runtime signature.
    let mut math_ids: Vec<FuncId> = Vec::with_capacity(MathFn::ALL.len());
    for f in MathFn::ALL {
        let (params, ret) = match f {
            MathFn::Clz32 => (vec![I64, I32], I32),
            MathFn::Imul => (vec![I64, I32, I32], I32),
            MathFn::F32ToBits => (vec![I64, F64], I32),
            MathFn::F32FromBits => (vec![I64, I32], F64),
            _ => {
                let mut params = vec![I64];
                params.extend(std::iter::repeat_n(F64, f.arity()));
                (params, F64)
            }
        };
        math_ids.push(mk(f.symbol(), &params, Some(ret))?);
    }
    let math: [FuncId; MathFn::ALL.len()] = math_ids
        .try_into()
        .map_err(|_| internal("math import table size"))?;
    // Number and parsing intrinsics (stdlib.md §11, Q25/Q26).
    // Every import starts with Context and is opaque to both tiers.
    let mut num_ids: Vec<FuncId> = Vec::with_capacity(NumFn::ALL.len());
    for f in NumFn::ALL {
        use NumFn as N;
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            N::IsNaN | N::IsFinite | N::IsInteger | N::IsSafeInteger => (&[I64, F64], Some(I32)),
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
    let num: [FuncId; NumFn::ALL.len()] = num_ids
        .try_into()
        .map_err(|_| internal("Number import table size"))?;
    // JSON builder leaves (stdlib.md §13). The checker emits a typed
    // serializer graph; these are its only runtime-specific operations.
    let mut json_ids: Vec<FuncId> = Vec::with_capacity(JsonFn::ALL.len());
    for f in JsonFn::ALL {
        use JsonFn as J;
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
            J::ParseBegin => (&[I64, I64, I32], Some(I64)),
            J::ParseEnd => (&[I64, I64, I32], None),
            J::ParseRoot => (&[I64, I64, I32], Some(I64)),
            J::ParseIsKind | J::ParseNumberFits => (&[I64, I64, I64, I32, I32], Some(I32)),
            J::ParseNumber => (&[I64, I64, I64, I32], Some(F64)),
            J::ParseInteger => (&[I64, I64, I64, I32, I32], Some(I64)),
            J::ParseBool => (&[I64, I64, I64, I32], Some(I32)),
            J::ParseString => (&[I64, I64, I64, I32], Some(I64)),
            J::ParseArrayLen => (&[I64, I64, I64, I32], Some(I32)),
            J::ParseArrayGet => (&[I64, I64, I64, I32, I32], Some(I64)),
            J::ParseObjectGet => (&[I64, I64, I64, I64, I32], Some(I64)),
            other => return Err(internal(format!("unknown JsonFn {other:?}"))),
        };
        json_ids.push(mk(f.symbol(), params, ret)?);
    }
    let json: [FuncId; JsonFn::ALL.len()] = json_ids
        .try_into()
        .map_err(|_| internal("JSON import table size"))?;
    // String method imports (stdlib.md §8): one opaque symbol per
    // accepted method, `(ctx, recv, params…[, pos_id])`, in StrFn::ALL
    // order so `f as usize` indexes the table. The signature is built
    // from the StrFn tables the checker normalized against, so the two
    // sides cannot drift independently.
    let mut str_ids: Vec<FuncId> = Vec::with_capacity(StrFn::ALL.len());
    for f in StrFn::ALL {
        let mut params = vec![I64, I64]; // ctx, receiver handle
        for p in f.params() {
            params.push(match p {
                StrParam::Str => I64,
                StrParam::I32 => I32,
                // `StrParam` is #[non_exhaustive]. An unknown variant means a
                // defect in this compiler.
                other => return Err(internal(format!("unknown StrParam {other:?}"))),
            });
        }
        if f.takes_pos_id() {
            params.push(I32);
        }
        let ret = match f.ret() {
            StrRet::I32 | StrRet::Bool => I32,
            StrRet::Str | StrRet::StrArray => I64,
            // See the StrParam arm above.
            other => return Err(internal(format!("unknown StrRet {other:?}"))),
        };
        str_ids.push(mk(f.symbol(), &params, Some(ret))?);
    }
    let str_ops: [FuncId; StrFn::ALL.len()] = str_ids
        .try_into()
        .map_err(|_| internal("string import table size"))?;
    let regex_ops: [FuncId; RegexFn::ALL.len()] = {
        let mut ids = Vec::with_capacity(RegexFn::ALL.len());
        for function in RegexFn::ALL {
            use RegexFn as R;
            let (params, ret): (&[types::Type], types::Type) = match function {
                R::New => (&[I64, I64, I64, I32], I64),
                R::Test => (&[I64, I64, I64, I32], I32),
                R::Source | R::Flags => (&[I64, I64, I32], I64),
                R::Search => (&[I64, I64, I64, I32], I32),
                R::Replace | R::ReplaceAll => (&[I64, I64, I64, I64, I32], I64),
                R::Split => (&[I64, I64, I64, I32], I64),
                R::MatchStart | R::MatchEnd => (&[I64, I64, I32, I32], I32),
                other => return Err(internal(format!("unknown RegexFn {other:?}"))),
            };
            ids.push(mk(function.symbol(), params, Some(ret))?);
        }
        ids.try_into()
            .map_err(|_| internal("regex import table size"))?
    };
    // Array method imports (stdlib.md §9): one opaque symbol per
    // accepted method, in ArrFn::ALL order so `f as usize` indexes the
    // table. Signatures per the §9 marshaling contract: element values
    // by pointer (I64), callbacks as `(code, env)` (I64, I64), kind
    // tags and pos ids as u32 (I32), sizes as u64 (I64).
    let mut arr_ids: Vec<FuncId> = Vec::with_capacity(ArrFn::ALL.len());
    for f in ArrFn::ALL {
        use ArrFn as A;
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
            A::Map => (&[I64, I64, I64, I64, I32, I32, I64, I32, I32], Some(I64)),
            // (ctx, recv, code, env, kind, pos_id, indexed) -> array handle
            A::Filter => (&[I64, I64, I64, I64, I32, I32, I32], Some(I64)),
            // (ctx, recv, code, env, elem_kind, acc_kind, acc_size,
            // acc_ptr, indexed)
            A::Reduce | A::ReduceRight => (&[I64, I64, I64, I64, I32, I32, I64, I64, I32], None),
            // (ctx, recv, code, env, kind, indexed) -> i32
            A::Some | A::Every | A::FindIndex => (&[I64, I64, I64, I64, I32, I32], Some(I32)),
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
            // `ArrFn` is #[non_exhaustive]. An unknown variant means a defect
            // in this compiler.
            other => return Err(internal(format!("unknown ArrFn {other:?}"))),
        };
        arr_ids.push(mk(f.symbol(), params, ret)?);
    }
    let arr_ops: [FuncId; ArrFn::ALL.len()] = arr_ids
        .try_into()
        .map_err(|_| internal("array import table size"))?;
    let mut fixed_arr_ids: Vec<Option<FuncId>> = Vec::with_capacity(ArrFn::ALL.len());
    for f in ArrFn::ALL {
        use ArrFn as A;
        let Some(symbol) = f.fixed_symbol() else {
            fixed_arr_ids.push(None);
            continue;
        };
        let (params, ret): (&[types::Type], Option<types::Type>) = match f {
            // (ctx, data, len, elem_size, code, env, kind, indexed)
            A::ForEach => (&[I64, I64, I64, I64, I64, I64, I32, I32], None),
            // Plus result kind/size and allocation position.
            A::Map => (
                &[I64, I64, I64, I64, I64, I64, I32, I32, I64, I32, I32],
                Some(I64),
            ),
            A::Filter => (&[I64, I64, I64, I64, I64, I64, I32, I32, I32], Some(I64)),
            A::Reduce | A::ReduceRight => (
                &[I64, I64, I64, I64, I64, I64, I32, I32, I64, I64, I32],
                None,
            ),
            A::Some | A::Every | A::FindIndex => {
                (&[I64, I64, I64, I64, I64, I64, I32, I32], Some(I32))
            }
            other => {
                return Err(internal(format!(
                    "FixedArray symbol on unsupported ArrFn {other:?}"
                )))
            }
        };
        fixed_arr_ids.push(Some(mk(symbol, params, ret)?));
    }
    let fixed_arr_ops: [Option<FuncId>; ArrFn::ALL.len()] = fixed_arr_ids
        .try_into()
        .map_err(|_| internal("FixedArray callback import table size"))?;
    // Map/Set operations (stdlib.md §10). Keys, values, and fallbacks
    // travel by pointer; new receives the concrete monomorphized widths
    // and key-kind tag. forEach receives a generated fixed-ABI bridge.
    let mut map_ids: Vec<FuncId> = Vec::with_capacity(MapFn::ALL.len());
    for f in MapFn::ALL {
        use MapFn as F;
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
    let map_ops: [FuncId; MapFn::ALL.len()] = map_ids
        .try_into()
        .map_err(|_| internal("Map import table size"))?;
    let mut set_ids: Vec<FuncId> = Vec::with_capacity(SetFn::ALL.len());
    for f in SetFn::ALL {
        use SetFn as F;
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
            F::IsSubsetOf | F::IsSupersetOf | F::IsDisjointFrom => (&[I64, I64, I64], Some(I32)),
            other => return Err(internal(format!("unknown SetFn {other:?}"))),
        };
        set_ids.push(mk(f.symbol(), params, ret)?);
    }
    let set_ops: [FuncId; SetFn::ALL.len()] = set_ids
        .try_into()
        .map_err(|_| internal("Set import table size"))?;
    Ok(RtFns {
        print: mk("subscript_rt_print", &[I64, I64], None)?,
        collect: mk("subscript_rt_collect", &[I64], None)?,
        alloc: mk("subscript_rt_alloc", &[I64, I64, I32, I32], Some(I64))?,
        globals_init: mk("subscript_rt_globals_init", &[I64, I64, I64], Some(I64))?,
        root_add: mk("subscript_rt_root_add", &[I64, I64, I64], None)?,
        boundary_scratch_mark: mk("subscript_rt_boundary_scratch_mark", &[I64], Some(I64))?,
        boundary_scratch_alloc: mk(
            "subscript_rt_boundary_scratch_alloc",
            &[I64, I64, I32],
            Some(I64),
        )?,
        boundary_scratch_release: mk("subscript_rt_boundary_scratch_release", &[I64, I64], None)?,
        delete: mk("subscript_rt_delete", &[I64, I64, I32], None)?,
        trap: mk("subscript_rt_trap", &[I64, I32, I32], None)?,
        trap_index_out_of_bounds: mk(
            "subscript_rt_trap_index_out_of_bounds",
            &[I64, I32, I32, I32],
            None,
        )?,
        trap_wire_enum: mk(
            "subscript_rt_trap_wire_enum",
            &[I64, I64, I64, I32, I32],
            None,
        )?,
        shadow_push: mk("subscript_rt_shadow_push", &[I64, I64, I64], None)?,
        shadow_pop: mk("subscript_rt_shadow_pop", &[I64], None)?,
        async_kick: mk("subscript_rt_async_kick", &[I64, I64, I64], None)?,
        async_register: mk("subscript_rt_async_register", &[I64, I64], None)?,
        async_retain: mk("subscript_rt_async_retain", &[I64, I64], None)?,
        async_release: mk("subscript_rt_async_release", &[I64, I64, I32], None)?,
        async_retain_array: mk("subscript_rt_async_retain_array", &[I64, I64], None)?,
        async_release_array: mk("subscript_rt_async_release_array", &[I64, I64, I32], None)?,
        async_is_stale: mk("subscript_rt_async_is_stale", &[I64, I64], Some(I8))?,
        async_complete: mk("subscript_rt_async_complete", &[I64, I64, I64, I64], None)?,
        async_result: mk("subscript_rt_async_result", &[I64, I64, I64, I64], Some(I8))?,
        str_lit: mk("subscript_rt_str_lit", &[I64, I64, I64, I32], Some(I64))?,
        str_from_view: mk(
            "subscript_rt_str_from_view",
            &[I64, I64, I64, I32],
            Some(I64),
        )?,
        str_len: mk("subscript_rt_str_len", &[I64, I64], Some(I32))?,
        str_concat: mk("subscript_rt_str_concat", &[I64, I64, I64, I32], Some(I64))?,
        str_eq: mk("subscript_rt_str_eq", &[I64, I64, I64], Some(I32))?,
        fmt_i32: mk("subscript_rt_fmt_i32", &[I64, I32, I32], Some(I64))?,
        fmt_u32: mk("subscript_rt_fmt_u32", &[I64, I32, I32], Some(I64))?,
        fmt_i64: mk("subscript_rt_fmt_i64", &[I64, I64, I32], Some(I64))?,
        fmt_u64: mk("subscript_rt_fmt_u64", &[I64, I64, I32], Some(I64))?,
        fmt_f32: mk("subscript_rt_fmt_f32", &[I64, F32, I32], Some(I64))?,
        fmt_f64: mk("subscript_rt_fmt_f64", &[I64, F64, I32], Some(I64))?,
        fmt_bool: mk("subscript_rt_fmt_bool", &[I64, I32, I32], Some(I64))?,
        f16_from_f64: mk("subscript_rt_f16_from_f64", &[F64], Some(I16))?,
        f16_to_f64: mk("subscript_rt_f16_to_f64", &[I16], Some(F64))?,
        fmod: mk("subscript_rt_fmod", &[I64, F64, F64], Some(F64))?,
        array_new: mk("subscript_rt_array_new", &[I64, I64, I32], Some(I64))?,
        array_from_bytes: mk(
            "subscript_rt_array_from_bytes",
            &[I64, I64, I32, I32],
            Some(I64),
        )?,
        array_byte_range: mk(
            "subscript_rt_array_byte_range",
            &[I64, I64, I32, I32, I32],
            Some(I64),
        )?,
        array_len: mk("subscript_rt_array_len", &[I64, I64], Some(I32))?,
        array_push: mk("subscript_rt_array_push", &[I64, I64, I64, I32], Some(I32))?,
        array_pop: mk("subscript_rt_array_pop", &[I64, I64, I64, I32], None)?,
        str_data: mk("subscript_rt_str_data", &[I64, I64], Some(I64))?,
        array_data: mk("subscript_rt_array_data", &[I64, I64], Some(I64))?,
        assoc_iter_begin: mk("subscript_rt_assoc_iter_begin", &[I64, I64, I32], Some(I64))?,
        assoc_iter_copy: mk(
            "subscript_rt_assoc_iter_copy",
            &[I64, I64, I64, I32, I64, I32],
            Some(I32),
        )?,
        str_iter_code_point: mk(
            "subscript_rt_str_iter_code_point",
            &[I64, I64, I32, I64, I32],
            Some(I64),
        )?,
        array_spread_array: mk(
            "subscript_rt_array_spread_array",
            &[I64, I64, I64, I32],
            None,
        )?,
        array_spread_fixed: mk(
            "subscript_rt_array_spread_fixed",
            &[I64, I64, I64, I64, I32],
            None,
        )?,
        array_spread_assoc: mk(
            "subscript_rt_array_spread_assoc",
            &[I64, I64, I64, I32],
            None,
        )?,
        array_spread_string: mk(
            "subscript_rt_array_spread_string",
            &[I64, I64, I64, I32],
            None,
        )?,
        // (ctx, code, env, userdata1, userdata2) → binding pointer (§14.4:
        // two userdata slots).
        cb_bind: mk(
            "subscript_rt_cb_bind",
            &[I64, I64, I64, I64, I64],
            Some(I64),
        )?,
        // The generic C-ABI callback trampoline (P5.2b, §14.4). Generated
        // code never calls it — a foreign C API does — so it is imported
        // only to take its address (`func_addr`) for a callback-info
        // struct's function-pointer slot; the declared signature (message
        // as two words, then the two userdata slots) is unused.
        cb_trampoline: mk("subscript_rt_cb_trampoline", &[I64, I64, I64, I64], None)?,
        math,
        num,
        // Date intrinsics (stdlib.md §3): opaque symbols on both tiers.
        date_utc: mk(
            "subscript_rt_date_utc",
            &[I64, I32, I32, I32, I32, I32, I32, I32, I32],
            Some(I64),
        )?,
        date_new: mk("subscript_rt_date_new", &[I64, I64, I32], Some(I64))?,
        date_now: mk("subscript_rt_date_now", &[I64], Some(I64))?,
        date_get: mk("subscript_rt_date_get", &[I64, I64, I32], Some(I32))?,
        date_to_iso: mk("subscript_rt_date_to_iso", &[I64, I64, I32], Some(I64))?,
        json,
        str_ops,
        regex_ops,
        arr_ops,
        fixed_arr_ops,
        map_ops,
        set_ops,
        worker_spawn: mk(
            "subscript_rt_worker_spawn",
            &[I64, I64, I64, I64, I64],
            Some(I64),
        )?,
        worker_post: mk("subscript_rt_worker_post", &[I64, I64, I64], Some(I32))?,
        worker_poll: mk("subscript_rt_worker_poll", &[I64, I64], Some(I64))?,
        worker_close: mk("subscript_rt_worker_close", &[I64, I64], None)?,
        worker_join: mk("subscript_rt_worker_join", &[I64, I64], Some(I32))?,
        worker_inbox_wait: mk("subscript_rt_worker_inbox_wait", &[I64, I64], Some(I64))?,
        worker_inbox_poll: mk("subscript_rt_worker_inbox_poll", &[I64, I64], Some(I64))?,
        worker_outbox_post: mk(
            "subscript_rt_worker_outbox_post",
            &[I64, I64, I64],
            Some(I32),
        )?,
    })
}

/// Cranelift settings for the dev tier: absolute-address code plus inline
/// probes. A frame larger than one page must touch the guard page before
/// its first write, or Windows faults on that write.
pub(crate) fn dev_flags() -> Result<cranelift_codegen::settings::Flags, String> {
    let mut fb = cranelift_codegen::settings::builder();
    fb.set("opt_level", "speed")
        .and_then(|()| fb.set("is_pic", "false"))
        .and_then(|()| fb.set("enable_probestack", "true"))
        .and_then(|()| fb.set("probestack_strategy", "inline"))
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
    let free_functions = ml
        .lir
        .functions
        .iter()
        .filter(|function| function.kind == lir::FunctionKind::Free)
        .cloned()
        .collect::<Vec<_>>();
    for function in free_functions {
        ml.reserve_slot(FnKey::Free(function.source_name.clone()));
        if function.is_generator || function.is_async {
            ml.reserve_slot(FnKey::Resume(function.source_name.clone()));
            if function.is_async && function.host_entry_traps.is_some() {
                ml.reserve_slot(FnKey::AsyncExport(function.source_name.clone()));
            }
        } else {
            ml.reserve_slot(FnKey::Wrapper(function.source_name.clone()));
        }
    }
    let classes = ml.lir.classes.clone();
    for class in classes {
        if class.constructor.is_some() {
            ml.reserve_slot(FnKey::Ctor(class.id.0));
        }
        for method in class.methods {
            ml.reserve_slot(FnKey::Method(class.id.0, method.source_name.clone()));
            if ml
                .lir
                .functions
                .get(method.function.0 as usize)
                .is_some_and(|function| function.is_async)
            {
                ml.reserve_slot(FnKey::MethodResume(class.id.0, method.source_name.clone()));
            }
        }
    }
    ml.reserve_slot(FnKey::Init);
}

fn reload_entry_signature(call_conv: CallConv) -> Signature {
    let mut signature = Signature::new(call_conv);
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature
}

fn explicit_parameter_types(function: &lir::Function) -> Result<Vec<Type>, String> {
    function
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == lir::ParameterKind::Explicit)
        .map(|parameter| {
            function
                .values
                .get(parameter.value.0 as usize)
                .and_then(|value| match &value.ty {
                    lir::ValueType::Data(ty) => Some(ty.clone()),
                    lir::ValueType::Address(_) | lir::ValueType::Iterator(_) => None,
                })
                .ok_or_else(|| {
                    internal(format!(
                        "function {} parameter {} has no data type",
                        function.id.0, parameter.value.0
                    ))
                })
        })
        .collect()
}

fn define_reload_entry_adapter<M: Module>(
    ml: &mut ModLower<'_, M>,
    function: &lir::Function,
) -> Result<(), String> {
    let id = ml.func_id(&FnKey::ReloadExport(function.source_name.clone()))?;
    let target = ml.func_id(&FnKey::LirFunction(function.id))?;
    let parameters = function
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == lir::ParameterKind::Explicit)
        .map(|parameter| {
            let ty = function
                .values
                .get(parameter.value.0 as usize)
                .and_then(|value| match &value.ty {
                    lir::ValueType::Data(ty) => Some(ty),
                    lir::ValueType::Address(_) | lir::ValueType::Iterator(_) => None,
                })
                .ok_or_else(|| internal("host entry parameter has no data value type"))?;
            Ok((parameter, ty))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let parameter_types = parameters
        .iter()
        .map(|(_, ty)| match ml.layouts.repr(ty)? {
            Repr::Scalar(repr) => Ok(repr),
            other => Err(internal(format!(
                "host export `{}` has non-scalar parameter representation {other:?}",
                function.source_name
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected_traps = function
        .host_entry_traps
        .as_ref()
        .ok_or_else(|| internal("reload adapter function has no host-entry attachment"))?;
    let mut matched_traps = vec![false; expected_traps.len()];
    let wire_validations = parameters
        .iter()
        .map(|(parameter, ty)| {
            let Type::StringAlias(alias) = ty else {
                return Ok(None);
            };
            let definition = ml
                .lir
                .string_aliases
                .get(alias.0)
                .ok_or_else(|| internal("host entry wire-alias id is out of range"))?;
            let wire_values = definition
                .wire_values
                .clone()
                .ok_or_else(|| internal("host entry string alias has no wire mapping"))?;
            let name_len = i64::try_from(definition.source_name.len())
                .map_err(|_| internal("host entry wire-alias name length does not fit i64"))?;
            let name_data = ml.literal_data(definition.source_name.as_bytes())?;
            let trap_index = expected_traps
                .iter()
                .zip(&matched_traps)
                .position(|(trap, matched)| {
                    !matched
                        && trap.kind == lir::TrapKind::WireEnumValue(*alias)
                        && trap.pos == parameter.pos
                })
                .ok_or_else(|| internal("host entry wire parameter has no LIR trap"))?;
            matched_traps[trap_index] = true;
            let trap = expected_traps[trap_index].clone();
            let pos_id = ml.pos_id(&trap.pos);
            Ok(Some((name_data, name_len, wire_values, pos_id, trap)))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut consumed_traps = Vec::new();
    let mut context = ml.module.make_context();
    context.func.signature = reload_entry_signature(ml.call_conv);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let ctx = builder.block_params(block)[0];
        let values = builder.block_params(block)[1];
        let mut arguments = Vec::with_capacity(parameter_types.len() + 1);
        arguments.push(ctx);
        for (index, (ty, validation)) in parameter_types
            .into_iter()
            .zip(wire_validations)
            .enumerate()
        {
            let offset = i32::try_from(index.checked_mul(8).ok_or_else(|| {
                internal(format!(
                    "host export `{}` argument layout overflows",
                    function.source_name
                ))
            })?)
            .map_err(|_| {
                internal(format!(
                    "host export `{}` argument layout exceeds i32",
                    function.source_name
                ))
            })?;
            let value = builder.ins().load(ty, MemFlags::new(), values, offset);
            if let Some((name_data, name_len, wire_values, pos_id, trap_site)) = validation {
                let mut valid = builder.ins().iconst(types::I8, 0);
                for wire_value in wire_values {
                    let matches =
                        builder
                            .ins()
                            .icmp_imm(IntCC::Equal, value, i64::from(wire_value));
                    valid = builder.ins().bor(valid, matches);
                }
                let accepted = builder.create_block();
                let rejected = builder.create_block();
                builder.ins().brif(valid, accepted, &[], rejected, &[]);
                builder.switch_to_block(rejected);
                let name_global = ml.module.declare_data_in_func(name_data, builder.func);
                let name_pointer = builder.ins().symbol_value(types::I64, name_global);
                let name_len = builder.ins().iconst(types::I64, name_len);
                let pos_id = builder.ins().iconst(types::I32, i64::from(pos_id));
                let trap = ml
                    .module
                    .declare_func_in_func(ml.rt.trap_wire_enum, builder.func);
                builder
                    .ins()
                    .call(trap, &[ctx, name_pointer, name_len, value, pos_id]);
                consumed_traps.push(trap_site);
                builder.ins().return_(&[]);
                builder.switch_to_block(accepted);
            }
            arguments.push(value);
        }
        let target_ref = ml.module.declare_func_in_func(target, builder.func);
        builder.ins().call(target_ref, &arguments);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    ml.module
        .define_function(id, &mut context)
        .map_err(|error| internal(format!("define reload entry adapter: {error}")))?;
    ml.module.clear_context(&mut context);
    func::verify_trap_consumption(function, expected_traps, &consumed_traps)?;
    Ok(())
}

/// Lowers a checked program into `module`.
pub(crate) fn lower_module_with<M: Module>(
    module: &mut M,
    hirm: &HirModule,
    opts: LowerOptions,
) -> Result<Lowered, String> {
    if let Some(import) = hirm.poisoned_imports.first() {
        return Err(format!(
            "cannot lower discovery HIR: poisoned import `{}`",
            import.module
        ));
    }
    let lirm = crate::lir::lower_module(hirm)
        .map_err(|error| internal(format!("LIR construction failed: {error}")))?;
    lower_lir_module_with(module, &lirm, opts)
}

fn lower_lir_module_with<M: Module>(
    module: &mut M,
    lirm: &lir::Module,
    opts: LowerOptions,
) -> Result<Lowered, String> {
    if module.isa().pointer_type() != types::I64 {
        return Err(internal(
            "only 64-bit targets are supported: the runtime ABI assumes 8-byte handles",
        ));
    }
    crate::lir::verify_module(lirm).map_err(|errors| {
        internal(format!(
            "LIR verification failed:\n{}",
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        ))
    })?;
    let call_conv = module.isa().default_call_conv();
    let rt = declare_rt(module, call_conv)?;
    let layouts = Layouts::build_lir(lirm)?;
    let context_globals = opts.reload || !lirm.worker_entries.is_empty();

    let mut ml = ModLower {
        module,
        lir: lirm,
        layouts,
        rt,
        opts,
        fns: HashMap::new(),
        fn_slot: HashMap::new(),
        slots: Vec::new(),
        str_data: HashMap::new(),
        string_alias_tables: HashMap::new(),
        worker_message_descriptors: HashMap::new(),
        globals: HashMap::new(),
        foreign_ids: HashMap::new(),
        foreign_symbols: Vec::new(),
        positions: Vec::new(),
        lambda_count: 0,
        str_count: 0,
        call_conv,
        context_globals,
        globals_size: 0,
        globals_align: 1,
    };

    reserve_slots(&mut ml);

    // Globals: zero-initialized writable module data, filled by the
    // synthesized init function, which also registers managed ones as
    // collection roots. In reload mode the same layout is computed
    // into a host-owned block instead, because module data dies with
    // the module and Context state must outlive a swap (§8.2).
    let mut globals_size = 0u32;
    let mut globals_align = 1u32;
    for (gi, g) in lirm.globals.iter().enumerate() {
        let (size, align) = ml.layouts.size_align(&g.ty)?;
        let (size, align) = (size.max(1), align.max(1));
        let slot = if context_globals {
            globals_align = globals_align.max(align);
            globals_size = round_up_layout(globals_size, align, "Context globals layout")?;
            let at = globals_size;
            globals_size = checked_layout_add(globals_size, size, "Context globals layout")?;
            GlobalSlot::Offset(at)
        } else {
            let name = format!("subscript_g{gi}");
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
        ml.globals
            .insert(g.source_name.clone(), (slot, g.ty.clone()));
    }
    globals_size = round_up_layout(globals_size, globals_align, "final Context globals layout")?;
    ml.globals_size = globals_size;
    ml.globals_align = globals_align;

    // Declare every script function up front so bodies can call in any
    // order. Symbol names are index-based and stable. HIR names can
    // contain `<...>` from monomorphization. Host-callable exports get a
    // stable `subscript_export_<name>` symbol with external linkage.
    let decl = |ml: &mut ModLower<M>, key: FnKey, sym: String, sig: &Signature, export: bool| {
        let linkage = if export {
            Linkage::Export
        } else {
            Linkage::Local
        };
        let id = ml
            .module
            .declare_function(&sym, linkage, sig)
            .map_err(|e| internal(format!("declare {sym}: {e}")))?;
        ml.bind_slot(&key, id);
        ml.fns.insert(key, id);
        Ok::<(), String>(())
    };
    let mut free_index = 0usize;
    for function in &lirm.functions {
        let parameters = explicit_parameter_types(function)?;
        let (key, symbol, resume) = match &function.kind {
            lir::FunctionKind::Free => {
                let index = free_index;
                free_index += 1;
                let exported = function.host_entry_traps.is_some() && !function.is_async;
                let symbol = if exported {
                    format!("subscript_export_{}", function.source_name)
                } else {
                    format!("subscript_f{index}")
                };
                (
                    FnKey::Free(function.source_name.clone()),
                    symbol,
                    Some((
                        FnKey::Resume(function.source_name.clone()),
                        format!("subscript_f{index}_resume"),
                    )),
                )
            }
            lir::FunctionKind::Constructor { class, .. } => (
                FnKey::Ctor(class.0),
                format!("subscript_ctor{}", class.0),
                None,
            ),
            lir::FunctionKind::Method { class, .. } => {
                let method_index = lirm
                    .classes
                    .get(class.0)
                    .and_then(|definition| {
                        definition
                            .methods
                            .iter()
                            .position(|method| method.function == function.id)
                    })
                    .ok_or_else(|| internal("LIR method has no class-table position"))?;
                (
                    FnKey::Method(class.0, function.source_name.clone()),
                    format!("subscript_m{}_{method_index}", class.0),
                    Some((
                        FnKey::MethodResume(class.0, function.source_name.clone()),
                        format!("subscript_m{}_{method_index}_resume", class.0),
                    )),
                )
            }
            lir::FunctionKind::Lambda | lir::FunctionKind::ModuleInitializer => continue,
        };
        let has_receiver = matches!(
            function.kind,
            lir::FunctionKind::Constructor { .. } | lir::FunctionKind::Method { .. }
        );
        let return_type = if function.is_generator || function.is_async {
            Type::Generator(Box::new(Type::Void))
        } else {
            function.return_type.clone()
        };
        let signature = ml.make_sig(&parameters, &return_type, false, has_receiver)?;
        let export = function.host_entry_traps.is_some() && !function.is_async;
        decl(&mut ml, key, symbol, &signature, export)?;
        if function.is_generator || function.is_async {
            let (resume_key, resume_symbol) =
                resume.ok_or_else(|| internal("coroutine function has no resume symbol"))?;
            let resume_signature = ml.resume_sig();
            decl(&mut ml, resume_key, resume_symbol, &resume_signature, false)?;
            if function.is_async && function.host_entry_traps.is_some() {
                let export_signature = ml.make_sig(&[], &Type::Void, false, false)?;
                decl(
                    &mut ml,
                    FnKey::AsyncExport(function.source_name.clone()),
                    format!("subscript_export_{}", function.source_name),
                    &export_signature,
                    true,
                )?;
            }
        } else if opts.reload && function.host_entry_traps.is_some() && !parameters.is_empty() {
            let adapter_signature = reload_entry_signature(call_conv);
            decl(
                &mut ml,
                FnKey::ReloadExport(function.source_name.clone()),
                format!("subscript_reload_export_{}", function.id.0),
                &adapter_signature,
                false,
            )?;
        }
    }
    {
        let sig = ml.make_sig(&[], &Type::Void, false, false)?;
        decl(
            &mut ml,
            FnKey::Init,
            "subscript_init".to_string(),
            &sig,
            true,
        )?;
    }
    if !lirm.worker_entries.is_empty() {
        let sig = ml.make_sig(&[], &Type::Void, false, false)?;
        decl(
            &mut ml,
            FnKey::WorkerInit,
            "subscript_worker_init".to_string(),
            &sig,
            false,
        )?;
        for (index, entry) in lirm.worker_entries.iter().enumerate() {
            let params = [
                Type::Inbox(Box::new(Type::Class(entry.input))),
                Type::Outbox(Box::new(Type::Class(entry.output))),
            ];
            let sig = ml.make_sig(&params, &Type::Void, false, false)?;
            decl(
                &mut ml,
                FnKey::WorkerEntry(index),
                format!("subscript_worker_entry{index}"),
                &sig,
                false,
            )?;
        }
    }
    {
        let sig = ml.make_sig(&[], &Type::Void, false, false)?;
        decl(
            &mut ml,
            FnKey::AsyncRunner,
            "subscript_kick_async_exports".to_string(),
            &sig,
            true,
        )?;
    }

    // Bind every executable LIR id to its already declared target entity.
    // Lambdas have no HIR declaration entity, so declare them here.
    for function in &lirm.functions {
        let target = match &function.kind {
            lir::FunctionKind::Free => FnKey::Free(function.source_name.clone()),
            lir::FunctionKind::Constructor { class, .. } => FnKey::Ctor(class.0),
            lir::FunctionKind::Method { class, .. } => {
                FnKey::Method(class.0, function.source_name.clone())
            }
            lir::FunctionKind::ModuleInitializer => FnKey::Init,
            lir::FunctionKind::Lambda => {
                let parameters = function
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.kind == lir::ParameterKind::Explicit)
                    .map(|parameter| {
                        function
                            .values
                            .get(parameter.value.0 as usize)
                            .and_then(|value| match &value.ty {
                                lir::ValueType::Data(ty) => Some(ty.clone()),
                                _ => None,
                            })
                            .ok_or_else(|| {
                                internal(format!(
                                    "lambda parameter value {} is not data",
                                    parameter.value.0
                                ))
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let signature = ml.make_sig(&parameters, &function.return_type, true, false)?;
                decl(
                    &mut ml,
                    FnKey::LirFunction(function.id),
                    format!("subscript_lir_f{}", function.id.0),
                    &signature,
                    false,
                )?;
                continue;
            }
        };
        ml.alias_function(FnKey::LirFunction(function.id), &target)?;
        if !function.is_generator
            && !function.is_async
            && matches!(function.kind, lir::FunctionKind::Free)
        {
            let parameters = function
                .parameters
                .iter()
                .filter(|parameter| parameter.kind == lir::ParameterKind::Explicit)
                .map(|parameter| {
                    function
                        .values
                        .get(parameter.value.0 as usize)
                        .and_then(|value| match &value.ty {
                            lir::ValueType::Data(ty) => Some(ty.clone()),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            internal(format!(
                                "wrapper parameter value {} is not data",
                                parameter.value.0
                            ))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let signature = ml.make_sig(&parameters, &function.return_type, true, false)?;
            decl(
                &mut ml,
                FnKey::LirWrapper(function.id),
                format!("subscript_lir_wrap{}", function.id.0),
                &signature,
                false,
            )?;
        }
        if function.is_generator || function.is_async {
            let resume = match &function.kind {
                lir::FunctionKind::Method { class, .. } => {
                    FnKey::MethodResume(class.0, function.source_name.clone())
                }
                _ => FnKey::Resume(function.source_name.clone()),
            };
            ml.alias_function(FnKey::LirResume(function.id), &resume)?;
            if function.is_async && function.host_entry_traps.is_some() {
                ml.alias_function(
                    FnKey::LirAsyncExport(function.id),
                    &FnKey::AsyncExport(function.source_name.clone()),
                )?;
            }
        }
    }

    // Define bodies.
    for function in &lirm.functions {
        if matches!(function.kind, lir::FunctionKind::ModuleInitializer) {
            continue;
        }
        if function.is_generator || function.is_async {
            func::define_coroutine(&mut ml, function)?;
            if function.is_async && function.host_entry_traps.is_some() {
                func::define_async_export(&mut ml, function)?;
            }
        } else {
            define_function(&mut ml, function)?;
            if matches!(function.kind, lir::FunctionKind::Free) {
                func::define_wrapper(&mut ml, function)?;
            }
        }
    }
    for function in &lirm.functions {
        if opts.reload
            && function.host_entry_traps.is_some()
            && function
                .parameters
                .iter()
                .any(|parameter| parameter.kind == lir::ParameterKind::Explicit)
        {
            define_reload_entry_adapter(&mut ml, function)?;
        }
    }
    func::define_init(&mut ml)?;
    if !lirm.worker_entries.is_empty() {
        func::define_worker_init(&mut ml)?;
        for (index, entry) in lirm.worker_entries.iter().enumerate() {
            func::define_worker_entry(&mut ml, index, entry)?;
        }
    }
    func::define_async_runner(&mut ml)?;

    let main = match lirm.entry {
        Some(entry) => {
            let function = lirm
                .functions
                .get(entry.0 as usize)
                .filter(|function| function.id == entry)
                .ok_or_else(|| internal("LIR entry function is missing"))?;
            let key = if function.is_async {
                FnKey::LirAsyncExport(entry)
            } else {
                FnKey::LirFunction(entry)
            };
            Some(ml.func_id(&key)?)
        }
        None if opts.require_main => return Err(internal(NO_MAIN_DIAGNOSTIC)),
        None => None,
    };
    let init = ml.func_id(&FnKey::Init)?;
    let mut entries = Vec::new();
    for function in &lirm.functions {
        if function.host_entry_traps.is_some() {
            let parameters = explicit_parameter_types(function)?;
            entries.push(EntryPoint {
                name: function.source_name.clone(),
                id: if function.is_async {
                    ml.func_id(&FnKey::LirAsyncExport(function.id))?
                } else {
                    ml.func_id(&FnKey::LirFunction(function.id))?
                },
                params: parameters
                    .iter()
                    .map(|parameter| match parameter {
                        Type::StringAlias(alias)
                            if lirm
                                .string_aliases
                                .get(alias.0)
                                .is_some_and(|definition| definition.wire_values.is_some()) =>
                        {
                            Type::I32
                        }
                        _ => parameter.clone(),
                    })
                    .collect(),
                reload_adapter: (opts.reload && !parameters.is_empty())
                    .then(|| ml.func_id(&FnKey::ReloadExport(function.source_name.clone())))
                    .transpose()?,
                is_async: function.is_async,
            });
        }
    }
    let positions = std::mem::take(&mut ml.positions);
    let slots = std::mem::take(&mut ml.slots);
    let foreign_symbols = std::mem::take(&mut ml.foreign_symbols);
    Ok(Lowered {
        main,
        init,
        positions,
        entries,
        slots,
        globals_size,
        globals_align,
        foreign_symbols,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        checked_layout_add, checked_layout_mul, dev_flags, lower_lir_module_with,
        lower_module_with, round_up_layout, LowerOptions,
    };
    use cranelift_codegen::settings::ProbestackStrategy;
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::default_libcall_names;
    use subscript_compiler::{check_program, check_program_with, CheckOptions, SourceFile, Type};

    #[test]
    fn dev_cranelift_flags_use_inline_stack_probes() {
        let flags = dev_flags().expect("dev flags");
        assert!(flags.enable_probestack());
        assert_eq!(flags.probestack_strategy(), ProbestackStrategy::Inline);
    }

    #[test]
    fn accumulated_lowering_layouts_report_overflow() {
        assert!(round_up_layout(u32::MAX, 8, "test layout").is_err());
        assert!(checked_layout_add(i32::MAX as u32, 1, "test layout").is_err());
        assert!(checked_layout_mul(i32::MAX as u32, 8, "test layout").is_err());
    }

    #[test]
    fn worker_string_descriptor_offsets_match_the_c_tier_and_class_layout() {
        let source = include_str!("../../../corpus/accept/a182-worker-string-message.ts");
        let hir = check_program(&[SourceFile::new("a182-worker-string-message.ts", source)])
            .expect("a182 checks");
        let lir = crate::lir::lower_module(&hir).expect("a182 lowers to LIR");
        let class = lir
            .classes
            .iter()
            .find(|class| class.source_name == "StringMessage")
            .expect("StringMessage class");
        let layouts = crate::layout::Layouts::build_lir(&lir).expect("a182 layouts");
        let class_layout = layouts.class(class.id.0).expect("StringMessage layout");
        assert_eq!(class_layout.field_offsets, vec![0, 8, 16]);
        assert_eq!(class.fields[0].ty, Type::Str);
        assert_eq!(class.fields[2].ty, Type::FixedArray(Box::new(Type::Str), 2));
        let expected_offsets = vec![0, 16, 24];
        assert_eq!(
            layouts
                .worker_message_string_slot_offsets(class.id.0)
                .expect("StringMessage string offsets"),
            expected_offsets
        );

        let mixed_class = lir
            .classes
            .iter()
            .find(|class| class.source_name == "MixedStringInput")
            .expect("MixedStringInput class");
        let mixed_layout = layouts
            .class(mixed_class.id.0)
            .expect("MixedStringInput layout");
        assert_eq!(mixed_layout.field_offsets, vec![0, 8, 16, 24]);
        assert_eq!(mixed_class.fields[0].ty, Type::U8);
        assert_eq!(mixed_class.fields[1].ty, Type::Str);
        assert_eq!(mixed_class.fields[2].ty, Type::I64);
        assert_eq!(
            mixed_class.fields[3].ty,
            Type::FixedArray(Box::new(Type::Str), 3)
        );
        let mixed_expected_offsets = vec![8, 24, 32, 40];
        assert_eq!(
            layouts
                .worker_message_string_slot_offsets(mixed_class.id.0)
                .expect("MixedStringInput string offsets"),
            mixed_expected_offsets
        );

        let c = crate::cemit::emit_lir_c(&lir, true)
            .expect("a182 C lowering")
            .source;
        assert!(c.contains(&format!(
            "static const uint64_t sub_worker_string_offsets_{}[] = {{ 0ull, 16ull, 24ull }};",
            class.id.0
        )));
        assert!(c.contains(&format!(
            "static const subscript_rt_worker_message_descriptor sub_worker_message_descriptor_{} = {{ (uint64_t)sizeof(SubC{}), 3ull, sub_worker_string_offsets_{} }};",
            class.id.0, class.id.0, class.id.0
        )));
        assert!(c.contains(&format!("&sub_worker_message_descriptor_{}", class.id.0)));
        assert!(c.contains(&format!(
            "static const uint64_t sub_worker_string_offsets_{}[] = {{ 8ull, 24ull, 32ull, 40ull }};",
            mixed_class.id.0
        )));
        assert!(c.contains(&format!(
            "static const subscript_rt_worker_message_descriptor sub_worker_message_descriptor_{} = {{ (uint64_t)sizeof(SubC{}), 4ull, sub_worker_string_offsets_{} }};",
            mixed_class.id.0, mixed_class.id.0, mixed_class.id.0
        )));
        assert!(c.contains(&format!(
            "&sub_worker_message_descriptor_{}",
            mixed_class.id.0
        )));
    }

    #[test]
    fn dev_lowering_rejects_a_discovery_hir() {
        let source = "import { A_SIZE } from \"./p.typegpu\";\n\
                      export function main(): void { const size: i32 = A_SIZE; }\n";
        let mut options = CheckOptions::default();
        options.poison_missing_modules = vec!["./p.typegpu".to_string()];
        let hir = check_program_with(&[SourceFile::new("main.ts", source)], &options)
            .expect("discovery check");

        let isa = cranelift_native::builder()
            .expect("host ISA")
            .finish(dev_flags().expect("dev flags"))
            .expect("ISA flags");
        let builder = JITBuilder::with_isa(isa, default_libcall_names());
        let mut module = JITModule::new(builder);
        let result = lower_module_with(&mut module, &hir, LowerOptions::default());

        // SAFETY: the discovery guard returned before the module received code or data.
        unsafe { module.free_memory() };
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("discovery HIR must not lower"),
        };

        assert_eq!(
            error,
            "cannot lower discovery HIR: poisoned import `./p.typegpu`"
        );
    }

    #[test]
    fn cranelift_json_guard_reads_the_lir_field_id() {
        let hir = check_program(&[SourceFile::new(
            "json-field-id.ts",
            "export function main(): void {\n  const result: JsonResult<i32> = JSON.parse<i32>(\"1\");\n  print(`${result.value}`);\n}\n",
        )])
        .expect("JSON field-id source checks");
        let mut lir = crate::lir::lower_module(&hir).expect("JSON field-id source lowers");
        let ok_field = lir
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .flat_map(|instruction| &instruction.traps)
            .find_map(|trap| match trap.kind {
                subscript_compiler::lir::TrapKind::JsonResultValue(field) => Some(field),
                _ => None,
            })
            .expect("JSON value load names its ok field");
        lir.classes
            .iter_mut()
            .flat_map(|class| &mut class.fields)
            .find(|field| field.id == ok_field)
            .expect("JSON ok field exists")
            .source_name = "not_ok".to_string();

        let isa = cranelift_native::builder()
            .expect("host ISA")
            .finish(dev_flags().expect("dev flags"))
            .expect("ISA flags");
        let builder = JITBuilder::with_isa(isa, default_libcall_names());
        let mut module = JITModule::new(builder);
        lower_lir_module_with(&mut module, &lir, LowerOptions::default())
            .expect("Cranelift locates the guard field by LIR id");

        // SAFETY: no finalized function address escapes this test.
        unsafe { module.free_memory() };
    }
}
