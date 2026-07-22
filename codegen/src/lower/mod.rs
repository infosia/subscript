//! HIR-to-CLIF lowering, tier-neutral (`specs/blocks/compiler.md` §1).
//!
//! The lowering targets the [`cranelift_module::Module`] trait, never
//! a concrete backend: the dev JIT instantiates it with `JITModule`
//! and P3's AOT path instantiates it with `ObjectModule`. Nothing in
//! here embeds runtime addresses or other JIT-only shortcuts — string
//! literals are module data, globals are module data, and the runtime
//! is reached through imported `extern "C"` symbols.
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
    pub str_slice: FuncId,
    pub str_eq: FuncId,
    pub fmt_i32: FuncId,
    pub fmt_u32: FuncId,
    pub fmt_i64: FuncId,
    pub fmt_u64: FuncId,
    pub fmt_f32: FuncId,
    pub fmt_f64: FuncId,
    pub fmt_bool: FuncId,
    pub array_new: FuncId,
    pub array_len: FuncId,
    pub array_push: FuncId,
    pub array_pop: FuncId,
    pub array_ptr: FuncId,
}

/// Result of lowering a whole program.
pub(crate) struct Lowered {
    /// The exported `main(): void` entry.
    pub main: FuncId,
    /// The synthesized global initializer; run before `main`.
    pub init: FuncId,
    /// Trap position table: `pos_id` -> TS position.
    pub positions: Vec<Pos>,
}

