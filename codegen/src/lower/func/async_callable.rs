//! Started handle producers for async function values (§167).
use super::*;

pub(crate) fn define_async_callable<M: Module>(
    ml: &mut ModLower<'_, M>,
    function: &l::Function,
) -> Result<(), String> {
    let id = ml.func_id(&FnKey::LirWrapper(function.id))?;
    let creator = ml.func_id(&FnKey::LirFunction(function.id))?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let incoming = builder.block_params(entry).to_vec();
        let ctx = incoming[0];
        let mut arguments = vec![ctx];
        if function
            .parameters
            .iter()
            .any(|parameter| parameter.kind == l::ParameterKind::OwnedEnvironment)
        {
            arguments.push(incoming[1]);
        }
        arguments.extend_from_slice(&incoming[2..incoming.len() - 1]);
        let pos_id = *incoming
            .last()
            .ok_or_else(|| internal("async callable has no call position"))?;
        let call = if ml.opts.reload && matches!(function.kind, l::FunctionKind::Free) {
            let slot = ml.slot_of(&FnKey::LirFunction(function.id))?;
            let offset = i32::try_from(u64::from(slot) * 8)
                .map_err(|_| internal("async callable slot exceeds i32"))?;
            let table = builder.ins().load(
                types::I64,
                flags(),
                ctx,
                ctx_off(rtc::Context::fn_table_offset())?,
            );
            let code = builder.ins().load(types::I64, flags(), table, offset);
            let signature = builder.import_signature(ml.signature_of(creator));
            builder.ins().call_indirect(signature, code, &arguments)
        } else {
            let reference = ml.module.declare_func_in_func(creator, builder.func);
            builder.ins().call(reference, &arguments)
        };
        let handle = builder.inst_results(call)[0];
        let finish = builder.create_block();
        let start = builder.create_block();
        let trap = builder.ins().load(types::I32, flags(), ctx, 0);
        let clear = builder.ins().icmp_imm(IntCC::Equal, trap, 0);
        builder.ins().brif(clear, start, &[], finish, &[]);
        builder.switch_to_block(start);
        let (output, size) = if function.return_type == Type::Void {
            (builder.ins().iconst(types::I64, 0), 0)
        } else {
            let (size, align) = ml.layouts.size_align(&function.return_type)?;
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size.max(1),
                align.max(1).trailing_zeros() as u8,
            ));
            (builder.ins().stack_addr(types::I64, slot, 0), size)
        };
        let start = ml
            .module
            .declare_func_in_func(ml.rt.async_start, builder.func);
        let call = builder.ins().call(start, &[ctx, handle, output, pos_id]);
        let done = builder.inst_results(call)[0];
        let trap = builder.ins().load(types::I32, flags(), ctx, 0);
        let clear = builder.ins().icmp_imm(IntCC::Equal, trap, 0);
        let completed = builder.ins().band(done, clear);
        let complete = builder.create_block();
        builder.ins().brif(completed, complete, &[], finish, &[]);
        builder.switch_to_block(complete);
        let size = builder.ins().iconst(types::I64, i64::from(size));
        let complete = ml
            .module
            .declare_func_in_func(ml.rt.async_complete, builder.func);
        builder.ins().call(complete, &[ctx, handle, output, size]);
        builder.ins().jump(finish, &[]);
        builder.switch_to_block(finish);
        builder.ins().return_(&[handle]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(
        ml,
        id,
        &mut context,
        &format!("async callable {}", function.id.0),
    )
}
