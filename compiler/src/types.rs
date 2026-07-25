//! The language's resolved type representation.

use std::fmt;

/// Index of a class definition inside [`crate::hir::Module::classes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassId(pub usize);

/// Index of an enum definition inside [`crate::hir::Module::enums`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumId(pub usize);

/// A fully resolved language type.
///
/// Sized numerics are all distinct; classes are nominal (two same-shaped
/// classes are different types); `Nullable` is the only union form
/// (`Ref | null`, C7).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Type {
    /// 8-bit signed integer.
    I8,
    /// 8-bit unsigned integer.
    U8,
    /// 16-bit signed integer.
    I16,
    /// 16-bit unsigned integer.
    U16,
    /// 32-bit signed integer.
    I32,
    /// 32-bit unsigned integer.
    U32,
    /// 64-bit signed integer.
    I64,
    /// 64-bit unsigned integer.
    U64,
    /// 32-bit IEEE float.
    F32,
    /// 64-bit IEEE float.
    F64,
    /// IEEE 754 binary16 storage value. Arithmetic is rejected by the
    /// checker (Q23); generated code carries its raw 16 bits.
    F16,
    /// Boolean.
    Bool,
    /// Immutable UTF-8 byte view (Q5).
    Str,
    /// The ambient `Date` value type (stdlib.md §3, Q20): an immutable
    /// UTC timestamp erasing to `i64` epoch milliseconds everywhere in
    /// codegen, but nominal in the checker — not interchangeable with
    /// `i64` without an explicit `getTime()`.
    Date,
    /// Absence of a value (function returns only).
    Void,
    /// The type of the `null` literal.
    Null,
    /// The boundary-opaque reference type (`object`, C7); recovered to a
    /// concrete class only through checked `as` narrowing.
    Object,
    /// A nominal class, value or reference (the definition says which).
    Class(ClassId),
    /// A numeric enum.
    Enum(EnumId),
    /// `FixedArray<T, N>` with `N` known at compile time (Q3).
    FixedArray(Box<Type>, u32),
    /// Dynamic array `T[]` (Q4/Q15).
    Array(Box<Type>),
    /// Non-capturing function type `(params) => ret`.
    Func(Box<FuncType>),
    /// `Ref | null` (C7). The inner type is a reference class, `object`,
    /// or a function type.
    Nullable(Box<Type>),
    /// Coroutine object produced by calling a `function*` (C8); yields
    /// the carried type through `.next()`.
    Generator(Box<Type>),
    /// The value-struct shape `{ done: boolean; value: T }` returned by
    /// `.next()` (C8).
    IterResult(Box<Type>),
    /// Poison type produced after a diagnostic; assignable everywhere so
    /// one error does not cascade. Never present in a successful check's
    /// HIR (success requires zero diagnostics).
    Error,
}

/// Parameter and return types of a function type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    /// Parameter types, in order.
    pub params: Vec<Type>,
    /// Return type.
    pub ret: Type,
}

impl Type {
    /// True for all sized numeric types.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Type::I8
                | Type::U8
                | Type::I16
                | Type::U16
                | Type::I32
                | Type::U32
                | Type::I64
                | Type::U64
                | Type::F16
                | Type::F32
                | Type::F64
        )
    }

    /// True for all sized integer types.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::I8
                | Type::U8
                | Type::I16
                | Type::U16
                | Type::I32
                | Type::U32
                | Type::I64
                | Type::U64
        )
    }

    /// True for all sized float types, including storage-only `f16`.
    #[must_use]
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F16 | Type::F32 | Type::F64)
    }

    /// True when the type may appear inside `Ref | null` (C7): reference
    /// classes (checked by the caller against the class table), `object`,
    /// and function types.
    #[must_use]
    pub fn is_reference_shape(&self) -> bool {
        matches!(self, Type::Class(_) | Type::Object | Type::Func(_))
    }
}

