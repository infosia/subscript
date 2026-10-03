use super::*;

#[test]
fn array_methods_type_and_normalize_optional_arguments() {
    // stdlib.md §9: every accepted method resolves to a Callee::Arr
    // intrinsic with the receiver first; the optional arguments are
    // normalized at check time (join separator -> ",",
    // slice/fill/copyWithin end -> the END_SENTINEL) so each runtime
    // symbol has a fixed arity; map's `U` is inferred from the
    // closure.
    let module = check_one(
        "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const i: i32 = xs.indexOf(2);\n  const s: string = xs.join();\n  const sl: i32[] = xs.slice(1);\n  const fl: i32[] = xs.fill(0);\n  const m: string[] = xs.map((v: i32) => `${v}`);\n  const r: string = xs.reduce((acc: string, v: i32): string => acc + `${v}`, \"#\");\n  const rr: string = xs.reduceRight((acc: string, v: i32): string => acc + `${v}`, \"#\");\n  const sp: i32[] = xs.splice(0, 1);\n  const sh: i32 = xs.shift();\n  const us: i32 = xs.unshift(0);\n  const cw: i32[] = xs.copyWithin(0, 1);\n  print(`${i}${s}${sl.length}${fl.length}${m.length}${r}${rr}${sp.length}${sh}${us}${cw.length}`);\n}\n",
    )
    .expect("clean check");
    let mut found = Vec::new();
    fn walk(e: &hir::Expr, found: &mut Vec<(hir::ArrFn, usize, Type)>) {
        if let hir::ExprKind::Call { callee, args } = &e.kind {
            if let hir::Callee::Arr(f) = callee {
                found.push((*f, args.len(), e.ty.clone()));
            }
            for a in args {
                walk(a, found);
            }
        }
    }
    for s in &module.functions[0].body {
        match s {
            hir::Stmt::Let { init, .. } => walk(init, &mut found),
            hir::Stmt::Expr(e) => walk(e, &mut found),
            _ => {}
        }
    }
    let get = |f: hir::ArrFn| {
        found
            .iter()
            .find(|(g, _, _)| *g == f)
            .unwrap_or_else(|| panic!("no Callee::Arr({}) call", f.name()))
            .clone()
    };
    assert_eq!(get(hir::ArrFn::IndexOf).1, 3); // recv + needle + from
    assert_eq!(get(hir::ArrFn::Join).1, 2); // recv + defaulted ","
    assert_eq!(get(hir::ArrFn::Slice).1, 3); // recv + start + end
    assert_eq!(get(hir::ArrFn::Fill).1, 4); // recv + x + start + end
                                            // map's U is inferred from the closure: string[].
    assert_eq!(get(hir::ArrFn::Map).2, Type::Array(Box::new(Type::Str)));
    // reduce's result is the init's type.
    assert_eq!(get(hir::ArrFn::Reduce).2, Type::Str);
    assert_eq!(get(hir::ArrFn::Reduce).1, 3); // recv + callback + init
    assert_eq!(get(hir::ArrFn::ReduceRight).2, Type::Str);
    assert_eq!(get(hir::ArrFn::ReduceRight).1, 3);
    assert_eq!(get(hir::ArrFn::Splice).1, 3); // recv + start + deleteCount
    assert_eq!(get(hir::ArrFn::Shift).1, 1); // receiver only
    assert_eq!(get(hir::ArrFn::Unshift).1, 2); // recv + value
    assert_eq!(get(hir::ArrFn::CopyWithin).1, 4); // recv + target + start + end
}

