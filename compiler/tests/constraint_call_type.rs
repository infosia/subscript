//! Calls through declared constraints (compiler.md §335 rules 1 to 4).
//! Cost: short checker calls; the corpus sweeps cover both execution tiers.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

fn check(source: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::new("constraint.ts", source)])
}

const CLASSES: &str = "class C { v: i32 = 7; }
interface R { m(): C | null; }
interface S { m(): C; }
class K implements R, S { m(): C { return new C(); } }";

#[test]
fn calls_keep_each_parameters_constraint_with_one_concrete_class() {
    let module = check(&format!(
        "{CLASSES}
         function both<U extends R, V extends S>(x: U, y: V): i32 {{
           const a = x.m(); const b = y.m();
           return a === null ? -1 : a.v + b.v;
         }}
         export function main(): void {{ print(`${{both(new K(), new K())}}`); }}"
    ))
    .expect("each parameter keeps its own constraint");
    let body = &module
        .functions
        .iter()
        .find(|f| f.name.starts_with("both<"))
        .expect("the concrete instance")
        .body;
    let hir::Stmt::Let {
        ty: first, init, ..
    } = &body[0]
    else {
        panic!("first result binding");
    };
    assert!(matches!(first, Type::Nullable(..)));
    let hir::ExprKind::Cast(call) = &init.kind else {
        panic!("the call result records the constraint conversion");
    };
    assert_eq!(&init.ty, first);
    assert!(matches!(call.ty, Type::Class(_)));
    let hir::ExprKind::Call {
        callee: hir::Callee::Method { recv, .. },
        ..
    } = &call.kind
    else {
        panic!("the instance method runs");
    };
    assert!(matches!(recv.ty, Type::Class(_)));
    let hir::Stmt::Let {
        ty: second, init, ..
    } = &body[1]
    else {
        panic!("second result binding");
    };
    assert!(matches!(second, Type::Class(_)));
    assert!(matches!(init.kind, hir::ExprKind::Call { .. }));
}

#[test]
fn declared_locals_inferred_aliases_and_lambda_parameters_keep_the_constraint() {
    for receiver in ["x", "declared", "alias"] {
        check(&format!(
            "{CLASSES}
             function g<U extends R>(x: U): i32 {{
               const declared: U = x; const alias = declared;
               const o = {receiver}.m(); return o === null ? -1 : o.v;
             }}
             export function main(): void {{ print(`${{g(new K())}}`); }}"
        ))
        .expect("a binding keeps the declared parameter");
    }
    check(&format!(
        "{CLASSES}
         function g<U extends R>(x: U): i32 {{
           const f = (y: U): i32 => {{ const o = y.m(); return o === null ? -1 : o.v; }};
           return f(x);
         }}
         export function main(): void {{ print(`${{g(new K())}}`); }}"
    ))
    .expect("lambda annotations keep the declared parameter");
}

