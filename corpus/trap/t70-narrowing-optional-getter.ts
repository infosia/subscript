// corpus: trap/t70-narrowing-optional-getter
// purpose: A getter ends the shared value before its narrowed read.
// exercises: shared-location-narrowing, getter, null-narrowing
// questions: compiler.md §124, C17
// tier-policy: both tiers trap
// js-comparable: no C17
// expected-trap: null-narrowing at the shared read after the getter
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
const g: Holder = new Holder();
class K { kill(): i32 { g.c = null; return 1; } get acc(): i32 { g.c = null; return 2; } }
class Outer { k: K | null = new K(); }
export function main(): void {
  const o = new Outer();
  if (g.c !== null) {
    const x: i32 = o.k?.acc ?? 0;
    print(`${g.c.v}`);
  }
}
