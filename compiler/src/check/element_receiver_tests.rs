//! Tests of the element receiver check (compiler.md §221).

use crate::check::rejection::RejectionSite;
use crate::hir::{ArrFn, BuiltinMethod, ElementMove};
use crate::{check_program, Diagnostic, SourceFile};

const CLASS: &str = "@ValueType class V { a: i32 = 1; \
     grow(list: V[]): void { list.push(new V()); } plain(): i32 { return this.a; } }\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("main.ts", source)])
        .err()
        .unwrap_or_default()
}

fn rejected(source: &str) -> Diagnostic {
    let found = diagnostics(source);
    assert_eq!(found.len(), 1, "{source}\n{found:?}");
    let first = found.into_iter().next().expect("one diagnostic");
    assert!(
        first.message.contains("can move a dynamic array of"),
        "{source}\n{}",
        first.message
    );
    first
}

fn accepted(source: &str) {
    assert_eq!(diagnostics(source), Vec::new(), "{source}");
}

/// A class with a method `step` whose body is `body`, and a call on an element.
fn on_element(body: &str, declarations: &str) -> String {
    format!(
        "@ValueType class V {{ a: i32 = 1; step(): void {{ {body} }} }}\n\
         const all: V[] = [new V()];\n{declarations}\n\
         export function main(): void {{ all[0].step(); }}\n"
    )
}

#[test]
fn the_diagnostic_names_the_method_the_operation_its_position_and_the_fix() {
    let source = format!(
        "{CLASS}export function main(): void {{\n  const list: V[] = [];\n  list[0].grow(list);\n}}\n"
    );
    let found = rejected(&source);
    assert_eq!(found.code, RejectionSite::ElementMethodMovesArray.class().0);
    assert_eq!(found.pos.line, 4);
    assert_eq!(
        found.message,
        "`V.grow` can move a dynamic array of `V` by `push` at main.ts:1:58, and `list[0]` is an \
         element of such an array; call it on a copy: `const t: V = list[0]; t.grow(...); \
         list[0] = t;`"
    );
    assert_eq!(
        found.divergence,
        RejectionSite::ElementMethodMovesArray
            .class()
            .1
            .divergence()
    );
}

#[test]
fn an_accessor_fix_spells_a_read() {
    let found = rejected(
        "class W { list: V[] = []; }\nconst w: W = new W();\n\
         @ValueType class V { a: i32 = 1; get size(): i32 { w.list.push(new V()); return 1; } }\n\
         export function main(): void { print(`${w.list[0].size}`); }\n",
    );
    assert!(
        found
            .message
            .ends_with("`const t: V = w.list[0]; t.size; w.list[0] = t;`"),
        "{}",
        found.message
    );
}

/// Rule 1. Each place in the first list crosses an element after its last
/// reference handle; each place in the second list does not. A value class
/// has no reference field, so a handle in a path is a reference element.
#[test]
fn a_place_is_unstable_after_its_last_reference_handle() {
    let program = |grow: &str, place: &str| {
        format!(
            "class R {{ v: V = new V(); list: V[] = []; }}\n\
             @ValueType class V {{ a: i32 = 1; grow(): void {{ {grow} }} }}\n\
             @ValueType class E {{ inner: V = new V(); a: FixedArray<V, 2> = [new V(), new V()]; }}\n\
             const es: E[] = [new E()]; const vs: V[] = []; const r: R = new R();\n\
             const rs: R[] = [r]; const nested: V[][] = [vs]; let g: V = new V();\n\
             export function main(): void {{ const l: V = new V(); {place}.grow(); }}\n"
        )
    };
    let moves_v = "vs.push(new V());";
    let moves_e = "es.push(new E());";
    for place in ["vs[0]", "r.list[0]", "rs[0].list[0]", "nested[0][0]"] {
        rejected(&program(moves_v, place));
    }
    // The inner array value is a handle, so a move of the outer array keeps it.
    accepted(&program("nested.push([]);", "nested[0][0]"));
    accepted(&program("nested.shift();", "nested[0][0]"));
    for place in ["es[0].inner", "es[0].a[1]"] {
        rejected(&program(moves_e, place));
        // Rule 5: the element type is `E`, so a move of a `V[]` is not a move of it.
        accepted(&program(moves_v, place));
    }
    for place in ["r.v", "rs[0].v", "g", "l"] {
        accepted(&program(moves_v, place));
    }
}

