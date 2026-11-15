//! §309: measure the rejected site and every form named in its note.
//! Per-test costs measured with one test thread; batch cost 0.03s.

use super::rejection::RejectionSite;
use super::rejection_total;
use crate::ambient::ApiRejectionId;
use crate::divergence::Divergence;
use crate::{check_program, render_diagnostics, SourceFile};

fn pair(site: RejectionSite, note: &str, rejected: &str, accepted: &[&str]) {
    assert!(!accepted.is_empty());
    let files = [SourceFile::entry("main.ts", rejected)];
    rejection_total::clear_reached();
    let diagnostics = check_program(&files).expect_err("original form stays rejected");
    let reached = rejection_total::take_reached();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert!(
        reached.iter().any(|(actual, message, pos)| {
            *actual == site && *message == diagnostic.message && *pos == diagnostic.pos
        }),
        "{reached:?}"
    );
    assert_eq!(diagnostic.note, Some(note));
    assert_eq!(diagnostic.divergence, None);
    assert_eq!(diagnostic.example, None);
    let rendered = render_diagnostics(&files, &diagnostics);
    assert!(rendered.contains(&format!("= note: {note}")), "{rendered}");
    assert!(!rendered.contains("= TypeScript accepts:"));
    for source in accepted {
        check_program(&[SourceFile::entry("main.ts", *source)])
            .expect("each form named by the note is accepted");
    }
}

/// §309: measured checker/render cost 1.358ms.
#[test]
fn s309_date_year_note() {
    let started = std::time::Instant::now();
    pair(
        RejectionSite::Api(ApiRejectionId::DateLocalGetYear, Divergence::DateSubset),
        "Use `date.getUTCFullYear()` to read the UTC year.",
        "const date = new Date(0); date.getYear();",
        &["const date = new Date(0); date.getUTCFullYear();"],
    );
    eprintln!("s309_date_year_note cost: {:?}", started.elapsed());
}

/// §309: measured checker/render cost 1.820ms.
#[test]
fn s309_group_by_note() {
    let started = std::time::Instant::now();
    pair(
        RejectionSite::Api(ApiRejectionId::FormGroupBy, Divergence::DynamicObjectModel),
        "Object groups have no admitted type. Use `Map.groupBy(items, callback)`.",
        "const items: i32[] = [1]; const callback = (x: i32): i32 => x; Object.groupBy(items, callback);",
        &["const items: i32[] = [1]; const callback = (x: i32): i32 => x; Map.groupBy(items, callback);"],
    );
    eprintln!("s309_group_by_note cost: {:?}", started.elapsed());
}

/// §309: measured checker/render cost 21.456ms.
#[test]
fn s309_byte_type_count_note() {
    let started = std::time::Instant::now();
    let accepted = [
        "@ValueType class Box { x: i32 = 1; } Context.bytesOf<Box>(new Box());",
        "@ValueType class Box { x: i32 = 1; } const target: u8[] = [0, 0, 0, 0]; Context.bytesInto<Box>(new Box(), target, 0);",
        "@ValueType class Box { x: i32 = 1; } const bytes: u8[] = [0, 0, 0, 0]; Context.fromBytes<Box>(bytes, 0);",
    ];
    for source in accepted {
        pair(
            RejectionSite::ContextBytesTypeCount,
            "Each byte conversion has one target type. Use `Context.bytesOf<T>(value)`, `Context.bytesInto<T>(value, target, offset)`, or `Context.fromBytes<T>(bytes, offset)`.",
            &source.replace("<Box>", "<Box, i32>"),
            &accepted,
        );
    }
    eprintln!("s309_byte_type_count_note cost: {:?}", started.elapsed());
}

/// §309: measured checker/render cost 1.400ms.
#[test]
fn s309_json_type_count_note() {
    let started = std::time::Instant::now();
    pair(
        RejectionSite::JsonParseTypeCount,
        "JSON parsing has one target type. Use `JSON.parse<T>(text)`.",
        "JSON.parse<i32, i32>(\"1\");",
        &["JSON.parse<i32>(\"1\");"],
    );
    eprintln!("s309_json_type_count_note cost: {:?}", started.elapsed());
}

/// §309: measured checker/render cost 3.905ms.
#[test]
fn s309_fixed_array_view_note() {
    let started = std::time::Instant::now();
    for method in ["keys", "values"] {
        pair(
            RejectionSite::ForOfKeys,
            "FixedArray has no iterator view methods. Iterate values with `for (const value of items)`, or indices with `for (let i: i32 = 0; i < items.length; i++)`.",
            &format!("const items: FixedArray<i32, 1> = [1]; for (const value of items.{method}()) {{}}"),
            &[
                "const items: FixedArray<i32, 1> = [1]; for (const value of items) {}",
                "const items: FixedArray<i32, 1> = [1]; for (let i: i32 = 0; i < items.length; i++) {}",
            ],
        );
    }
    eprintln!("s309_fixed_array_view_note cost: {:?}", started.elapsed());
}

