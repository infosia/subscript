// corpus: trap/t68-narrowing-accessor-compound
// purpose: A getter ends the shared value before its narrowed read.
// exercises: shared-location-narrowing, getter, null-narrowing
// questions: compiler.md §124, C17
// tier-policy: both tiers trap
// js-comparable: no C17
// expected-trap: null-narrowing at the shared read after the getter
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
class Box { h: Holder; constructor(h: Holder) { this.h = h; }
  get acc(): i32 { this.h.c = null; return 1; } set acc(v: i32) { print(`${v}`); } }
export function main(): void { const h = new Holder(); const b = new Box(h);
  if (h.c !== null) { b.acc += h.c.v; print("done"); } }
