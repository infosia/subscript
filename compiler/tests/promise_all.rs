use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

#[test]
fn counted_array_results_reject_with_a_non_counted_array_control() {
    for counted in [false, true] {
        let (ty, body) = if counted {
            ("Promise<i32>[]", "const h: Promise<i32> = value(7); const xs: Promise<i32>[] = [h]; await h; return xs;")
        } else {
            ("f64[]", "const xs: f64[] = [7]; await value(0); return xs;")
        };
        let source = format!(
            "async function value(n: i32): Promise<i32> {{ return n; }}
             async function result(): Promise<{ty}> {{ {body} }}
             export async function main(): Promise<void> {{
                const jobs: Promise<{ty}>[] = [result()];
                await Promise.all(jobs);
             }}"
        );
        let checked = check_program(&[SourceFile::new("counted-result.ts", source)]);
        if counted {
            let errors = checked.expect_err("a counted array cannot enter the byte-copy store");
            assert!(errors.iter().any(|error| error.code == RuleCode::S013
                && error.divergence == Some(Divergence::PromiseAllCountedResult)
                && error.message == "Promise.all cannot store a counted result"));
        } else {
            checked.expect("a non-counted array uses a byte-copy store");
        }
    }
}

#[test]
fn stored_void_aggregate_accepts_only_discarded_await_results() {
    for used in [false, true] {
        let use_result = if used {
            "const xs = await all;"
        } else {
            "await all;"
        };
        let source = format!(
            "async function quiet(): Promise<void> {{}}
            export async function main(): Promise<void> {{
                const jobs: Promise<void>[] = [quiet()];
                const all: Promise<void[]> = Promise.all(jobs);
                {use_result}
            }}"
        );
        let checked = check_program(&[SourceFile::new("void-result.ts", source)]);
        if used {
            assert!(checked
                .expect_err("void array is not a value")
                .iter()
                .any(|error| error.divergence == Some(Divergence::PromiseAllVoidValue)));
        } else {
            checked.expect("a void aggregate can be awaited as a statement");
        }
    }
}

#[test]
fn aggregate_has_its_own_must_await_obligation() {
    for awaited in [false, true] {
        let completion = if awaited { "await all;" } else { "" };
        let source = format!(
            "async function value(): Promise<i32> {{ return 7; }}
            export async function main(): Promise<void> {{
                const jobs: Promise<i32>[] = [value()];
                const all: Promise<i32[]> = Promise.all(jobs);
                {completion}
            }}"
        );
        let checked = check_program(&[SourceFile::new("observation.ts", source)]);
        if awaited {
            checked.expect("aggregate observes the inputs");
        } else {
            assert!(checked
                .expect_err("aggregate needs an await")
                .iter()
                .any(|error| error.message
                    == "an async handle is dropped without any await of its completion"));
        }
    }
}
