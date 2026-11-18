//! Array-element captures use the holder's name and lifetime (compiler.md §331 rules 2 and 2a).
//! Measured cost: the seven tests take 0.05 seconds.
use subscript_compiler::{check_program, divergence::Divergence, Diagnostic, RuleCode, SourceFile};

const PRELUDE: &str = "async function work(n: i32): Promise<i32> { return n; }";
const LOCAL: &str = "let f: () => Promise<i32> = () => work(0);";
const ARRAY: &str = "const hs: Promise<i32>[] = [work(1), work(2)];";
const RELEASED: &str = "captured binding `h` is released when its block exits; local `f` is declared outside that block";

fn rejection(files: &[SourceFile], array: Option<&str>) -> Diagnostic {
    let errors = check_program(files).expect_err("an element capture leaves its block");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let error = errors.into_iter().next().unwrap();
    assert_eq!(error.code, RuleCode::S009, "{error:?}");
    assert_eq!(error.divergence, Some(Divergence::CaptureOutlivesBlock));
    if let Some(array) = array {
        assert_eq!(error.message, format!("captured binding `h` borrows an element of {array}; the array can drop that element before the lambda runs; local `f` is declared outside that block"));
        let example = error.example.expect("an outer array has an array example");
        assert!(example.subscript.contains("f = () => hs[1]"));
        assert_eq!(example.collision, "C5");
        assert!(check_program(&[SourceFile::new("alternative.ts", example.subscript)]).is_ok());
    } else {
        assert_eq!(error.message, RELEASED);
        assert!(error.example.is_none(), "{error:?}");
    }
    error
}

fn source(setup: &str, body: &str) -> String {
    format!("{PRELUDE} export async function main(): Promise<void> {{ {setup} {LOCAL} {body} print(`${{await f()}}`); }}")
}

fn pair(prelude: &str, setup: &str, body: &str, array: Option<&str>) {
    let text = format!("{prelude} {}", source(setup, body));
    rejection(&[SourceFile::new("capture.ts", &text)], array);
    // compiler.md §331 rule 1: f can stay in the captured binding's block.
    let local_body = body.replace("f =", &format!("{LOCAL} f ="));
    let local_text = format!("{prelude} {}", source(setup, &local_body));
    let result = check_program(&[SourceFile::new("local.ts", local_text)]);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn both_element_forms_name_an_outer_array() {
    for body in [
        "for (const h of hs) { f = () => h; }",
        "{ const h = hs[1]; f = () => h; }",
    ] {
        pair("", ARRAY, body, Some("array `hs`"));
        pair(
            "",
            "const hs: FixedArray<Promise<i32>, 2> = [work(1), work(2)];",
            body,
            Some("array `hs`"),
        );
    }
}

#[test]
fn patterns_follow_the_recorded_initializer_and_nested_reads_name_an_array() {
    pair(
        "",
        ARRAY,
        "{ const [h] = hs; f = () => h; }",
        Some("array `hs`"),
    );
    let setup = "const hs: Promise<i32>[][] = [[work(1), work(2)]];";
    for body in [
        "for (const [h] of hs) { f = () => h; }",
        "{ const h = hs[0][1]; f = () => h; }",
        "for (const h of hs[0]) { f = () => h; }",
    ] {
        pair("", setup, body, Some("an array"));
    }
    pair("", ARRAY, "{ const [h] = hs.slice(0); f = () => h; }", None);
    pair(
        "",
        "",
        &format!("{{ {ARRAY} const [h] = hs; f = () => h; }}"),
        None,
    );
}

#[test]
fn block_arrays_and_temporaries_keep_the_release_text_and_example() {
    for body in [
        format!("{{ {ARRAY} for (const h of hs) {{ f = () => h; }} }}"),
        format!("{{ {ARRAY} const h = hs[0]; f = () => h; }}"),
        "for (const h of [work(1)]) { f = () => h; }".to_owned(),
        "{ const h = [work(1)][0]; f = () => h; }".to_owned(),
    ] {
        pair("", "", &body, None);
    }
    for body in [
        "for (const h of hs.slice(0)) { f = () => h; }",
        "{ const h = hs.slice(0)[0]; f = () => h; }",
    ] {
        pair("", ARRAY, body, None);
    }
}

#[test]
fn fields_name_the_declared_field_and_call_results_keep_the_release_text() {
    let prelude = "class Box { jobs: Promise<i32>[] = [work(1), work(2)]; }";
    for body in [
        "for (const h of box.jobs) { f = () => h; }",
        "{ const h = box.jobs[1]; f = () => h; }",
    ] {
        pair(
            prelude,
            "const box = new Box();",
            body,
            Some("array `jobs`"),
        );
    }
    let prelude = "function jobs(): Promise<i32>[] { return [work(1), work(2)]; }";
    for body in [
        "for (const h of jobs()) { f = () => h; }",
        "{ const h = jobs()[1]; f = () => h; }",
    ] {
        pair(prelude, "", body, None);
    }
}

#[test]
fn other_initializers_keep_the_release_text_in_the_same_shape() {
    pair("", "", "{ const h = work(1); f = () => h; }", None);
    pair(
        "async function jobs(): Promise<Promise<i32>[]> { return [work(1)]; }",
        "",
        "{ const h = await jobs(); f = () => h[0]; }",
        None,
    );
}

#[test]
fn import_aliases_name_the_declaration_for_both_forms() {
    for body in [
        "for (const h of alias) { f = () => h; }",
        "{ const h = alias[1]; f = () => h; }",
    ] {
        let files = |body: &str| {
            [
                SourceFile::new("array.ts", "export const hs: Promise<i32>[] = [];"),
                SourceFile::entry(
                    "main.ts",
                    format!(
                        "import {{ hs as alias }} from './array'; {}",
                        source("", body)
                    ),
                ),
            ]
        };
        rejection(&files(body), Some("array `hs`"));
        let local_body = body.replace("f =", &format!("{LOCAL} f ="));
        assert!(check_program(&files(&local_body)).is_ok());
    }
}

#[test]
fn the_reject_entries_pin_both_forms() {
    for text in [
        include_str!("../../corpus/reject/r546-capture-of-a-for-of-element.ts"),
        include_str!("../../corpus/reject/r547-capture-of-an-index-copy.ts"),
    ] {
        rejection(&[SourceFile::new("entry.ts", text)], Some("array `hs`"));
    }
}