/// Rule 2 over the closed method tables.
#[test]
fn the_move_table_is_total() {
    let moving = |effect: ElementMove| {
        ArrFn::ALL
            .into_iter()
            .filter(|function| function.element_move() == Some(effect))
            .map(ArrFn::name)
            .collect::<Vec<_>>()
    };
    assert_eq!(moving(ElementMove::Growth), Vec::<&str>::new());
    assert_eq!(
        moving(ElementMove::Rearrangement),
        [
            "reverse",
            "sort",
            "splice",
            "shift",
            "unshift",
            "copyWithin"
        ]
    );
    assert_eq!(
        BuiltinMethod::ArrayPush.element_move(),
        Some(("push", ElementMove::Growth))
    );
    assert_eq!(
        BuiltinMethod::ArrayClear.element_move(),
        Some(("length = 0", ElementMove::Rearrangement))
    );
    assert_eq!(BuiltinMethod::ArrayPop.element_move(), None);
    assert_eq!(BuiltinMethod::StringSlice.element_move(), None);
    assert_eq!(BuiltinMethod::GeneratorNext.element_move(), None);
}

#[test]
fn each_moving_operation_is_rejected_and_each_other_is_accepted() {
    for (body, operation) in [
        ("all.push(new V());", "push"),
        ("all.unshift(new V());", "unshift"),
        ("all.shift();", "shift"),
        ("all.splice(0, 1);", "splice"),
        ("all.reverse();", "reverse"),
        ("all.copyWithin(0, 1);", "copyWithin"),
        ("all.length = 0;", "length = 0"),
    ] {
        let found = rejected(&on_element(body, ""));
        assert!(
            found.message.contains(&format!("by `{operation}` at")),
            "{}",
            found.message
        );
    }
    for body in [
        "all.pop();",
        "all[0] = new V();",
        "all.fill(new V());",
        "const c: V[] = all.slice(0);",
    ] {
        accepted(&on_element(body, ""));
    }
}

/// `sort` has no accepted receiver of value-class elements.
#[test]
fn a_sort_of_scalars_moves_no_value_element() {
    accepted(&on_element(
        "const xs: i32[] = [2, 1]; xs.sort((l: i32, r: i32): i32 => l - r);",
        "",
    ));
}

/// Rule 4: the subset has no inheritance, so no call has an override.
#[test]
fn no_override_exists_and_a_constraint_call_is_direct() {
    for class in [
        "class A {} class B extends A {}",
        "@ValueType class A { a: i32 = 0; } @ValueType class B extends A {}",
    ] {
        let found = diagnostics(&format!("{class}\nexport function main(): void {{}}\n"));
        assert!(!found.is_empty(), "{class}");
    }
    let generic = |body: &str| {
        format!(
            "interface Go {{ go(): void; }}\n\
             class Mover implements Go {{ go(): void {{ {body} }} }}\n\
             class Other implements Go {{ go(): void {{}} }}\n\
             function run<T extends Go>(value: T): void {{ value.go(); }}\n\
             @ValueType class V {{ a: i32 = 1; step(): void {{ run<Mover>(new Mover()); }} }}\n\
             const all: V[] = [new V()];\n\
             export function main(): void {{ run<Other>(new Other()); all[0].step(); }}\n"
        )
    };
    rejected(&generic("all.push(new V());"));
    accepted(&generic(""));
}

