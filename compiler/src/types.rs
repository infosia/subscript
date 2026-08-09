//! The language's resolved type representation.

use std::fmt;

/// Maximum supported byte size of one aggregate layout.
///
/// Cranelift memory operations use signed 32-bit direct displacements
/// for class, frame, and global offsets, so no offset within one
/// aggregate may exceed `i32::MAX`.
pub const MAX_AGGREGATE_BYTES: u32 = i32::MAX as u32;

/// Cranelift's required stack-frame alignment on the supported 64-bit
/// targets.
pub const CRANELIFT_FRAME_ALIGNMENT: u32 = 16;

/// Reserved plain-Q32 discriminant used for an absent descriptor member (R16).
///
/// Ordinary plain-alias members are numbered from zero in declaration order,
/// so this value is outside every plain member set. Wire-mapped aliases choose
/// an alias-specific value outside their wire set instead (§52.1).
pub const ABSENT_STRING_ALIAS_DISCRIMINANT: i64 = -1;

/// Maximum supported accumulated Cranelift frame storage.
///
/// The aarch64 ABI lowering adjusts the stack pointer with a signed
/// 32-bit amount after rounding the frame to
/// [`CRANELIFT_FRAME_ALIGNMENT`]. This is therefore the greatest
/// frame-aligned value representable by a positive `i32`, derived by
/// clearing the alignment bits of `i32::MAX`.
pub const MAX_FRAME_BYTES: u32 =
    (i32::MAX as u32) & !(CRANELIFT_FRAME_ALIGNMENT - 1);

/// Index of a class definition inside [`crate::hir::Module::classes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassId(pub usize);

/// Index of an enum definition inside [`crate::hir::Module::enums`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumId(pub usize);

/// Index of a string-literal union alias inside
/// [`crate::hir::Module::string_aliases`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringAliasId(pub usize);

/// A fully resolved language type.
///
/// Sized numerics are all distinct; classes are nominal (two same-shaped
/// classes are different types); `Nullable` and named string-literal
/// aliases are the only union forms (`Ref | null`, C7/Q32).
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
    /// A compiled ECMAScript regular expression (stdlib.md §15, Q31).
    ///
    /// Its runtime representation is a Context-owned handle.
    RegExp,
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
    /// A nominal, closed string-literal union alias (Q32).
    ///
    /// Plain aliases are represented by an `i32` member index. Wire-mapped
    /// aliases are represented by their declared `i32` wire value (§52.1).
    /// The alias id is retained so same-membered declarations remain distinct.
    StringAlias(StringAliasId),
    /// `FixedArray<T, N>` with `N` known at compile time (Q3).
    FixedArray(Box<Type>, u32),
    /// Dynamic array `T[]` (Q4/Q15).
    Array(Box<Type>),
    /// Monomorphized generic reference class `Map<K, V>` (Q24).
    Map(Box<Type>, Box<Type>),
    /// Monomorphized generic reference class `Set<K>` (Q24).
    Set(Box<Type>),
    /// Parent-side worker handle, monomorphized by its input/output
    /// message-class pair (Q35).
    Worker(Box<Type>, Box<Type>),
    /// Worker-side receiving endpoint, monomorphized by message class
    /// (Q35).
    Inbox(Box<Type>),
    /// Worker-side sending endpoint, monomorphized by message class
    /// (Q35).
    Outbox(Box<Type>),
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

