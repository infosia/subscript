// corpus: reject/r260-narrowing-try-join
// purpose: Rejects a shared read after a subtree ends its narrowing.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 14
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
function clear(h: Holder): void { h.c = null; }
export function main(): void {
  const h = new Holder();
  if (h.c !== null) {
    try { clear(h); } catch { }
    print(`${h.c.v}`);
  }
}
