//! Shared LIR type classifications used by both code-generation tiers.

use std::collections::HashSet;

use subscript_compiler::{hir, lir as l};
use subscript_compiler::{ClassId, Type};
use subscript_runtime::TrapKind;

fn internal(message: impl AsRef<str>) -> String {
    format!("internal error: {}", message.as_ref())
}

pub(crate) fn runtime_trap_kind(kind: &l::TrapKind) -> Option<TrapKind> {
    Some(match kind {
        l::TrapKind::Allocation => TrapKind::AllocationFailure,
        l::TrapKind::Call => return None,
        l::TrapKind::Unreachable => TrapKind::UnreachableReached,
        l::TrapKind::DivisionByZero => TrapKind::DivisionByZero,
        l::TrapKind::IndexRead | l::TrapKind::IndexWrite => TrapKind::IndexOutOfBounds,
        l::TrapKind::JsonResultValue(_) => TrapKind::JsonResultValue,
        l::TrapKind::NullNarrowing => TrapKind::NullNarrowing,
        l::TrapKind::ClassMismatch(_) => TrapKind::ClassMismatch,
        l::TrapKind::DevOnlyLifetime => TrapKind::UseAfterDelete,
        l::TrapKind::DevReloadOnlyStaleCoroutine => TrapKind::StaleCoroutine,
        l::TrapKind::WireEnumValue(_) => TrapKind::WireEnumUnknownValue,
    })
}

fn runtime_trap_matches_lir(runtime: TrapKind, lir: &l::TrapKind) -> bool {
    if runtime_trap_kind(lir) == Some(runtime) {
        return true;
    }
    match runtime {
        TrapKind::DoubleDelete | TrapKind::InvalidDelete | TrapKind::CallbackUserdataFreed => {
            *lir == l::TrapKind::DevOnlyLifetime
        }
        TrapKind::EmptyPop
        | TrapKind::StringSlice
        | TrapKind::Internal
        | TrapKind::DateRange
        | TrapKind::StrRange
        | TrapKind::NumberRange
        | TrapKind::JsonNumber
        | TrapKind::JsonCycle
        | TrapKind::Regex
        | TrapKind::RegexBudget
        | TrapKind::WorkerTrapped => *lir == l::TrapKind::Call,
        _ => false,
    }
}

pub(crate) fn runtime_trap_site(runtime: TrapKind, sites: &[l::Trap]) -> Option<&l::Trap> {
    sites
        .iter()
        .find(|site| runtime_trap_matches_lir(runtime, &site.kind))
        .or_else(|| sites.iter().find(|site| site.kind == l::TrapKind::Call))
}

pub(crate) fn data_type(ty: &l::ValueType) -> Result<&Type, String> {
    match ty {
        l::ValueType::Data(ty) => Ok(ty),
        other => Err(internal(format!("expected a data type, found {other:?}"))),
    }
}

pub(crate) fn value_type(function: &l::Function, id: l::ValueId) -> Result<&l::ValueType, String> {
    function
        .values
        .get(id.0 as usize)
        .filter(|value| value.id == id)
        .map(|value| &value.ty)
        .ok_or_else(|| internal(format!("value {} is missing", id.0)))
}

pub(crate) fn operand_type(
    function: &l::Function,
    operand: &l::Operand,
) -> Result<l::ValueType, String> {
    Ok(match operand {
        l::Operand::Value(value) => value_type(function, *value)?.clone(),
        l::Operand::Constant(constant) => l::ValueType::Data(constant.ty.clone()),
    })
}

pub(crate) fn foreign_parameter_type_matches(
    module: &l::Module,
    actual: &l::ValueType,
    declared: &Type,
) -> bool {
    if actual == &l::ValueType::Data(declared.clone()) {
        return true;
    }
    let l::ValueType::Address(address) = actual else {
        return false;
    };
    boundary_box_class(module, declared).is_some_and(|class| address.pointee == Type::Class(class))
}

pub(crate) fn explicit_parameters(function: &l::Function) -> impl Iterator<Item = &l::Parameter> {
    function
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == l::ParameterKind::Explicit)
}

pub(crate) fn capture_parameters(function: &l::Function) -> impl Iterator<Item = &l::Parameter> {
    function
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == l::ParameterKind::Capture)
}

