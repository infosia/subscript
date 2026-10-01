//! Constraint kinds derived from the resolved Type variants (§143 rule 4).

use super::*;

macro_rules! kind {
    ($name:literal, $constraint:literal, $argument_type:literal, $argument:literal) => {
        Kind {
            name: $name,
            constraint: concat!(" extends ", $constraint),
            value_type: "T",
            argument_type: $argument_type,
            argument: $argument,
        }
    };
}

// Each named variant has a constraint. Class covers reference, value, and generic instances.
const NAMED: &[(&str, Kind)] = &[
    ("I8", kind!("i8", "i8", "i32", "3")),
    ("I16", kind!("i16", "i16", "i32", "3")),
    ("U16", kind!("u16", "u16", "i32", "3")),
    ("U32", kind!("u32", "u32", "i32", "3")),
    ("I64", kind!("i64", "i64", "i32", "3")),
    ("U64", kind!("u64", "u64", "i32", "3")),
    ("F32", kind!("f32", "f32", "i32", "3")),
    ("F16", kind!("f16", "f16", "i32", "3")),
    ("Bool", kind!("boolean", "boolean", "boolean", "true")),
    ("Str", kind!("string", "string", "string", "\"s\"")),
    ("Date", kind!("date", "Date", "Date", "new Date(0)")),
    (
        "RegExp",
        kind!("regexp", "RegExp", "RegExp", "new RegExp(\"a\")"),
    ),
    ("Enum", kind!("enum", "E", "E", "E.B")),
    (
        "StringAlias",
        kind!("string-alias", "Label", "Label", "\"a\""),
    ),
    (
        "Class",
        kind!("value-class", "Value", "Value", "new Value()"),
    ),
    (
        "Class",
        kind!(
            "generic-class",
            "GenericBox<i32>",
            "GenericBox<i32>",
            "new GenericBox<i32>(3)"
        ),
    ),
    (
        "Nullable",
        kind!("nullable-class", "Box | null", "Box", "new Box()"),
    ),
    (
        "FixedArray",
        kind!(
            "fixed-array",
            "FixedArray<i32, 2>",
            "FixedArray<i32, 2>",
            "[1, 2]"
        ),
    ),
    (
        "Map",
        kind!(
            "map",
            "Map<i32, i32>",
            "Map<i32, i32>",
            "new Map<i32, i32>()"
        ),
    ),
    (
        "Set",
        kind!("set", "Set<i32>", "Set<i32>", "new Set<i32>()"),
    ),
    ("Void", kind!("void", "void", "void", "nothing()")),
    ("Null", kind!("null", "null", "null", "null")),
    (
        "Worker",
        kind!("worker", "Worker<Box, Box>", "Worker<Box, Box>", "worker()"),
    ),
    (
        "Inbox",
        kind!("inbox", "Inbox<Box>", "Inbox<Box>", "inbox()"),
    ),
    (
        "Outbox",
        kind!("outbox", "Outbox<Box>", "Outbox<Box>", "outbox()"),
    ),
    (
        "Generator",
        kind!(
            "generator",
            "Generator<i32>",
            "Generator<i32>",
            "generate()"
        ),
    ),
    (
        "AsyncHandle",
        kind!("async-handle", "Promise<i32>", "Promise<i32>", "promised()"),
    ),
];

// These variants have no legal constraint spelling in the project surface.
const UNNAMED: &[(&str, &str)] = &[
    (
        "Object",
        "object is a boundary-only type, so a script constraint cannot name it.",
    ),
    (
        "TypeParameter",
        "Parameter identity is covered by the linked value roles, not a concrete constraint.",
    ),
    (
        "GenericNumber",
        "An operation creates this unsized result; a source constraint resolves number as f64.",
    ),
    (
        "GenericUnion",
        "An operation creates this union; general union constraints are outside the surface.",
    ),
    (
        "IterResult",
        "The next() result has no named source type in the project surface.",
    ),
    (
        "Error",
        "The checker creates this poison type after an error; source code cannot name it.",
    ),
];

