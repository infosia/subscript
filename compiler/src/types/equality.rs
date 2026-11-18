//! Type equality and hashing use plain instance types (compiler.md §335 rule 1c).

use super::{
    Absence, ClassId, EnumId, FuncType, InterfaceType, StringAliasId, Type, TypeParameterType,
};
use std::hash::{Hash, Hasher};

/// A borrowed structural key; nested types use the same equality and hash definition.
#[derive(PartialEq, Eq, Hash)]
enum Equality<'a> {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    F16,
    Bool,
    Str,
    Date,
    RegExp,
    TaskGroup,
    Void,
    Null,
    Object,
    GenericNumber,
    Error,
    Class(&'a ClassId),
    TypeParameter(&'a TypeParameterType),
    Interface(&'a InterfaceType),
    GenericUnion(&'a [Type]),
    Enum(&'a EnumId),
    StringAlias(&'a StringAliasId),
    Array(&'a Type),
    Set(&'a Type),
    Inbox(&'a Type),
    Outbox(&'a Type),
    Func(&'a FuncType),
    Generator(&'a Type),
    AsyncHandle(&'a Type),
    IterResult(&'a Type),
    FixedArray(&'a Type, u32),
    Map(&'a Type, &'a Type),
    Worker(&'a Type, &'a Type),
    Nullable(&'a Type, Absence),
}

impl Type {
    fn equality_key(&self) -> Equality<'_> {
        match self {
            Self::ConstrainedInstance(instance) => instance.instance.equality_key(),
            Self::I8 => Equality::I8,
            Self::U8 => Equality::U8,
            Self::I16 => Equality::I16,
            Self::U16 => Equality::U16,
            Self::I32 => Equality::I32,
            Self::U32 => Equality::U32,
            Self::I64 => Equality::I64,
            Self::U64 => Equality::U64,
            Self::F32 => Equality::F32,
            Self::F64 => Equality::F64,
            Self::F16 => Equality::F16,
            Self::Bool => Equality::Bool,
            Self::Str => Equality::Str,
            Self::Date => Equality::Date,
            Self::RegExp => Equality::RegExp,
            Self::TaskGroup => Equality::TaskGroup,
            Self::Void => Equality::Void,
            Self::Null => Equality::Null,
            Self::Object => Equality::Object,
            Self::GenericNumber => Equality::GenericNumber,
            Self::Error => Equality::Error,
            Self::Class(value) => Equality::Class(value),
            Self::TypeParameter(value) => Equality::TypeParameter(value),
            Self::Interface(value) => Equality::Interface(value),
            Self::GenericUnion(value) => Equality::GenericUnion(value),
            Self::Enum(value) => Equality::Enum(value),
            Self::StringAlias(value) => Equality::StringAlias(value),
            Self::Array(value) => Equality::Array(value),
            Self::Set(value) => Equality::Set(value),
            Self::Inbox(value) => Equality::Inbox(value),
            Self::Outbox(value) => Equality::Outbox(value),
            Self::Func(value) => Equality::Func(value),
            Self::Generator(value) => Equality::Generator(value),
            Self::AsyncHandle(value) => Equality::AsyncHandle(value),
            Self::IterResult(value) => Equality::IterResult(value),
            Self::FixedArray(left, right) => Equality::FixedArray(left, *right),
            Self::Map(left, right) => Equality::Map(left, right),
            Self::Worker(left, right) => Equality::Worker(left, right),
            Self::Nullable(left, right) => Equality::Nullable(left, *right),
        }
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.equality_key() == other.equality_key()
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.equality_key().hash(state);
    }
}