/// Rule 4: an indirect call reaches each value with the same signature. The
/// control has no such value, and the second control has another signature.
#[test]
fn an_indirect_call_reaches_a_value_with_the_same_signature() {
    let program = |value: &str| {
        format!(
            "@ValueType class V {{ a: i32 = 1; step(hook: (n: i32) => void): void {{ hook(1); }} }}\n\
             const all: V[] = [new V()];\n\
             function quiet(n: i32): void {{}}\n\
             function grow(n: i32): void {{ all.push(new V()); }}\n\
             function other(n: i64): void {{ all.push(new V()); }}\n\
             {value}\n\
             export function main(): void {{ all[0].step(quiet); }}\n"
        )
    };
    accepted(&program(""));
    rejected(&program("const kept: (n: i32) => void = grow;"));
    rejected(&program(
        "const kept: (n: i32) => void = (n: i32): void => { all.pop(); all.shift(); };",
    ));
    accepted(&program("const kept: (n: i64) => void = other;"));
}

#[test]
fn a_built_in_calls_a_function_value() {
    let program = |value: &str| {
        format!(
            "@ValueType class V {{ a: i32 = 1; step(xs: i32[], m: Map<i32, i32>, s: Set<i32>, \
             f: (x: i32) => void, g: (v: i32, k: i32) => void): void {{ {value} }} }}\n\
             const all: V[] = [new V()];\n\
             function moved(x: i32): void {{ all.pop(); all.shift(); }}\n\
             function moved2(v: i32, k: i32): void {{ all.pop(); all.shift(); }}\n\
             export function main(): void {{ all[0].step([], new Map<i32, i32>(), new Set<i32>(), moved, moved2); }}\n"
        )
    };
    rejected(&program("xs.forEach(f);"));
    rejected(&program("m.forEach(g);"));
    rejected(&program("s.forEach(f);"));
    accepted(&program(""));
}

#[test]
fn a_function_value_in_a_field_is_reached() {
    rejected(
        "class H { f: () => void; constructor(f: () => void) { this.f = f; } }\n\
         @ValueType class V { a: i32 = 1; step(h: H): void { h.f(); } }\n\
         const all: V[] = [new V()];\n\
         export function main(): void { all[0].step(new H((): void => { all.pop(); all.shift(); })); }\n",
    );
}

/// Rule 4: a generator body runs at `next`, so a call of a generator alone
/// reaches nothing. A `for…of` is a `next`.
#[test]
fn a_generator_body_is_reached_at_next() {
    let program = |body: &str| {
        on_element(
            body,
            "function* grower(): Generator<i32> { all.push(new V()); yield 1; }",
        )
    };
    accepted(&program("const g: Generator<i32> = grower(); this.a = 1;"));
    rejected(&program("const g: Generator<i32> = grower(); g.next();"));
    rejected(&program("for (const x of grower()) { this.a = x; }"));
    accepted(&on_element(
        "const g: Generator<i64> = other(); g.next();",
        "function* grower(): Generator<i32> { all.push(new V()); yield 1; }\n\
         function* other(): Generator<i64> { yield 1 as i64; }\n\
         const kept: Generator<i32> = grower();",
    ));
}

#[test]
fn an_async_body_is_reached_from_a_lambda() {
    rejected(&on_element(
        "const f = async (): Promise<void> => { await grower(); }; this.a = 2; f;",
        "async function grower(): Promise<void> { all.push(new V()); }",
    ));
}

#[test]
fn an_imported_module_is_in_the_reach() {
    let library = |body: &str| {
        format!(
            "@ValueType export class V {{ a: i32 = 1; step(): void {{ {body} }} }}\n\
             export const all: V[] = [new V()];\n"
        )
    };
    let entry = "import { all } from \"./lib\";\nexport function main(): void { all[0].step(); }\n";
    let found = check_program(&[
        SourceFile::new("lib.ts", library("all.shift();")),
        SourceFile::entry("main.ts", entry),
    ])
    .expect_err("the import reaches the move");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].pos.file, "main.ts");
    assert!(
        found[0].message.contains("at lib.ts:1:"),
        "{}",
        found[0].message
    );
    check_program(&[
        SourceFile::new("lib.ts", library("")),
        SourceFile::entry("main.ts", entry),
    ])
    .expect("no move");
}

