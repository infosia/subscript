//! Counted captures obey lexical blocks and the §118 fixed point (§175).

use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

const PRELUDE: &str = "async function work(): Promise<i32> { return 7; }";

fn accepted(source: &str) {
    let result = check_program(&[SourceFile::new("capture.ts", source)]);
    assert!(result.is_ok(), "{:?}", result.err());
}

fn rejected(source: &str, count: usize) {
    let errors = check_program(&[SourceFile::new("capture.ts", source)])
        .expect_err("a capture leaves its binding block");
    assert_eq!(errors.len(), count, "{errors:?}");
    for error in errors {
        assert_eq!(error.code, RuleCode::S009, "{source}\n{error:?}");
        assert_eq!(error.divergence, Some(Divergence::CaptureOutlivesBlock));
        assert_eq!(error.message, "captured binding `h` is released when its block exits; local `f` is declared outside that block");
        let assigned = source.lines().nth(error.pos.line as usize - 1).unwrap();
        let value = &assigned[(error.pos.col - 1) as usize..];
        assert!(
            ["()", "g", "k", "a", "true", "new"]
                .iter()
                .any(|prefix| value.starts_with(prefix)),
            "{value}"
        );
    }
}

fn pair(prefix: &str, value: &str, scope: &str) {
    let local = "let f: () => Promise<i32> = work;";
    rejected(&format!("{PRELUDE} export function main(): void {{ {local} {scope} {{ const h = work(); {prefix} f = {value}; }} }}"), 1);
    accepted(&format!("{PRELUDE} export function main(): void {{ {scope} {{ const h = work(); {local} {prefix} f = {value}; }} }}"));
}

#[test]
fn a_direct_capture_stays_in_its_binding_block() {
    pair("", "() => h", "");
    let entry = include_str!("../../corpus/reject/r396-counted-capture-direct.ts");
    rejected(entry, 1);
    let fragment = Divergence::CaptureOutlivesBlock.entry();
    assert_eq!(fragment.collision, "C5");
    assert!(RuleCode::S009
        .explanation()
        .contains("captured counted binding block"));
    assert!(fragment.why.contains("borrows"));
    rejected(fragment.ts, 1);
    accepted(fragment.subscript);
}

#[test]
fn a_const_function_copy_carries_the_capture_block() {
    pair("const g = () => h;", "g", "");
}

#[test]
fn a_lambda_carries_the_blocks_of_its_captured_carriers() {
    pair("const g = () => h;", "() => g()", "");
    pair("const g = () => h; const k = () => g();", "k", "");
}

#[test]
fn each_loop_iteration_limits_its_counted_captures_to_its_block() {
    for scope in [
        "for (let i: i32 = 0; i < 2; i++)",
        "while (false)",
        "for (const i of [1, 2])",
    ] {
        pair("", "() => h", scope);
    }
    let body = "for (const h of [work()]) { f = () => h; }";
    rejected(
        &format!(
            "{PRELUDE} export function main(): void {{ let f: () => Promise<i32> = work; {body} }}"
        ),
        1,
    );
    accepted(&format!("{PRELUDE} export function main(): void {{ for (const h of [work()]) {{ let f: () => Promise<i32> = () => h; }} }}"));
}

#[test]
fn local_facts_include_later_assignments_without_a_later_call() {
    pair("let a: () => Promise<i32> = work;", "a; a = () => h", "");
    pair("const g = () => h;", "true ? g : work", "");
    pair(
        "let a: () => Promise<i32> = work; while (false) { a = () => h; }",
        "a",
        "",
    );
}

#[test]
fn every_recursive_counted_type_contributes_its_binding_block() {
    for (setup, read) in [
        ("const h = work();", "h"),
        ("const h: Promise<i32>[] = [work()]; await h[0];", "h[0]"),
        (
            "const h: FixedArray<Promise<i32>, 1> = [work()]; await h[0];",
            "h[0]",
        ),
        (
            "const h: Promise<i32>[][] = [[work()]]; await h[0][0];",
            "h[0][0]",
        ),
        ("const h = jobs().next(); await h.value;", "h.value"),
    ] {
        let prelude = format!(
            "{PRELUDE} let observed: Promise<i32>[] = []; function consume(job: Promise<i32>): void {{ observed.push(job); }} function* jobs(): Generator<Promise<i32>> {{ const job = work(); consume(job); observed.pop(); yield job; }}"
        );
        rejected(&format!("{prelude} export async function main(): Promise<void> {{ let f: () => Promise<i32> = work; {{ {setup} f = () => {read}; }} }}"), 1);
        accepted(&format!("{prelude} export async function main(): Promise<void> {{ {{ {setup} let f: () => Promise<i32> = () => {read}; }} }}"));
    }
}

#[test]
fn a_body_binding_and_a_parameter_copy_cover_inner_blocks() {
    accepted(include_str!(
        "../../corpus/accept/a345-counted-capture-block.ts"
    ));
    accepted(&format!("{PRELUDE} function use(h: Promise<i32>): void {{ const p = h; let f: () => Promise<i32> = work; {{ f = () => p; }} }} export function main(): void {{ const h = work(); let f: () => Promise<i32> = work; {{ {{ f = () => h; }} }} }}"));
    pair("", "() => h", "");
}

