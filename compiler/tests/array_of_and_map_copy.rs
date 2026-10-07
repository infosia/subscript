#[allow(dead_code)]
#[path = "../../codegen/tests/corpus/mod.rs"]
mod corpus;

use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
#[ignore = "Measures corpus checker wall time; run explicitly for performance evidence."]
fn corpus_checker_wall_time() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    let mut entries = Vec::new();
    let head = std::env::var_os("SUBSCRIPT_BENCH_HEAD").is_some();
    for category in ["accept", "warn", "trap", "reject"] {
        let directory = root.join(category);
        for id in corpus::entry_ids(&directory) {
            if head && id == "a269-array-of-and-map-copy" {
                continue;
            }
            if head && id == "r200-map-source-map" {
                entries.push(vec![SourceFile::new(
                    "map-copy-probe.ts",
                    "export function main(): void {
                        const source = new Map<i32, i32>();
                        const copy = new Map<i32, i32>(source);
                        print(`${copy.size}`);
                    }",
                )]);
            } else {
                entries.push(corpus::entry_sources(&directory, &id));
            }
        }
    }
    let mut times = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        for files in &entries {
            let _ = std::hint::black_box(check_program(std::hint::black_box(files)));
        }
        times.push(start.elapsed().as_secs_f64());
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{} entries; times={times:?}; median={} seconds",
        entries.len(),
        times[2]
    );
}

#[test]
fn fixed_arity_forms_check_and_invalid_forms_keep_their_reasons() {
    for form in [
        "Array.of<i32>()",
        "Array.of(7)",
        "Array.of<i32>(7)",
        "new Map(m)",
        "new Map<string, i32>(m)",
    ] {
        let source = format!("export function main(): void {{ const m = new Map<string, i32>(); const value = {form}; }}");
        assert!(
            check_program(&[SourceFile::new("test.ts", source)]).is_ok(),
            "{form}"
        );
    }
    for (form, reason) in [
        ("Array.of()", "empty array literal without context"),
        ("Array.of(1, 2)", "variadic-parameter prerequisite"),
        ("Array.of<i32>(1, 2, 3)", "variadic-parameter prerequisite"),
        ("Array.of<i32, i32>(1)", "exactly one type argument"),
        ("Array.of<i32>(\"wrong\")", "array element"),
        ("Array.of(...[1])", "variadic-parameter prerequisite"),
        ("new Map<string, i32>([[\"a\", 1]])", "no tuple type"),
        ("new Map<string, i32>([1])", "no tuple type"),
        ("new Map<string, i32>(new Set<i32>())", "no tuple type"),
        ("new Map<string, i32>(\"a\")", "no tuple type"),
        ("new Map<string, i32>(m, m)", "no tuple type"),
        ("new Map<i32>(m)", "exactly 2 type argument"),
        ("new Map<i32, i32>(m)", "Map source"),
        ("new Map<string, string>(m)", "Map source"),
        ("new Map()", "requires explicit type arguments"),
        ("new Map([1])", "no tuple type"),
    ] {
        let source = format!("export function main(): void {{ const m = new Map<string, i32>(); const value = {form}; }}");
        let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        if reason == "no tuple type" {
            assert_eq!(errors.len(), 1, "{form}: {errors:?}");
            assert_eq!(errors[0].code, subscript_compiler::RuleCode::S014);
        }
        let errors = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(errors.contains(reason), "{form}: {errors}");
    }
}

#[test]
fn array_of_uses_literal_element_type_checks() {
    for element in [
        "Worker<Message, Message>",
        "Inbox<Message>",
        "Outbox<Message>",
        "i32",
    ] {
        for declaration in [
            format!("const xs: {element}[] = [];"),
            format!("const xs = Array.of<{element}>();"),
        ] {
            let source = format!(
                "class Message {{ value: i32 = 0; }} export function main(): void {{ {declaration} }}"
            );
            let result = check_program(&[SourceFile::new("test.ts", source)]);
            if element == "i32" {
                assert!(result.is_ok(), "{declaration}: {result:?}");
            } else {
                let errors = result.unwrap_err();
                assert!(
                    errors.iter().any(|error| {
                        error.code == subscript_compiler::RuleCode::S100
                            && error.to_string().contains("may not be array elements")
                    }),
                    "{declaration}: {errors:?}"
                );
            }
        }
    }
}

fn source_errors(source: &str, form: &str) -> Vec<subscript_compiler::Diagnostic> {
    check_program(&[SourceFile::new(
        "test.ts",
        format!(
        "function copy(source: Map<i32, i32> | null): void {{ const value = {form}; }} {source}"
    ),
    )])
    .unwrap_err()
}

#[test]
fn map_copy_keeps_unknown_name_diagnostic() {
    for form in ["new Map(missing)", "new Map<i32, i32>(missing)"] {
        let errors = source_errors("", form);
        assert_eq!(errors.len(), 1, "{form}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S016, "{form}: {errors:?}");
    }
}

#[test]
fn map_copy_keeps_source_arity_diagnostic() {
    for form in ["new Map(make())", "new Map<i32, i32>(make())"] {
        let errors = source_errors("function make(key: i32): i32 { return key; }", form);
        assert_eq!(errors.len(), 1, "{form}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100, "{form}: {errors:?}");
        assert!(
            errors[0].to_string().contains("expects 1 argument(s)"),
            "{form}: {errors:?}"
        );
    }
}

#[test]
fn map_copy_requires_null_narrowing() {
    for form in ["new Map(source)", "new Map<i32, i32>(source)"] {
        let errors = source_errors("", form);
        assert_eq!(errors.len(), 1, "{form}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011, "{form}: {errors:?}");
        assert!(
            errors[0]
                .to_string()
                .contains("narrow with a null check first"),
            "{form}: {errors:?}"
        );
        let source = format!("function copy(source: Map<i32, i32> | null): void {{ if (source !== null) {{ const value = {form}; }} }}");
        assert!(check_program(&[SourceFile::new("test.ts", source)]).is_ok());
    }
}

#[test]
fn map_copy_rejects_set_with_pair_iterable_reason() {
    for form in [
        "new Map(new Set<i32>())",
        "new Map<i32, i32>(new Set<i32>())",
    ] {
        let errors = source_errors("", form);
        assert_eq!(errors.len(), 1, "{form}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S014, "{form}: {errors:?}");
        assert!(
            errors[0].to_string().contains("no tuple type"),
            "{form}: {errors:?}"
        );
    }
}

#[test]
fn type_argument_arity_uses_s100() {
    for form in [
        "Array.of<i32, i32>(1)",
        "new Map<i32>(new Map<i32, i32>())",
        "new Map<i32>()",
    ] {
        let errors = source_errors("", form);
        assert_eq!(errors.len(), 1, "{form}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100, "{form}: {errors:?}");
    }
}