/// A generic class has one body for each instance. Two instances with one
/// position and one text give one diagnostic.
#[test]
fn an_instance_repeats_no_diagnostic() {
    let found = diagnostics(
        "@ValueType class B<T> { v: T; constructor(v: T) { this.v = v; } grow(): void { all.pop(); all.shift(); } }\n\
         const all: B<i32>[] = [];\n\
         function f<U>(u: U): void { all[0].grow(); }\n\
         export function main(): void { f<i32>(1); f<i64>(2 as i64); }\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
}

/// Rule 10.
#[test]
fn a_foreign_call_reaches_no_operation() {
    check_program(&[
        SourceFile::ambient(
            "mirror.d.ts",
            "// @subscript-c-header include=\"x.h\"\ndeclare function hostCall(f: i32): void;\n",
        ),
        SourceFile::entry(
            "main.ts",
            "@ValueType class V { a: i32 = 1; step(): void { hostCall(1); } }\n\
             const all: V[] = [new V()];\n\
             export function main(): void { all[0].step(); }\n",
        ),
    ])
    .expect("a foreign call");
}

/// Rule 7.
#[test]
fn a_stable_receiver_and_a_move_in_an_argument_stay_accepted() {
    accepted(&format!(
        "{CLASS}function moved(list: V[]): void {{ list.push(new V()); }}\n\
         class R {{ v: V = new V(); }}\n\
         export function main(): void {{\n\
           const list: V[] = [new V()];\n\
           list[0].plain();\n\
           const r: R = new R();\n\
           r.v.grow(list);\n\
           const t: V = list[0];\n\
           t.grow(list);\n\
           list[0] = t;\n\
         }}\n"
    ));
}

const INTEROP_MIRROR: &str = include_str!("../../../corpus/interop/interop.generated.d.ts");

fn with_interop(source: &str) -> Vec<Diagnostic> {
    check_program(&[
        SourceFile::ambient("interop.generated.d.ts", INTEROP_MIRROR),
        SourceFile::entry("main.ts", source),
    ])
    .err()
    .unwrap_or_default()
}

/// A method on an element that calls `subDevicePump`, with `value` before the
/// call in `main`.
fn pump_program(value: &str) -> String {
    format!(
        "@ValueType class V {{ a: i32 = 1; step(d: SubDevice): void {{ subDevicePump(d); }} }}\n\
         const all: V[] = [new V()];\n\
         export function main(): void {{\n\
           const d: SubDevice = subDeviceCreate(null);\n\
           {value}\n\
           all[0].step(d);\n\
         }}\n"
    )
}

/// Rule 10: a function value crosses as an argument or in a mirror field. A
/// program with no such value is the control.
#[test]
fn a_foreign_call_reaches_a_value_that_crosses_the_boundary() {
    const MOVING: &str = "(m, a, b) => { all.shift(); }";
    const QUIET: &str = "(m, a, b) => {}";
    let stored = |callback: &str| {
        format!(
            "const info: SubCompletionInfo = new SubCompletionInfo({callback}, null); \
             subDeviceOnComplete(d, info);"
        )
    };
    assert_eq!(with_interop(&pump_program("")), Vec::new());
    assert_eq!(with_interop(&pump_program(&stored(QUIET))), Vec::new());
    let found = with_interop(&pump_program(&stored(MOVING)));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].message.contains("by `shift` at"),
        "{}",
        found[0].message
    );
    let written = format!(
        "{} const moved: SubLogCallback = {MOVING}; info.callback = moved;",
        stored(QUIET)
    );
    assert_eq!(with_interop(&pump_program(&written)).len(), 1);
    // A function value that stays in the script does not cross.
    let kept = format!("const kept: SubLogCallback = {MOVING};");
    assert_eq!(with_interop(&pump_program(&kept)), Vec::new());
}

#[test]
fn a_rearrangement_tells_the_reader_to_run_it_after_the_method() {
    for (body, operation) in [
        ("all.shift();", "shift"),
        ("all.splice(0, 1);", "splice"),
        ("all.unshift(new V());", "unshift"),
        ("all.reverse();", "reverse"),
        ("all.copyWithin(0, 1);", "copyWithin"),
        ("all.length = 0;", "length = 0"),
    ] {
        let found = rejected(&on_element(body, ""));
        assert!(!found.message.contains("const t"), "{}", found.message);
        assert!(
            found.message.ends_with(&format!(
                "and `all[0]` is an element of such an array; after `{operation}` the index no \
                 longer names the element, so run `{operation}` after the method returns"
            )),
            "{}",
            found.message
        );
    }
}

