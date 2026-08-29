//! Shared LIR type classifications used by both code-generation tiers.

use std::collections::HashSet;

use subscript_compiler::lir as l;
use subscript_compiler::{ClassId, Type};

pub(crate) fn lir_class_is_value(module: &l::Module, class: ClassId) -> bool {
    module
        .classes
        .get(class.0)
        .is_some_and(|definition| definition.id == class && definition.is_value)
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

pub(crate) fn boundary_class_is_embedded_header(module: &l::Module, header: ClassId) -> bool {
    if !module
        .classes
        .get(header.0)
        .is_some_and(|class| class.id == header && class.is_value && class.is_boundary)
    {
        return false;
    }
    let nullable_header = Type::Nullable(Box::new(Type::Class(header)));
    let used_as_link = module.classes.iter().any(|class| {
        class.is_boundary && class.fields.iter().any(|field| field.ty == nullable_header)
    }) || module.foreign_functions.iter().any(|function| {
        function
            .parameters
            .iter()
            .any(|parameter| parameter.ty == nullable_header)
    });
    used_as_link
        && module.classes.iter().any(|class| {
            class.is_value
                && class.is_boundary
                && class
                    .fields
                    .first()
                    .is_some_and(|field| field.ty == Type::Class(header))
        })
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
