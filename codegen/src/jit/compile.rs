//! Compilation of one dev-tier module and the call into a finalized entry.

use cranelift_jit::{JITBuilder, JITModule};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::Context;

use super::memory::install_reservation;
use super::symbols::register_runtime;
use super::RunError;
use crate::lower::{dev_flags, internal, lower_module_with, LowerOptions, Lowered};
use crate::native::{missing_symbol, register_symbols};
use crate::NativeLibrary;

/// Checks `files`, lowers the typed HIR through the shared CLIF
/// lowering, and finalizes the code in a live JIT module.
///
/// Every stage runs on the thread that calls this function (§114.2
/// rule 1).
pub(super) fn compile_jit(
    files: &[SourceFile],
    libraries: &[NativeLibrary],
) -> Result<(JITModule, Lowered), RunError> {
    let hir = check_program(files).map_err(RunError::Rejected)?;

    let flags = dev_flags().map_err(RunError::Internal)?;
    let isa = cranelift_native::builder()
        .map_err(|e| RunError::Internal(internal(format!("host ISA: {e}"))))
        .and_then(|b| {
            b.finish(flags)
                .map_err(|e| RunError::Internal(internal(format!("ISA flags: {e}"))))
        })?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    // §110 rule 1: one module holds its code and its data in one
    // reservation, so the module's own size bounds every displacement
    // a non-PIC lowering emits.
    install_reservation(&mut builder, hir.source_bytes).map_err(RunError::Internal)?;
    register_runtime(&mut builder);
    register_symbols(&mut builder, libraries);
    let mut module = JITModule::new(builder);

    let lowered = lower_module_with(&mut module, &hir, LowerOptions::default())
        .map_err(RunError::Internal)?;
    if let Some(name) = missing_symbol(&lowered.foreign_symbols, libraries) {
        // Cranelift-JIT retains a platform symbol-lookup fallback and
        // exposes only an API for appending more lookup functions. The
        // lowering's list is total: its sole `Callee::Foreign` import path
        // records every imported name. Refuse finalization here so the
        // fallback is never consulted for an unregistered foreign symbol.
        // SAFETY: no definition was finalized or executed and no pointer
        // into this module escaped.
        unsafe { module.free_memory() };
        return Err(RunError::UnresolvedForeignSymbol(name.to_string()));
    }
    module
        .finalize_definitions()
        .map_err(|e| RunError::Internal(internal(format!("finalize: {e}"))))?;
    Ok((module, lowered))
}

/// Calls one finalized `(subscript_rt_context*) -> void` script entry under the host
/// depth discipline.
///
/// # Safety
///
/// `entry` must be finalized generated code of this exact signature and
/// remain live for the call.
pub(super) unsafe fn call_script_entry(entry: *const u8, ctx: &mut Context) {
    type Entry = unsafe extern "C" fn(*mut Context);
    // SAFETY: guaranteed by the caller.
    let entry: Entry = unsafe { std::mem::transmute(entry) };
    ctx.enter_script();
    // SAFETY: generated code never unwinds across this C boundary.
    unsafe { entry(ctx) };
    ctx.exit_script();
}
