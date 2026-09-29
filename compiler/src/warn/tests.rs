use super::*;
use crate::{check_program, SourceFile};

fn warnings(source: &str) -> Vec<Warning> {
    let module = check_program(&[SourceFile::new("test.ts", source)])
        .expect("warning fixture must be accepted");
    check_warnings(&module)
}

fn callback_warnings(source: &str) -> Vec<Warning> {
    let mirror = "\
// @subscript-c-header include=\"warning-fixture.h\"
// @subscript-c-callback typedef=\"FixtureCallback\"
type FixtureCallback = (message: string, userdata1: object | null, userdata2: object | null) => void;
declare class FixtureCallbackInfo {
  callback: FixtureCallback;
  userdata1: object | null;
  userdata2: object | null;
  constructor(callback: FixtureCallback, userdata1: object | null, userdata2: object | null);
}
declare function fixtureRegister(info: FixtureCallbackInfo): void;
";
    let module = check_program(&[
        SourceFile::ambient("warning-fixture.generated.d.ts", mirror),
        SourceFile::new("test.ts", source),
    ])
    .expect("callback warning fixture must be accepted");
    check_warnings(&module)
}

#[test]
fn every_warning_code_has_a_single_line_explanation() {
    for code in WarnCode::ALL {
        assert!(!code.explanation().is_empty(), "{code}");
        assert!(!code.explanation().contains('\n'), "{code}");
    }
}

#[test]
fn warning_constructor_preserves_fields() {
    let warning = Warning::new(WarnCode::W001, "message", Pos::new("test.ts", 2, 3));
    assert_eq!(warning.code.as_str(), "W001");
    assert_eq!(warning.message, "message");
    assert_eq!(warning.pos, Pos::new("test.ts", 2, 3));
}

#[test]
fn loop_allocation_warns_but_one_shot_and_collect_do_not() {
    let source = "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
                  export function main(): void {\n\
                  \x20 for (let i: i32 = 0; i < 2; i += 1) {\n\
                  \x20   const token: Token = new Token(i);\n\
                  \x20   print(`${token.value}`);\n\
                  \x20 }\n\
                  }\n";
    let result = warnings(source);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].code, WarnCode::W001);
    assert_eq!(result[0].pos.line, 4);

    let one_shot = "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
         \x20 const token: Token = new Token(1);\n\
         \x20 print(`${token.value}`);\n\
         }\n";
    assert!(warnings(one_shot).is_empty());

    let with_collect = source.replace(
        "  print(`${token.value}`);\n",
        "  print(`${token.value}`);\n  Context.collect();\n",
    );
    assert!(warnings(&with_collect).is_empty());
}

#[test]
fn use_after_free_warns_until_reassignment() {
    let source = "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
                  export function main(): void {\n\
                  \x20 let token: Token = new Token(1);\n\
                  \x20 Context.free(token);\n\
                  \x20 print(`${token.value}`);\n\
                  \x20 token = new Token(2);\n\
                  \x20 print(`${token.value}`);\n\
                  }\n";
    let result = warnings(source);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].code, WarnCode::W002);
    assert_eq!(result[0].pos.line, 5);
}

#[test]
fn use_after_free_state_does_not_cross_a_branch_join() {
    let source = "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
         \x20 let token: Token = new Token(1);\n\
         \x20 Context.free(token);\n\
         \x20 if (token.value === 1) {\n\
         \x20   token = new Token(2);\n\
         \x20 }\n\
         \x20 print(`${token.value}`);\n\
         }\n";
    let result = warnings(source);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].code, WarnCode::W002);
    assert_eq!(result[0].pos.line, 5);
}