/// Shared lowering state across all functions of one module.
pub(crate) struct ModLower<'a, M: Module> {
    pub module: &'a mut M,
    pub hir: &'a hir::Module,
    pub layouts: Layouts,
    pub rt: RtFns,
    pub fns: HashMap<FnKey, FuncId>,
    pub fn_index: HashMap<String, usize>,
    pub str_data: HashMap<Vec<u8>, DataId>,
    pub globals: HashMap<String, (DataId, Type)>,
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
    ) -> Signature {
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ctx
        if has_env {
            sig.params.push(AbiParam::new(types::I64));
        }
        let ret_repr = self.layouts.repr(ret);
        if matches!(ret_repr, Repr::Agg { .. }) {
            sig.params.push(AbiParam::new(types::I64)); // sret
        }
        if has_this {
            sig.params.push(AbiParam::new(types::I64));
        }
        for p in params {
            match self.layouts.repr(p) {
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
        sig
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
    use types::{F32, F64, I32, I64};
    Ok(RtFns {
        print: mk("sub_rt_print", &[I64, I64], None)?,
        collect: mk("sub_rt_collect", &[I64], None)?,
        alloc: mk("sub_rt_alloc", &[I64, I64, I32, I32], Some(I64))?,
        delete: mk("sub_rt_delete", &[I64, I64, I32], None)?,
        trap: mk("sub_rt_trap", &[I64, I32, I32], None)?,
        root_add: mk("sub_rt_root_add", &[I64, I64], None)?,
        shadow_push: mk("sub_rt_shadow_push", &[I64, I64, I64], None)?,
        shadow_pop: mk("sub_rt_shadow_pop", &[I64], None)?,
        str_lit: mk("sub_rt_str_lit", &[I64, I64, I64, I32], Some(I64))?,
        str_len: mk("sub_rt_str_len", &[I64, I64], Some(I32))?,
        str_concat: mk("sub_rt_str_concat", &[I64, I64, I64, I32], Some(I64))?,
        str_slice: mk("sub_rt_str_slice", &[I64, I64, I32, I32, I32], Some(I64))?,
        str_eq: mk("sub_rt_str_eq", &[I64, I64, I64], Some(I32))?,
        fmt_i32: mk("sub_rt_fmt_i32", &[I64, I32, I32], Some(I64))?,
        fmt_u32: mk("sub_rt_fmt_u32", &[I64, I32, I32], Some(I64))?,
        fmt_i64: mk("sub_rt_fmt_i64", &[I64, I64, I32], Some(I64))?,
        fmt_u64: mk("sub_rt_fmt_u64", &[I64, I64, I32], Some(I64))?,
        fmt_f32: mk("sub_rt_fmt_f32", &[I64, F32, I32], Some(I64))?,
        fmt_f64: mk("sub_rt_fmt_f64", &[I64, F64, I32], Some(I64))?,
        fmt_bool: mk("sub_rt_fmt_bool", &[I64, I32, I32], Some(I64))?,
        array_new: mk("sub_rt_array_new", &[I64, I64, I32], Some(I64))?,
        array_len: mk("sub_rt_array_len", &[I64, I64], Some(I32))?,
        array_push: mk("sub_rt_array_push", &[I64, I64, I64, I32], Some(I32))?,
        array_pop: mk("sub_rt_array_pop", &[I64, I64, I64, I32], None)?,
        array_ptr: mk("sub_rt_array_ptr", &[I64, I64, I32, I32], Some(I64))?,
    })
}

/// Cranelift settings for the dev tier.
pub(crate) fn dev_flags() -> Result<cranelift_codegen::settings::Flags, String> {
    let mut fb = cranelift_codegen::settings::builder();
    fb.set("opt_level", "speed")
        .and_then(|()| fb.set("is_pic", "false"))
        .map_err(|e| internal(format!("settings: {e}")))?;
    Ok(cranelift_codegen::settings::Flags::new(fb))
}

/// Lowers a checked program into `module`.
pub(crate) fn lower_module<M: Module>(
    module: &mut M,
    hirm: &hir::Module,
) -> Result<Lowered, String> {
    if module.isa().pointer_type() != types::I64 {
        return Err(internal("only 64-bit hosts are supported in the dev tier"));
    }
    let call_conv = module.isa().default_call_conv();
    let rt = declare_rt(module, call_conv)?;
    let layouts = Layouts::build(hirm);

    let mut ml = ModLower {
        module,
        hir: hirm,
        layouts,
        rt,
        fns: HashMap::new(),
        fn_index: HashMap::new(),
        str_data: HashMap::new(),
        globals: HashMap::new(),
        positions: Vec::new(),
        lambda_count: 0,
        str_count: 0,
        call_conv,
    };

    for (i, f) in hirm.functions.iter().enumerate() {
        ml.fn_index.insert(f.name.clone(), i);
    }

    // Globals: zero-initialized writable module data; the synthesized
    // init function fills them and registers managed ones as roots.
    for (gi, g) in hirm.globals.iter().enumerate() {
        let (size, align) = ml.layouts.size_align(&g.ty);
        let (size, align) = (size.max(1), align.max(1));
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
        ml.globals.insert(g.name.clone(), (id, g.ty.clone()));
    }

    // Declare every script function up front so bodies can call in any
    // order. Symbol names are index-based (stable and linker-clean for
    // the AOT tier; HIR names may contain `<...>` from monomorphization).
    let decl = |ml: &mut ModLower<M>, key: FnKey, sym: String, sig: &Signature| {
        let id = ml
            .module
            .declare_function(&sym, Linkage::Local, sig)
            .map_err(|e| internal(format!("declare {sym}: {e}")))?;
        ml.fns.insert(key, id);
        Ok::<(), String>(())
    };
    for (i, f) in hirm.functions.iter().enumerate() {
        let params: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
        if f.is_generator {
            let sig = ml.make_sig(&params, &Type::Generator(Box::new(Type::Void)), false, false);
            decl(&mut ml, FnKey::Free(f.name.clone()), format!("ss_f{i}"), &sig)?;
            let rsig = ml.resume_sig();
            decl(&mut ml, FnKey::Resume(f.name.clone()), format!("ss_f{i}_resume"), &rsig)?;
        } else {
            let sig = ml.make_sig(&params, &f.ret, false, false);
            decl(&mut ml, FnKey::Free(f.name.clone()), format!("ss_f{i}"), &sig)?;
        }
    }
    for (ci, c) in hirm.classes.iter().enumerate() {
        if let Some(ctor) = &c.ctor {
            let params: Vec<Type> = ctor.params.iter().map(|p| p.ty.clone()).collect();
            let sig = ml.make_sig(&params, &Type::Void, false, true);
            decl(&mut ml, FnKey::Ctor(ci), format!("ss_ctor{ci}"), &sig)?;
        }
        for (mi, m) in c.methods.iter().enumerate() {
            let params: Vec<Type> = m.params.iter().map(|p| p.ty.clone()).collect();
            let sig = ml.make_sig(&params, &m.ret, false, true);
            decl(
                &mut ml,
                FnKey::Method(ci, m.name.clone()),
                format!("ss_m{ci}_{mi}"),
                &sig,
            )?;
        }
    }
    {
        let sig = ml.make_sig(&[], &Type::Void, false, false);
        decl(&mut ml, FnKey::Init, "ss_init".to_string(), &sig)?;
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
    let positions = std::mem::take(&mut ml.positions);
    Ok(Lowered {
        main,
        init,
        positions,
    })
}
