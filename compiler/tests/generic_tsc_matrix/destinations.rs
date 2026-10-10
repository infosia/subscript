//! Destination constructors derived from Type (§143 rules 1b and 4).

use super::*;

// Each constructor has a component position. Function parameters use contravariance.
const CONSTRUCTORS: &[(&str, &str, &str)] = &[
    ("Nullable", "nullable", "A | null"),
    ("Array", "array", "A[]"),
    ("FixedArray", "fixed-array", "FixedArray<A, 2>"),
    ("Map", "map-value", "Map<string, A>"),
    ("Map", "map-array-value", "Map<string, A[]>"),
    ("Map", "map-key", "Map<A, i32>"),
    ("Set", "set", "Set<A>"),
    ("Class", "class", "G<A>"),
    ("Func", "function-parameter", "(x: A) => i32"),
    ("Func", "function-return", "() => A"),
    ("Worker", "worker-input", "Worker<A, Box>"),
    ("Worker", "worker-output", "Worker<Box, A>"),
    ("Inbox", "inbox", "Inbox<A>"),
    ("Outbox", "outbox", "Outbox<A>"),
    ("Generator", "generator", "Generator<A>"),
    ("AsyncHandle", "async-handle", "Promise<A>"),
];

const UNEXPRESSED: &[(&str, &str)] = &[
    (
        "TypeParameter",
        "A parameter identity is the source component, not a constructor.",
    ),
    (
        "GenericUnion",
        "General union declarations are outside the project surface.",
    ),
    (
        "IterResult",
        "The iterator result has no named declaration type.",
    ),
];

fn uncovered(source: &str) -> Vec<&str> {
    let body = source.split_once("pub enum Type {").unwrap().1;
    kinds::variants(source)
        .into_iter()
        .filter(|variant| {
            let declaration = body
                .lines()
                .find(|line| line.trim().starts_with(&format!("{variant}(")));
            declaration.is_some_and(|line| {
                line.contains("Type") || line.contains("FuncType") || line.contains("ClassId")
            }) && !CONSTRUCTORS.iter().any(|(name, _, _)| name == variant)
                && !UNEXPRESSED
                    .iter()
                    .any(|(name, reason)| name == variant && !reason.is_empty())
        })
        .collect()
}

/// Builds each constraint and constructor at all three assignment sites.
pub(super) fn cells() -> Vec<Cell> {
    assert!(uncovered(include_str!("../../src/types.rs")).is_empty());
    let mut cells = Vec::new();
    let mut omitted = 0;
    for kind in kinds::all()
        .iter()
        .filter(|kind| !kind.constraint.is_empty())
    {
        let constraint = kind.constraint.trim_start_matches(" extends ");
        for (_, constructor, spelling) in
            CONSTRUCTORS.iter().copied().chain([("", "identity", "A")])
        {
            let from = api::substitute(spelling, &[("A".into(), "T".into())]);
            let to = if constructor == "nullable" && kind.name == "nullable-class" {
                constraint.into()
            } else {
                api::substitute(spelling, &[("A".into(), format!("({constraint})"))])
            };
            for reverse in [false, true] {
                let (from, to) = if reverse { (&to, &from) } else { (&from, &to) };
                for site in ["initializer", "return", "argument"] {
                    let body = match site {
                        "initializer" => format!("const result: {to} = x;"),
                        "return" => "return x;".into(),
                        _ => "take(x);".into(),
                    };
                    let ret = if site == "return" {
                        to.as_str()
                    } else {
                        "void"
                    };
                    let prelude = format!(
                        "class Box {{ v: i32 = 1; }} {} \
                     class G<A> {{ value: A; constructor(value: A) {{ this.value = value; }} }} \
                     function take(x: {to}): void {{}}",
                        kinds::prelude(kind)
                    );
                    let declaration = format!(
                        "{prelude} function g<T{}>(x: {from}): {ret} {{ {body} }}",
                        kind.constraint
                    );
                    let argument = constraint;
                    let concrete_from =
                        api::substitute(spelling, &[("A".into(), format!("({argument})"))]);
                    let main = format!(
                        "function admitted(x: {concrete_from}): void {{ g<{argument}>(x); }}"
                    );
                    let concrete = format!("{prelude} function g(x: {from}): {ret} {{ {body} }} function admitted(x: {concrete_from}): void {{ g(x); }} export function main(): void {{}}");
                    let concrete = replace_parameter_names(&concrete, &format!("({argument})"));
                    for instance in [false, true] {
                        if instance
                            && check_program(&[SourceFile::new("control.ts", &concrete)]).is_err()
                        {
                            omitted += 1;
                            continue;
                        }
                        cells.push(build_cell(CellInput {
                            name: &format!(
                                "destination-{}-{}{constructor}-{site}",
                                kind.name,
                                if reverse { "reverse-" } else { "" }
                            ),
                            declaration: &format!(
                                "{declaration} {}",
                                if instance { &main } else { "" }
                            ),
                            main_body: "",
                            instance,
                            concrete_source: if instance {
                                Some(concrete.clone())
                            } else {
                                None
                            },
                            divergence: None,
                        }));
                    }
                }
            }
        }
    }
    // C21 excludes concrete destinations that use void outside a result type.
    // S013 excludes those with a parameter that holds a handle and is not
    // discharged (compiler.md §188.1 rule 2).
    assert_eq!(omitted, 1494, "destination instance admission changed");
    eprintln!("destination axis: 35 constrained kinds, 17 constructors, 2 directions, 3 sites, {} cells, {omitted} omitted instances", cells.len());
    cells
}