/// C-ABI size and alignment of every type whose in-memory layout does
/// not depend on a class definition or nested aggregate.
///
/// The checker and backend both use this table; keeping it here makes
/// scalar layout agreement structural rather than test-only.
#[must_use]
pub fn scalar_size_align(ty: &Type) -> Option<(u32, u32)> {
    Some(match ty {
        Type::Bool | Type::I8 | Type::U8 => (1, 1),
        Type::I16 | Type::U16 | Type::F16 => (2, 2),
        Type::I32 | Type::U32 | Type::F32 | Type::Enum(_) | Type::StringAlias(_) => (4, 4),
        Type::I64 | Type::U64 | Type::F64 | Type::Date => (8, 8),
        Type::Str
        | Type::RegExp
        | Type::Object
        | Type::Array(_)
        | Type::Map(..)
        | Type::Set(_)
        | Type::Worker(..)
        | Type::Inbox(_)
        | Type::Outbox(_)
        | Type::Generator(_)
        | Type::Nullable(_)
        | Type::Null => (8, 8),
        Type::Func(_) => (16, 8),
        Type::Void | Type::Error => (0, 1),
        Type::Class(_) | Type::FixedArray(..) | Type::IterResult(_) => return None,
    })
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
        matches!(
            self,
            Type::Class(_)
                | Type::Map(..)
                | Type::Set(_)
                | Type::Worker(..)
                | Type::Inbox(_)
                | Type::Outbox(_)
                | Type::Object
                | Type::Func(_)
        )
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
    string_alias_name: &dyn Fn(StringAliasId) -> String,
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
        Type::RegExp => "RegExp".to_string(),
        Type::Void => "void".to_string(),
        Type::Null => "null".to_string(),
        Type::Object => "object".to_string(),
        Type::Class(id) => class_name(*id),
        Type::Enum(id) => enum_name(*id),
        Type::StringAlias(id) => string_alias_name(*id),
        Type::FixedArray(elem, n) => format!(
            "FixedArray<{}, {}>",
            display_type(elem, class_name, enum_name, string_alias_name),
            n
        ),
        Type::Array(elem) => format!(
            "{}[]",
            display_type(elem, class_name, enum_name, string_alias_name)
        ),
        Type::Map(key, value) => format!(
            "Map<{}, {}>",
            display_type(key, class_name, enum_name, string_alias_name),
            display_type(value, class_name, enum_name, string_alias_name)
        ),
        Type::Set(key) => format!(
            "Set<{}>",
            display_type(key, class_name, enum_name, string_alias_name)
        ),
        Type::Worker(input, output) => format!(
            "Worker<{}, {}>",
            display_type(input, class_name, enum_name, string_alias_name),
            display_type(output, class_name, enum_name, string_alias_name)
        ),
        Type::Inbox(message) => format!(
            "Inbox<{}>",
            display_type(message, class_name, enum_name, string_alias_name)
        ),
        Type::Outbox(message) => format!(
            "Outbox<{}>",
            display_type(message, class_name, enum_name, string_alias_name)
        ),
        Type::Func(f) => {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|p| display_type(p, class_name, enum_name, string_alias_name))
                .collect();
            format!(
                "({}) => {}",
                params.join(", "),
                display_type(&f.ret, class_name, enum_name, string_alias_name)
            )
        }
        Type::Nullable(inner) => {
            format!(
                "{} | null",
                display_type(inner, class_name, enum_name, string_alias_name)
            )
        }
        Type::Generator(y) => format!(
            "Generator<{}>",
            display_type(y, class_name, enum_name, string_alias_name)
        ),
        Type::IterResult(v) => format!(
            "{{ done: boolean; value: {} }}",
            display_type(v, class_name, enum_name, string_alias_name)
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
            &|id| format!("<string alias #{}>", id.0),
        );
        f.write_str(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_limit_matches_the_backend_displacement_range() {
        assert_eq!(MAX_AGGREGATE_BYTES, 2_147_483_647);
        assert_eq!(CRANELIFT_FRAME_ALIGNMENT, 16);
        assert_eq!(MAX_FRAME_BYTES, 2_147_483_632);
        assert_eq!(
            MAX_FRAME_BYTES,
            MAX_AGGREGATE_BYTES & !(CRANELIFT_FRAME_ALIGNMENT - 1)
        );
    }

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
        let aliases = |_: StringAliasId| "Format".to_string();
        assert_eq!(
            display_type(
                &Type::FixedArray(Box::new(Type::F32), 16),
                &names,
                &enums,
                &aliases
            ),
            "FixedArray<f32, 16>"
        );
        assert_eq!(
            display_type(
                &Type::Nullable(Box::new(Type::Class(ClassId(0)))),
                &names,
                &enums,
                &aliases
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
                &enums,
                &aliases
            ),
            "(i32) => i32"
        );
        assert_eq!(
            display_type(
                &Type::Generator(Box::new(Type::I32)),
                &names,
                &enums,
                &aliases
            ),
            "Generator<i32>"
        );
        assert_eq!(
            display_type(
                &Type::StringAlias(StringAliasId(0)),
                &names,
                &enums,
                &aliases
            ),
            "Format"
        );
    }
}