pub(crate) fn lir_class_is_value(module: &l::Module, class: ClassId) -> bool {
    module
        .classes
        .get(class.0)
        .is_some_and(|definition| definition.id == class && definition.is_value)
}

pub(crate) trait BoundaryBoxModule {
    fn is_boundary_value_class(&self, class: ClassId) -> bool;
}

impl BoundaryBoxModule for l::Module {
    fn is_boundary_value_class(&self, class: ClassId) -> bool {
        self.classes.get(class.0).is_some_and(|definition| {
            definition.id == class && definition.is_value && definition.is_boundary
        })
    }
}

impl BoundaryBoxModule for hir::Module {
    fn is_boundary_value_class(&self, class: ClassId) -> bool {
        self.classes
            .get(class.0)
            .is_some_and(|definition| definition.is_value && definition.is_boundary)
    }
}

pub(crate) fn boundary_box_class(module: &impl BoundaryBoxModule, ty: &Type) -> Option<ClassId> {
    let Type::Nullable(inner) = ty else {
        return None;
    };
    let Type::Class(class) = inner.as_ref() else {
        return None;
    };
    module.is_boundary_value_class(*class).then_some(*class)
}

pub(crate) fn array_element_kind(module: &l::Module, ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::Bool
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Object
        | Type::Array(_)
        | Type::Map(..)
        | Type::Set(_) => 0,
        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Enum(_) | Type::Date => 5,
        Type::Class(class) if !lir_class_is_value(module, *class) => 0,
        Type::Nullable(inner) if !matches!(**inner, Type::Func(_)) => 0,
        Type::F32 => 1,
        Type::F64 => 2,
        Type::Str => 3,
        Type::F16 => 4,
        other => {
            return Err(format!(
                "internal error: array element type {other:?} has no runtime kind"
            ));
        }
    })
}

pub(crate) fn array_format_kind(ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::I32 | Type::Enum(_) => 0,
        Type::U32 => 1,
        Type::I64 => 2,
        Type::U64 => 3,
        Type::F32 => 4,
        Type::F64 => 5,
        Type::Bool => 6,
        Type::Str => 7,
        Type::I8 => 8,
        Type::U8 => 9,
        Type::I16 => 10,
        Type::U16 => 11,
        Type::F16 => 12,
        other => {
            return Err(format!(
                "internal error: array element {other:?} is not formattable"
            ));
        }
    })
}

pub(crate) fn association_key_kind(module: &l::Module, ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::I8
        | Type::U8
        | Type::I16
        | Type::U16
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::Bool
        | Type::Enum(_)
        | Type::Date => 0,
        Type::F32 => 1,
        Type::F64 => 2,
        Type::Str => 3,
        Type::Class(class) if !lir_class_is_value(module, *class) => 4,
        other => {
            return Err(format!(
                "internal error: Map/Set key type {other:?} has no runtime kind"
            ));
        }
    })
}

pub(crate) fn is_userdata_slot(ty: &Type) -> bool {
    matches!(ty, Type::Object) || matches!(ty, Type::Nullable(inner) if **inner == Type::Object)
}

fn boundary_class(module: &l::Module, class: ClassId) -> Result<&l::Class, String> {
    module
        .classes
        .get(class.0)
        .filter(|definition| definition.id == class)
        .ok_or_else(|| format!("internal error: boundary class {} is missing", class.0))
}

fn boundary_type_needs_scratch_inner(
    module: &l::Module,
    ty: &Type,
    visiting: &mut HashSet<ClassId>,
) -> Result<bool, String> {
    match ty {
        Type::Str | Type::Array(_) => Ok(true),
        Type::Nullable(inner) => boundary_type_needs_scratch_inner(module, inner, visiting),
        Type::Class(class) => boundary_class_needs_scratch_inner(module, *class, visiting),
        _ => Ok(false),
    }
}

fn boundary_class_needs_scratch_inner(
    module: &l::Module,
    class: ClassId,
    visiting: &mut HashSet<ClassId>,
) -> Result<bool, String> {
    let definition = boundary_class(module, class)?;
    if !definition.is_value || !visiting.insert(class) {
        return Ok(false);
    }
    let result = definition.fields.iter().try_fold(false, |found, field| {
        Ok(found || boundary_type_needs_scratch_inner(module, &field.ty, visiting)?)
    });
    visiting.remove(&class);
    result
}

