// corpus: reject/r394-tsc-rejected-reverse-cost
// purpose: Pin the fact split of compiler section 173.
// exercises: rejection-class, divergence-block
// questions: compiler.md §173
// tsc: rejects TS18046, TS2322, TS2448, TS2454
// expected-error: S010: the catch binding `e` is used outside `instanceof` and `throw`

function f(): void {}
export function main(): void {
  try { throw new Error("x"); } catch (e) { print(e.message); }
  const value: i32 = f();
  print(`${x}`); const x: i32 = 1;
}