#[test]
fn uncounted_bindings_keep_the_same_outer_assignment() {
    for (setup, read) in [
        ("const h: i32 = 7;", "h"),
        ("const h: i32[] = [7];", "h[0]"),
        ("const h = new Box();", "h.value"),
    ] {
        accepted(&format!("class Box {{ value: i32 = 7; }} function* numbers(): Generator<i32> {{ yield 7; }} export function main(): void {{ let f: () => i32 = () => 0; {{ {setup} f = () => {read}; }} }}"));
    }
    pair("", "() => h", "");
}

#[test]
fn a_rejected_escape_boundary_owns_the_diagnostic() {
    let source = format!("{PRELUDE} function escape(): () => Promise<i32> {{ let f: () => Promise<i32> = work; {{ const h = work(); return (f = () => h); }} }}");
    let errors = check_program(&[SourceFile::new("escape.ts", source)]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].divergence, Some(Divergence::EscapingCapture));
    accepted(&format!("{PRELUDE} function escape(): () => Promise<i32> {{ let f: () => Promise<i32> = work; {{ return (f = work); }} }}"));

    for value in ["f", "alias", "() => alias()"] {
        let alias = if value == "f" { "" } else { "const alias = f;" };
        let source = format!("{PRELUDE} function escape(): () => Promise<i32> {{ let f: () => Promise<i32> = work; {{ const h = work(); const g = () => h; f = g; }} {alias} return {value}; }}");
        let errors = check_program(&[SourceFile::new("alias.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].divergence, Some(Divergence::EscapingCapture));
        accepted(&format!("{PRELUDE} function escape(): () => Promise<i32> {{ let f: () => Promise<i32> = work; {{ const g = work; f = g; }} return f; }}"));
    }
}

#[test]
fn nested_blocks_keep_binding_identity_when_names_repeat() {
    rejected(&format!("{PRELUDE} export function main(): void {{ const h = work(); let f: () => Promise<i32> = () => h; {{ const h = work(); f = () => h; }} }}"), 1);
    accepted(&format!("{PRELUDE} export function main(): void {{ const h = work(); let f: () => Promise<i32> = () => h; {{ const other: i32 = 1; {{ f = () => h; }} }} }}"));
}

#[test]
fn a_constructor_result_carries_its_argument_blocks() {
    let prelude = format!("{PRELUDE} class Holder {{ cb: () => Promise<i32> = work; constructor(g: () => Promise<i32>) {{ g(); }} }}");
    rejected(&format!("{prelude} export function main(): void {{ let f = new Holder(work); {{ const h = work(); const g = () => h; f = new Holder(g); }} }}"), 1);
    accepted(&format!("{prelude} export function main(): void {{ {{ const h = work(); const g = () => h; let f = new Holder(work); f = new Holder(g); }} }}"));
}

#[test]
fn a_generator_result_carries_its_argument_blocks() {
    let prelude = format!(
        "{PRELUDE} function* generate(g: () => Promise<i32>): Generator<i32> {{ yield 1; }}"
    );
    rejected(&format!("{prelude} export function main(): void {{ let f = generate(work); {{ const h = work(); f = generate(() => h); }} }}"), 1);
    accepted(&format!("{prelude} export function main(): void {{ {{ const h = work(); let f = generate(work); f = generate(() => h); }} }}"));
}

#[test]
fn a_using_scope_keeps_the_source_block() {
    let prelude = format!("{PRELUDE} class Resource {{ [Symbol.dispose](): void {{ }} }}");
    accepted(&format!("{prelude} export function main(): void {{ let f: () => Promise<i32> = work; using r = new Resource(); const h = work(); f = () => h; }}"));
    rejected(&format!("{prelude} export function main(): void {{ let f: () => Promise<i32> = work; {{ using r = new Resource(); const h = work(); f = () => h; }} }}"), 1);
}

#[test]
fn a_generator_capture_uses_its_counted_binding_block() {
    let prelude = "function* numbers(): Generator<i32> { yield 7; }";
    rejected(&format!("{prelude} export function main(): void {{ let f: () => i32 = () => 0; {{ const h = numbers(); f = () => h.next().value; }} }}"), 1);
    accepted(&format!("{prelude} export function main(): void {{ {{ const h = numbers(); let f: () => i32 = () => h.next().value; }} }}"));
}

#[test]
fn a_generator_callback_diagnostic_gives_the_for_of_fix() {
    let source = "function* gen():Generator<i32>{yield 1;} export function main():void{const values:Generator<i32>[]=[gen()];values.forEach(value=>{value.next();});}";
    let files = [SourceFile::new("generator-callback.ts", source)];
    let errors = check_program(&files).expect_err("counted callback");
    assert_eq!(errors[0].code, RuleCode::S014);
    let rendered = subscript_compiler::render_diagnostics(&files, &errors);
    assert!(
        rendered.contains("a generator is a counted type (§176)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("for (const value of values) { value.next(); }"),
        "{rendered}"
    );
    assert!(!rendered.contains("toFixed"), "{rendered}");
    accepted("function* gen():Generator<i32>{yield 1;} export function main():void{const values:Generator<i32>[]=[gen()];for(const value of values){value.next();}}");
}