/// §309: measured checker/render cost 7.251ms for both call forms and eight argument types.
#[test]
fn s309_number_coercion_names_typed_conversions() {
    let started = std::time::Instant::now();
    for (declaration, conversion) in [
        ("const value: string = \"1.5\";", "parseFloat(value)"),
        ("const value: i32 = 1;", "value as f64"),
        ("const value: i64 = 1;", "value as f64"),
        ("const value: u8 = 1;", "value as f64"),
        ("const value: boolean = true;", "value ? 1.0 : 0.0"),
        (
            "enum Kind { A = 1 } const value: Kind = Kind.A;",
            "(value as i32) as f64",
        ),
        ("const value: f32 = 1.5;", "value as f64"),
        ("const value: f64 = 1.5;", "value as f64"),
    ] {
        for call in ["Number(value)", "new Number(value)"] {
            let source = format!("{declaration} {call};");
            let diagnostics = check_program(&[SourceFile::entry("main.ts", source)])
                .expect_err("coercion stays rejected");
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            for form in [
                "parseFloat(value)",
                "value as f64",
                "value ? 1.0 : 0.0",
                "(value as i32) as f64",
            ] {
                assert!(
                    diagnostics[0].message.contains(form),
                    "{}",
                    diagnostics[0].message
                );
            }
            assert_eq!(
                diagnostics[0].divergence,
                Some(Divergence::NumberCoercionAndArguments)
            );
            check_program(&[SourceFile::entry(
                "main.ts",
                format!("{declaration} const number: f64 = {conversion};"),
            )])
            .expect("the named conversion accepts this argument type");
        }
    }
    eprintln!(
        "s309_number_coercion_names_typed_conversions cost: {:?}",
        started.elapsed()
    );
}

/// §309.1a: a spread takes precedence over the syntactic argument count.
#[test]
fn s309_spread_precedes_count() {
    let files = [SourceFile::entry(
        "main.ts",
        "const x: f64 = 1.0; x.toFixed(1, ...[]);",
    )];
    rejection_total::clear_reached();
    let diagnostics = check_program(&files).expect_err("spread stays rejected");
    let reached = rejection_total::take_reached();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        reached
            .iter()
            .any(|(site, _, _)| *site == RejectionSite::CallSpread),
        "{reached:?}"
    );
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::VariadicArguments)
    );
}

/// §309.1a: count notes name accepted calls; measured cost below 0.02s.
#[test]
fn s309_argument_count_notes() {
    pair(
        RejectionSite::NumberMethodArgumentCount,
        "Use zero or one argument for `x.toFixed(digits)` and `x.toExponential(digits)`, or exactly one for `x.toString(radix)` and `x.toPrecision(digits)`.",
        "const x: f64 = 1.0; x.toFixed(1, 2);",
        &[
            "const x: f64 = 1.0; x.toFixed(); x.toFixed(1);",
            "const x: f64 = 1.0; x.toExponential(); x.toExponential(1);",
            "const x: f64 = 1.0; x.toString(10); x.toPrecision(1);",
        ],
    );
    let byte_calls = [
        "@ValueType class Box { x: i32 = 1; } Context.bytesOf<Box>(new Box());",
        "@ValueType class Box { x: i32 = 1; } const target: u8[] = [0, 0, 0, 0]; Context.bytesInto<Box>(new Box(), target, 0);",
        "@ValueType class Box { x: i32 = 1; } const bytes: u8[] = [0, 0, 0, 0]; Context.fromBytes<Box>(bytes, 0);",
    ];
    for rejected in [
        "@ValueType class Box { x: i32 = 1; } Context.bytesOf<Box>(new Box(), 0);",
        "@ValueType class Box { x: i32 = 1; } Context.bytesInto<Box>(new Box());",
        "@ValueType class Box { x: i32 = 1; } Context.fromBytes<Box>();",
    ] {
        pair(
            RejectionSite::ContextBytesArgumentCount,
            "Use one argument for `Context.bytesOf<T>(value)`, three for `Context.bytesInto<T>(value, target, offset)`, or two for `Context.fromBytes<T>(bytes, offset)`.",
            rejected,
            &byte_calls,
        );
    }
    pair(
        RejectionSite::ArrayFromArgumentCount,
        "Use exactly one source argument: `Array.from(source)`.",
        "Array.from();",
        &["Array.from([1]);"],
    );
    pair(
        RejectionSite::NumberPredicateCount,
        "Use exactly one f64 argument: `Number.isFinite(value)`, `Number.isNaN(value)`, `Number.isInteger(value)`, or `Number.isSafeInteger(value)`.",
        "Number.isFinite();",
        &["const value: f64 = 1.0; Number.isFinite(value); Number.isNaN(value); Number.isInteger(value); Number.isSafeInteger(value);"],
    );
    for rejected in ["JSON.parse<i32>();", "JSON.parse(\"1\", 0, 0);"] {
        pair(
            RejectionSite::JsonParseCount,
            "Use exactly one text argument: `JSON.parse<T>(text)`.",
            rejected,
            &["JSON.parse<i32>(\"1\");"],
        );
    }
}
