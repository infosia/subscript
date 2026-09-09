// corpus: accept/a212-using-while-true
// purpose: Keeps reachable disposal unchanged across literal-true while shapes.
// exercises: using-declaration, scope-exit, infinite-loop
// questions: compiler section 101
// tsc: accepts; js-comparable: yes
class R { [Symbol.dispose](): void { print("dispose"); } }
function returns(): void {
  using r: R = new R();
  while (true) { return; }
}
function trailing(): void {
  using r: R = new R();
  while (true) { return; }
  print("unreachable");
}
function initialized(): void {
  using r: R = new R();
  let i: i32 = 0;
  while (true) { print(`${i}`); return; }
}
// main returns before the infinite loop; emission still covers its body.
function infinite(stop: boolean): void {
  using r: R = new R();
  if (stop) { return; }
  while (true) { print("unreachable"); }
}
// main returns before the infinite loop; emission still covers its body.
function block(stop: boolean): void {
  using r: R = new R();
  if (stop) { return; }
  { while (true) { print("unreachable"); } }
}
// main returns before the infinite loop; emission still covers its body.
function conditional(stop: boolean): void {
  using r: R = new R();
  if (stop) { return; }
  if (true) { while (true) { print("unreachable"); } }
}
export function main(): void {
  returns();
  trailing();
  initialized();
  infinite(true);
  block(true);
  conditional(true);
  print("returned");
}
