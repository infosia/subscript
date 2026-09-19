//! Context probes the crate's own tests drive over finalized dev-tier
//! code. Each one runs the entries directly, so a test reads the Context
//! the run left behind.

use subscript_compiler::{Profile, SourceFile};
use subscript_runtime::{ffi, Context};

use super::compile::{call_script_entry, compile_jit};
use super::entry::{execute_entry, memory_accounting, EntryOptions};
use super::RunError;
use crate::lower::internal;
use crate::position_table::PositionTable;

pub(crate) fn memory_accounting_after_run(
    files: &[SourceFile],
) -> Result<(u64, u64, u64), RunError> {
    let (module, lowered, _) = compile_jit(files, &[], Profile::Default)?;
    let result = execute_entry(&module, &lowered, EntryOptions::default(), None)
        .run
        .map(|run| {
            let p: *const Context = &*run.ctx;
            let accounting = memory_accounting(&run.ctx);
            // SAFETY: shared host accessors over a live Context after every
            // script entry returned.
            unsafe {
                (
                    ffi::subscript_rt_ctx_live_allocations(p),
                    accounting.live_bytes,
                    accounting.reserved_bytes,
                )
            }
        });
    // SAFETY: all entries returned and no code pointer survives.
    unsafe { module.free_memory() };
    result
}

pub(crate) fn live_allocations_after_main_calls(
    files: &[SourceFile],
    calls: usize,
) -> Result<Vec<u64>, RunError> {
    let (module, lowered, _) = compile_jit(files, &[], Profile::Default)?;
    let init = module.get_finalized_function(lowered.init);
    let main = module.get_finalized_function(lowered.main_id().map_err(RunError::Internal)?);
    let mut ctx = Context::new();
    let result = (|| {
        // SAFETY: both finalized entries use the `(ctx) -> void` host ABI.
        unsafe { call_script_entry(init, &mut ctx) };
        let mut counts = Vec::with_capacity(calls);
        for _ in 0..calls {
            if ctx.trapped() {
                let trap = ctx
                    .trap_record()
                    .map(|record| {
                        // §112 rules 1 and 3: every recorded id reads the
                        // table, and id 0 is its reserved entry. An id
                        // the table does not hold keeps this probe's
                        // rendering, which names the recorded id.
                        let position = lowered
                            .positions
                            .get(record.pos_id)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format!("position {}", record.pos_id));
                        format!("{:?} at {position}: {}", record.kind, record.message)
                    })
                    .unwrap_or_else(|| "unknown trap".to_string());
                return Err(RunError::Internal(internal(format!(
                    "allocation probe trapped before all calls completed: {trap}"
                ))));
            }
            // SAFETY: the finalized main entry and Context remain live.
            unsafe { call_script_entry(main, &mut ctx) };
            counts.push(ctx.live_count() as u64);
        }
        Ok(counts)
    })();
    // SAFETY: every generated-code call returned and no code pointer survives.
    unsafe { module.free_memory() };
    result
}

type AllocationAttribution = (Vec<(u32, u32, u64)>, PositionTable);

pub(crate) fn allocation_attribution_after_run(
    files: &[SourceFile],
) -> Result<AllocationAttribution, RunError> {
    unsafe extern "C" fn collect(
        userdata: *mut std::ffi::c_void,
        class_id: u32,
        pos_id: u32,
        payload_bytes: u64,
    ) {
        // SAFETY: this helper passes a live Vec of this exact type.
        let triples = unsafe { &mut *userdata.cast::<Vec<(u32, u32, u64)>>() };
        triples.push((class_id, pos_id, payload_bytes));
    }

    let (module, lowered, _) = compile_jit(files, &[], Profile::Default)?;
    let init = module.get_finalized_function(lowered.init);
    let main_id = lowered.main_id().map_err(RunError::Internal)?;
    let main = module.get_finalized_function(main_id);
    let mut ctx = Context::new();
    // SAFETY: both pointers are finalized entries and the module remains
    // live through the calls.
    unsafe {
        call_script_entry(init, &mut ctx);
        if !ctx.trapped() {
            call_script_entry(main, &mut ctx);
        }
    }
    let result = match ctx.trap_record() {
        Some(record) => Err(RunError::Internal(internal(format!(
            "attribution probe trapped: {}",
            record.message
        )))),
        None => {
            let mut triples = Vec::new();
            let p: *const Context = &*ctx;
            // SAFETY: shared host inspection after every script entry
            // returned; callback userdata is a live Vec.
            let visited = unsafe {
                ffi::subscript_rt_ctx_visit_live_allocations(
                    p,
                    Some(collect),
                    (&mut triples as *mut Vec<(u32, u32, u64)>).cast(),
                )
            };
            if visited as usize != triples.len() {
                Err(RunError::Internal(internal(
                    "allocation visitor count differs from callbacks",
                )))
            } else {
                triples.sort_unstable();
                Ok((triples, lowered.positions.clone()))
            }
        }
    };
    // SAFETY: all entries and callbacks returned; no code pointer survives.
    unsafe { module.free_memory() };
    result
}