#[test]
fn fresh_callback_userdata_warns_for_local_and_direct_new_inside_loop_only() {
    let source =
        "class Token { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
         \x20 for (let i: i32 = 0; i < 2; i += 1) {\n\
         \x20   const local: Token = new Token(i);\n\
         \x20   fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, local, null));\n\
         \x20   fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, new Token(i), null));\n\
         \x20 }\n\
         \x20 const outside: Token = new Token(3);\n\
         \x20 fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, outside, null));\n\
         }\n";
    let result = callback_warnings(source);
    assert_eq!(
        result
            .iter()
            .filter(|warning| warning.code == WarnCode::W003)
            .count(),
        2
    );
    assert!(
        result
            .iter()
            .all(|warning| warning.code == WarnCode::W003),
        "fresh userdata escapes through the callback aggregate, so W001 must stay muted: {result:?}"
    );
    assert_eq!(
        result
            .iter()
            .map(|warning| warning.pos.line)
            .collect::<Vec<_>>(),
        [5, 6]
    );
}

#[test]
fn conditional_fresh_callback_userdata_does_not_warn() {
    let source =
        "class LogSink { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
         \x20 const keep: LogSink = new LogSink(0);\n\
         \x20 for (let i: i32 = 0; i < 2; i += 1) {\n\
         \x20   fixtureRegister(new FixtureCallbackInfo((message, userdata1, userdata2) => {}, i > 1 ? new LogSink(i) : keep, null));\n\
         \x20 }\n\
         }\n";
    assert!(callback_warnings(source)
        .iter()
        .all(|warning| warning.code != WarnCode::W003));
}

const W004_TYPES: &str = "\
@CStruct
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
  read(): f32 { return this.x; }
}
class State {
  point: Point;
  constructor(point: Point) { this.point = point; }
}
";

fn w004_warnings(body: &str) -> Vec<Warning> {
    warnings(&format!("{W004_TYPES}\n{body}\n"))
}

fn only_w004(result: &[Warning]) -> Vec<&Warning> {
    result
        .iter()
        .filter(|warning| warning.code == WarnCode::W004)
        .collect()
}

#[test]
fn write_only_value_parameter_warns() {
    let result = w004_warnings(
        "function mutate(point: Point): void { point.x = 2.0; }\n\
         export function main(): void { mutate(new Point(1.0)); }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("parameter copy"));
}

#[test]
fn write_only_local_copied_from_field_warns() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const state: State = new State(new Point(1.0));\n\
           const alias: Point = state.point;\n\
           alias.x = 2.0;\n\
           Context.free(state);\n\
         }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`state.point`"));
}

#[test]
fn write_only_local_copied_from_local_warns() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const original: Point = new Point(1.0);\n\
           const alias: Point = original;\n\
           alias.x = 2.0;\n\
         }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`original`"));
}

#[test]
fn write_only_local_copied_from_index_warns() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const points: Point[] = [new Point(1.0)];\n\
           const alias: Point = points[0];\n\
           alias.x = 2.0;\n\
         }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`points[…]`"));
}

