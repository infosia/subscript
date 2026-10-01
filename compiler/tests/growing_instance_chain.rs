//! Generic instance chains are finite (compiler.md §140).
//! Cost: checker calls only; no native program compiles.

use subscript_compiler::{
    check_program, divergence::Divergence, render_diagnostics, RuleCode, SourceFile,
};

fn rejected(source: &str, template: &str, previous: &str, next: &str) {
    let errors = check_program(&[SourceFile::new("main.ts", source)])
        .expect_err("a chain that grows without bound");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, format!(
        "generic template `{template}`: the chain of instances grows without bound from `{previous}` to `{next}` through argument `{}`", next.split_once('<').unwrap().1.strip_suffix('>').unwrap().split(", ").next().unwrap()
    ));
    assert_eq!(errors[0].divergence, Some(Divergence::GrowingInstanceChain));
    assert_eq!(errors[0].pos.file, "main.ts");
    let request = match template {
        "nest" => "nest<W<T>>(",
        "N" if source.contains("N<W<T>>") => "N<W<T>>",
        "N" => "N<T[]>",
        "f" if source.contains("return f<U>(") => "f<U>(",
        "f" if previous == "f<T, U>" => "f<T[], U>(",
        "f" => "f<T[]>(",
        _ => panic!("test needs a request site"),
    };
    let (line, column) = source
        .lines()
        .enumerate()
        .find_map(|(line, text)| {
            text.find(request)
                .map(|column| (line as u32 + 1, column as u32 + 1))
        })
        .expect("request in the test source");
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (line, column));
    let rendered = render_diagnostics(&[SourceFile::new("main.ts", source)], &errors);
    assert!(rendered.contains("C19"), "{rendered}");
    let entry = Divergence::GrowingInstanceChain.entry();
    for fact in [entry.ts, entry.subscript, entry.why] {
        assert!(rendered.contains(fact), "{rendered}");
    }
}

#[test]
fn a_function_request_rejects_a_nested_class_argument() {
    rejected(
        include_str!("../../corpus/reject/r288-growing-instance-chain.ts"),
        "nest",
        "nest<T>",
        "nest<W<T>>",
    );
}

#[test]
fn an_uninstantiated_function_rejects_an_array_expansion() {
    rejected(
        "function f<T>(x: T): void { f<T[]>([x]); } export function main(): void {}",
        "f",
        "f<T>",
        "f<T[]>",
    );
}

#[test]
fn a_method_request_rejects_an_array_expansion() {
    rejected("class H { f<T>(x: T): void { this.f<T[]>([x]); } } export function main(): void { new H().f<i32>(1); }",
        "f", "f<T>", "f<T[]>");
}

#[test]
fn a_static_method_request_rejects_an_array_expansion() {
    rejected("class H { static f<T>(x: T): void { H.f<T[]>([x]); } } export function main(): void { H.f<i32>(1); }",
        "f", "f<T>", "f<T[]>");
}

#[test]
fn a_class_shape_rejects_a_nested_class_argument() {
    rejected("class W<T> { v: T; constructor(v: T) { this.v = v; } } class N<T> { next(): N<W<T>> | null { return null; } } export function main(): void { new N<i32>(); }",
        "N", "N<T>", "N<W<T>>");
}

#[test]
fn a_deferred_class_body_keeps_its_active_chain() {
    rejected("class N<T> { next(): void { new N<T[]>(); } } function use(n: N<i32>): void {} export function main(): void {}",
        "N", "N<T>", "N<T[]>");
}

#[test]
fn mutual_recursion_rejects_the_return_to_the_first_template() {
    rejected("function f<T>(x: T, n: i32): i32 { if (n <= 0) { return 0; } return g<T[]>([x], n - 1); } function g<U>(x: U, n: i32): i32 { return f<U>(x, n); } export function main(): void { f<i32>(1, 3); }",
        "f", "f<i32>", "f<i32[]>");
}

#[test]
fn growth_in_one_argument_keeps_an_equal_argument() {
    rejected(
        "function f<T, U>(x: T, y: U): void { f<T[], U>([x], y); } export function main(): void {}",
        "f",
        "f<T, U>",
        "f<T[], U>",
    );
}

