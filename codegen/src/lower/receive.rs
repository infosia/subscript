//! Shared layouts and received-element records (compiler.md §325 rules 2 and 8).
use super::internal;
use crate::layout::{round_up_layout, Layouts};
use subscript_compiler::crossing::ReceivePlan;
use subscript_compiler::{lir as l, ClassId, Type};

pub(crate) fn c_type(module: &l::Module, ty: &Type) -> Result<(u32, u32), String> {
    if let Some(kind) = subscript_compiler::types::boundary_kind(ty) {
        return Ok((kind.size, kind.align));
    }
    match ty {
        Type::Str | Type::Array(_) => Ok((16, 8)),
        Type::Class(class)
            if module
                .classes
                .get(class.0)
                .is_some_and(|class| class.is_value) =>
        {
            let (_, size, align) = c_class(module, *class)?;
            Ok((size, align))
        }
        Type::FixedArray(element, length) => {
            let (size, align) = c_type(module, element)?;
            Ok((
                size.checked_mul(*length)
                    .ok_or_else(|| internal("received array layout overflow"))?,
                align,
            ))
        }
        Type::Func(_)
        | Type::Object
        | Type::Nullable(..)
        | Type::Class(_)
        | Type::Map(..)
        | Type::Set(_) => Ok((8, 8)),
        other => Err(internal(format!("boundary C field type {other:?}"))),
    }
}

pub(crate) fn c_class(module: &l::Module, class: ClassId) -> Result<(Vec<u32>, u32, u32), String> {
    let mut offsets = Vec::new();
    let mut size = 0;
    let mut align = 1;
    let definition = module
        .classes
        .get(class.0)
        .ok_or_else(|| internal(format!("boundary class {} is missing", class.0)))?;
    for field in &definition.fields {
        let (width, alignment) = c_type(module, &field.ty)?;
        size = round_up_layout(size, alignment, "received C layout")?;
        offsets.push(size);
        size = size
            .checked_add(width)
            .ok_or_else(|| internal("received C layout overflow"))?;
        align = align.max(alignment);
    }
    Ok((
        offsets,
        round_up_layout(size.max(1), align, "received C layout")?,
        align,
    ))
}

/// Script array handles that a changed pair must retain from before the call.
pub(crate) fn input_arrays(
    layouts: &Layouts,
    plan: &subscript_compiler::crossing::WriteBackPlan,
) -> Result<Vec<u32>, String> {
    use subscript_compiler::crossing::WriteBack;
    let layout = layouts.class(plan.class.0)?;
    let mut offsets = Vec::new();
    for member in &plan.members {
        let offset = layout.field_offsets[member.field];
        match &member.write {
            WriteBack::Pair { .. } => offsets.push(offset),
            WriteBack::FixedArray(elements) => {
                for (index, child) in elements.iter().enumerate() {
                    let stride = layouts.class(child.class.0)?.size;
                    for child in input_arrays(layouts, child)? {
                        offsets.push(offset + index as u32 * stride + child);
                    }
                }
            }
            WriteBack::Embedded(child) => {
                for child in input_arrays(layouts, child)? {
                    offsets.push(
                        offset
                            .checked_add(child)
                            .ok_or_else(|| internal("input array offset overflow"))?,
                    );
                }
            }
            _ => {}
        }
    }
    Ok(offsets)
}