/// Reads each Type variant from its declaration.
pub(super) fn variants(source: &str) -> Vec<&str> {
    let body = source
        .split_once("pub enum Type {")
        .unwrap()
        .1
        .split_once("\n}")
        .unwrap()
        .0;
    body.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                return None;
            }
            let name = line.split(['(', ',']).next().unwrap();
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "unparsed Type variant: {line}"
            );
            Some(name)
        })
        .collect()
}

fn uncovered(source: &str) -> Vec<&str> {
    variants(source)
        .into_iter()
        .filter(|name| {
            !matches!(*name, "I32" | "U8" | "F64" | "Array" | "Func")
                && !NAMED.iter().any(|(variant, _)| variant == name)
                && !UNNAMED
                    .iter()
                    .any(|(variant, reason)| variant == name && !reason.is_empty())
        })
        .collect()
}

/// Derives the additional constraint kinds from Type variants.
pub(super) fn additional() -> Vec<Kind> {
    let source = include_str!("../../src/types.rs");
    assert!(
        uncovered(source).is_empty(),
        "uncovered Type variants: {:?}",
        uncovered(source)
    );
    variants(source)
        .into_iter()
        .flat_map(|name| {
            NAMED
                .iter()
                .filter(move |(variant, _)| *variant == name)
                .map(|(_, kind)| *kind)
        })
        .collect()
}

/// Includes the required base kinds and every additional named variant.
pub(super) fn all() -> Vec<Kind> {
    KINDS
        .iter()
        .chain(product::FUNCTION_KINDS)
        .copied()
        .chain(additional())
        .collect()
}

/// Supplies the declarations that each named constraint needs.
pub(super) fn prelude(kind: &Kind) -> &'static str {
    match kind.name {
        "enum" => "enum E { A = 0, B = 1 }",
        "string-alias" => "type Label = \"a\" | \"b\";",
        "value-class" => "@ValueType class Value { v: i32 = 1; }",
        "generic-class" => {
            "class GenericBox<A> { value: A; constructor(value: A) { this.value = value; } }"
        }
        "void" => "function nothing(): void {}",
        "worker" => "function worker(): Worker<Box, Box> { throw new Error(\"unused\"); }",
        "inbox" => "function inbox(): Inbox<Box> { throw new Error(\"unused\"); }",
        "outbox" => "function outbox(): Outbox<Box> { throw new Error(\"unused\"); }",
        "generator" => "function* generate(): Generator<i32> { yield 1; }",
        "async-handle" => "async function promised(): Promise<i32> { return 1; }",
        _ => "",
    }
}

/// Selects kinds whose base roles include a fresh numeric result.
pub(super) fn numeric(kind: &Kind) -> bool {
    matches!(
        kind.name,
        "numeric"
            | "f64"
            | "u8"
            | "i8"
            | "i16"
            | "u16"
            | "u32"
            | "i64"
            | "u64"
            | "f32"
            | "f16"
            | "enum"
    )
}

#[test]
fn every_type_variant_has_a_constraint_or_a_reason() {
    let source = include_str!("../../src/types.rs");
    assert!(uncovered(source).is_empty(), "{:?}", uncovered(source));
    assert_eq!(variants(source).len(), 37);
    assert_eq!(additional().len(), 27);
    assert_eq!(uncovered("pub enum Type {\nFuture,\n}"), ["Future"]);
}

