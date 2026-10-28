//! `do…while` lowering and tier agreement (compiler.md §203).
//!
//! Measured cost: 0.56 s for the three tests on arm64 debug. The
//! lowering cases check and lower six small functions with no native
//! step. The tier case runs one program under the interpreter, the dev
//! JIT, and one C compilation; the C step is most of the cost.

use subscript_codegen::lir::lower_module;
use subscript_compiler::lir::{BlockId, Function, Terminator};
use subscript_compiler::{check_program, SourceFile};

fn probe(source: &str) -> Function {
    let module = check_program(&[SourceFile::new(
        "do-while.ts",
        format!("function probe(x: i32): i32 {{ {source} }}"),
    )])
    .unwrap_or_else(|diagnostics| panic!("{source}: {diagnostics:?}"));
    let lir = lower_module(&module).expect("the program lowers");
    lir.functions
        .into_iter()
        .find(|function| function.source_name == "probe")
        .expect("probe")
}

fn named(function: &Function, name: &str) -> BlockId {
    function
        .blocks
        .iter()
        .find(|block| block.source_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("block {name}"))
        .id
}

fn terminator(function: &Function, id: BlockId) -> &Terminator {
    &function
        .blocks
        .iter()
        .find(|block| block.id == id)
        .expect("block")
        .terminator
}

/// The block that `continue` ends: the `if.then` block of the loop.
fn continue_successors(function: &Function) -> Vec<BlockId> {
    terminator(function, named(function, "if.then")).successors()
}

#[test]
fn the_body_block_comes_first_and_continue_branches_to_the_test() {
    let loop_body = "x += 1; if (x === 2) { continue; } x += 10;";
    let lowered = probe(&format!("do {{ {loop_body} }} while (x < 30); return x;"));
    let body = named(&lowered, "do.body");
    let test = named(&lowered, "do.cond");
    let exit = named(&lowered, "do.exit");
    let entry = lowered.blocks[0].id;
    assert_eq!(terminator(&lowered, entry).successors(), vec![body]);
    assert_eq!(continue_successors(&lowered), vec![test]);
    let Terminator::ConditionalBranch {
        then_target,
        else_target,
        ..
    } = terminator(&lowered, test)
    else {
        panic!("the test branches on its condition");
    };
    assert_eq!((then_target.block, else_target.block), (body, exit));

    // Control: a `while` loop enters its test first, and `continue`
    // branches to that test.
    let control = probe(&format!("while (x < 30) {{ {loop_body} }} return x;"));
    let header = named(&control, "while.cond");
    let entry = control.blocks[0].id;
    assert_eq!(terminator(&control, entry).successors(), vec![header]);
    assert_eq!(continue_successors(&control), vec![header]);
}

#[test]
fn literal_tests_have_one_edge_and_a_true_test_without_break_has_no_exit() {
    let once = probe("do { x += 1; } while (false); return x;");
    let test = named(&once, "do.cond");
    assert_eq!(
        terminator(&once, test).successors(),
        vec![named(&once, "do.exit")]
    );

    let forever = probe("do { x += 1; if (x > 3) { return x; } } while (true);");
    let test = named(&forever, "do.cond");
    assert_eq!(
        terminator(&forever, test).successors(),
        vec![named(&forever, "do.body")]
    );
    let exit = named(&forever, "do.exit");
    assert!(forever
        .blocks
        .iter()
        .all(|block| !block.terminator.successors().contains(&exit)));

    // A body that always returns leaves the test with no predecessor.
    let returns = probe("do { return x; } while (x > 0);");
    let test = named(&returns, "do.cond");
    assert!(returns
        .blocks
        .iter()
        .all(|block| !block.terminator.successors().contains(&test)));
}

#[test]
fn do_while_forms_agree_on_all_tiers() {
    // One C compilation covers a function, a lambda, a generator, an async
    // function with an await in the body and in the test, and scope exits.
    let source = r#"
class R {
  [Symbol.dispose](): void {
    print("dispose");
  }
}

async function step(i: i32): Promise<i32> {
  return i + 1;
}

async function counted(): Promise<i32> {
  let i: i32 = 0;
  do {
    i = await step(i);
  } while ((await step(i)) < 4);
  return i;
}

function first(c: boolean): i32 {
  do {
    return 1;
  } while (c);
}

function forever(): i32 {
  let n: i32 = 0;
  do {
    n += 1;
    if (n > 3) {
      return n;
    }
  } while (true);
}

function scoped(): void {
  let k: i32 = 0;
  do {
    using r = new R();
    k += 1;
    if (k === 1) {
      continue;
    }
    try {
      if (k === 3) {
        break;
      }
    } finally {
      print(`finally ${k}`);
    }
  } while (k < 5);
  print(`k ${k}`);
}

function* countdown(from: i32): Generator<i32> {
  let n = from;
  do {
    yield n;
    n -= 1;
  } while (n > 0);
}

export async function main(): Promise<void> {
  print(`${await counted()}`);
  print(`${first(false)} ${forever()}`);
  scoped();
  const twice = (x: i32): i32 => {
    let y = x;
    do {
      y *= 2;
    } while (y < 100);
    return y;
  };
  print(`${twice(3)}`);
  const seen: i32[] = [];
  for (const v of countdown(2)) {
    seen.push(v);
  }
  print(seen.join(","));
}
"#;
    let files = [SourceFile::entry("do-while-tiers.ts", source)];
    let expected =
        b"3\n1 4\ndispose\nfinally 2\ndispose\nfinally 3\ndispose\nk 3\n192\n2,1\n".to_vec();
    let module = check_program(&files).unwrap();
    let lir = lower_module(&module).unwrap();
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).unwrap(),
        expected
    );
    assert_eq!(subscript_codegen::run_jit(&files).unwrap(), expected);
    assert_eq!(subscript_codegen::run_c_aot(&files).unwrap(), expected);
}
