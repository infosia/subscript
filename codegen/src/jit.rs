//! The dev-tier JIT driver: instantiates the tier-neutral lowering
//! with `cranelift-jit`, resolves the runtime's `extern "C"` symbols,
//! runs the exported `main(): void`, and returns the captured stdout
//! bytes or a trap report.

use cranelift_jit::{JITBuilder, JITModule};
use subscript_compiler::{check_program, Diagnostic, Pos, SourceFile};
use subscript_runtime::{ffi, Context, TrapKind};

use crate::lower::{dev_flags, internal, lower_module_with, LowerOptions};

/// A runtime fault that stopped the script (collisions.md C6). The
/// host process survives; this is the report the Context recorded.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TrapReport {
    /// The violated rule.
    pub rule: TrapKind,
    /// Human-readable detail.
    pub message: String,
    /// TS position of the faulting construct (from the position table
    /// the compiler embeds).
    pub pos: Pos,
}

impl std::fmt::Display for TrapReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: trap [{}]: {}", self.pos, self.rule, self.message)
    }
}

/// Why a run produced no output.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunError {
    /// The program was rejected by the checker (P1 diagnostics).
    Rejected(Vec<Diagnostic>),
    /// The program ran and trapped.
    Trap(TrapReport),
    /// An internal lowering/backend failure (a bug, not a user error).
    Internal(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Rejected(diags) => {
                write!(f, "rejected with {} diagnostic(s)", diags.len())?;
                for d in diags {
                    write!(f, "\n  {d}")?;
                }
                Ok(())
            }
            RunError::Trap(t) => write!(f, "{t}"),
            RunError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Registers every runtime symbol the lowering imports.
///
/// The dev tier binds them by address; the ship tier resolves the same
/// names from the runtime static library at link time, so this list and
/// the lowering's imports must stay in step with `runtime::ffi`.
pub(crate) fn register_runtime(builder: &mut JITBuilder) {
    let syms: &[(&str, *const u8)] = &[
        ("sub_rt_print", ffi::sub_rt_print as *const u8),
        ("sub_rt_collect", ffi::sub_rt_collect as *const u8),
        ("sub_rt_alloc", ffi::sub_rt_alloc as *const u8),
        ("sub_rt_delete", ffi::sub_rt_delete as *const u8),
        ("sub_rt_trap", ffi::sub_rt_trap as *const u8),
        ("sub_rt_root_add", ffi::sub_rt_root_add as *const u8),
        ("sub_rt_shadow_push", ffi::sub_rt_shadow_push as *const u8),
        ("sub_rt_shadow_pop", ffi::sub_rt_shadow_pop as *const u8),
        ("sub_rt_str_lit", ffi::sub_rt_str_lit as *const u8),
        ("sub_rt_str_len", ffi::sub_rt_str_len as *const u8),
        ("sub_rt_str_concat", ffi::sub_rt_str_concat as *const u8),
        ("sub_rt_str_slice", ffi::sub_rt_str_slice as *const u8),
        ("sub_rt_str_eq", ffi::sub_rt_str_eq as *const u8),
        ("sub_rt_fmt_i32", ffi::sub_rt_fmt_i32 as *const u8),
        ("sub_rt_fmt_u32", ffi::sub_rt_fmt_u32 as *const u8),
        ("sub_rt_fmt_i64", ffi::sub_rt_fmt_i64 as *const u8),
        ("sub_rt_fmt_u64", ffi::sub_rt_fmt_u64 as *const u8),
        ("sub_rt_fmt_f32", ffi::sub_rt_fmt_f32 as *const u8),
        ("sub_rt_fmt_f64", ffi::sub_rt_fmt_f64 as *const u8),
        ("sub_rt_fmt_bool", ffi::sub_rt_fmt_bool as *const u8),
        ("sub_rt_array_new", ffi::sub_rt_array_new as *const u8),
        ("sub_rt_array_len", ffi::sub_rt_array_len as *const u8),
        ("sub_rt_array_push", ffi::sub_rt_array_push as *const u8),
        ("sub_rt_array_pop", ffi::sub_rt_array_pop as *const u8),
        ("sub_rt_array_ptr", ffi::sub_rt_array_ptr as *const u8),
    ];
    for (name, addr) in syms {
        builder.symbol(*name, *addr);
    }
}

/// Checks `files`, lowers the typed HIR through the shared CLIF
/// lowering, executes the exported `main(): void` under the dev JIT,
/// and returns the exact stdout bytes the run produced.
///
/// # Errors
///
/// [`RunError::Rejected`] when the checker rejects the program,
/// [`RunError::Trap`] when the run trapped (rule + message + TS
/// position), [`RunError::Internal`] on backend failures.
pub fn run_jit(files: &[SourceFile]) -> Result<Vec<u8>, RunError> {
    let hir = check_program(files).map_err(RunError::Rejected)?;

    let flags = dev_flags().map_err(RunError::Internal)?;
    let isa = cranelift_native::builder()
        .map_err(|e| RunError::Internal(internal(format!("host ISA: {e}"))))
        .and_then(|b| {
            b.finish(flags)
                .map_err(|e| RunError::Internal(internal(format!("ISA flags: {e}"))))
        })?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    register_runtime(&mut builder);
    let mut module = JITModule::new(builder);

    let lowered = lower_module_with(&mut module, &hir, LowerOptions::default())
        .map_err(RunError::Internal)?;
    module
        .finalize_definitions()
        .map_err(|e| RunError::Internal(internal(format!("finalize: {e}"))))?;

    let init_ptr = module.get_finalized_function(lowered.init);
    let main_ptr = module.get_finalized_function(lowered.main);

    let mut ctx = Context::new();
    {
        type Entry = unsafe extern "C" fn(*mut Context);
        // SAFETY: `init_ptr`/`main_ptr` are finalized JIT code for
        // functions the lowering built with exactly this signature
        // (`(ctx) -> void`, host C calling convention); the module
        // outlives both calls; `ctx` is a live exclusive Context.
        // Generated code never unwinds (traps return through the
        // flag-check paths), so no panic crosses this boundary.
        unsafe {
            let init: Entry = std::mem::transmute(init_ptr);
            init(&mut *ctx);
            if !ctx.trapped() {
                let main: Entry = std::mem::transmute(main_ptr);
                main(&mut *ctx);
            }
        }
    }

    let outcome = match ctx.trap_record() {
        Some(r) => {
            let pos = lowered
                .positions
                .get(r.pos_id as usize)
                .cloned()
                .unwrap_or_else(|| Pos::new(String::new(), 0, 0));
            Err(RunError::Trap(TrapReport {
                rule: r.kind,
                message: r.message.clone(),
                pos,
            }))
        }
        None => Ok(ctx.take_stdout()),
    };
    drop(ctx);
    // SAFETY: all executions above have returned and no pointer into
    // the JIT-allocated code/data survives (the Context held none).
    unsafe { module.free_memory() };
    outcome
}