#[test]
fn plain_recursion_and_unrelated_arguments_stay_accepted() {
    for source in [
        "function f<T>(x: T, n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<T>(x, n - 1); } export function main(): void { print(`${f<i32>(1, 3)}`); }",
        "function f<T>(x: T, n: i32): i32 { if (n <= 0) { return 0; } return 1 + f<string>(\"s\", n - 1); } export function main(): void { print(`${f<i32>(1, 3)}`); }",
        "class N<T> { next(): N<T> | null { return null; } } export function main(): void { new N<i32>(); }",
        "class H { f<T>(x: T, n: i32): i32 { if (n <= 0) { return 0; } return 1 + this.f<T>(x, n - 1); } } export function main(): void { print(`${new H().f<i32>(1, 3)}`); }",
    ] {
        check_program(&[SourceFile::new("main.ts", source)]).expect("a finite instance chain");
    }
}

#[test]
fn an_opaque_check_of_a_concrete_request_rejects_mutual_growth() {
    rejected("function root<T>(x: T): void { f<i32>(1, 3); } function f<T>(x: T, n: i32): i32 { if (n <= 0) { return 0; } return g<T[]>([x], n - 1); } function g<U>(x: U, n: i32): i32 { return f<U>(x, n); } export function main(): void {}",
        "f", "f<i32>", "f<i32[]>");
}

#[test]
fn static_and_instance_methods_keep_distinct_template_identities() {
    let source = "class H { static f<T>(x: T, n: i32): i32 { if (n <= 0) { return 0; } return new H().f<T[]>([x], n - 1); } f<U>(x: U, n: i32): i32 { return 3; } } export function main(): void { print(`${H.f<i32>(1, 3)}`); }";
    check_program(&[SourceFile::new("main.ts", source)]).expect("two distinct templates");
}

#[test]
fn an_uninstantiated_method_rejects_an_array_expansion() {
    rejected(
        "class H { f<T>(x: T): void { this.f<T[]>([x]); } } export function main(): void {}",
        "f",
        "f<T>",
        "f<T[]>",
    );
}

#[test]
fn an_uninstantiated_class_body_rejects_an_array_expansion() {
    rejected(
        "class N<T> { next(): void { new N<T[]>(); } } export function main(): void {}",
        "N",
        "N<T>",
        "N<T[]>",
    );
}

#[test]
fn instances_from_distinct_declarations_keep_the_file_label() {
    let files = [
        SourceFile::entry("main.ts", "import { W as Other } from './other'; class W<T> { v: T; constructor(v: T) { this.v = v; } } function f<T>(x: T): void { new Other<T>(x); f<W<T>>(new W<T>(x)); } export function main(): void {}"),
        SourceFile::new("other.ts", "export class W<T> { v: T; constructor(v: T) { this.v = v; } }"),
    ];
    let errors = check_program(&files).expect_err("a chain that grows without bound");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, "generic template `f`: the chain of instances grows without bound from `f<T>` to `f<W<T> (main.ts)>` through argument `W<T> (main.ts)`");
    assert_eq!(errors[0].divergence, Some(Divergence::GrowingInstanceChain));
    let column = files[0].source.find("f<W<T>>").expect("request site") as u32 + 1;
    assert_eq!((errors[0].pos.line, errors[0].pos.col), (1, column));
    assert!(render_diagnostics(&files, &errors).contains("collisions.md C19"));
}

fn one_growth(
    files: &[SourceFile],
    template: &str,
    previous: &str,
    next: &str,
    argument: &str,
    request: &str,
) {
    let errors = check_program(files).expect_err("an expanding cycle");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let error = &errors[0];
    assert_eq!(error.code, RuleCode::S100);
    assert_eq!(error.message, format!("generic template `{template}`: the chain of instances grows without bound from `{previous}` to `{next}` through argument `{argument}`"));
    assert_eq!(error.divergence, Some(Divergence::GrowingInstanceChain));
    let source = files
        .iter()
        .find(|file| file.name == error.pos.file)
        .expect("diagnostic file");
    let offset = source.source.find(request).expect("request site");
    let prefix = &source.source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix.rsplit('\n').next().unwrap().len() as u32 + 1;
    assert_eq!((error.pos.line, error.pos.col), (line, column));
    assert!(render_diagnostics(files, &errors).contains("collisions.md C19"));
}

