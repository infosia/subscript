//! Fresh locals exclude only moves in their own body (compiler.md §333).
//! Measured cost: six tests take 0.05 seconds, excluding the build.

use subscript_compiler::{check_program, RuleCode, SourceFile};

fn check(body: &str, rejected: bool) {
    let source = format!(
        "@ValueType class V {{
           a: i32 = 1;
           run(p: V[]): void {{ {body} }}
         }}
         class Holder {{ xs: V[] = []; }}
         const holder: Holder = new Holder();
         const keep: V[] = [];
         function take(p: V[]): void {{ p.push(new V()); }}
         function make(): V[] {{ return []; }}
         const all: V[] = [new V()];
         all[0].run(keep);"
    );
    let result = check_program(&[SourceFile::entry("fresh.ts", source)]);
    if rejected {
        let errors = result.expect_err(body);
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(errors[0].code, RuleCode::S100, "{body}");
        assert!(
            errors[0]
                .message
                .contains("`V.run` can move a dynamic array of `V`"),
            "{body}: {errors:?}"
        );
    } else {
        assert!(result.is_ok(), "{body}: {:?}", result.err());
    }
}

#[test]
fn every_value_of_a_binding_must_be_a_literal() {
    for declaration in [
        "const xs: V[] = [];",
        "let xs: V[] = [new V()];",
        "let xs: V[] = []; xs = [new V()];",
        "let xs: V[]; xs = [];",
        "const xs: V[] = [...keep];",
    ] {
        check(&format!("{declaration} xs.push(this);"), false);
    }
    for body in [
        "let xs: V[] = keep; xs.push(this);",
        "let xs: V[]; xs = keep; xs.push(this);",
        "let xs: V[] = []; xs = keep; xs.push(this);",
        "let xs: V[] = []; xs.push(this); xs = keep;",
        "let xs: V[] = []; if (this.a > 0) { xs = keep; } xs.push(this);",
        "let xs: V[] = []; while (this.a < 0) { xs = keep; } xs.push(this);",
        "let xs: V[] = []; for (let i: i32 = 0; i < 1; i++) { xs = keep; } xs.push(this);",
    ] {
        check(body, true);
    }
    check(
        "let xs: V[] = []; if (this.a > 0) { xs = [new V()]; } xs.push(this);",
        false,
    );
    check(
        "let xs: V[] = []; while (this.a < 0) { xs = [new V()]; } xs.push(this);",
        false,
    );
    check(
        "let xs: V[] = []; for (let i: i32 = 0; i < 1; i++) { xs = [new V()]; } xs.push(this);",
        false,
    );
}

#[test]
fn each_move_excludes_a_fresh_operand_only() {
    for operation in [
        "xs.push(this)",
        "xs.unshift(this)",
        "xs.shift()",
        "xs.splice(0, 1)",
        "xs.reverse()",
        "xs.length = 0",
        "xs.copyWithin(0, 1)",
    ] {
        check(
            &format!("const xs: V[] = [new V(), new V()]; {operation};"),
            false,
        );
        check(&format!("const xs: V[] = keep; {operation};"), true);
    }
}

#[test]
fn other_operands_and_other_bodies_stay_moves() {
    check("const xs: V[] = []; xs.push(this);", false);
    for body in [
        "p.push(this);",
        "holder.xs.push(this);",
        "keep.push(this);",
        "make().push(this);",
        "const xs: V[] = keep.slice(0); xs.push(this);",
        "const xs: V[] = keep.concat([]); xs.push(this);",
        "const xs: V[] = []; const alias: V[] = xs; alias.push(this);",
        "const xs: V[] = []; take(xs);",
        "const xs: V[] = []; const f = (): void => { xs.push(new V()); }; f();",
    ] {
        check(body, true);
    }
    check(
        "const f = (): void => { const xs: V[] = []; xs.push(new V()); }; f();",
        false,
    );
    check(
        "const f = (): void => { const xs: V[] = keep; xs.push(new V()); }; f();",
        true,
    );
}

#[test]
fn shadowed_bindings_keep_separate_values() {
    check(
        "const xs: V[] = keep; try { const xs: V[] = []; } finally { xs.push(this); }",
        true,
    );
    check(
        "const xs: V[] = []; try { const xs: V[] = keep; } finally { xs.push(this); }",
        false,
    );
    check(
        "let xs: V[] = []; try { let xs: V[] = []; } finally { xs = keep; } xs.push(this);",
        true,
    );
    check(
        "let xs: V[] = []; try { let xs: V[] = keep; } finally { xs = []; } xs.push(this);",
        false,
    );
    check(
        "const xs: V[] = []; { const xs: V[] = keep; xs.push(this); }",
        true,
    );
    check(
        "const xs: V[] = keep; { const xs: V[] = []; xs.push(this); }",
        false,
    );
    check(
        "let xs: V[] = []; { let xs: V[] = []; xs = keep; } xs.push(this);",
        false,
    );
    check(
        "let xs: V[] = []; { let ys: V[] = []; xs = keep; } xs.push(this);",
        true,
    );
    check(
        "const xs: V[] = keep; for (const xs of [keep]) { xs.push(this); }",
        true,
    );
    check(
        "const xs: V[] = keep; for (let xs: V[] = []; xs.length < 1; xs.push(new V())) {}",
        false,
    );
}

#[test]
fn function_reach_keeps_the_body_of_the_operand() {
    for (initializer, rejected) in [("[]", false), ("keep", true)] {
        let source = format!(
            "@ValueType class V {{ a: i32 = 0; run(): void {{ helper(); }} }}
             const keep: V[] = [];
             function helper(): void {{ const xs: V[] = {initializer}; xs.push(new V()); }}
             const all: V[] = [new V()]; all[0].run();"
        );
        let result = check_program(&[SourceFile::entry("helper.ts", source)]);
        assert_eq!(result.is_err(), rejected, "{initializer}: {result:?}");
    }
}

#[test]
fn a_generator_local_stays_a_move_across_next_calls() {
    let source = "@ValueType
        class V {
          a: i32;
          constructor(a: i32) { this.a = a; }
          m(): i32 { it.next(); this.a = this.a + 40; return this.a; }
        }
        let g: V[] = [];
        function* gen(): Generator<i32> {
          const xs: V[] = [new V(1)];
          g = xs;
          yield 0;
          for (let i: i32 = 0; i < 1000; i++) { xs.push(new V(2)); }
          yield 1;
        }
        const it: Generator<i32> = gen();
        it.next();
        const r: i32 = g[0].m();
        print(`${r} ${g[0].a} ${g.length}`);
        export function main(): void {}";
    let errors = check_program(&[SourceFile::entry("generator.ts", source)])
        .expect_err("a resumed generator can move the receiver array");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert!(
        errors[0]
            .message
            .contains("`V.m` can move a dynamic array of `V` by `push`"),
        "{errors:?}"
    );

    let plain = source
        .replace("function* gen(): Generator<i32>", "function gen(): void")
        .replace("yield 0;", "")
        .replace("yield 1;", "")
        .replace("const it: Generator<i32> = gen();", "")
        .replace("it.next();", "gen();");
    let result = check_program(&[SourceFile::entry("plain.ts", plain)]);
    assert!(result.is_ok(), "{:?}", result.err());
}
