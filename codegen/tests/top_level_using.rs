//! Top-level block disposal under compiler.md §139.
//! Cost: warm debug execution 0.40 s; two ship-C program compiles.

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn all_engines(source: &str, expected: &str) {
    let files = [SourceFile::entry("main.ts", source)];
    let hir = check_program(&files).expect("accepted top-level block");
    let lir = lower_module(&hir).expect("block LIR");
    assert_eq!(interpret(&lir).expect("interpreter"), expected.as_bytes());
    assert_eq!(run_jit(&files).expect("JIT"), expected.as_bytes());
    assert_eq!(run_c_aot(&files).expect("ship C"), expected.as_bytes());
}

#[test]
fn top_level_blocks_dispose_at_natural_loop_and_exception_exits() {
    all_engines(
        r#"
class R {
  name: string;
  constructor(name: string) { this.name = name; }
  [Symbol.dispose](): void { print(`d ${this.name}`); }
}
{
  using a: R = new R("a"), b: R | null = null, c: R = new R("c");
  print("natural");
}
const resources: R[] = [new R("continue"), new R("break")];
for (let i: i32 = 0; i < 2; i++) {
  using r: R = resources[i];
  if (i === 0) { print("continue"); continue; }
  print("break"); break;
}
try {
  using outer: R = new R("outer");
  {
    using inner: R = new R("inner");
    print("throw");
    throw new Error("stop");
  }
} catch (e) { print("caught"); }
export function main(): void {}
"#,
        "natural\nd c\nd a\ncontinue\nd continue\nbreak\nd break\nthrow\nd inner\nd outer\ncaught\n",
    );
}

#[test]
fn top_level_dispose_route_runs_after_the_global_declaration() {
    all_engines(
        "class Foo { v: i32 = 7; } class R { [Symbol.dispose](): void { print(`${later.v}`); } } const later: Foo = new Foo(); { using r: R = new R(); print('body'); } export function main(): void {}",
        "body\n7\n",
    );
}