#[test]
fn a_growth_keeps_the_spelled_copy() {
    let found = rejected(&on_element("all.push(new V());", ""));
    assert!(
        found
            .message
            .ends_with("call it on a copy: `const t: V = all[0]; t.step(); all[0] = t;`"),
        "{}",
        found.message
    );
}

/// Rule 6: the copy is wrong if the method can also rearrange the array.
#[test]
fn a_rearrangement_wins_over_a_growth_in_one_method() {
    let found = rejected(&on_element("all.push(new V()); all.shift();", ""));
    assert!(found.message.contains("by `shift` at"), "{}", found.message);
}

/// Rule 6: a place spells as the source does, or the fix is in words.
#[test]
fn a_place_with_no_plain_form_gets_the_fix_in_words() {
    let program = |place: &str| {
        format!(
            "{CLASS}const all: V[] = [new V(), new V()]; const i: i32 = 0;\n\
             function arr(): V[] {{ return all; }}\n\
             class H {{ n: i32 = 0; }} const h: H = new H();\n\
             export function main(): void {{ {place}.grow(all); }}\n"
        )
    };
    let words = "and the receiver is an element of such an array; copy the element to a \
         `const`, call the method on the copy, and assign the copy back";
    for place in ["all[i + 1]", "arr()[0]"] {
        let found = rejected(&program(place));
        assert!(found.message.ends_with(words), "{place}: {}", found.message);
    }
    for place in ["all[i]", "all[h.n]", "all[1]"] {
        let found = rejected(&program(place));
        let expected = format!("`const t: V = {place}; t.grow(...); {place} = t;`");
        assert!(found.message.ends_with(&expected), "{}", found.message);
    }
}

/// A reference class element has no address in the array storage.
#[test]
fn a_reference_element_that_moves_its_own_array_stays_accepted() {
    let program = |class: &str| {
        format!(
            "{class} class R {{ a: i32 = 1; grow(rs: R[]): void {{ rs.push(new R()); }} }}\n\
             export function main(): void {{ const rs: R[] = [new R()]; rs[0].grow(rs); }}\n"
        )
    };
    accepted(&program(""));
    rejected(&program("@ValueType"));
}

/// The walk ends at a reference element, so a move of `R[]` keeps `rs[0].v`.
#[test]
fn a_field_of_a_reference_element_is_not_in_its_array() {
    accepted(
        "class R { v: V = new V(); }\n\
         @ValueType class V { a: i32 = 1; grow(): void { rs.push(new R()); } }\n\
         const rs: R[] = [new R()];\n\
         export function main(): void { rs[0].v.grow(); }\n",
    );
}

/// The dispose hook of a nullable binding, of a binding in a `switch` arm, and
/// of a plain binding. The hook with no move is the control.
#[test]
fn each_using_form_reaches_its_dispose_hook() {
    let program = |hook: &str, body: &str| {
        on_element(
            body,
            &format!("class D {{ [Symbol.dispose](): void {{ {hook} }} }}"),
        )
    };
    for body in [
        "using d: D = new D();",
        "using d: D | null = new D();",
        "const on: i32 = 1; switch (on) { case 1: using d: D = new D(); break; default: break; }",
    ] {
        rejected(&program("all.push(new V());", body));
        accepted(&program("", body));
    }
}

#[test]
fn a_top_level_call_is_a_site() {
    let program = |body: &str| {
        format!(
            "@ValueType class V {{ a: i32 = 1; step(): void {{ {body} }} }}\n\
             const all: V[] = [new V()];\n\
             all[0].step();\n\
             export function main(): void {{}}\n"
        )
    };
    rejected(&program("all.push(new V());"));
    accepted(&program(""));
}