#[test]
fn concrete_calls_keep_the_class_result_and_constraint_calls_reject_unsafe_uses() {
    for (source, code) in [
        (format!("{CLASSES} export function main(): void {{ const o = new K().m(); if (o === null) {{}} }}"), RuleCode::S100),
        (format!("{CLASSES} function g<U extends R>(x: U): C {{ return x.m(); }}
                 export function main(): void {{ g(new K()); }}"), RuleCode::S005),
        (format!("{CLASSES} function g<U extends R>(x: U): i32 {{ return x.m().v; }}
                 export function main(): void {{ g(new K()); }}"), RuleCode::S011),
    ] {
        let errors = check(&source).expect_err("the source builds the violating form");
        assert!(errors.iter().any(|d| d.code == code), "{errors:?}");
    }
}

#[test]
fn other_value_uses_and_class_constraints_keep_the_instance_type() {
    check(&format!(
        "{CLASSES}
         class Store<T extends R> {{ value: T; constructor(value: T) {{ this.value = value; }} }}
         function take<T extends R>(x: T): void {{}}
         function g<U extends R>(x: U): U {{ take(x); const s = new Store<U>(x); return x; }}
         function h<U extends K>(x: U): C {{ return x.m(); }}
         export function main(): void {{ const k: K = g(new K()); const c: C = h(k); }}"
    ))
    .expect("compiler.md §335 rule 4");
}

#[test]
fn acceptance_entry_checks() {
    check(include_str!(
        "../../corpus/accept/a485-call-through-constraint-type.ts"
    ))
    .expect("compiler.md §335 acceptance 1");
}

#[test]
fn every_value_form_keeps_the_constraint_in_its_type() {
    let shapes = [
        (
            "field",
            "class B<T extends R> { t: T; constructor(t: T) { this.t = t; }
         run(): i32 { let o = this.t.m(); if (o === null) { return -1; } o = null; return 0; } }",
            "new B(new INSTANCE()).run()",
        ),
        (
            "loop",
            "function g<U extends R>(xs: U[]): i32 { for (const x of xs) {
         let o = x.m(); if (o === null) { return -1; } o = null; } return 0; }",
            "g([new INSTANCE()])",
        ),
        (
            "element",
            "function g<U extends R>(xs: U[]): i32 { let o = xs[0].m();
         if (o === null) { return -1; } o = null; return 0; }",
            "g([new INSTANCE()])",
        ),
        (
            "narrowed",
            "function g<U extends R>(x: U | null): i32 { if (x !== null) {
         let o = x.m(); if (o === null) { return -1; } o = null; } return 0; }",
            "g(new INSTANCE())",
        ),
        (
            "result",
            "function id<U extends R>(x: U): U { return x; }
         function g<U extends R>(x: U): i32 { let o = id(x).m();
         if (o === null) { return -1; } o = null; return 0; }",
            "g(new INSTANCE())",
        ),
    ];
    for (name, declaration, call) in shapes {
        for instance in ["K", "K0"] {
            let source = format!(
                "{CLASSES}
             class K0 implements R {{ m(): C | null {{ return new C(); }} }}
             {declaration} export function main(): void {{ {}; }}",
                call.replace("INSTANCE", instance)
            );
            check(&source).unwrap_or_else(|errors| panic!("{name}/{instance}: {errors:?}"));
        }
    }
}

#[test]
fn every_value_form_rejects_an_unchecked_nullable_result() {
    for expression in ["x.m()", "xs[0].m()", "id(x).m()"] {
        let source = format!(
            "{CLASSES}
         function id<U extends R>(x: U): U {{ return x; }}
         function g<U extends R>(x: U, xs: U[]): i32 {{ return {expression}.v; }}
         export function main(): void {{ const k = new K(); g(k, [k]); }}"
        );
        let errors = check(&source).expect_err("the source omits the required null test");
        assert!(
            errors.iter().any(|d| d.code == RuleCode::S011),
            "{errors:?}"
        );
    }
}

