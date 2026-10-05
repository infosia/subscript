// corpus: accept/a334-exit-predicate
// purpose: Constant branches and grouped cases keep the function exit unreachable.
// exercises: constant-branch, grouped-case, using, try
// questions: compiler.md §164, §101
// tsc: accepts; js-comparable: yes

class R { [Symbol.dispose](): void { print("dispose"); } }
function row_while(x: i32): i32 { while (true) { return x; } }
function row_for_true(x: i32): i32 { for (; true;) { return x; } }
function row_conditional(x: i32): i32 { while (true) { if (x > 0) { return x; } } }
function row_throw(x: i32): i32 { while (true) { throw new Error("e"); } }
function row_switch_loop(x: i32): i32 { while (true) { switch (x) { case 1: break; default: return x; } } }
function row_try(x: i32): i32 { try { while (true) { return x; } } catch (e) { return 0; } }
function row_using(x: i32): i32 { using r: R = new R(); while (true) { return x; } }
function row_for_absent(x: i32): i32 { for (;;) { return x; } }
function row_if_true(x: i32): i32 { if (true) { return x; } }
function row_if_false(x: i32): i32 { if (false) {} else { return x; } }
function row_grouped(x: i32): i32 { switch (x) { case 1: case 2: return 1; default: return 0; } }
function row_else_true(x: i32): i32 { if (x > 0) { return 1; } else if (true) { return 2; } }
export function main(): void {
  print(`${row_while(3)}`);
  print(`${row_for_true(4)}`);
  print(`${row_conditional(5)}`);
  try { row_throw(6); } catch (e) { print("throw"); }
  print(`${row_switch_loop(2)}`);
  print(`${row_try(7)}`);
  print(`${row_using(8)}`);
  print(`${row_for_absent(9)}`);
  print(`${row_if_true(10)}`);
  print(`${row_if_false(11)}`);
  print(`${row_grouped(2)}`);
  print(`${row_else_true(0)}`);
}
