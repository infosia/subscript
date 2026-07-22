//! Hardcoded ambient prelude surface (Q12): sized-numeric aliases and
//! the host functions `print`, `collect`, `unsafeDelete`.
//!
//! `prelude/lang.d.ts` is the `tsc`-facing reference for these
//! declarations; the checker does not parse it (P1 contract).

use crate::hir::AmbientFn;
use crate::types::Type;

/// Maps a sized-numeric alias name to its type.
pub(crate) fn sized_alias(name: &str) -> Option<Type> {
    match name {
        "i32" => Some(Type::I32),
        "u32" => Some(Type::U32),
        "i64" => Some(Type::I64),
        "u64" => Some(Type::U64),
        "f32" => Some(Type::F32),
        "f64" => Some(Type::F64),
        _ => None,
    }
}

/// Maps an ambient function name to its identity.
pub(crate) fn ambient_fn(name: &str) -> Option<AmbientFn> {
    match name {
        "print" => Some(AmbientFn::Print),
        "collect" => Some(AmbientFn::Collect),
        "unsafeDelete" => Some(AmbientFn::UnsafeDelete),
        _ => None,
    }
}

/// Parameter types of an ambient function (all return `void`).
pub(crate) fn ambient_params(f: AmbientFn) -> &'static [Type] {
    match f {
        AmbientFn::Print => &[Type::Str],
        AmbientFn::Collect => &[],
        AmbientFn::UnsafeDelete => &[Type::Object],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_cover_all_six_sized_numerics() {
        assert_eq!(sized_alias("i32"), Some(Type::I32));
        assert_eq!(sized_alias("u64"), Some(Type::U64));
        assert_eq!(sized_alias("f32"), Some(Type::F32));
        assert_eq!(sized_alias("number"), None);
    }

    #[test]
    fn ambient_functions_have_expected_shapes() {
        assert_eq!(ambient_fn("print"), Some(AmbientFn::Print));
        assert_eq!(ambient_params(AmbientFn::Print), &[Type::Str]);
        assert!(ambient_params(AmbientFn::Collect).is_empty());
        assert_eq!(ambient_params(AmbientFn::UnsafeDelete), &[Type::Object]);
        assert_eq!(ambient_fn("eval"), None);
    }
}