#[test]
fn parameter_shadowed_by_nested_copy_binding_does_not_warn() {
    let result = w004_warnings(
        "let source: Point = new Point(1.0);\n\
         function mutate(point: Point): void {\n\
           {\n\
             const point: Point = source;\n\
             point.x = 2.0;\n\
           }\n\
         }\n\
         export function main(): void { mutate(source); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn same_name_copy_bindings_in_sibling_blocks_do_not_warn() {
    let result = w004_warnings(
        "let source: Point = new Point(1.0);\n\
         export function main(): void {\n\
           { const copy: Point = source; copy.x = 2.0; }\n\
           { const copy: Point = source; copy.x = 3.0; }\n\
         }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn single_unshadowed_copy_binding_still_warns() {
    let result = w004_warnings(
        "let source: Point = new Point(1.0);\n\
         export function main(): void {\n\
           { const copy: Point = source; copy.x = 2.0; }\n\
         }",
    );
    assert_eq!(only_w004(&result).len(), 1, "{result:?}");
}

#[test]
fn write_only_compound_assignment_warns() {
    let result = w004_warnings(
        "function mutate(point: Point): void { point.x += 1.0; }\n\
         export function main(): void { mutate(new Point(1.0)); }",
    );
    assert_eq!(only_w004(&result).len(), 1, "{result:?}");
}

#[test]
fn write_only_for_step_assignment_warns() {
    let result = w004_warnings(
        "function mutate(copy: Point): void {\n\
           for (; false; copy.x += 1.0) {}\n\
         }\n\
         export function main(): void { mutate(new Point(1.0)); }",
    );
    assert_eq!(only_w004(&result).len(), 1, "{result:?}");
}

#[test]
fn read_in_for_body_mutes_for_step_assignment() {
    let result = w004_warnings(
        "function mutate(copy: Point): void {\n\
           for (; false; copy.x += 1.0) { print(`${copy.x}`); }\n\
         }\n\
         export function main(): void { mutate(new Point(1.0)); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn write_only_fixed_array_parameter_warns() {
    let result = w004_warnings(
        "function mutate(value: FixedArray<i32, 3>): void { value[0] = 777; }\n\
         export function main(): void {\n\
           const value: FixedArray<i32, 3> = [1, 2, 3];\n\
           mutate(value);\n\
         }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`value`"));
    assert!(warnings[0].message.contains("parameter copy"));
}

#[test]
fn write_only_value_parameter_in_lambda_warns() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const points: Map<i32, Point> = new Map<i32, Point>();\n\
           points.set(1, new Point(1.0));\n\
           points.forEach((point: Point, key: i32): void => {\n\
             point.x = 2.0;\n\
           });\n\
         }",
    );
    assert_eq!(only_w004(&result).len(), 1, "{result:?}");
}

#[test]
fn lambda_capture_mutes_outer_copy_binding() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const state: State = new State(new Point(1.0));\n\
           const copy: Point = state.point;\n\
           copy.x = 2.0;\n\
           const values: i32[] = [1];\n\
           values.forEach((value: i32): void => { print(`${copy.x}:${value}`); });\n\
           Context.free(state);\n\
         }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn write_only_value_for_of_binding_warns() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const points: Point[] = [new Point(1.0)];\n\
           for (const point of points) { point.x = 2.0; }\n\
         }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`points`"), "{result:?}");
}

#[test]
fn read_after_write_mutes_value_for_of_binding() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const points: Point[] = [new Point(1.0)];\n\
           for (const point of points) {\n\
             point.x = 2.0;\n\
             print(`${point.x}`);\n\
           }\n\
         }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn value_for_of_call_subject_renders_the_callee() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const scores: Map<i32, Point> = new Map<i32, Point>();\n\
           scores.set(1, new Point(1.0));\n\
           for (const value of scores.values()) { value.x = 2.0; }\n\
         }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(
        warnings[0].message.contains("`scores.values(…)`"),
        "{result:?}"
    );
}