#[test]
fn rejected_array_member_is_s014_naming_the_member() {
    for (member, call, q_rule) in [
        ("sort", "xs.sort()", "Q22"),
        (
            "reduce",
            "xs.reduce((acc: i32, v: i32): i32 => acc + v)",
            "Q22",
        ),
        ("find", "xs.find((v: i32): boolean => v > 1)", "Q22"),
        ("findLast", "xs.findLast((v: i32): boolean => v > 1)", "Q22"),
        ("flat", "xs.flat()", "Q22"),
        ("keys", "xs.keys()", "Q30"),
    ] {
        let err = check_one(&format!(
            "export function main(): void {{\n  const xs: i32[] = [1, 2, 3];\n  {call};\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{member}");
        assert!(
            err[0].message.contains(member),
            "{member}: {}",
            err[0].message
        );
        assert!(
            err[0].message.contains(q_rule),
            "{member}: {}",
            err[0].message
        );
    }
}

#[test]
fn array_callbacks_accept_the_q27_index_arity() {
    check_one(
        "function indexedMap(v: i32, i: i32): i32 { return v + i; }\n\
         export function main(): void {\n\
           const xs: i32[] = [1, 2, 3];\n\
           xs.forEach((v: i32, i: i32): void => { print(`${v}:${i}`); });\n\
           const m: i32[] = xs.map(indexedMap);\n\
           xs.filter((v: i32, i: i32): boolean => v > i);\n\
           xs.some((v: i32, i: i32): boolean => v === i);\n\
           xs.every((v: i32, i: i32): boolean => v > i);\n\
           xs.findIndex((v: i32, i: i32): boolean => v === i);\n\
           xs.reduce((acc: i32, v: i32, i: i32): i32 => acc + v + i, 0);\n\
           xs.reduceRight((acc: i32, v: i32, i: i32): i32 => acc + v + i, 0);\n\
           print(`${m.length}`);\n\
         }\n",
    )
    .expect("Q27 indexed Array callbacks check");
}

#[test]
fn fixed_array_callback_family_accepts_both_q27_arities_and_dynamic_results() {
    check_one(
        "function indexedMap(v: i32, i: i32): string { return `${i}:${v}`; }\n\
         export function main(): void {\n\
           const xs: FixedArray<i32, 3> = [1, 2, 3];\n\
           xs.forEach((v: i32): void => { print(`${v}`); });\n\
           xs.forEach((v: i32, i: i32): void => { print(`${i}:${v}`); });\n\
           const m1: i32[] = xs.map((v: i32): i32 => v * 2);\n\
           const m2: string[] = xs.map(indexedMap);\n\
           const f1: i32[] = xs.filter((v: i32): boolean => v > 1);\n\
           const f2: i32[] = xs.filter((v: i32, i: i32): boolean => v > i);\n\
           xs.some((v: i32): boolean => v === 2);\n\
           xs.some((v: i32, i: i32): boolean => v === i);\n\
           xs.every((v: i32): boolean => v > 0);\n\
           xs.every((v: i32, i: i32): boolean => v > i);\n\
           xs.findIndex((v: i32): boolean => v === 3);\n\
           xs.findIndex((v: i32, i: i32): boolean => v === i);\n\
           const r1: i32 = xs.reduce((a: i32, v: i32): i32 => a + v, 0);\n\
           const r2: string = xs.reduce((a: string, v: i32, i: i32): string => a + `${i}:${v}`, \"\");\n\
           const rr1: i32 = xs.reduceRight((a: i32, v: i32): i32 => a + v, 0);\n\
           const rr2: string = xs.reduceRight((a: string, v: i32, i: i32): string => a + `${i}:${v}`, \"\");\n\
           print(`${m1.length}${m2.length}${f1.length}${f2.length}${r1}${r2}${rr1}${rr2}`);\n\
         }\n",
    )
    .expect("Q27 FixedArray callback family checks");
}

#[test]
fn array_callback_container_parameter_is_s014_naming_c5() {
    let err = check_one(
        "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const m: i32[] = xs.map((v: i32, i: i32, arr: i32[]): i32 => v + i + arr.length);\n  print(`${m.length}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("container"));
    assert!(err[0].message.contains("C5"));
    assert!(err[0].message.contains("non-escaping-by-construction"));
    assert!(err[0].message.contains("Q27"));
}

#[test]
fn array_method_on_fixed_array_is_s014() {
    // Q27 adds only the callback family. Other checker-owned Array
    // methods remain dynamic-array-only.
    let err = check_one(
        "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(xs.join(\",\"));\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("dynamic-array-only"));
    assert!(err[0].message.contains("Q22/Q27"));
}

#[test]
fn push_and_pop_on_a_fixed_array_are_not_blamed_on_q22() {
    // `push` and `pop` are not Q22 Array methods. They are outside
    // `ambient::arr_method`, so the FixedArray error uses S018.
    for call in ["xs.push(4)", "xs.pop()"] {
        let err = check_one(&format!(
            "export function main(): void {{\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  {call};\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S018, "{call}: {}", err[0].message);
        assert!(
            !err[0].message.contains("Q22"),
            "{call}: {}",
            err[0].message
        );
        assert!(
            err[0].message.contains("has no method"),
            "{call}: {}",
            err[0].message
        );
    }
}

#[test]
fn reduce_init_takes_its_contextual_type_from_the_callback() {
    // C4: the callback's annotated accumulator type is `init`'s
    // contextual type, so a plain literal init does not default to
    // `i32` and poison `U`.
    for (acc, cb) in [
        ("i64", "(a: i64, v: i32): i64 => a + (v as i64)"),
        ("f64", "(a: f64, v: i32): f64 => a + (v as f64)"),
        ("u32", "(a: u32, v: i32): u32 => a + (v as u32)"),
    ] {
        let src = format!(
            "export function main(): void {{\n  const xs: i32[] = [1, 2, 3];\n  const total: {acc} = xs.reduce({cb}, 0);\n  print(`${{total}}`);\n}}\n"
        );
        let module =
            check_one(&src).unwrap_or_else(|e| panic!("{acc} accumulator rejected: {e:?}"));
        assert_eq!(module.functions.len(), 1, "{acc}");
    }
}

#[test]
fn reduce_init_takes_its_type_from_a_function_value_callback() {
    // The same rule when the callback is a function value: its
    // declared accumulator type is `init`'s context.
    let src = "function add(acc: i64, v: i32): i64 {\n  return acc + (v as i64);\n}\nexport function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const total: i64 = xs.reduce(add, 0);\n  print(`${total}`);\n}\n";
    check_one(src).unwrap_or_else(|e| panic!("function-value callback rejected: {e:?}"));
}

#[test]
fn reduce_init_that_does_not_fit_the_accumulator_names_the_init() {
    // A genuine mismatch still errors — against the init, which is
    // the offending argument, not the callback.
    let err = check_one(
        "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const total: i64 = xs.reduce((a: i64, v: i32): i64 => a + (v as i64), \"x\");\n  print(`${total}`);\n}\n",
    )
    .unwrap_err();
    assert!(
        err[0].message.contains("`reduce` init"),
        "{}",
        err[0].message
    );
    assert_eq!(err[0].pos.line, 3);
}

#[test]
fn reduce_without_an_annotated_accumulator_still_types_from_the_init() {
    // An un-annotated arrow does not spell `U`; `init` gives it, and
    // contextual typing then flows to the callback.
    let module = check_one(
        "export function main(): void {\n  const xs: i32[] = [1, 2, 3];\n  const joined: string = xs.reduce((acc, v) => acc + `${v}`, \"#\");\n  print(joined);\n}\n",
    )
    .expect("clean check");
    assert_eq!(module.functions.len(), 1);
}

#[test]
fn array_callback_over_value_class_elements_is_s014() {
    // Value-class elements cannot cross the runtime->script element
    // boundary (stdlib.md §9); the checker gates them.
    let err = check_one(
        "@ValueType\nclass V { x: i32; constructor(x: i32) { this.x = x; } }\nexport function main(): void {\n  const xs: V[] = [new V(1)];\n  xs.forEach((v: V): void => {});\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("Q22"));
}

#[test]
fn array_join_on_date_elements_is_s014() {
    // Date is not interpolatable (Q20); join follows the Q14 rules.
    let err = check_one(
        "export function main(): void {\n  const ds: Date[] = [new Date(0)];\n  print(ds.join(\",\"));\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("join"));
}

#[test]
fn array_method_read_as_a_value_is_rejected() {
    let err = check_one("export function main(): void {\n  const xs: i32[] = [1];\n  xs.map;\n}\n")
        .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("only be called"));
}

#[test]
fn unknown_array_member_keeps_the_surface_diagnostic() {
    let err = check_one(
        "export function main(): void {\n  const xs: i32[] = [1];\n  xs.frobnicate();\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("Q22"), "{}", err[0].message);
}

#[test]
fn map_set_key_whitelist_rejections_name_q24() {
    for key in [
        "f16",
        "i32[]",
        "FixedArray<i32, 2>",
        "object",
        "(x: i32) => i32",
        "C | null",
        "void",
        "Map<i32, i32>",
        "Set<i32>",
    ] {
        let src = format!(
            "class C {{ x: i32; constructor() {{ this.x = 1; }} }}\n\
             export function main(): void {{\n\
               const map: Map<{key}, i32> = new Map<{key}, i32>();\n\
               print(`${{map.size}}`);\n\
             }}\n"
        );
        let err = check_one(&src).unwrap_err();
        if key == "void" {
            assert_eq!(err[0].code, RuleCode::S100, "{err:?}");
            assert_eq!(
                err[0].divergence,
                Some(crate::divergence::Divergence::VoidValue)
            );
        } else {
            assert_eq!(err[0].code, RuleCode::S014, "{key}: {err:?}");
            assert!(err[0].message.contains("Q24"), "{key}: {}", err[0].message);
        }
    }
    let err = check_one(
        "@ValueType\nclass V { x: i32; constructor() { this.x = 1; } }\n\
         export function main(): void {\n\
           const set: Set<V> = new Set<V>();\n\
           print(`${set.size}`);\n\
         }\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("Q24"));
}

#[test]
fn nested_container_value_does_not_inherit_key_resolution_context() {
    let err = check_one(
        "export function main(): void {\n\
           const map: Map<Map<i32, object>, i32> = \
             new Map<Map<i32, object>, i32>();\n\
           print(`${map.size}`);\n\
         }\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S011);
    assert!(err[0].message.contains("boundary-only"));
}

#[test]
fn literal_and_computed_nan_keys_are_accepted() {
    check_one(
        "export function main(): void {\n\
           const map: Map<f64, i32> = new Map<f64, i32>();\n\
           map.set(NaN, 1);\n\
           const set: Set<f64> = new Set<f64>();\n\
           set.add(Number.NaN);\n\
           print(`${map.has(NaN)} ${set.has(NaN)}`);\n\
         }\n",
    )
    .expect("literal NaN keys are accepted");
    check_one(
        "export function main(): void {\n\
           const map: Map<f64, i32> = new Map<f64, i32>();\n\
           const zero: f64 = 0.0;\n\
           const nan: f64 = zero / zero;\n\
           map.set(nan, 1);\n\
           print(`${map.has(nan)} ${map.size}`);\n\
         }\n",
    )
    .expect("computed NaN key is accepted");
}

#[test]
fn number_q25_q26_surface_types_and_rejections() {
    check_one(
        "export function main(): void {\n\
           const parsed: f64 = parseInt(\"ff\", 16);\n\
           const decimal: f64 = parseFloat(\"1.5tail\");\n\
           const parsedStatic: f64 = Number.parseInt(\"ff\", 16);\n\
           const decimalStatic: f64 = Number.parseFloat(\"1.5tail\");\n\
           const f: f32 = 1.25;\n\
           print(`${Number.MAX_SAFE_INTEGER} ${Number.isNaN(Number.NaN)} \
                    ${Number.isFinite(parsed)} ${Number.isInteger(decimal)} \
                    ${Number.isSafeInteger(parsed)}`);\n\
           print(f.toFixed(1));\n\
           print(f.toString(16));\n\
           print(parsed.toExponential());\n\
           print(decimal.toPrecision(2));\n\
           const leading: i32 = Math.clz32(0 as u32);\n\
           const wrapped: i32 = Math.imul(2147483647, 2);\n\
           const rounded: f64 = Math.fround(1.1);\n\
           print(`${leading} ${wrapped} ${rounded} ${parsedStatic} ${decimalStatic}`);\n\
         }\n",
    )
    .expect("accepted Q25/Q26 surface");

    for body in [
        "isNaN(1.0);",
        "isFinite(1.0);",
        "Number(1.0);",
        "parseInt(\"1\");",
        "Number.parseInt(\"1\");",
    ] {
        let err =
            check_one(&format!("export function main(): void {{\n  {body}\n}}\n")).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{body}: {err:?}");
        assert!(err[0].message.contains("Q25"), "{body}: {}", err[0].message);
    }

    for body in ["(1.0 as f64).toPrecision();", "(1.0 as f64).toString();"] {
        let err =
            check_one(&format!("export function main(): void {{\n  {body}\n}}\n")).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{body}: {err:?}");
        assert!(err[0].message.contains("Q26"), "{body}: {}", err[0].message);
    }
}

#[test]
fn map_get_is_nullable_only_for_reference_values() {
    let module = check_one(
        "class C { x: i32; constructor() { this.x = 1; } }\n\
         export function main(): void {\n\
           const map: Map<string, C> = new Map<string, C>();\n\
           const value = map.get(\"x\");\n\
           print(`${value === null}`);\n\
         }\n",
    )
    .expect("reference-valued get checks");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let hir::Stmt::Let { ty, .. } = &main.body[1] else {
        panic!("expected get binding");
    };
    assert!(matches!(ty, Type::Nullable(inner) if matches!(**inner, Type::Class(_))));

    let err = check_one(
        "export function main(): void {\n\
           const map: Map<i32, i32> = new Map<i32, i32>();\n\
           print(`${map.get(1)}`);\n\
         }\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("getOr"));
}

#[test]
fn q30_accepts_fused_container_iteration_and_array_literal_spread() {
    check_one(
        "export function main(): void {\n\
           const map: Map<i32, i32> = new Map<i32, i32>();\n\
           const set: Set<i32> = new Set<i32>();\n\
           for (const key of map.keys()) { print(`${key}`); }\n\
           for (const value of map.values()) { print(`${value}`); }\n\
           const values: i32[] = [...set];\n\
           print(`${values.length}`);\n\
         }\n",
    )
    .expect("Q30 container traversal and literal spread");

    let err = check_one(
        "export function main(): void {\n\
           const map: Map<i32, i32> = new Map<i32, i32>();\n\
           const keys = map.keys();\n\
         }\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("direct subject"));
}

/// compiler.md §104.1 rules 1 and 2: both positions reject a bare
/// `Map`, and neither reads how the bound value is used.
#[test]
fn a_bare_map_is_rejected_in_both_iteration_positions() {
    let forms = [
        "for (const key of map) { print(`${key}`); }",
        "for (const key of map) { const value: i32 = key; print(`${value}`); }",
        "const keys = [...map]; print(`${keys.length}`);",
        "const keys: i32[] = [...map]; print(`${keys.length}`);",
    ];
    for form in forms {
        let source = format!(
            "export function main(): void {{\n\
               const map: Map<i32, i32> = new Map<i32, i32>();\n\
               {form}\n\
             }}\n"
        );
        let Err(err) = check_one(&source) else {
            panic!("a bare Map must be rejected in `{form}`");
        };
        assert_eq!(err[0].code, RuleCode::S014, "{form}");
        assert!(
            err[0].message.contains("bare `Map`")
                && err[0].message.contains("`[K, V]` pair")
                && err[0].message.contains("no tuple representation"),
            "{form}: {}",
            err[0].message
        );
    }
}

/// compiler.md §105.1 rule 1: `Array` resolves through ordinary name
/// resolution, so an ordinary declaration shadows it.
#[test]
fn a_user_declaration_shadows_the_array_namespace() {
    check_one(
        "export function main(): void {\n\
           const Array: i32 = 3;\n\
           print(`${Array}`);\n\
         }\n",
    )
    .expect("a local binding named Array shadows the builtin namespace");

    // The shadow owns the member path too: `from` here is the
    // user's method, not §105.2's builtin.
    check_one(
        "class Source {\n\
           from(value: i32): i32 { return value; }\n\
         }\n\
         export function main(): void {\n\
           const Array: Source = new Source();\n\
           print(`${Array.from(3)}`);\n\
         }\n",
    )
    .expect("a local binding named Array owns `Array.from`");
}

/// compiler.md §105.1 rule 2: the general unknown-name diagnostic
/// keeps its meaning, and no `Array` divergence attaches to it.
#[test]
fn the_array_namespace_leaves_the_unknown_name_diagnostic_alone() {
    let err = check_one("const n: i32 = zz;\n").expect_err("an unknown name is rejected");
    assert_eq!(err[0].code, RuleCode::S016);
    assert!(err[0].message.contains("unknown name `zz`"), "{err:#?}");
    assert!(err[0].divergence.is_none(), "{err:#?}");
}

/// compiler.md §105.2 rule 1: the four accepted sources, and rule 5's
/// explicit type argument.
#[test]
fn array_from_accepts_every_recorded_source() {
    check_one(
        "export function main(): void {\n\
           const xs: i32[] = [1, 2];\n\
           const fixed: FixedArray<i32, 2> = [3, 4];\n\
           const set: Set<i32> = new Set<i32>();\n\
           const a: i32[] = Array.from(xs);\n\
           const b: i32[] = Array.from(fixed);\n\
           const c: i32[] = Array.from(set);\n\
           const d: string[] = Array.from(\"ab\");\n\
           const e: i32[] = Array.from<i32>([]);\n\
           print(`${a.length}${b.length}${c.length}${d.length}${e.length}`);\n\
         }\n",
    )
    .expect("§105.2 rule 1 accepts every source");

    // Rule 5's other half: with no type argument the empty literal
    // is checked with no contextual type.
    let err = check_one(
        "export function main(): void {\n\
           const xs: i32[] = Array.from([]);\n\
           print(`${xs.length}`);\n\
         }\n",
    )
    .expect_err("an untyped empty source keeps the empty-literal rejection");
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("empty array literal"), "{err:#?}");
}

/// compiler.md §105.5: `Array.from(map)` serves both `tsc` classes.
/// §79 rule 6 puts the unannotated form in the corpus, and this
/// test pins the annotated one.
///
/// This half is the rejection here. `compiler/tests/tsc_corpus.rs`
/// is the `tsc` half. It runs the pinned TypeScript compiler on the
/// same form and compares the code it measures.
#[test]
fn the_annotated_array_from_map_is_rejected_here() {
    let err = check_one(
        "export function main(): void {\n\
           const map: Map<i32, string> = new Map<i32, string>();\n\
           map.set(1, \"one\");\n\
           const keys: i32[] = Array.from(map);\n\
           print(`${keys.length}`);\n\
         }\n",
    )
    .expect_err("the annotated form is rejected here too");
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("`[K, V]` pair"), "{err:#?}");
}

/// compiler.md §105.2 rule 6: three rejected sources, three reasons.
/// One shared message names a prerequisite that two of them do not
/// have.
#[test]
fn each_rejected_array_from_source_states_its_own_reason() {
    let cases = [
        (
            "const map: Map<i32, i32> = new Map<i32, i32>();\n                   const out = Array.from(map);",
            "`[K, V]` pair",
        ),
        (
            "const out = Array.from(gen());",
            "single-use",
        ),
        (
            "const map: Map<i32, i32> = new Map<i32, i32>();\n                   const out = Array.from(map.keys());",
            "direct subject",
        ),
    ];
    for (body, needle) in cases {
        let source = format!(
            "function* gen(): Generator<i32> {{ yield 1; }}\n\
             export function main(): void {{\n                   {body}\n                   print(`${{out.length}}`);\n\
             }}\n"
        );
        let err = check_one(&source).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{body}");
        assert!(
            err[0].message.contains(needle),
            "{body}: {}",
            err[0].message
        );
    }
}

/// compiler.md §105.3: the other three members are rejected, each
/// with its own record, and none of them is a candidate refusal.
#[test]
fn the_other_array_members_carry_their_own_rejection() {
    let cases = [
        ("const flag: boolean = Array.isArray(xs);", "statically"),
        (
            "const made: i32[] = Array.of<i32>(1, 2);",
            "variadic-parameter prerequisite",
        ),
        ("const sized: i32[] = new Array<i32>(3);", "array hole"),
    ];
    for (body, needle) in cases {
        let source = format!(
            "export function main(): void {{\n\
             \x20 const xs: i32[] = [1, 2];\n\
             \x20 {body}\n\
             \x20 print(`${{xs.length}}`);\n\
             }}\n"
        );
        let err = check_one(&source).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{body}");
        assert!(
            err[0].message.contains(needle),
            "{body}: {}",
            err[0].message
        );
        assert!(
            err[0].divergence.is_some(),
            "{body}: a tsc-accepted form must carry a §79 variant"
        );
    }
}

/// compiler.md §103.8 rule 1: a rejected call still walks its
/// arguments, so an unknown name inside one reaches the user beside
/// the rejection. Every early return of the `Array.from` check
/// walks the argument list.
#[test]
fn a_rejected_array_call_still_reports_an_unknown_argument_name() {
    for body in [
        "const xs: i32[] = Array.from(...nope);",
        "const xs: i32[] = Array.from(nope, nope);",
        "const xs: i32[] = Array.from(nope, nope, nope, nope);",
        "const flag: boolean = Array.isArray(nope);",
    ] {
        let source =
            format!("export function main(): void {{\n\x20 {body}\n\x20 print(`x`);\n}}\n");
        let err = check_one(&source).unwrap_err();
        assert!(
            err.iter()
                .any(|diagnostic| diagnostic.code == RuleCode::S016
                    && diagnostic.message.contains("unknown name `nope`")),
            "{body}: {err:#?}"
        );
    }
}

/// compiler.md §105.4: `Array` is a namespace, so it is not a value
/// and a member read is not a value.
#[test]
fn the_array_namespace_is_not_a_value() {
    for body in [
        "const held = Array;",
        "const held = Array.from;",
        "const held = Array.isArray;",
    ] {
        let source =
            format!("export function main(): void {{\n\x20 {body}\n\x20 print(`x`);\n}}\n");
        let err = check_one(&source).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{body}");
    }
}

/// compiler.md §104.4: one rejection states one reason. A spread
/// literal whose only operand is rejected carries that diagnostic
/// alone. The empty-literal reason names a shape this literal does
/// not have, so this literal does not carry it.
#[test]
fn a_rejected_spread_operand_reports_one_reason() {
    let err = check_one(
        "export function main(): void {\n\
           const map: Map<i32, i32> = new Map<i32, i32>();\n\
           const keys = [...map];\n\
           print(`${keys.length}`);\n\
         }\n",
    )
    .expect_err("a bare Map operand is rejected");
    assert_eq!(err.len(), 1, "{err:#?}");
    assert_eq!(err[0].code, RuleCode::S014);

    // The control: the empty-literal reason still fires where the
    // literal really is empty.
    let empty = check_one(
        "export function main(): void {\n\
           const values = [];\n\
           print(`${values.length}`);\n\
         }\n",
    )
    .expect_err("an empty array literal with no context is rejected");
    assert_eq!(empty[0].code, RuleCode::S100);
    assert!(
        empty[0].message.contains("empty array literal"),
        "{}",
        empty[0].message
    );
}

/// compiler.md §104.6: the typed forms cannot become reject corpus
/// entries, because §79 rule 6 fixes each site's variant and §79
/// rule 4 then forbids a `tsc: rejects` entry there. This test is
/// the pin instead.
///
/// This half is the rejection here. `compiler/tests/tsc_corpus.rs`
/// is the `tsc` half. It runs the pinned TypeScript compiler on the
/// same forms and compares the codes it measures.
#[test]
fn the_typed_bare_map_forms_are_rejected_here() {
    let forms = [
        "  const keys: i32[] = [...map];\n  print(`${keys.length}`);",
        "  for (const key of map) {\n    const n: i32 = key;\n    print(`${n}`);\n  }",
    ];
    for form in forms {
        let source = format!(
            "export function main(): void {{\n\
             \x20 const map: Map<i32, string> = new Map<i32, string>();\n\
             \x20 map.set(1, \"one\");\n\
             {form}\n\
             }}\n"
        );
        let Err(err) = check_one(&source) else {
            panic!("the typed form must be rejected here: {form}");
        };
        assert_eq!(err[0].code, RuleCode::S014, "{form}");
        assert!(
            err[0].message.contains("bare `Map`"),
            "{form}: {}",
            err[0].message
        );
    }
}

/// The narrowing of §104.1 rule 3 reaches no other subject or
/// operand type.
#[test]
fn the_bare_map_rejection_leaves_every_other_container_accepted() {
    check_one(
        "function* one(): Generator<i32> { yield 1; }\n\
         export function main(): void {\n\
           const map: Map<i32, i32> = new Map<i32, i32>();\n\
           const set: Set<i32> = new Set<i32>();\n\
           const fixed: FixedArray<i32, 2> = [1, 2];\n\
           for (const key of map.keys()) { print(`${key}`); }\n\
           for (const value of map.values()) { print(`${value}`); }\n\
           for (const value of set) { print(`${value}`); }\n\
           for (const value of fixed) { print(`${value}`); }\n\
           for (const point of \"ab\") { print(point); }\n\
           for (const value of one()) { print(`${value}`); }\n\
           const values: i32[] = [...set, ...fixed];\n\
           print(`${values.length}`);\n\
         }\n",
    )
    .expect("§104.1 rule 3 leaves the other containers accepted");
}

#[test]
fn map_group_by_and_set_algebra_are_accepted_by_q27() {
    check_one(
        "export function main(): void {\n\
           const set: Set<i32> = new Set<i32>();\n\
           const grouped: Map<i32, i32[]> = Map.groupBy(\n\
             [1],\n\
             (value: i32): i32 => value,\n\
           );\n\
           set.union(set);\n\
           set.intersection(set);\n\
           set.difference(set);\n\
           set.symmetricDifference(set);\n\
           print(`${grouped.size} ${set.isSubsetOf(set)} ${set.isSupersetOf(set)} ${set.isDisjointFrom(set)}`);\n\
         }\n",
    )
    .expect("Q27 stage 4 Map/Set surface checks");
}

#[test]
fn q27_array_index_arity_does_not_reach_map_or_set_callbacks() {
    for (surface, q_rule, source) in [
        (
            "Map.forEach",
            "Q24",
            "export function main(): void {\n\
               const map: Map<i32, i32> = new Map<i32, i32>();\n\
               map.forEach((value: i32, key: i32, index: i32): void => {});\n\
             }\n",
        ),
        (
            "Set.forEach",
            "Q24",
            "export function main(): void {\n\
               const set: Set<i32> = new Set<i32>();\n\
               set.forEach((value: i32, index: i32): void => {});\n\
             }\n",
        ),
        (
            "Map.groupBy",
            "Q27",
            "export function main(): void {\n\
               Map.groupBy([1], (value: i32, index: i32): i32 => value + index);\n\
             }\n",
        ),
    ] {
        let err = check_one(source).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{surface}");
        assert!(
            err[0].message.contains(q_rule),
            "{surface}: {}",
            err[0].message
        );
    }
}

#[test]
fn capturing_lambda_may_not_capture_mutable_locals() {
    let err = check_one(
        "export function main(): void {\n  let n: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + n;\n  print(`${f(1)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S009);
}

#[test]
fn member_access_on_nullable_without_narrowing_is_s011() {
    let err = check_one(
        "class C { x: i32; constructor() { this.x = 1; } }\nfunction f(c: C | null): i32 {\n  return c.x;\n}\nexport function main(): void {\n  print(`${f(null)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S011);
    assert_eq!(err[0].pos.line, 3);
}

#[test]
fn two_file_program_with_import_checks_clean() {
    let module = check_program(&[
        SourceFile::entry(
            "main.ts",
            "import { double } from \"./util\";\nexport function main(): void {\n  print(`${double(2)}`);\n}\n",
        ),
        SourceFile::new(
            "util.ts",
            "export function double(x: i32): i32 {\n  return x * 2;\n}\n",
        ),
    ])
    .expect("clean two-file check");
    assert_eq!(module.functions.len(), 2);
}

#[test]
fn value_class_is_nominal_and_marked_value() {
    let module = check_one(
        "@ValueType\nclass V { x: f32; constructor(x: f32) { this.x = x; } }\nexport function main(): void {\n  const v: V = new V(1.0);\n  print(`${v.x}`);\n}\n",
    )
    .expect("clean");
    assert_eq!(module.classes.len(), 2);
    assert!(module.classes[1].is_value);
    assert_eq!(module.classes[1].fields[0].ty, Type::F32);
}

#[test]
fn fixed_array_length_mismatch_is_rejected() {
    let err = check_one(
        "const xs: FixedArray<f32, 4> = [1.0, 2.0, 3.0];\nexport function main(): void {\n  print(`${xs[0]}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("FixedArray"));
}

#[test]
fn fixed_array_literal_constructs_with_matching_length() {
    let module = check_one(
        "export function main(): void {\n  const xs: FixedArray<i32, 3> = [1, 2, 3];\n  print(`${xs[2]}`);\n}\n",
    )
    .expect("clean");
    let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
        panic!("expected let");
    };
    assert_eq!(*ty, Type::FixedArray(Box::new(Type::I32), 3));
}

#[test]
fn const_rebinding_is_rejected_but_field_writes_are_not() {
    // Q17: `const` blocks rebinding only.
    let err = check_one("export function main(): void {\n  const x: i32 = 1;\n  x = 2;\n}\n")
        .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("rebind"));

    check_one(
        "@ValueType\nclass V { x: f32; constructor(x: f32) { this.x = x; } }\nexport function main(): void {\n  const v: V = new V(1.0);\n  v.x = 2.0;\n  print(`${v.x}`);\n}\n",
    )
    .expect("field writes through const value bindings are legal");
}

#[test]
fn mixed_width_bitwise_requires_as() {
    // Q18.
    let err = check_one(
        "export function main(): void {\n  const a: u64 = 1;\n  const b: u32 = 2;\n  const c: u64 = a | b;\n  print(`${c}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S007);
    assert!(err[0].message.contains("mixed-type bitwise"));
    assert!(!err[0].message.contains("arithmetic"));

    check_one(
        "export function main(): void {\n  const a: u64 = 1;\n  const b: u32 = 2;\n  const c: u64 = a | (b as u64);\n  print(`${c}`);\n}\n",
    )
    .expect("same-width bitwise after `as` is legal");
}

#[test]
fn literal_and_nonliteral_shift_counts_are_accepted() {
    check_one(
        "export function main(): void {\n  const one: u8 = 1;\n  const x: u8 = one << 8;\n  print(`${x}`);\n}\n",
    )
    .expect("literal shift amounts are masked at runtime");

    check_one(
        "export function main(): void {\n  const one: u8 = 1;\n  const amount: u8 = 8;\n  const x: u8 = one << amount;\n  print(`${x}`);\n}\n",
    )
    .expect("nonliteral shift amounts are masked at runtime");
}

#[test]
fn returning_a_local_holding_a_capturing_lambda_is_s009() {
    let err = check_one(
        "function make(): (x: i32) => i32 {\n  const k: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + k;\n  return f;\n}\nexport function main(): void {\n  print(`${make()(1)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S009);
    assert_eq!(err[0].pos.line, 4);
}

#[test]
fn enum_casts_to_integer_but_not_the_reverse() {
    let module = check_one(
        "enum E { A = 1 }\nexport function main(): void {\n  const e: E = E.A;\n  print(`${e as i32}`);\n}\n",
    )
    .expect("enum to integer cast is legal");
    assert_eq!(module.enums.len(), 1);

    let err = check_one(
        "enum E { A = 1 }\nexport function main(): void {\n  const e: E = 1 as E;\n  print(`${e as i32}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
}

#[test]
fn optional_parameters_are_s012() {
    let err = check_one("function f(x?: i32): void {}\nexport function main(): void { f(); }\n")
        .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S012);
}

#[test]
fn narrowing_is_invalidated_by_reassignment() {
    let err = check_one(
        "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  let c: C | null = new C();\n  if (c !== null) {\n    print(`${c.x}`);\n  }\n  c = null;\n  print(`${c.x}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S011);
    assert_eq!(err[0].pos.line, 8);
}

#[test]
fn context_free_takes_reference_instances_only() {
    check_one(
        "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  Context.free(c);\n}\n",
    )
    .expect("reference instances cross into `object`");

    let err = check_one(
        "@ValueType\nclass V { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const v: V = new V();\n  Context.free(v);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
}

#[test]
fn context_namespace_is_neither_a_value_nor_a_class() {
    let value_err =
        check_one("export function main(): void {\n  const value = Context;\n}\n").unwrap_err();
    assert_eq!(value_err[0].code, RuleCode::S014);
    assert_eq!(
        value_err[0].message,
        "`Context` is an ambient namespace, not a value; use \
         `Context.collect()`, `Context.free(value)`, or await \
         `Context.suspend()` (Q6/Q7/Q34)"
    );

    let construct_err =
        check_one("export function main(): void {\n  const value = new Context();\n}\n")
            .unwrap_err();
    assert_eq!(construct_err[0].code, RuleCode::S100);
    assert_eq!(construct_err[0].message, "unknown class `Context`");
}

// ----- checker regression tests -----

#[test]
fn m1_enum_implicit_value_overflow_is_s008_not_a_panic() {
    let err = check_one(
        "enum E { A = 2147483647, B }\nexport function main(): void {\n  print(`${E.A as i32}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S008);
    assert!(
        err[0].message.contains("B"),
        "message names the member: {}",
        err[0].message
    );
}

#[test]
fn m2a_push_of_a_capturing_lambda_is_s009() {
    let err = check_one(
        "export function main(): void {\n  const k: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + k;\n  const xs: ((x: i32) => i32)[] = [];\n  xs.push(f);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S009);
    assert_eq!(err[0].pos.line, 5);
}

#[test]
fn m2b_array_literals_reject_capturing_lambdas_in_every_context() {
    // Inferred element type.
    let err = check_one(
        "export function main(): void {\n  const k: i32 = 1;\n  const fs = [(x: i32): i32 => x + k];\n  print(`${fs[0](1)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S009);

    // FixedArray context.
    let err = check_one(
        "export function main(): void {\n  const k: i32 = 1;\n  const fs: FixedArray<(x: i32) => i32, 1> = [(x: i32): i32 => x + k];\n  print(`${fs[0](1)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S009);
}

#[test]
fn m2b_returning_a_local_bound_to_a_capturing_array_literal_is_s009() {
    let diags = check_one(
        "function make(): ((x: i32) => i32)[] {\n  const k: i32 = 1;\n  const fs = [(x: i32): i32 => x + k];\n  return fs;\n}\nexport function main(): void {\n  print(`${make()[0](1)}`);\n}\n",
    )
    .unwrap_err();
    assert!(
        diags.iter().any(|d| d.code == RuleCode::S009),
        "expected an S009 among: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn m2c_returning_a_conditional_over_capturing_lambdas_is_s009() {
    let err = check_one(
        "function pick(flag: boolean): (x: i32) => i32 {\n  const a: i32 = 1;\n  const f: (x: i32) => i32 = (x: i32): i32 => x + a;\n  const g: (x: i32) => i32 = (x: i32): i32 => x - a;\n  return flag ? f : g;\n}\nexport function main(): void {\n  print(`${pick(true)(1)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S009);
    assert_eq!(err[0].pos.line, 5);
}

#[test]
fn m3_missing_return_path_is_s100_at_the_function() {
    let err = check_one(
        "function f(flag: boolean): i32 {\n  if (flag) {\n    return 1;\n  }\n}\nexport function main(): void {\n  print(`${f(true)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("return"));
    assert_eq!(err[0].pos.line, 1);
}

#[test]
fn m3_all_return_shapes_check_clean() {
    // if/else with both arms returning.
    check_one(
        "function f(flag: boolean): i32 {\n  if (flag) {\n    return 1;\n  } else {\n    return 2;\n  }\n}\nexport function main(): void {\n  print(`${f(true)}`);\n}\n",
    )
    .expect("if/else return");
    // Infinite loop with no break never falls through.
    check_one(
        "function f(): i32 {\n  while (true) {\n    return 1;\n  }\n}\nexport function main(): void {\n  print(`${f()}`);\n}\n",
    )
    .expect("while(true) return");
}

#[test]
fn m3_while_true_with_break_does_not_count_as_returning() {
    let err = check_one(
        "function f(flag: boolean): i32 {\n  while (true) {\n    if (flag) {\n      return 1;\n    }\n    break;\n  }\n}\nexport function main(): void {\n  print(`${f(true)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("return"));
}

#[test]
fn m3_lambda_block_bodies_need_all_paths_to_return() {
    let err = check_one(
        "export function main(): void {\n  const f: (x: i32) => i32 = (x: i32): i32 => {\n    if (x > 0) {\n      return x;\n    }\n  };\n  print(`${f(1)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("return"));
}

#[test]
fn minor1_update_operators_respect_const_bindings() {
    let err = check_one(
        "export function main(): void {\n  const x: i32 = 1;\n  x++;\n  print(`${x}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("rebind"));
}

#[test]
fn minor2_user_written_object_annotations_are_s011() {
    let err = check_one("let o: object | null = null;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S011);

    let err =
        check_one("function f(o: object): void {}\nexport function main(): void {}\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S011);

    // The ambient `Context.free(value: object)` stays callable.
    check_one(
        "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  Context.free(c);\n}\n",
    )
    .expect("ambient object parameter unaffected");
}

#[test]
fn cross_file_class_names_have_distinct_identities() {
    let module = check_program(&[
        SourceFile::entry("api.ts", ""),
        SourceFile::new(
            "a.ts",
            "export class C { x: i32 = 1; }\nexport function main(): void {}\n",
        ),
        SourceFile::new("b.ts", "export class C { x: i32 = 1; }\n"),
    ])
    .expect("independent class declarations");
    let classes: Vec<_> = module.classes.iter().filter(|c| c.name == "C").collect();
    assert_eq!(classes.len(), 2);
    assert_eq!(classes[0].pos.file, "a.ts");
    assert_eq!(classes[1].pos.file, "b.ts");
}

#[test]
fn minor4_fixed_array_length_beyond_u32_is_s008() {
    let err = check_one(
        "function f(xs: FixedArray<i32, 4294967296>): void {}\nexport function main(): void {}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S008);
    assert!(err[0].message.contains("FixedArray length"));
}

// ----- Date (stdlib.md §3, Q20) -----

#[test]
fn date_round_trip_program_checks_clean_with_nominal_types() {
    let module = check_one(
        "export function main(): void {\n  const d: Date = new Date(Date.UTC(2020, 0, 2));\n  const t: i64 = d.getTime();\n  print(`${t},${d.getUTCFullYear()}`);\n  print(d.toISOString());\n}\n",
    )
    .expect("clean");
    let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
        panic!("expected let");
    };
    assert_eq!(*ty, Type::Date);
    // getTime folds to the receiver retyped i64 — no call survives.
    let hir::Stmt::Let { ty, init, .. } = &module.functions[0].body[1] else {
        panic!("expected let");
    };
    assert_eq!(*ty, Type::I64);
    assert!(
        matches!(init.kind, hir::ExprKind::Local(..)),
        "getTime must fold to the receiver, got {:?}",
        init.kind
    );
}

#[test]
fn date_utc_normalizes_missing_trailing_arguments_to_defaults() {
    let module = check_one(
        "export function main(): void {\n  const t: i64 = Date.UTC(2020, 0);\n  print(`${t}`);\n}\n",
    )
    .expect("clean");
    let hir::Stmt::Let { init, .. } = &module.functions[0].body[0] else {
        panic!("expected let");
    };
    let hir::ExprKind::Call { callee, args } = &init.kind else {
        panic!("expected a call");
    };
    assert_eq!(*callee, hir::Callee::Date(hir::DateFn::Utc));
    assert_eq!(args.len(), 7, "the runtime signature is always 7-argument");
    // day defaults to 1, the time components to 0.
    let values: Vec<i64> = args
        .iter()
        .skip(2)
        .map(|a| match a.kind {
            hir::ExprKind::Int(v) => v,
            ref other => panic!("expected an int default, got {other:?}"),
        })
        .collect();
    assert_eq!(values, vec![1, 0, 0, 0, 0]);
}

#[test]
fn date_is_not_interchangeable_with_i64() {
    // i64 → Date needs `new Date(ms)`.
    let err = check_one(
        "export function main(): void {\n  const d: Date = 0;\n  print(d.toISOString());\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    // Date → i64 needs `getTime()`.
    let err = check_one(
        "export function main(): void {\n  const d: Date = new Date(0);\n  const t: i64 = d;\n  print(`${t}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
}

#[test]
fn date_equality_is_s014_with_the_gettime_hint() {
    let err = check_one(
        "export function main(): void {\n  const a: Date = new Date(0);\n  const b: Date = new Date(0);\n  if (a === b) {\n    print(\"same\");\n  }\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(
        err[0].message.contains("getTime"),
        "message: {}",
        err[0].message
    );
    // Relational comparison is the same rejection.
    let err = check_one(
        "export function main(): void {\n  const a: Date = new Date(0);\n  const b: Date = new Date(1);\n  if (a < b) {\n    print(\"before\");\n  }\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
}

#[test]
fn date_nullable_union_is_s011() {
    let err = check_one("let d: Date | null = null;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S011);
}

#[test]
fn date_as_a_value_and_static_member_reads_are_s014() {
    let err = check_one("export function main(): void {\n  const d = Date;\n}\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    let err = check_one("export function main(): void {\n  const f = Date.now;\n}\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    let err =
        check_one("export function main(): void {\n  const t: i64 = Date.parse(\"2020\");\n}\n")
            .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S014);
    assert!(err[0].message.contains("parse"));
}

#[test]
fn a_user_class_named_date_shadows_the_builtin() {
    // Same rule as Math: program declarations win. The user class's
    // own constructor and methods apply, including local-sounding
    // names the ambient subset would reject.
    let module = check_one(
        "class Date { ms: i32;\n  constructor(ms: i32) { this.ms = ms; }\n  getFullYear(): i32 { return 1970; }\n}\nexport function main(): void {\n  const d: Date = new Date(3);\n  print(`${d.getFullYear()},${d.ms}`);\n}\n",
    )
    .expect("shadowing class checks clean");
    assert_eq!(module.classes.len(), 2);
    assert_eq!(module.classes[1].name, "Date");
}

#[test]
fn a_function_local_const_named_date_shadows_the_builtin_ctor() {
    // Stock tsc rejects this (TS2351: the local i32 is not
    // constructable); the ambient constructor must not apply once a
    // function-local binding shadows the name.
    let err = check_one(
        "export function main(): void {\n  const Date: i32 = 5;\n  const d = new Date(1);\n  print(`${Date}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert_eq!(
        err[0].message,
        "`Date` names a local value here, not a class"
    );
    assert_eq!(err[0].pos.line, 3);
}

#[test]
fn a_function_local_let_named_date_shadows_the_builtin_ctor() {
    let err = check_one(
        "export function main(): void {\n  let Date: i32 = 5;\n  Date += 1;\n  const d = new Date(1);\n  print(`${Date}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert_eq!(
        err[0].message,
        "`Date` names a local value here, not a class"
    );
    assert_eq!(err[0].pos.line, 4);
}

#[test]
fn a_parameter_named_date_shadows_the_builtin_ctor() {
    let err = check_one(
        "function f(Date: i32): i32 {\n  const d = new Date(1);\n  return Date;\n}\nexport function main(): void {\n  print(`${f(1)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert_eq!(
        err[0].message,
        "`Date` names a local value here, not a class"
    );
    assert_eq!(err[0].pos.line, 2);
}

// ----- Q35 workers -----

const WORKER_DECLS: &str = "class WorkerMessage { value: i32 = 0; }\nfunction workerEntry(inbox: Inbox<WorkerMessage>, outbox: Outbox<WorkerMessage>): void {\n  const message: WorkerMessage | null = inbox.wait();\n  if (message !== null) { outbox.post(message); }\n}\n";

#[test]
fn q35_spawn_infers_the_monomorphized_pair_and_records_one_adapter() {
    let source = format!(
        "{WORKER_DECLS}export function main(): void {{\n  const worker: Worker<WorkerMessage, WorkerMessage> = Worker.spawn(workerEntry);\n  worker.close();\n  worker.join();\n}}\n"
    );
    let module = check_one(&source).expect("Q35 worker program checks");
    assert_eq!(module.worker_entries.len(), 1);
    assert!(module.worker_entries[0].function == module.functions[0].symbol);
    let hir::Stmt::Let { ty, init, .. } = &module.functions[1].body[0] else {
        panic!("expected worker local");
    };
    assert!(matches!(ty, Type::Worker(_, _)));
    assert!(matches!(
        init.kind,
        hir::ExprKind::Call {
            callee: hir::Callee::Worker(hir::WorkerFn::Spawn(0)),
            ..
        }
    ));
}

#[test]
fn q35_context_affinity_rejects_all_four_escape_positions() {
    let cases = [
        (
            "module global",
            format!(
                "{WORKER_DECLS}const escaped: Worker<WorkerMessage, WorkerMessage> = Worker.spawn(workerEntry);\nexport function main(): void {{}}\n"
            ),
        ),
        (
            "class field",
            format!(
                "{WORKER_DECLS}class Holder {{ worker: Worker<WorkerMessage, WorkerMessage>; constructor(worker: Worker<WorkerMessage, WorkerMessage>) {{ this.worker = worker; }} }}\nexport function main(): void {{}}\n"
            ),
        ),
        (
            "array element",
            format!(
                "{WORKER_DECLS}export function main(): void {{\n  const worker: Worker<WorkerMessage, WorkerMessage> = Worker.spawn(workerEntry);\n  const escaped: Worker<WorkerMessage, WorkerMessage>[] = [worker];\n  worker.close(); worker.join();\n}}\n"
            ),
        ),
        (
            "lambda capture",
            format!(
                "{WORKER_DECLS}export function main(): void {{\n  const worker: Worker<WorkerMessage, WorkerMessage> = Worker.spawn(workerEntry);\n  const escaped: () => void = (): void => {{ worker.close(); }};\n  escaped(); worker.close(); worker.join();\n}}\n"
            ),
        ),
    ];
    for (position, source) in cases {
        let diagnostics = match check_one(&source) {
            Err(diagnostics) => diagnostics,
            Ok(_) => panic!("{position} escape was accepted"),
        };
        // compiler.md §132.2 acceptance 3: exactly one diagnostic.
        assert_eq!(diagnostics.len(), 1, "{position}: {diagnostics:?}");
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == RuleCode::S100
                    && (diagnostic.message.contains("Worker")
                        || diagnostic.message.contains("Context-affine"))
            }),
            "{position}: {diagnostics:?}"
        );
    }
}

#[test]
fn q35_context_affinity_rejects_every_container_type_argument() {
    let cases = [
        ("Map key", "Map<Worker<WorkerMessage, WorkerMessage>, i32>"),
        ("Set element", "Set<Worker<WorkerMessage, WorkerMessage>>"),
        (
            "local Map value",
            "Map<i32, Worker<WorkerMessage, WorkerMessage>>",
        ),
    ];
    for (position, annotation) in cases {
        let source = format!(
            "{WORKER_DECLS}export function main(): void {{\n  const escaped: {annotation} = new {annotation}();\n}}\n"
        );
        let diagnostics = check_one(&source).expect_err("affine container argument");
        // compiler.md §132.2 acceptance 3: the annotation reports the
        // failure once, and the construction reports nothing more.
        assert_eq!(diagnostics.len(), 1, "{position}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{position}");
        assert_eq!(
            diagnostics[0].message, AFFINE_CONTAINER_ARGUMENT,
            "{position}"
        );
        assert_eq!(
            (diagnostics[0].pos.line, diagnostics[0].pos.col),
            (
                7,
                18 + annotation.find("Worker").expect("Worker argument") as u32
            ),
            "{position}"
        );
    }
}

const AFFINE_CONTAINER_ARGUMENT: &str =
    "Worker, Inbox, and Outbox values may not be container type arguments";

/// compiler.md §132.2 acceptance 3: the r111 program reports the §40.1
/// S100 and no diagnostic that follows from it.
#[test]
fn q35_r111_program_reports_exactly_one_diagnostic() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../corpus/reject/r111-worker-in-map-value.ts"
    ))
    .expect("read r111");
    let diagnostics = check_one(&source).expect_err("r111 is rejected");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].message, AFFINE_CONTAINER_ARGUMENT);
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (19, 25));
}

/// compiler.md §132.2 acceptance 2: the type arguments of `new` pass
/// the §40.1 rule, for each affine type in each container slot. The
/// surface has no type alias for a non-literal type, so no alias form
/// exists to test.
#[test]
fn q35_affine_type_arguments_of_new_report_exactly_one_diagnostic() {
    let affine = [
        "Worker<WorkerMessage, WorkerMessage>",
        "Inbox<WorkerMessage>",
        "Outbox<WorkerMessage>",
    ];
    for ty in affine {
        for (slot, construction) in [
            ("Map key", format!("new Map<{ty}, i32>()")),
            ("Map value", format!("new Map<i32, {ty}>()")),
            ("Set element", format!("new Set<{ty}>()")),
        ] {
            for statement in [
                format!("const escaped = {construction};"),
                format!("{construction};"),
            ] {
                let source =
                    format!("{WORKER_DECLS}export function main(): void {{\n  {statement}\n}}\n");
                let diagnostics = check_one(&source).expect_err("affine type argument of new");
                assert_eq!(diagnostics.len(), 1, "{slot} {statement}: {diagnostics:?}");
                assert_eq!(diagnostics[0].code, RuleCode::S100, "{slot} {statement}");
                assert_eq!(
                    diagnostics[0].message, AFFINE_CONTAINER_ARGUMENT,
                    "{slot} {statement}"
                );
                let column = 3 + statement.find(ty).expect("affine argument") as u32;
                assert_eq!(
                    (diagnostics[0].pos.line, diagnostics[0].pos.col),
                    (7, column),
                    "{slot} {statement}"
                );
                assert_eq!(
                    diagnostics[0].divergence,
                    Some(crate::divergence::Divergence::ContextAffineContainerArgument),
                    "{slot} {statement}"
                );
            }
        }
    }

    // Control: the same constructions with a message class are accepted.
    for (slot, construction) in [
        ("Map key", "new Map<WorkerMessage, i32>()"),
        ("Map value", "new Map<i32, WorkerMessage>()"),
        ("Set element", "new Set<WorkerMessage>()"),
    ] {
        let source = format!(
            "{WORKER_DECLS}export function main(): void {{\n  const kept = {construction};\n  {construction};\n}}\n"
        );
        let result = check_one(&source);
        assert!(result.is_ok(), "{slot}: {:?}", result.err());
    }
}

/// compiler.md §132 rule 2: an annotated declaration with an affine
/// argument reports once in each slot, whatever the initializer forms.
#[test]
fn q35_annotated_affine_container_reports_exactly_one_diagnostic() {
    let worker = "Worker<WorkerMessage, WorkerMessage>";
    let cases = [
        format!("const m: Map<i32, {worker}> = new Map<i32, {worker}>();"),
        format!("const m: Map<{worker}, i32> = new Map<{worker}, i32>();"),
        format!("const s: Set<{worker}> = new Set<{worker}>();"),
        format!("const m: Map<i32, {worker}> | null = new Map<i32, {worker}>();"),
        format!("const a: {worker}[] = [];"),
    ];
    for statement in cases {
        let source = format!("{WORKER_DECLS}export function main(): void {{\n  {statement}\n}}\n");
        let diagnostics = check_one(&source).expect_err("annotated affine container");
        assert_eq!(diagnostics.len(), 1, "{statement}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{statement}");
        let message = if statement.contains("[]") {
            "Worker, Inbox, and Outbox values may not be array elements"
        } else {
            "Worker, Inbox, and Outbox values may not be container type arguments"
        };
        assert_eq!(diagnostics[0].message, message, "{statement}");
        let column = 3 + statement.find(worker).expect("affine argument") as u32;
        assert_eq!(
            (diagnostics[0].pos.line, diagnostics[0].pos.col),
            (7, column),
            "{statement}"
        );
    }

    // Control: the same declarations with a message class are accepted.
    let message = "WorkerMessage";
    let controls = [
        format!("const m: Map<i32, {message}> = new Map<i32, {message}>();"),
        format!("const m: Map<{message}, i32> = new Map<{message}, i32>();"),
        format!("const s: Set<{message}> = new Set<{message}>();"),
        format!("const m: Map<i32, {message}> | null = new Map<i32, {message}>();"),
        format!("const a: {message}[] = [];"),
    ];
    for statement in controls {
        let source = format!("{WORKER_DECLS}export function main(): void {{\n  {statement}\n}}\n");
        let result = check_one(&source);
        assert!(result.is_ok(), "{statement}: {:?}", result.err());
    }
}

#[test]
fn q35_spawn_rejects_every_non_entry_argument_form() {
    let arguments = [
        "(inbox: Inbox<WorkerMessage>, outbox: Outbox<WorkerMessage>): void => {}",
        "localEntry",
        "new WorkerMessage()",
    ];
    for argument in arguments {
        let local = if argument == "localEntry" {
            "  const localEntry: (inbox: Inbox<WorkerMessage>, outbox: Outbox<WorkerMessage>) => void = workerEntry;\n"
        } else {
            ""
        };
        let source = format!(
            "{WORKER_DECLS}export function main(): void {{\n{local}  const worker = Worker.spawn({argument});\n}}\n"
        );
        let diagnostics = check_one(&source).expect_err("non-entry spawn argument");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{argument}");
        assert!(
            diagnostics[0].message.contains("Worker.spawn"),
            "{argument}"
        );
    }
}

#[test]
fn q35_new_rejects_all_runtime_created_handle_types_in_our_checker() {
    for construction in [
        "new Worker<WorkerMessage, WorkerMessage>()",
        "new Inbox<WorkerMessage>()",
        "new Outbox<WorkerMessage>()",
    ] {
        let source = format!(
            "{WORKER_DECLS}export function main(): void {{\n  const value = {construction};\n}}\n"
        );
        let diagnostics = check_one(&source).expect_err("runtime-created handle construction");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{construction}");
        assert!(diagnostics[0].message.contains("runtime-created"));
    }
}

#[test]
fn q35_transferability_diagnostic_names_the_innermost_field() {
    let source = "enum Kind { First }\n@ValueType class Stamp { kind: Kind = Kind.First; }\nclass BoxedCount { value: i32 = 0; }\nclass BadMessage { stamps: FixedArray<Stamp, 2> = [new Stamp(), new Stamp()]; boxed: BoxedCount = new BoxedCount(); }\nfunction entry(inbox: Inbox<BadMessage>, outbox: Outbox<BadMessage>): void {}\nexport function main(): void { const worker = Worker.spawn(entry); }\n";
    let diagnostics = check_one(source).expect_err("reference message field");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].pos.line, 4);
    assert!(diagnostics[0].message.contains("BadMessage.boxed"));
    assert!(diagnostics[0].message.contains("BoxedCount"));
}

#[test]
fn r34_context_bytes_call_keeps_the_explicit_type_in_hir() {
    let module = check_one(
        "@ValueType\nclass Word { value: u32 = 0; }\nexport function main(): void {\n  const word: Word = new Word();\n  const bytes: u8[] = Context.bytesOf<Word>(word);\n  print(`${bytes.length}`);\n}\n",
    )
    .expect("Context.bytesOf must check");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    let hir::Stmt::Let { init, .. } = &main.body[1] else {
        panic!("expected bytes local");
    };
    assert!(matches!(
        &init.kind,
        hir::ExprKind::Call {
            callee: hir::Callee::ContextBytes {
                function: hir::ContextBytesFn::BytesOf,
                ty: Type::Class(_),
            },
            ..
        }
    ));
}

#[test]
fn r34_boundary_string_field_is_s100_with_the_field_name() {
    let diagnostics = check_program(&[
        SourceFile::ambient(
            "record.d.ts",
            "declare class Record { label: string; serial: u32; constructor(label: string, serial: u32); }\n",
        ),
        SourceFile::new(
            "test.ts",
            "export function main(): void {\n  const record: Record = new Record(\"x\", 1);\n  Context.bytesOf<Record>(record);\n}\n",
        ),
    ])
    .expect_err("boundary storage must be rejected");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "`Context.bytesOf<T>` cannot use `Record`; field `label` has unsupported type `string` (its storage type is not eligible)"
    );
}

#[test]
fn r34_missing_type_argument_is_s014() {
    let diagnostics = check_one(
        "@ValueType\nclass Word { value: u32 = 0; }\nexport function main(): void {\n  const word: Word = new Word();\n  Context.bytesOf(word);\n}\n",
    )
    .expect_err("the type argument must be explicit");
    assert_eq!(diagnostics[0].code, RuleCode::S014);
    assert_eq!(
        diagnostics[0].message,
        "`Context.bytesOf<T>` takes exactly one type argument"
    );
}

#[test]
fn same_shaped_classes_do_not_substitute() {
    let err = check_one(
        "class A { x: i32 = 1; }\nclass B { x: i32 = 1; }\nfunction f(a: A): i32 { return a.x; }\nexport function main(): void {\n  const b: B = new B();\n  print(`${f(b)}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S005);
}

/// One rejected binding pattern in each position that accepts a
/// pattern (compiler.md §107.1), plus the three positions that
/// reject every pattern (§107.5): a module-level declaration, a
/// mirror `declare const`, and an assignment target.
const PATTERN_POSITIONS: &[(&str, &str)] = &[
    (
        "declaration",
        "export function main(): void {\n  const xs: i32[] = [1, 2];\n  const [a, ...b] = xs;\n  print(`${a} ${b.length}`);\n}\n",
    ),
    (
        "field declaration",
        "class P { x: i32 = 1; }\nexport function main(): void {\n  const p: P = new P();\n  const { x, ...rest } = p;\n  print(`${x}`);\n}\n",
    ),
    (
        "free-function parameter",
        "function take([a, ...b]: i32[]): i32 { return a + b.length; }\nexport function main(): void {\n  print(`${take([1, 2])}`);\n}\n",
    ),
    (
        "method parameter",
        "class Box {\n  sum([a, ...b]: i32[]): i32 { return a + b.length; }\n}\nexport function main(): void {\n  print(`${new Box().sum([1, 2])}`);\n}\n",
    ),
    (
        "static method parameter",
        "class Box {\n  static sum([a, ...b]: i32[]): i32 { return a + b.length; }\n}\nexport function main(): void {\n  print(`${Box.sum([1, 2])}`);\n}\n",
    ),
    (
        "lambda parameter",
        "export function main(): void {\n  const f = ([a, ...b]: i32[]): i32 => a + b.length;\n  print(`${f([1, 2])}`);\n}\n",
    ),
    (
        "constructor parameter",
        "class Box {\n  total: i32;\n  constructor([a, ...b]: i32[]) { this.total = a + b.length; }\n}\nexport function main(): void {\n  print(`${new Box([1, 2]).total}`);\n}\n",
    ),
    (
        "for-of binding",
        "export function main(): void {\n  const xss: i32[][] = [[1, 2]];\n  for (const [a, ...b] of xss) {\n    print(`${a} ${b.length}`);\n  }\n}\n",
    ),
    (
        "module-level declaration",
        "const xs: i32[] = [1, 2];\nconst [a, b] = xs;\nexport function main(): void {\n  print(`${a} ${b}`);\n}\n",
    ),
    (
        "ambient declaration",
        "declare const [a, b]: i32[];\nexport function main(): void {\n  print(`${a} ${b}`);\n}\n",
    ),
    (
        "assignment target",
        "export function main(): void {\n  const xs: i32[] = [1, 2];\n  let a: i32 = 0;\n  let b: i32 = 0;\n  [a, b] = xs;\n  print(`${a} ${b}`);\n}\n",
    ),
];

#[test]
fn a_rejected_pattern_reports_one_diagnostic_in_every_position() {
    let mut counts = Vec::new();
    for (position, source) in PATTERN_POSITIONS {
        let diagnostics =
            check_one(source).expect_err("a rest element in a pattern stays rejected");
        if diagnostics.len() != 1 {
            let messages: Vec<&str> = diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect();
            counts.push(format!("{position}: {} — {messages:?}", diagnostics.len()));
        }
    }
    assert!(
        counts.is_empty(),
        "a rejected pattern must report one time:\n{}",
        counts.join("\n")
    );
}

/// The count above is a real measurement: two rejected patterns in
/// one program report two times.
#[test]
fn two_rejected_patterns_report_two_diagnostics() {
    let diagnostics = check_one(
        "export function main(): void {\n  const xs: i32[] = [1, 2];\n  const [a, ...b] = xs;\n  const [c, ...d] = xs;\n  print(`${a} ${b.length} ${c} ${d.length}`);\n}\n",
    )
    .expect_err("both patterns stay rejected");
    assert_eq!(diagnostics.len(), 2);
}

#[test]
fn a_binding_pattern_binds_in_every_accepted_position() {
    for source in [
        "export function main(): void {\n  const xs: i32[] = [1, 2];\n  const [a, b] = xs;\n  print(`${a} ${b}`);\n}\n",
        "class P { x: i32 = 1; }\nexport function main(): void {\n  const { x } = new P();\n  print(`${x}`);\n}\n",
        "function take([a, b]: i32[]): i32 { return a + b; }\nexport function main(): void {\n  print(`${take([1, 2])}`);\n}\n",
        "class Box {\n  sum([a, b]: i32[]): i32 { return a + b; }\n}\nexport function main(): void {\n  print(`${new Box().sum([1, 2])}`);\n}\n",
        "export function main(): void {\n  const f = ([a, b]: i32[]): i32 => a + b;\n  print(`${f([1, 2])}`);\n}\n",
        "class Box {\n  total: i32;\n  constructor([a, b]: i32[]) { this.total = a + b; }\n}\nexport function main(): void {\n  print(`${new Box([1, 2]).total}`);\n}\n",
        "export function main(): void {\n  const xss: i32[][] = [[1, 2]];\n  for (const [a, b] of xss) {\n    print(`${a} ${b}`);\n  }\n}\n",
    ] {
        check_one(source).unwrap_or_else(|diagnostics| {
            panic!("{source}\nrejected: {:?}", diagnostics[0].message)
        });
    }
}

#[test]
fn a_pattern_source_of_another_shape_names_the_shape_it_reads() {
    let diagnostics = check_one(
        "export function main(): void {\n  const text: string = \"ab\";\n  const [first, second] = text;\n  print(`${first} ${second}`);\n}\n",
    )
    .expect_err("a string is not an array source");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(
        diagnostics[0]
            .message
            .contains("`T[]` or a `FixedArray<T, N>`")
            && diagnostics[0].message.contains("`string`"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn a_boundary_signature_rejects_a_binding_pattern_parameter() {
    let diagnostics = check_program(&[
        SourceFile::ambient(
            "boundary.d.ts",
            "// @subscript-c-header include=\"host.h\"\n\
             declare function hostTake([first, second]: i32[]): void;\n",
        ),
        SourceFile::new(
            "test.ts",
            "export function main(): void { print(\"ok\"); }\n",
        ),
    ])
    .expect_err("a boundary signature holds no entry prologue");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(
        diagnostics[0].message.contains("parameter pattern"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn a_read_before_a_pattern_declaration_names_the_order() {
    let diagnostics = check_one(
        "export function main(): void {\n  const xs: i32[] = [1, 2];\n  print(`${a}`);\n  const [a, b] = xs;\n  print(`${b}`);\n}\n",
    )
    .expect_err("a read before the pattern is rejected");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(
        diagnostics[0]
            .message
            .contains("read before its declaration"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn a_pattern_evaluates_its_source_one_time() {
    let module = check_one(
        "function made(): i32[] {\n  print(\"made\");\n  return [1, 2];\n}\nexport function main(): void {\n  const [a, b] = made();\n  print(`${a} ${b}`);\n}\n",
    )
    .expect("the pattern binds");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let calls = main
        .body
        .iter()
        .filter(|statement| match statement {
            hir::Stmt::Let { init, .. } => matches!(init.kind, hir::ExprKind::Call { .. }),
            _ => false,
        })
        .count();
    assert_eq!(calls, 1, "the source must be called one time");
}