// These records apply to the named kind/site pair, independently of its checker verdict.
/// Supplies restriction records for the independent kind and consumer site.
pub(super) fn records(cell: &Cell) -> Vec<Divergence> {
    let name = &cell.name;
    let mut records: Vec<_> = cell.divergence.into_iter().collect();
    if name.starts_with("product-void-")
        && [
            "-parameter-forward-argument-",
            "-parameter-ternary-arm-",
            "-parameter-nullish-right-",
            "-parameter-default-parameter-",
            "-nullable-nullish-right-",
        ]
        .iter()
        .any(|site| name.contains(site))
    {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "C21",
            message: "cannot bind a `void` value",
            token: "binding whose type is inferred from a `void`",
        });
    }
    if name.contains("async-handle-") {
        records.push(Divergence {
            code: RuleCode::S013,
            record: "compiler.md §70",
            message: "an async handle is dropped without any await",
            token: "Dropping it without awaiting remains",
        });
    }
    if [
        "string",
        "string-alias",
        "map",
        "set",
        "generator",
        "async-handle",
    ]
    .iter()
    .any(|kind| name.starts_with(&format!("product-{kind}-")))
        && name.contains("-array-pattern-")
    {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "compiler.md §107.1",
            message: "an array binding pattern reads",
            token: "An array binding pattern over a `T[]` or a `FixedArray<T, N>`",
        });
    }
    if name.starts_with("product-")
        && ["worker", "inbox", "outbox"]
            .iter()
            .any(|kind| name.starts_with(&format!("product-{kind}-")))
        && [
            "-forward-argument-",
            "-class-field-type-",
            "-default-parameter-",
        ]
        .iter()
        .any(|site| name.contains(site))
    {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "compiler.md §40",
            message: "Worker, Inbox, and Outbox values may not be class fields",
            token: "class field, array element, or lambda capture rejects",
        });
    }
    if name.starts_with("product-null-") {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "compiler.md §97",
            message: "null",
            token: "bare `null` initializer infers no",
        });
    }
    if (name.starts_with("api-print-0-value-") || name.starts_with("api-JSON-parse-0-value-"))
        && name.contains("-string-alias-")
    {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "Q32",
            message: if name.starts_with("api-print-") {
                "type mismatch: the argument expects `string`"
            } else {
                "type mismatch: `JSON.parse` text expects `string`"
            },
            token: "Comparison or assignment with plain",
        });
    }
    if name.starts_with("product-string-alias-") && name.contains("-case-parameter-") {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "compiler.md §41",
            message: "case",
            token: "label must be a string literal naming a member",
        });
    }
    if name.starts_with("product-") && name.contains("-field-initializer-") {
        if name.starts_with("product-void-") {
            records.push(Divergence {
                code: RuleCode::S100,
                record: "C21",
                token: "binding whose type is inferred from a `void`",
                message: "cannot bind a `void` value",
            });
        }
        if name.contains("-nullish-result-") {
            records.push(Divergence {
                code: RuleCode::S100,
                record: "C7",
                token: "A non-nullable left",
                message: "left operand of `??` is not nullable",
            });
        }
        if ["worker", "inbox", "outbox"]
            .iter()
            .any(|kind| name.starts_with(&format!("product-{kind}-")))
        {
            records.push(Divergence {
                code: RuleCode::S100,
                record: "compiler.md §40",
                token: "class field, array element, or lambda capture rejects",
                message: "Worker, Inbox, and Outbox values may not be class fields",
            });
        }
    }
    if ["worker", "inbox", "outbox"]
        .iter()
        .any(|kind| name.starts_with(&format!("product-{kind}-")))
        && name.contains("-default-parameter-")
    {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "compiler.md §40",
            token: "class field, array element, or lambda capture rejects",
            message: "values may not be captured",
        });
    }
    if name.starts_with("api-")
        && ["Outbox-post-", "Worker-post-"]
            .iter()
            .any(|api| name.contains(api))
        && name.contains("-value-value-class-")
    {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "compiler.md §40",
            token: "Message classes are transferable per stdlib §16.2",
            message: "type mismatch: the argument expects `Box`",
        });
    }
    if name.starts_with("api-Worker-spawn-") {
        records.push(Divergence {
            code: RuleCode::S100,
            record: "compiler.md §40",
            token: "Message classes are transferable per stdlib §16.2",
            message: "worker message type",
        });
    }
    records.extend(super::destinations::records(cell));
    records
}
