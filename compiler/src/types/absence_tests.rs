//! Direct tests of the absence spelling of a nullable type (compiler.md
//! §195.1 rule 1, §195.7).

use super::{Absence, ClassId, FuncType, Type};

fn class() -> Type {
    Type::Class(ClassId(0))
}

fn undefined(inner: Type) -> Type {
    Type::Nullable(Box::new(inner), Absence::Undefined)
}

#[test]
fn absence_spellings_are_distinct_values() {
    assert_ne!(Absence::Null, Absence::Undefined);
    assert_eq!(Absence::Undefined, Absence::Undefined);
    assert_eq!(format!("{:?}", Absence::Null), "Null");
    assert_eq!(format!("{:?}", Absence::Undefined), "Undefined");
    // §195.1 rule 2: the two spellings make two different types.
    assert_ne!(Type::nullable(class()), undefined(class()));
}

#[test]
fn nullable_spelled_keeps_the_spelling_and_poisons_on_error() {
    for absence in [Absence::Null, Absence::Undefined] {
        assert_eq!(
            Type::nullable_spelled(class(), absence),
            Type::Nullable(Box::new(class()), absence)
        );
        assert_eq!(Type::nullable_spelled(Type::Error, absence), Type::Error);
    }
    assert_eq!(
        Type::nullable(class()),
        Type::nullable_spelled(class(), Absence::Null)
    );
}

#[test]
fn absence_reads_the_top_level_spelling_only() {
    assert_eq!(Type::nullable(class()).absence(), Some(Absence::Null));
    assert_eq!(undefined(class()).absence(), Some(Absence::Undefined));
    assert_eq!(class().absence(), None);
    assert_eq!(Type::Null.absence(), None);
    assert_eq!(Type::Array(Box::new(undefined(class()))).absence(), None);
}

#[test]
fn holds_undefined_finds_the_spelling_at_each_depth() {
    let nested = Type::Array(Box::new(undefined(class())));
    let in_result = Type::Func(Box::new(FuncType {
        params: vec![Type::I32],
        ret: undefined(class()),
    }));
    let in_map = Type::Map(Box::new(Type::I32), Box::new(undefined(class())));

    assert!(undefined(class()).holds_undefined());
    assert!(!undefined(class()).holds_nested_undefined());

    for ty in [&nested, &in_result, &in_map] {
        assert!(ty.holds_undefined(), "{ty:?}");
        assert!(ty.holds_nested_undefined(), "{ty:?}");
    }

    // Control: the `null` spelling at the same depths holds no `undefined`.
    for ty in [
        Type::nullable(class()),
        Type::Array(Box::new(Type::nullable(class()))),
        Type::Map(Box::new(Type::I32), Box::new(Type::nullable(class()))),
        Type::Null,
        class(),
    ] {
        assert!(!ty.holds_undefined(), "{ty:?}");
        assert!(!ty.holds_nested_undefined(), "{ty:?}");
    }
}

#[test]
fn erase_undefined_maps_every_spelling_to_null() {
    let mut ty = Type::Map(
        Box::new(Type::I32),
        Box::new(Type::Func(Box::new(FuncType {
            params: vec![undefined(class())],
            ret: Type::Array(Box::new(undefined(class()))),
        }))),
    );
    let expected = Type::Map(
        Box::new(Type::I32),
        Box::new(Type::Func(Box::new(FuncType {
            params: vec![Type::nullable(class())],
            ret: Type::Array(Box::new(Type::nullable(class()))),
        }))),
    );
    assert!(ty.holds_undefined());
    ty.erase_undefined();
    assert_eq!(ty, expected);
    assert!(!ty.holds_undefined());

    // A type with no `undefined` spelling does not change.
    let mut plain = Type::Array(Box::new(Type::nullable(class())));
    let before = plain.clone();
    plain.erase_undefined();
    assert_eq!(plain, before);
}

#[test]
fn debug_prints_the_null_spelling_without_a_spelling_field() {
    // §195.7 rule 2: HIR and LIR text carries no spelling for `T | null`.
    assert_eq!(
        format!("{:?}", Type::nullable(class())),
        "Nullable(Class(ClassId(0)))"
    );
    // `T | undefined` prints its spelling as a second field.
    assert_eq!(
        format!("{:?}", undefined(class())),
        "Nullable(Class(ClassId(0)), Undefined)"
    );
    assert_eq!(
        format!("{:?}", Type::Array(Box::new(undefined(Type::I32)))),
        "Array(Nullable(I32, Undefined))"
    );
    assert_eq!(format!("{:?}", Type::Null), "Null");
}