pub(crate) fn boundary_class_needs_scratch(
    module: &l::Module,
    class: ClassId,
) -> Result<bool, String> {
    boundary_class_needs_scratch_inner(module, class, &mut HashSet::new())
}

fn boundary_class_contains_pointer_inner(
    module: &l::Module,
    class: ClassId,
    visiting: &mut HashSet<ClassId>,
) -> Result<bool, String> {
    let definition = boundary_class(module, class)?;
    if !definition.is_value || !visiting.insert(class) {
        return Ok(false);
    }
    let mut result = false;
    for field in &definition.fields {
        result = match &field.ty {
            Type::Nullable(inner) => {
                matches!(inner.as_ref(), Type::Class(inner) if lir_class_is_value(module, *inner))
            }
            Type::Class(inner) if lir_class_is_value(module, *inner) => {
                boundary_class_contains_pointer_inner(module, *inner, visiting)?
            }
            Type::Array(element) => match element.as_ref() {
                Type::Class(inner) if lir_class_is_value(module, *inner) => {
                    boundary_class_contains_pointer_inner(module, *inner, visiting)?
                }
                _ => false,
            },
            _ => false,
        };
        if result {
            break;
        }
    }
    visiting.remove(&class);
    Ok(result)
}

pub(crate) fn boundary_class_contains_pointer(
    module: &l::Module,
    class: ClassId,
) -> Result<bool, String> {
    boundary_class_contains_pointer_inner(module, class, &mut HashSet::new())
}

pub(crate) fn boundary_class_requires_build(
    module: &l::Module,
    class: ClassId,
) -> Result<bool, String> {
    Ok(boundary_class_needs_scratch(module, class)?
        || boundary_class_contains_pointer(module, class)?)
}

pub(crate) fn boundary_type_requires_build(module: &l::Module, ty: &Type) -> Result<bool, String> {
    match ty {
        Type::Array(element) => match element.as_ref() {
            Type::Class(class) if lir_class_is_value(module, *class) => {
                boundary_class_requires_build(module, *class)
            }
            _ => Ok(false),
        },
        Type::Nullable(inner) => boundary_type_requires_build(module, inner),
        Type::Class(class) if lir_class_is_value(module, *class) => {
            boundary_class_requires_build(module, *class)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use subscript_compiler::StringAliasId;

    use super::*;

    #[test]
    fn every_lir_trap_kind_has_its_recorded_runtime_kind() {
        let cases = [
            (l::TrapKind::Allocation, Some(TrapKind::AllocationFailure)),
            (l::TrapKind::Call, None),
            (l::TrapKind::Unreachable, Some(TrapKind::UnreachableReached)),
            (l::TrapKind::DivisionByZero, Some(TrapKind::DivisionByZero)),
            (l::TrapKind::IndexRead, Some(TrapKind::IndexOutOfBounds)),
            (l::TrapKind::IndexWrite, Some(TrapKind::IndexOutOfBounds)),
            (
                l::TrapKind::JsonResultValue(l::FieldId(1)),
                Some(TrapKind::JsonResultValue),
            ),
            (l::TrapKind::NullNarrowing, Some(TrapKind::NullNarrowing)),
            (
                l::TrapKind::ClassMismatch(ClassId(2)),
                Some(TrapKind::ClassMismatch),
            ),
            (l::TrapKind::DevOnlyLifetime, Some(TrapKind::UseAfterDelete)),
            (
                l::TrapKind::DevReloadOnlyStaleCoroutine,
                Some(TrapKind::StaleCoroutine),
            ),
            (
                l::TrapKind::WireEnumValue(StringAliasId(3)),
                Some(TrapKind::WireEnumUnknownValue),
            ),
        ];
        assert_eq!(cases.len(), 12);
        for (lir, runtime) in cases {
            assert_eq!(runtime_trap_kind(&lir), runtime, "{lir:?}");
        }

        let call = l::Trap {
            kind: l::TrapKind::Call,
            pos: subscript_compiler::Pos::new("call.ts", 7, 11),
        };
        assert_eq!(
            runtime_trap_site(TrapKind::UnreachableReached, &[call]).map(|site| site.pos.clone()),
            Some(subscript_compiler::Pos::new("call.ts", 7, 11))
        );
    }
}