/// Script slots that a readonly scratch element must retain for count-only receives.
pub(crate) fn input_slots(
    module: &l::Module,
    layouts: &Layouts,
    class: ClassId,
) -> Result<Vec<(u32, u32)>, String> {
    fn slots(
        module: &l::Module,
        layouts: &Layouts,
        ty: &Type,
        base: u32,
        out: &mut Vec<(u32, u32)>,
    ) -> Result<(), String> {
        use subscript_boundary::FieldShape;
        use subscript_compiler::boundary_pass::field_shape;
        if let Type::FixedArray(element, length) = ty {
            let stride = layouts.stride(element)?;
            for index in 0..*length {
                slots(module, layouts, element, base + index * stride, out)?;
            }
            return Ok(());
        }
        match field_shape(module, ty) {
            FieldShape::Pointer { .. } | FieldShape::Userdata | FieldShape::Callback => {
                out.push((base, layouts.size_align(ty)?.0))
            }
            FieldShape::Pair { .. } | FieldShape::ValidatedPair => out.push((base, 8)),
            FieldShape::Embedded(class) => {
                let layout = layouts.class(class.0)?;
                for (index, field) in module.classes[class.0].fields.iter().enumerate() {
                    slots(
                        module,
                        layouts,
                        &field.ty,
                        base + layout.field_offsets[index],
                        out,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut result = Vec::new();
    slots(module, layouts, &Type::Class(class), 0, &mut result)?;
    Ok(result)
}

fn word(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

pub(crate) fn schema(
    module: &l::Module,
    layouts: &Layouts,
    plan: &ReceivePlan,
) -> Result<Vec<u8>, String> {
    let mut payload = Vec::new();
    let (tag, c_size, script_size) = match plan {
        ReceivePlan::Fixed { length, element } => {
            let child = schema(module, layouts, element)?;
            let c_stride = u32::from_le_bytes(
                child[4..8]
                    .try_into()
                    .map_err(|_| internal("received fixed C stride"))?,
            );
            let script_stride = u32::from_le_bytes(
                child[8..12]
                    .try_into()
                    .map_err(|_| internal("received fixed script stride"))?,
            );
            word(&mut payload, *length);
            payload.extend(child);
            (
                5,
                c_stride
                    .checked_mul(*length)
                    .ok_or_else(|| internal("received fixed C size"))?,
                script_stride
                    .checked_mul(*length)
                    .ok_or_else(|| internal("received fixed script size"))?,
            )
        }
        ReceivePlan::Validated(alias) => {
            let definition = &module.string_aliases[alias.0];
            let values = definition
                .wire_values
                .as_ref()
                .ok_or_else(|| internal("received wire enum has no mapping"))?;
            word(&mut payload, values.len() as u32);
            for value in values {
                word(&mut payload, *value as u32);
            }
            word(&mut payload, definition.source_name.len() as u32);
            payload.extend(definition.source_name.as_bytes());
            (6, 4, 4)
        }
        ReceivePlan::Bytes(ty) => (0, c_type(module, ty)?.0, layouts.size_align(ty)?.0),
        ReceivePlan::StringView => (1, 16, 8),
        ReceivePlan::Pointer { nullable, ty } => {
            word(&mut payload, u32::from(*nullable));
            (2, 8, layouts.size_align(ty)?.0)
        }
        ReceivePlan::Struct { class, members } => {
            let (offsets, size, _) = c_class(module, *class)?;
            let script = layouts.class(class.0)?;
            word(&mut payload, members.len() as u32);
            for member in members {
                word(&mut payload, offsets[member.field]);
                word(&mut payload, script.field_offsets[member.field]);
                let name = &module.classes[class.0].fields[member.field].source_name;
                word(&mut payload, name.len() as u32);
                payload.extend(name.as_bytes());
                payload.extend(schema(module, layouts, &member.read)?);
            }
            let scratch = subscript_boundary::struct_pass(
                &subscript_compiler::boundary_pass::Classes(module),
                *class,
            ) == subscript_boundary::StructPass::Scratch;
            let tag = if scratch && !input_slots(module, layouts, *class)?.is_empty() {
                3 | 256
            } else {
                3
            };
            (tag, size, script.size)
        }
        ReceivePlan::Pair {
            pointer_first,
            element,
        } => {
            word(&mut payload, u32::from(*pointer_first));
            payload.extend(schema(module, layouts, element)?);
            (4, 16, 8)
        }
        other => {
            return Err(internal(format!(
                "unsupported received-element plan {other:?}"
            )))
        }
    };
    let mut out = Vec::new();
    for value in [tag, c_size, script_size, payload.len() as u32] {
        word(&mut out, value);
    }
    out.extend(payload);
    Ok(out)
}

/// Encodes the finite graph of script-memory pointer checks.
pub(crate) fn watch_schema(
    module: &l::Module,
    layouts: &Layouts,
    plan: &subscript_compiler::crossing::WatchPlan,
) -> Result<Vec<u8>, String> {
    use subscript_compiler::crossing::WatchMember;
    let mut out = Vec::new();
    word(&mut out, plan.root as u32);
    word(&mut out, plan.nodes.len() as u32);
    word(&mut out, layouts.class(plan.nodes[plan.root].class.0)?.size);
    for node in &plan.nodes {
        word(&mut out, node.members.len() as u32);
        word(&mut out, layouts.class(node.class.0)?.size);
        for member in &node.members {
            let (tag, field, extra) = match member {
                WatchMember::Array { field, target } => (3, *field, *target as u32),
                WatchMember::Element { field, target, .. } => (2, *field, *target as u32),
                WatchMember::Compare { field, nullable } => (0, *field, u32::from(*nullable)),
                WatchMember::Pointer { field, target } => (1, *field, *target as u32),
                WatchMember::Embedded { field, target } => (2, *field, *target as u32),
                other => {
                    return Err(internal(format!(
                        "unsupported pointer-watch member {other:?}"
                    )))
                }
            };
            word(&mut out, tag);
            let mut offset = layouts.class(node.class.0)?.field_offsets[field];
            let mut name = module.classes[node.class.0].fields[field]
                .source_name
                .clone();
            if let WatchMember::Element { index, .. } = member {
                let Type::FixedArray(element, _) = &module.classes[node.class.0].fields[field].ty
                else {
                    return Err(internal("fixed watch has no element"));
                };
                offset += index * layouts.stride(element)?;
                name.push_str(&format!("[{index}]"));
            }
            word(&mut out, offset);
            word(&mut out, extra);
            word(&mut out, name.len() as u32);
            out.extend(name.as_bytes());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_layout_rejects_types_without_a_boundary_representation() {
        let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "layout.ts",
            "export function main(): void {}",
        )])
        .expect("checks");
        let module = crate::lir::lower_module(&hir).expect("lowers");
        assert_eq!(c_type(&module, &Type::I32).expect("scalar"), (4, 4));
        for ty in [Type::Void, Type::FixedArray(Box::new(Type::Void), 2)] {
            assert!(c_type(&module, &ty).is_err(), "{ty:?}");
        }
    }
}
