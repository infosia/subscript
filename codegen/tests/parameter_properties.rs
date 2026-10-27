//! compiler.md §198.1 rule 3: the field initializers run first, then
//! each parameter property takes its parameter, then the constructor
//! body runs. `tsc` 5.9.2 (ES2022) output under `node` prints the same
//! two lines.
//! Measured cost: one checker run, one LIR lowering, one interpreter
//! run; no native compile.

use subscript_codegen::{interpreter::interpret, lir::lower_module};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn field_initializer_then_parameter_property_then_body() {
    let source = r#"
let order: string = "";
function mark(step: string, value: i32): i32 {
  order = order + step;
  return value;
}
class Probe {
  first: i32 = mark("field ", 1);
  constructor(public second: i32 = mark("default ", 2)) {
    order = order + `body:${this.first},${this.second}`;
  }
}
export function main(): void {
  new Probe();
  print(order);
  print(`${new Probe(5).second}`);
}
"#;
    let hir = check_program(&[SourceFile::entry("main.ts", source)])
        .unwrap_or_else(|diagnostics| panic!("check: {diagnostics:?}"));
    let lir = lower_module(&hir).unwrap_or_else(|error| panic!("lower: {error:?}"));
    let output = interpret(&lir).unwrap_or_else(|error| panic!("interpret: {error:?}"));
    assert_eq!(
        String::from_utf8(output).expect("UTF-8 output"),
        "field default body:1,2\n5\n"
    );
}