#[test]
fn every_composite_variant_has_a_destination_or_a_reason() {
    assert!(uncovered(include_str!("../../src/types.rs")).is_empty());
    assert_eq!(
        uncovered("pub enum Type {\nFuture(Box<Type>),\n}"),
        ["Future"]
    );
}

/// Selects restriction records from the declared constructor and constraint.
pub(super) fn records(cell: &Cell) -> Vec<Divergence> {
    let mut records = Vec::new();
    if !cell.name.starts_with("destination-") {
        return records;
    }
    let kinds = kinds::all();
    let kind = kinds
        .iter()
        .filter(|kind| {
            cell.name
                .starts_with(&format!("destination-{}-", kind.name))
        })
        .max_by_key(|kind| kind.name.len())
        .unwrap();
    let site = cell
        .name
        .strip_prefix(&format!("destination-{}-", kind.name))
        .unwrap();
    let site = site.strip_prefix("reverse-").unwrap_or(site);
    let record = |code, record, token| Divergence {
        code,
        record,
        token,
        message: match record {
            "C7" => "unions are limited to `Ref | null`",
            "stdlib.md §10" => "key",
            "stdlib.md §16.2" => "worker message type",
            "compiler.md §40" => "Worker, Inbox, and Outbox values may not be",
            _ => "unused",
        },
    };
    if site.starts_with("nullable-")
        && !matches!(
            kind.name,
            "class"
                | "nullable"
                | "nullable-class"
                | "generic-class"
                | "map"
                | "set"
                | "function-return"
                | "function-parameter"
                | "worker"
                | "inbox"
                | "outbox"
        )
    {
        records.push(record(
            RuleCode::S011,
            "C7",
            "the only union form is `Ref | null`",
        ));
    }
    if (site.starts_with("map-key-") || site.starts_with("set-"))
        && matches!(
            kind.name,
            "array"
                | "async-handle"
                | "f16"
                | "fixed-array"
                | "function-parameter"
                | "function-return"
                | "generator"
                | "map"
                | "null"
                | "nullable-class"
                | "regexp"
                | "set"
                | "string-alias"
                | "value-class"
                | "void"
        )
    {
        records.push(record(
            RuleCode::S014,
            "stdlib.md §10",
            "A key type must have a defined equality",
        ));
    }
    if ["worker-input-", "worker-output-", "inbox-", "outbox-"]
        .iter()
        .any(|prefix| site.starts_with(prefix))
    {
        records.push(record(
            RuleCode::S100,
            "stdlib.md §16.2",
            "A message type is a plain reference class",
        ));
    }
    if matches!(kind.name, "worker" | "inbox" | "outbox") {
        if [
            "array-",
            "fixed-array-",
            "map-value-",
            "map-array-value-",
            "map-key-",
            "set-",
        ]
        .iter()
        .any(|prefix| site.starts_with(prefix))
        {
            records.push(record(
                RuleCode::S100,
                "compiler.md §40",
                "an affine type is illegal as ANY container type",
            ));
        }
        if site.starts_with("class-") {
            records.push(record(
                RuleCode::S100,
                "compiler.md §40",
                "class field, array element, or lambda capture rejects",
            ));
        }
    }
    records
}
