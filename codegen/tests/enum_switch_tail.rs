//! A non-member enum value at a tail switch traps; at a switch that is
//! not in tail position it reaches the next statement (§200.1 rules 2
//! and 2a).

use subscript_codegen::interpreter::{interpret, InterpretError};
use subscript_codegen::lir::lower_module;
use subscript_compiler::{check_program, SourceFile};

const SOURCE: &str = r#"enum Shape {
  Circle,
  Square,
}

@ValueType
class Wire {
  shape: Shape = Shape.Circle;
}

function later(s: Shape): i32 {
  switch (s) {
    case Shape.Circle:
      return 1;
    case Shape.Square:
      return 2;
  }
  return 3;
}

function branch(s: Shape, flag: boolean): i32 {
  if (flag) {
    switch (s) {
      case Shape.Circle:
        return 1;
      case Shape.Square:
        return 2;
    }
  } else {
    return 0;
  }
}

export function main(): void {
  const bytes: u8[] = [9, 0, 0, 0];
  const w = Context.fromBytes<Wire>(bytes, 0);
  print(`${later(w.shape)} ${branch(w.shape, false)}`);
  print(`${branch(w.shape, true)}`);
}
"#;

#[test]
fn a_non_member_value_traps_only_at_a_tail_switch() {
    let files = [SourceFile::new("tail.ts", SOURCE)];
    let hir = check_program(&files).expect("accepted tail switch fixture");
    let lir = lower_module(&hir).expect("lowered tail switch fixture");
    let error = interpret(&lir).expect_err("the tail switch traps");
    assert_eq!(error.output(), b"3 0\n");
    let InterpretError::Execution { source, .. } = error else {
        panic!("the interpreter keeps the output before the trap");
    };
    let InterpretError::Trap { kind, pos, .. } = *source else {
        panic!("the interpreter reports a semantic trap");
    };
    assert_eq!(kind, "unreachable-reached");
    assert_eq!((pos.line, pos.col), (23, 5));
}
