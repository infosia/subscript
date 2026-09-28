// corpus: trap/t71-narrowing-destructuring-getter
// purpose: A getter ends the shared value before its narrowed read.
// exercises: shared-location-narrowing, getter, null-narrowing
// questions: compiler.md §124, C17
// tier-policy: both tiers trap
// js-comparable: no C17
// expected-trap: null-narrowing at the shared read after the getter
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
const g: Holder = new Holder();
class K { get acc(): i32 { g.c = null; return 2; } }
export function main(): void {
  const k = new K();
  if (g.c !== null) {
    const { acc } = k;
    print(`${g.c.v} ${acc}`);
  }
}
