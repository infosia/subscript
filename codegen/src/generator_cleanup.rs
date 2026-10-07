//! Static native cleanup descriptions from the LIR generator states.

use crate::{layout::Layouts, lower::internal};
use subscript_compiler::{lir as l, Type};

pub(crate) fn fields(
    function: &l::Function,
    state: Option<l::BlockId>,
) -> Result<Vec<(l::ValueId, &Type)>, String> {
    let cleanup = function
        .liveness
        .generator_cleanup
        .iter()
        .find(|cleanup| cleanup.suspension == state)
        .ok_or_else(|| internal("generator state has no cleanup description"))?;
    cleanup
        .owners
        .iter()
        .map(
            |value| match function.values.get(value.0 as usize).map(|value| &value.ty) {
                Some(l::ValueType::Data(ty)) => Ok((*value, ty)),
                _ => Err(internal("generator cleanup owner has no data type")),
            },
        )
        .collect()
}

pub(crate) fn description(
    layouts: &Layouts,
    function: &l::Function,
    state: Option<l::BlockId>,
    slots: &[(l::ValueId, u32)],
) -> Result<Vec<u8>, String> {
    let fields = fields(function, state)?;
    let mut bytes = (fields.len() as u64).to_ne_bytes().to_vec();
    for (value, ty) in fields {
        let offset = slots
            .iter()
            .find_map(|(id, offset)| (*id == value).then_some(*offset))
            .ok_or_else(|| {
                internal(format!(
                    "generator cleanup value {} has no saved slot",
                    value.0
                ))
            })?;
        let child = crate::counted::description(layouts, ty)?;
        bytes.extend_from_slice(&u64::from(offset).to_ne_bytes());
        bytes.extend_from_slice(&(child.len() as u64).to_ne_bytes());
        bytes.extend_from_slice(&child);
    }
    Ok(bytes)
}
