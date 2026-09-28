// corpus: trap/t69-narrowing-static-accessor-compound
// purpose: A getter ends the shared value before its narrowed read.
// exercises: shared-location-narrowing, getter, null-narrowing
// questions: compiler.md §124, C17
// tier-policy: both tiers trap
// js-comparable: no C17
// expected-trap: null-narrowing at the shared read after the getter
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
const g: Holder = new Holder();
class S { static get acc(): i32 { g.c = null; return 1; } static set acc(v: i32) { print(`${v}`); } }
export function main(): void {
  if (g.c !== null) { S.acc += g.c.v; print("done"); } }