#[test]
fn generic_results_keep_actual_constraints_across_cached_instances() {
    for calls in [
        "wide(new K()); narrow(new K());",
        "narrow(new K()); wide(new K());",
    ] {
        for expression in ["id(x)", "id<U>(x)"] {
            check(&format!("{CLASSES}
             function id<T extends R>(x: T): T {{ return x; }}
             function wide<U extends R>(x: U): i32 {{ const o = {expression}.m(); return o === null ? -1 : o.v; }}
             function narrow<U extends K>(x: U): i32 {{ return {expression}.m().v; }}
             export function main(): void {{ {calls} const c: C = id(new K()).m(); }}"))
            .expect("a cached runtime instance preserves each call-site type argument");
        }
    }
}

#[test]
fn external_class_members_have_plain_types_and_internal_members_keep_constraints() {
    check(&format!(
        "{CLASSES}
     class B<T extends R> {{
       t: T; items: T[];
       constructor(t: T) {{ this.t = t; this.items = [t]; }}
       get value(): T {{ return this.t; }}
       get(): T {{ return this.t; }}
       run(): i32 {{
         let a = this.t.m(); let b = this.value.m();
         let c = this.get().m(); let d = this.items[0].m();
         if (a === null || b === null || c === null || d === null) {{ return -1; }}
         a = null; b = null; c = null; d = null; return 0;
       }}
     }}
     export function main(): void {{
       const b = new B(new K());
       const a: C = b.t.m(); const c: C = b.value.m();
       const d: C = b.get().m(); const e: C = b.items[0].m(); b.run();
     }}"
    ))
    .expect("compiler.md §335 rule 1a distinguishes the class body from external reads");
    let errors = check(include_str!(
        "../../corpus/reject/r556-generic-class-external-member-constraint.ts"
    ))
    .expect_err("the external result cannot receive null");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S100);
    assert!(errors[0]
        .message
        .contains("the assignment expects `C`, got `null`"));
}

#[test]
fn a_member_read_in_another_generic_body_keeps_the_recorded_plain_type_limit() {
    for (instance, accepted) in [("K", false), ("K0", true)] {
        let source = format!("{CLASSES}
         class K0 implements R {{ m(): C | null {{ return new C(); }} }}
         class B<T extends R> {{ t: T; constructor(t: T) {{ this.t = t; }} }}
         function g<U extends R>(b: B<U>): i32 {{ const o = b.t.m(); return o === null ? -1 : o.v; }}
         export function main(): void {{ g(new B(new {instance}())); }}");
        if accepted {
            check(&source).expect("the plain nullable class result stays nullable");
        } else {
            let errors =
                check(&source).expect_err("compiler.md §335 rule 1a records this false rejection");
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == RuleCode::S100 && d.message.contains("operator `===`")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn awaited_generic_results_keep_each_callers_type_arguments() {
    check(&format!(
        "{CLASSES}
     async function id<T extends R>(x: T): Promise<T> {{ return x; }}
     async function wide<U extends R>(x: U): Promise<i32> {{
       const k = await id(x); const o = k.m(); return o === null ? -1 : o.v;
     }}
     async function narrow<U extends K>(x: U): Promise<i32> {{
       const k = await id(x); return k.m().v;
     }}
     export async function main(): Promise<void> {{
       print(`${{await wide(new K())}}:${{await narrow(new K())}}`);
       const k = await id(new K()); const c: C = k.m();
     }}"
    ))
    .expect("compiler.md §335 rule 1 applies to awaited generic results");
    let errors = check(&format!(
        "{CLASSES}
     async function id<T extends R>(x: T): Promise<T> {{ return x; }}
     export async function main(): Promise<void> {{
       const k = await id(new K()); let o = k.m(); o = null;
     }}"
    ))
    .expect_err("the concrete call result cannot receive null");
    assert!(
        errors.iter().any(|d| d.code == RuleCode::S100
            && d.message.contains("the assignment expects `C`, got `null`")),
        "{errors:?}"
    );
}

#[test]
fn generic_method_results_keep_the_callers_type_arguments() {
    for (declaration, call) in [
        ("id<T extends R>(x: T): T { return x; }", "util.id(x)"),
        (
            "static id<T extends R>(x: T): T { return x; }",
            "Util.id(x)",
        ),
        (
            "async id<T extends R>(x: T): Promise<T> { return x; }",
            "await util.id(x)",
        ),
    ] {
        check(&format!("{CLASSES} class Util {{ {declaration} }}
         async function wide<U extends R>(x: U): Promise<i32> {{ const util = new Util();
           const k = {call}; const o = k.m(); return o === null ? -1 : o.v;
         }}
         async function narrow<U extends K>(x: U): Promise<i32> {{ const util = new Util();
           const k = {call}; return k.m().v;
         }}
         export async function main(): Promise<void> {{ print(`${{await wide(new K())}}:${{await narrow(new K())}}`); }}"))
        .expect("compiler.md §335 rule 1 also applies to generic method results");
        let source = format!(
            "{CLASSES} class Util {{ {declaration} }}
         export async function main(): Promise<void> {{ const util = new Util(); const x = new K();
           const k = {call}; let o = k.m(); o = null;
         }}"
        );
        let errors = check(&source).expect_err("a concrete method result cannot receive null");
        assert!(
            errors.iter().any(|d| d.code == RuleCode::S100
                && d.message.contains("the assignment expects `C`, got `null`")),
            "{errors:?}"
        );
    }
}

#[test]
fn unconstrained_instances_keep_each_callers_result_in_both_call_orders() {
    for (declaration, call, container) in [
        (
            "function first<T>(xs: T[]): T { return xs[0]; }",
            "first(xs)",
            true,
        ),
        ("function pick<T>(x: T): T { return x; }", "pick(x)", false),
        (
            "class Util { id<T>(x: T): T { return x; } }",
            "new Util().id(x)",
            false,
        ),
        (
            "class Util { static id<T>(x: T): T { return x; } }",
            "Util.id(x)",
            false,
        ),
        (
            "async function pick<T>(x: T): Promise<T> { return x; }",
            "await pick(x)",
            false,
        ),
        (
            "class Util { async id<T>(x: T): Promise<T> { return x; } }",
            "await new Util().id(x)",
            false,
        ),
    ] {
        let parameter = if container { "xs: U[]" } else { "x: U" };
        let plain_call = call.replace("xs", "[k]").replace("(x)", "(k)");
        let via = if container { "via([k])" } else { "via(k)" };
        for first in [true, false] {
            let statements = if first {
                format!("await {via}; const result = {plain_call}; const c: C = result.m();")
            } else {
                format!("const result = {plain_call}; const c: C = result.m(); await {via};")
            };
            let source = format!(
                "{CLASSES} {declaration}
             async function via<U extends R>({parameter}): Promise<i32> {{
               const result = {call}; const o = result.m(); return o === null ? -1 : o.v;
             }}
             export async function main(): Promise<void> {{ const k = new K(); {statements} }}"
            );
            check(&source)
                .unwrap_or_else(|errors| panic!("{call}, constrained first={first}: {errors:?}"));
        }
    }
}

#[test]
fn unconstrained_results_use_parameter_identity_for_equal_instance_arguments() {
    check(&format!(
        "{CLASSES}
     function left<T, V>(x: T, y: V): T {{ return x; }}
     function right<T, V>(x: T, y: V): V {{ return y; }}
     function g<U extends R, V extends S>(x: U, y: V): i32 {{
       const a = left(x, y).m(); const b = right(x, y).m();
       return a === null ? -1 : a.v + b.v;
     }}
     export function main(): void {{ const k = new K(); g(k, k); }}"
    ))
    .expect("compiler.md §335 rule 1b matches parameter identities, not concrete types");
    let errors = check(include_str!(
        "../../corpus/reject/r557-unconstrained-result-keeps-caller-type.ts"
    ))
    .expect_err("a constrained call cannot change the next caller's concrete result");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S100);
    assert!(errors[0]
        .message
        .contains("the assignment expects `C`, got `null`"));
}

#[test]
fn async_constraint_calls_keep_the_interface_parameters_and_handle_obligation() {
    for method in [
        "async run(value: C | null): Promise<i32> { return 7; }",
        "run(value: C | null): Promise<i32> { return work(); }",
    ] {
        for call in [
            "await x.run(new C())",
            "await (x.run(new C()))",
            "await held",
        ] {
            let held = if call == "await held" {
                "const held = x.run(new C());"
            } else {
                ""
            };
            let source = format!(
                "class C {{ v: i32 = 7; }}
          interface R {{ run(value: C): Promise<i32>; }}
          async function work(): Promise<i32> {{ return 7; }}
          class K implements R {{ {method} }}
          async function g<U extends R>(x: U): Promise<i32> {{
            {held} return {call};
          }}
          export async function main(): Promise<void> {{ await g(new K()); }}"
            );
            check(&source).expect("an async constraint call keeps its handle obligation");
            let errors = check(&source.replace("new C()", "null"))
                .expect_err("a constraint call checks the interface parameter");
            assert!(
                errors.iter().any(|error| error.code == RuleCode::S100),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn instance_equality_joins_parameter_and_plain_types() {
    for expression in ["c ? x : y", "c ? x : new K()"] {
        check(&format!("{CLASSES}
          function choose<U extends R, V extends R>(x: U, y: V, c: boolean): i32 {{
            const z = {expression}; return z === x ? 1 : 0;
          }}
          export function main(): void {{ const k = new K(); choose(k, k, true); choose(k, k, false); }}"))
            .expect("compiler.md §335 rule 1c compares plain instance types");
    }
}

#[test]
fn definition_check_keeps_distinct_parameter_identities() {
    let errors = check(&format!(
        "{CLASSES}
      function bad<U extends R, V extends R>(u: U, v: V): void {{ const other: V = u; }}
      export function main(): void {{ const k = new K(); bad(k, k); }}"
    ))
    .expect_err("compiler.md §335 rule 1c keeps the generic definition check");
    assert!(
        errors.iter().any(|error| error.code == RuleCode::S100
            && error.message == "type mismatch: the initializer expects `V`, got `U`"),
        "{errors:?}"
    );
}

#[test]
fn whole_external_member_types_are_plain_and_internal_contexts_keep_constraints() {
    let declarations = format!("{CLASSES}
      class B<T extends R> {{
        t: T; field: (f: (t: T) => i32) => i32;
        constructor(t: T) {{ this.t = t; const value = t; this.field = (f: (t: T) => i32): i32 => f(value); }}
        each(f: (t: T) => i32): i32 {{ return f(this.t); }}
        nested(f: () => ((t: T) => i32)): i32 {{ return f()(this.t); }}
        async later(f: (t: T) => i32): Promise<i32> {{ return f(this.t); }}
        get callback(): (t: T) => i32 {{ return (t: T): i32 => 0; }}
        set callback(f: (t: T) => i32) {{ f(this.t); }}
        run(): i32 {{ INTERNAL; return 0; }}
      }}");
    for call in [
        "RECEIVER.each((t) => { let o = t.m(); o = null; return 1; })",
        "RECEIVER.nested(() => (t) => { let o = t.m(); o = null; return 1; })",
        "RECEIVER.field((t) => { let o = t.m(); o = null; return 1; })",
        "RECEIVER.callback = (t) => { let o = t.m(); o = null; return 1; }",
    ] {
        let internal = declarations.replace("INTERNAL", &call.replace("RECEIVER", "this"));
        check(&format!(
            "{internal} export function main(): void {{ new B(new K()).run(); }}"
        ))
        .unwrap_or_else(|errors| panic!("in-class {call}: {errors:?}"));
        let source = format!(
            "{internal} export function main(): void {{
          const b = new B(new K()); {}; }}",
            call.replace("RECEIVER", "b")
        );
        let errors =
            check(&source).expect_err("an external callback receives the plain class type");
        assert!(
            errors.iter().any(|error| error.code == RuleCode::S100
                && error
                    .message
                    .contains("the assignment expects `C`, got `null`")),
            "{call}: {errors:?}"
        );
    }
    let source = declarations.replace("INTERNAL", "0");
    let errors = check(&format!(
        "{source} export async function main(): Promise<void> {{
      const b = new B(new K()); await b.later((t) => {{ let o = t.m(); o = null; return 1; }}); }}"
    ))
    .expect_err("an awaited method projects its parameter type too");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("the assignment expects `C`, got `null`")),
        "{errors:?}"
    );
}

#[test]
fn conditional_with_plain_first_keeps_the_recorded_rejection() {
    let errors = check(&format!(
        "{CLASSES}
      function g<U extends R>(x: U, c: boolean): i32 {{
        const z = c ? new K() : x; const o = z.m(); return o === null ? -1 : o.v;
      }}
      export function main(): void {{ g(new K(), true); }}"
    ))
    .expect_err("compiler.md §335 retains the recorded conditional order limit");
    assert!(
        errors
            .iter()
            .any(|error| error.code == RuleCode::S100 && error.message.contains("operator `===`")),
        "{errors:?}"
    );
}