/// A checker-synthesized local never reaches a W004 message: the
/// `for…of` subject storage and every pattern storage form resolve
/// to the user's expression. Each row names the binding and the
/// origin text the message carries.
#[test]
fn synthesized_storage_never_reaches_a_w004_message() {
    let forms = [
        (
            "fixed-array for…of subject",
            "export function main(): void {\n\
               const points: FixedArray<Point, 1> = [new Point(1.0)];\n\
               for (const point of points) { point.x = 2.0; }\n\
             }",
            "`point` is copied from `points`",
        ),
        (
            "pattern source, field",
            "export function main(): void {\n\
               const state: State = new State(new Point(1.0));\n\
               const { point } = state;\n\
               point.x = 2.0;\n\
             }",
            "`point` is copied from `state.point`",
        ),
        (
            "pattern source, index",
            "export function main(): void {\n\
               const points: Point[] = [new Point(1.0)];\n\
               const [first] = points;\n\
               first.x = 2.0;\n\
             }",
            "`first` is copied from `points[…]`",
        ),
        (
            "pattern source, call",
            "function make(): State { return new State(new Point(1.0)); }\n\
             export function main(): void {\n\
               const { point } = make();\n\
               point.x = 2.0;\n\
             }",
            "`point` is copied from `make(…).point`",
        ),
        (
            "pattern parameter, free function",
            "function move({ point }: State): void { point.x = 2.0; }\n\
             export function main(): void { move(new State(new Point(1.0))); }",
            "`point` is a value-type parameter copy",
        ),
        (
            "pattern parameter, lambda",
            "export function main(): void {\n\
               const states: State[] = [new State(new Point(1.0))];\n\
               states.forEach(({ point }: State): void => { point.x = 2.0; });\n\
             }",
            "`point` is a value-type parameter copy",
        ),
        (
            "pattern element, array field",
            "export function main(): void {\n\
               const states: State[] = [new State(new Point(1.0))];\n\
               for (const { point } of states) { point.x = 2.0; }\n\
             }",
            "`point` is copied from `states[…].point`",
        ),
        (
            "pattern element, array index",
            "export function main(): void {\n\
               const pointss: Point[][] = [[new Point(1.0)]];\n\
               for (const [first] of pointss) { first.x = 2.0; }\n\
             }",
            "`first` is copied from `pointss[…][…]`",
        ),
        (
            "pattern element, map values",
            "export function main(): void {\n\
               const states: Map<i32, State> = new Map<i32, State>();\n\
               states.set(1, new State(new Point(1.0)));\n\
               for (const { point } of states.values()) { point.x = 2.0; }\n\
             }",
            "`point` is copied from `states.values(…).point`",
        ),
    ];
    let mut violations = Vec::new();
    for (form, source, origin) in forms {
        let result = w004_warnings(source);
        let warnings = only_w004(&result);
        let message = match warnings.as_slice() {
            [warning] => warning.message.as_str(),
            _ => {
                violations.push(format!("{form}: {} W004 — {result:?}", warnings.len()));
                continue;
            }
        };
        if message.contains("[[") || !message.contains(origin) {
            violations.push(format!("{form}: printed `{message}`; wants `{origin}`"));
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn local_copied_from_field_chain_rooted_in_index_warns() {
    let result = w004_warnings(
        "@CStruct\n\
         class Outer {\n\
           inner: Point;\n\
           constructor(inner: Point) { this.inner = inner; }\n\
         }\n\
         export function main(): void {\n\
           const values: Outer[] = [new Outer(new Point(1.0))];\n\
           const copy: Point = values[0].inner;\n\
           copy.x = 2.0;\n\
         }",
    );
    let warnings = only_w004(&result);
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`values[…].inner`"));
}

#[test]
fn field_read_after_write_mutes_w004() {
    let result = w004_warnings(
        "function mutate(point: Point): f32 { point.x = 2.0; return point.x; }\n\
         export function main(): void { print(`${mutate(new Point(1.0))}`); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn two_unread_field_writes_produce_two_warnings() {
    let body = "function mutate(point: Point): void {\n\
                  point.x = 2.0;\n\
                  point.x = 3.0;\n\
                }\n\
                export function main(): void { mutate(new Point(1.0)); }";
    let result = w004_warnings(body);
    let warnings = only_w004(&result);
    let first_line =
        u32::try_from(W004_TYPES.lines().count()).expect("test fixture line count fits u32") + 3;
    assert_eq!(warnings.len(), 2, "{result:?}");
    assert_eq!(
        warnings
            .iter()
            .map(|warning| warning.pos.line)
            .collect::<Vec<_>>(),
        [first_line, first_line + 1]
    );
}

#[test]
fn fixed_array_local_index_read_after_write_mutes_w004() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const original: FixedArray<i32, 3> = [1, 2, 3];\n\
           const copy: FixedArray<i32, 3> = original;\n\
           copy[0] = 777;\n\
           print(`${copy[0]}`);\n\
         }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn assignment_in_value_position_mutes_w004() {
    let result = w004_warnings(
        "function mutate(point: Point): void {\n\
           const value: f32 = (point.x = 2.0);\n\
           print(`${value}`);\n\
         }\n\
         export function main(): void { mutate(new Point(1.0)); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn method_call_on_binding_mutes_w004() {
    let result = w004_warnings(
        "function mutate(point: Point): f32 { point.x = 2.0; return point.read(); }\n\
         export function main(): void { print(`${mutate(new Point(1.0))}`); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn passing_binding_as_argument_mutes_w004() {
    let result = w004_warnings(
        "function consume(point: Point): void { print(`${point.x}`); }\n\
         function mutate(point: Point): void { point.x = 2.0; consume(point); }\n\
         export function main(): void { mutate(new Point(1.0)); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn returning_binding_mutes_w004() {
    let result = w004_warnings(
        "function mutate(point: Point): Point { point.x = 2.0; return point; }\n\
         export function main(): void { const result: Point = mutate(new Point(1.0)); print(`${result.x}`); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn using_binding_as_assignment_value_mutes_w004() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const state: State = new State(new Point(1.0));\n\
           const alias: Point = state.point;\n\
           alias.x = 2.0;\n\
           state.point = alias;\n\
           Context.free(state);\n\
         }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn read_before_write_inside_loop_mutes_w004() {
    let result = w004_warnings(
        "function mutate(point: Point): void {\n\
           for (let i: i32 = 0; i < 2; i += 1) {\n\
             print(`${point.x}`);\n\
             point.x = i as f32;\n\
           }\n\
         }\n\
         export function main(): void { mutate(new Point(1.0)); }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn new_initializer_is_not_a_copy_binding() {
    let result = w004_warnings(
        "export function main(): void {\n\
           const point: Point = new Point(1.0);\n\
           point.x = 2.0;\n\
         }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn call_initializer_is_not_a_copy_binding() {
    let result = w004_warnings(
        "function make(): Point { return new Point(1.0); }\n\
         export function main(): void {\n\
           const point: Point = make();\n\
           point.x = 2.0;\n\
         }",
    );
    assert!(only_w004(&result).is_empty(), "{result:?}");
}

#[test]
fn value_class_this_write_does_not_warn() {
    let source = "\
@CStruct
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
  set(other: Point): void {
this.x = 2.0;
other.x = 3.0;
  }
}
export function main(): void { const point: Point = new Point(1.0); point.set(new Point(2.0)); }
";
    let result = warnings(source);
    let warnings = only_w004(&result);
    let other_write_line = u32::try_from(
        source
            .lines()
            .position(|line| line.contains("other.x"))
            .expect("test source contains the parameter write")
            + 1,
    )
    .expect("test source line fits u32");
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`other`"), "{result:?}");
    assert_eq!(warnings[0].pos.line, other_write_line);
}

#[test]
fn write_only_value_parameter_in_constructor_warns() {
    let source = "\
@CStruct
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
}
@CStruct
class Holder {
  point: Point;
  constructor(other: Point) {
this.point = new Point(0.0);
other.x = 3.0;
  }
}
export function main(): void { const holder: Holder = new Holder(new Point(1.0)); }
";
    let result = warnings(source);
    let warnings = only_w004(&result);
    let other_write_line = u32::try_from(
        source
            .lines()
            .position(|line| line.contains("other.x"))
            .expect("test source contains the constructor parameter write")
            + 1,
    )
    .expect("test source line fits u32");
    assert_eq!(warnings.len(), 1, "{result:?}");
    assert!(warnings[0].message.contains("`other`"), "{result:?}");
    assert_eq!(warnings[0].pos.line, other_write_line);
}

#[test]
fn reference_class_parameter_does_not_warn() {
    let source = "\
class Point {
  x: f32;
  constructor(x: f32) { this.x = x; }
}
function mutate(point: Point): void { point.x = 2.0; }
export function main(): void { const point: Point = new Point(1.0); mutate(point); Context.free(point); }
";
    let result = warnings(source);
    assert!(only_w004(&result).is_empty(), "{result:?}");
}