#[test]
fn every_supported_wrapper_records_an_expanding_edge() {
    for (source_argument, rendered) in [
        ("Map<i32, T>", "Map<i32, T>"),
        ("(x: T) => i32", "(T) => i32"),
        ("() => T", "() => T"),
        ("Array<T>", "T[]"),
        ("Set<T>", "Set<T>"),
        ("T[][]", "T[][]"),
        ("Promise<T>", "Promise<T>"),
        ("T | null", "T | null"),
    ] {
        let source = format!(
            "function f<T>(): void {{ f<{source_argument}>(); }} export function main(): void {{}}"
        );
        one_growth(
            &[SourceFile::new("main.ts", source)],
            "f",
            "f<T>",
            &format!("f<{rendered}>"),
            rendered,
            &format!("f<{source_argument}>"),
        );
    }
}

#[test]
fn class_signature_field_and_constructor_requests_keep_edges() {
    for (source, next, argument, request) in [
        ("class N<T> { next(): N<N<T>> | null { return null; } } export function main(): void { new N<i32>(); }", "N<N<T>>", "N<T>", "N<N<T>>"),
        ("class N<T> { next: N<T[]> | null = null; } export function main(): void { new N<i32>(); }", "N<T[]>", "T[]", "N<T[]>"),
        ("class N<T> { constructor() { new N<T[]>(); } } export function main(): void { new N<i32>(); }", "N<T[]>", "T[]", "N<T[]>"),
    ] {
        one_growth(&[SourceFile::new("main.ts", source)], "N", "N<T>", next, argument, request);
    }
}

#[test]
fn swapped_growth_returns_to_a_position_at_depth_two() {
    let source =
        "class W<T> {} function f<A, B>(): void { f<B, W<A>>(); } export function main(): void { f<i32, string>(); }";
    one_growth(
        &[SourceFile::new("main.ts", source)],
        "f",
        "f<i32, string>",
        "f<W<i32>, W<string>>",
        "W<i32>",
        "f<B, W<A>>",
    );
}

#[test]
fn a_plain_edge_then_an_expanding_edge_rejects_mutual_growth() {
    let source = "class W<T> {} function f<T>(): void { g<T>(); } function g<U>(): void { f<W<U>>(); } export function main(): void { f<i32>(); }";
    one_growth(
        &[SourceFile::new("main.ts", source)],
        "f",
        "f<i32>",
        "f<W<i32>>",
        "W<i32>",
        "f<W<U>>",
    );
}

#[test]
fn cross_module_requests_compose_edges() {
    let files = [
        SourceFile::entry("main.ts", "import { g } from './other'; export function f<T>(): void { g<T>(); } export function main(): void { f<i32>(); }"),
        SourceFile::new("other.ts", "import { f } from './main'; class W<T> {} export function g<U>(): void { f<W<U>>(); }"),
    ];
    one_growth(&files, "f", "f<i32>", "f<W<i32>>", "W<i32>", "f<W<U>>");
}

#[test]
fn a_method_target_from_a_call_receiver_uses_the_resolved_chain() {
    let source = "class R { step<T>(x: T): void { f<T[]>([x]); } } function receiver<T>(): R { return new R(); } function f<U>(x: U): void { receiver<U[]>().step<U>(x); } export function main(): void { f<i32>(1); }";
    one_growth(
        &[SourceFile::new("main.ts", source)],
        "f",
        "f<i32>",
        "f<i32[]>",
        "i32[]",
        "f<T[]>([x])",
    );
}

#[test]
fn an_s011_inside_an_expanding_argument_has_priority() {
    for source in [
        "class N<T> { next(): void { new N<(T | null)[]>(); } } export function main(): void { new N<i32>(); }",
        "@ValueType class W<T> { v: T; constructor(v:T) { this.v=v; } } function f<T>(): void { f<W<T> | null>(); } export function main(): void { f<i32>(); }",
    ] {
        let errors = check_program(&[SourceFile::new("main.ts", source)]).expect_err("an invalid union");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, RuleCode::S011);
    }
}

#[test]
fn unused_templates_do_not_check_growth_through_another_body() {
    for source in [
        "class W<T> {} function f<T>(): void { g<T>(); } function g<U>(): void { f<W<U>>(); } export function main(): void {}",
        "class W<T> {} function f<A, B>(): void { f<B, W<A>>(); } export function main(): void {}",
        "class R { step<T>(x: T): void { f<T[]>([x]); } } function receiver<T>(): R { return new R(); } function f<U>(x: U): void { receiver<U[]>().step<U>(x); } export function main(): void {}",
    ] {
        check_program(&[SourceFile::new("main.ts", source)]).expect("opaque roots do not check nested instance bodies");
    }
}