/// Renders a type using a class/enum name lookup supplied by the caller.
///
/// The `Type` enum stores ids, not names; the checker and the HIR module
/// own the tables, so display goes through this helper.
#[must_use]
pub fn display_type(
    ty: &Type,
    class_name: &dyn Fn(ClassId) -> String,
    enum_name: &dyn Fn(EnumId) -> String,
) -> String {
    match ty {
        Type::I8 => "i8".to_string(),
        Type::U8 => "u8".to_string(),
        Type::I16 => "i16".to_string(),
        Type::U16 => "u16".to_string(),
        Type::I32 => "i32".to_string(),
        Type::U32 => "u32".to_string(),
        Type::I64 => "i64".to_string(),
        Type::U64 => "u64".to_string(),
        Type::F32 => "f32".to_string(),
        Type::F64 => "f64".to_string(),
        Type::F16 => "f16".to_string(),
        Type::Bool => "boolean".to_string(),
        Type::Str => "string".to_string(),
        Type::Date => "Date".to_string(),
        Type::Void => "void".to_string(),
        Type::Null => "null".to_string(),
        Type::Object => "object".to_string(),
        Type::Class(id) => class_name(*id),
        Type::Enum(id) => enum_name(*id),
        Type::FixedArray(elem, n) => format!(
            "FixedArray<{}, {}>",
            display_type(elem, class_name, enum_name),
            n
        ),
        Type::Array(elem) => format!("{}[]", display_type(elem, class_name, enum_name)),
        Type::Func(f) => {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|p| display_type(p, class_name, enum_name))
                .collect();
            format!(
                "({}) => {}",
                params.join(", "),
                display_type(&f.ret, class_name, enum_name)
            )
        }
        Type::Nullable(inner) => {
            format!("{} | null", display_type(inner, class_name, enum_name))
        }
        Type::Generator(y) => format!("Generator<{}>", display_type(y, class_name, enum_name)),
        Type::IterResult(v) => format!(
            "{{ done: boolean; value: {} }}",
            display_type(v, class_name, enum_name)
        ),
        Type::Error => "<error>".to_string(),
    }
}

impl fmt::Display for Type {
    /// Displays the type with placeholder names for classes and enums;
    /// use [`display_type`] where real names are available.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = display_type(
            self,
            &|id| format!("<class #{}>", id.0),
            &|id| format!("<enum #{}>", id.0),
        );
        f.write_str(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sized_numerics_are_distinct() {
        assert_ne!(Type::I8, Type::U8);
        assert_ne!(Type::I16, Type::U16);
        assert_ne!(Type::I32, Type::U32);
        assert_ne!(Type::I64, Type::U64);
        assert_ne!(Type::F32, Type::F64);
        assert!(Type::U64.is_integer());
        assert!(Type::F16.is_float());
        assert!(Type::F32.is_float());
        assert!(!Type::Bool.is_numeric());
    }

    #[test]
    fn nominal_classes_are_distinct_by_id() {
        assert_ne!(Type::Class(ClassId(0)), Type::Class(ClassId(1)));
    }

    #[test]
    fn display_covers_compound_types() {
        let names = |_: ClassId| "Vec3".to_string();
        let enums = |_: EnumId| "Status".to_string();
        assert_eq!(
            display_type(&Type::FixedArray(Box::new(Type::F32), 16), &names, &enums),
            "FixedArray<f32, 16>"
        );
        assert_eq!(
            display_type(
                &Type::Nullable(Box::new(Type::Class(ClassId(0)))),
                &names,
                &enums
            ),
            "Vec3 | null"
        );
        assert_eq!(
            display_type(
                &Type::Func(Box::new(FuncType {
                    params: vec![Type::I32],
                    ret: Type::I32
                })),
                &names,
                &enums
            ),
            "(i32) => i32"
        );
        assert_eq!(
            display_type(&Type::Generator(Box::new(Type::I32)), &names, &enums),
            "Generator<i32>"
        );
    }
}
