// corpus: accept/a331-local-definite-assignment
// purpose: Assigns typed locals on every path before each read.
// exercises: let, if, switch, try, while, assignment, compound-assignment
// questions: compiler.md §158, collisions.md C24
// tsc: accepts; js-comparable: yes

class Box { value: i32 = 7; }
function cond(): boolean { return true; }
function choice(): i32 { return 0; }
function integer_branches(): void {
  let x: i32;
  if (cond()) { x = 1; } else { x = 2; }
  print(`${x}`);
}
function integer_throw(): void {
  let x: i32;
  if (cond()) { x = 1; } else { throw new Error("stop"); }
  print(`${x}`);
}
function integer_return(): void {
  let x: i32;
  if (cond()) { x = 1; } else { return; }
  print(`${x}`);
}
function integer_never(): void {
  let x: i32;
  if (cond()) { x = 1; } else { unreachable(); }
  print(`${x}`);
}
function integer_switch(): void {
  let x: i32;
  switch (choice()) { case 0: x = 1; break; default: x = 2; break; }
  print(`${x}`);
}
function integer_catch(): void {
  let x: i32;
  try { x = 1; } catch { x = 2; }
  print(`${x}`);
}
function integer_while(): void {
  let x: i32;
  while (true) { x = 1; break; }
  print(`${x}`);
}
function integer_conditional(): void {
  let x: i32;
  x = cond() ? 1 : 2;
  print(`${x}`);
}
function integer_chain(): void {
  let x: i32;
  let t: i32; x = t = 1; print(`${t}`);
  print(`${x}`);
}
function float_branches(): void {
  let x: f64;
  if (cond()) { x = 1.5; } else { x = 2.5; }
  print(`${x}`);
}
function float_throw(): void {
  let x: f64;
  if (cond()) { x = 1.5; } else { throw new Error("stop"); }
  print(`${x}`);
}
function float_return(): void {
  let x: f64;
  if (cond()) { x = 1.5; } else { return; }
  print(`${x}`);
}
function float_never(): void {
  let x: f64;
  if (cond()) { x = 1.5; } else { unreachable(); }
  print(`${x}`);
}
function float_switch(): void {
  let x: f64;
  switch (choice()) { case 0: x = 1.5; break; default: x = 2.5; break; }
  print(`${x}`);
}
function float_catch(): void {
  let x: f64;
  try { x = 1.5; } catch { x = 2.5; }
  print(`${x}`);
}
function float_while(): void {
  let x: f64;
  while (true) { x = 1.5; break; }
  print(`${x}`);
}
function float_conditional(): void {
  let x: f64;
  x = cond() ? 1.5 : 2.5;
  print(`${x}`);
}
function float_chain(): void {
  let x: f64;
  let t: f64; x = t = 1.5; print(`${t}`);
  print(`${x}`);
}
function string_branches(): void {
  let x: string;
  if (cond()) { x = "a"; } else { x = "b"; }
  print(x);
}
function string_throw(): void {
  let x: string;
  if (cond()) { x = "a"; } else { throw new Error("stop"); }
  print(x);
}
function string_return(): void {
  let x: string;
  if (cond()) { x = "a"; } else { return; }
  print(x);
}
function string_never(): void {
  let x: string;
  if (cond()) { x = "a"; } else { unreachable(); }
  print(x);
}
function string_switch(): void {
  let x: string;
  switch (choice()) { case 0: x = "a"; break; default: x = "b"; break; }
  print(x);
}
function string_catch(): void {
  let x: string;
  try { x = "a"; } catch { x = "b"; }
  print(x);
}
function string_while(): void {
  let x: string;
  while (true) { x = "a"; break; }
  print(x);
}
function string_conditional(): void {
  let x: string;
  x = cond() ? "a" : "b";
  print(x);
}
function string_chain(): void {
  let x: string;
  let t: string; x = t = "a"; print(t);
  print(x);
}
function reference_branches(): void {
  let x: Box;
  if (cond()) { x = new Box(); } else { x = new Box(); }
  print(`${x.value}`);
}
function reference_throw(): void {
  let x: Box;
  if (cond()) { x = new Box(); } else { throw new Error("stop"); }
  print(`${x.value}`);
}
function reference_return(): void {
  let x: Box;
  if (cond()) { x = new Box(); } else { return; }
  print(`${x.value}`);
}
function reference_never(): void {
  let x: Box;
  if (cond()) { x = new Box(); } else { unreachable(); }
  print(`${x.value}`);
}
function reference_switch(): void {
  let x: Box;
  switch (choice()) { case 0: x = new Box(); break; default: x = new Box(); break; }
  print(`${x.value}`);
}
function reference_catch(): void {
  let x: Box;
  try { x = new Box(); } catch { x = new Box(); }
  print(`${x.value}`);
}
function reference_while(): void {
  let x: Box;
  while (true) { x = new Box(); break; }
  print(`${x.value}`);
}
function reference_conditional(): void {
  let x: Box;
  x = cond() ? new Box() : new Box();
  print(`${x.value}`);
}
function reference_chain(): void {
  let x: Box;
  let t: Box; x = t = new Box(); print(`${t.value}`);
  print(`${x.value}`);
}
function nullable_branches(): void {
  let x: Box | null;
  if (cond()) { x = new Box(); } else { x = null; }
  print(`${x === null ? 0 : x.value}`);
}
function nullable_throw(): void {
  let x: Box | null;
  if (cond()) { x = new Box(); } else { throw new Error("stop"); }
  print(`${x === null ? 0 : x.value}`);
}
function nullable_return(): void {
  let x: Box | null;
  if (cond()) { x = new Box(); } else { return; }
  print(`${x === null ? 0 : x.value}`);
}
function nullable_never(): void {
  let x: Box | null;
  if (cond()) { x = new Box(); } else { unreachable(); }
  print(`${x === null ? 0 : x.value}`);
}
function nullable_switch(): void {
  let x: Box | null;
  switch (choice()) { case 0: x = new Box(); break; default: x = null; break; }
  print(`${x === null ? 0 : x.value}`);
}
function nullable_catch(): void {
  let x: Box | null;
  try { x = new Box(); } catch { x = null; }
  print(`${x === null ? 0 : x.value}`);
}
function nullable_while(): void {
  let x: Box | null;
  while (true) { x = new Box(); break; }
  print(`${x === null ? 0 : x.value}`);
}
function nullable_conditional(): void {
  let x: Box | null;
  x = cond() ? new Box() : null;
  print(`${x === null ? 0 : x.value}`);
}
function nullable_chain(): void {
  let x: Box | null;
  let t: Box | null; x = t = new Box(); print(`${t === null ? 0 : t.value}`);
  print(`${x === null ? 0 : x.value}`);
}
function compounds(): void {
  let x: i32;
  x = cond() ? 1 : 2;
  x += 1;
  ++x;
  x--;
  print(`${x}`);
  let s: string;
  let t: string;
  s = t = "a";
  print(`${s} ${t}`);
}
export function main(): void {
  integer_branches();
  integer_throw();
  integer_return();
  integer_never();
  integer_switch();
  integer_catch();
  integer_while();
  integer_conditional();
  integer_chain();
  float_branches();
  float_throw();
  float_return();
  float_never();
  float_switch();
  float_catch();
  float_while();
  float_conditional();
  float_chain();
  string_branches();
  string_throw();
  string_return();
  string_never();
  string_switch();
  string_catch();
  string_while();
  string_conditional();
  string_chain();
  reference_branches();
  reference_throw();
  reference_return();
  reference_never();
  reference_switch();
  reference_catch();
  reference_while();
  reference_conditional();
  reference_chain();
  nullable_branches();
  nullable_throw();
  nullable_return();
  nullable_never();
  nullable_switch();
  nullable_catch();
  nullable_while();
  nullable_conditional();
  nullable_chain();
  compounds();
}
