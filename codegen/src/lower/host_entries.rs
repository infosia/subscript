use super::*;

/// Assigns an indirection-table slot to every user function the module
/// declares, in declaration order and *only* from declarations, so
/// that a recompile with an unchanged declaration hash produces the
/// same slot for the same function (§8.2). Slots are reserved for env
/// wrappers too, whether or not the program uses the function as a
/// value: wrapper creation is body-driven and must not shift the
/// numbering. Synthesized helpers have no slot (compiler.md §119).
pub(super) fn reserve_slots<M: Module>(ml: &mut ModLower<'_, M>) {
    let free_functions = ml
        .lir
        .functions
        .iter()
        .filter(|function| function.kind == lir::FunctionKind::Free)
        .cloned()
        .collect::<Vec<_>>();
    for function in free_functions {
        ml.reserve_slot(FnKey::Free(function.id));
        if function.is_generator || function.is_async {
            ml.reserve_slot(FnKey::Resume(function.id));
            // An alias target can change without a declaration change (§129).
            // Reserve the optional adapter slot from the declaration signature.
            if function.is_async
                && function.parameters.is_empty()
                && function.return_type == Type::Void
            {
                ml.reserve_slot(FnKey::AsyncExport(function.id));
            }
        } else {
            ml.reserve_slot(FnKey::Wrapper(function.id));
        }
    }
    let classes = ml.lir.classes.clone();
    for class in classes {
        if let Some(constructor) = &class.constructor {
            ml.reserve_slot(FnKey::Ctor(constructor.function));
        }
        for method in class.methods {
            ml.reserve_slot(FnKey::Method(method.function));
            if ml
                .lir
                .functions
                .get(method.function.0 as usize)
                .is_some_and(|function| function.is_async)
            {
                ml.reserve_slot(FnKey::MethodResume(method.function));
            }
        }
    }
    ml.reserve_slot(FnKey::Init);
}

pub(super) fn reload_entry_signature(call_conv: CallConv) -> Signature {
    let mut signature = Signature::new(call_conv);
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature
}

pub(super) fn explicit_parameter_types(function: &lir::Function) -> Result<Vec<Type>, String> {
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

pub(super) fn define_reload_entry_adapter<M: Module>(
    ml: &mut ModLower<'_, M>,
    function: &lir::Function,
) -> Result<(), String> {
    let id = ml.func_id(&FnKey::ReloadExport(function.id))?;
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

/// Defines a distinct public symbol that calls the shared implementation.
pub(super) fn define_host_alias<M: Module>(
    ml: &mut ModLower<'_, M>,
    entry: &lir::HostEntry,
    index: usize,
) -> Result<(), String> {
    let id = ml.func_id(&FnKey::HostAlias(index))?;
    let target_key = if entry.signature.is_async {
        FnKey::LirAsyncExport(entry.target)
    } else {
        FnKey::LirFunction(entry.target)
    };
    let target = ml.func_id(&target_key)?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.make_sig(&entry.signature.parameters, &Type::Void, false, false)?;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let arguments = builder.block_params(block).to_vec();
        let target_ref = ml.module.declare_func_in_func(target, builder.func);
        builder.ins().call(target_ref, &arguments);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    ml.module
        .define_function(id, &mut context)
        .map_err(|error| internal(format!("define host alias: {error}")))?;
    ml.module.clear_context(&mut context);
    Ok(())
}
