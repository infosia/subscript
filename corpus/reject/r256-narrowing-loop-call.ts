// corpus: reject/r256-narrowing-loop-call
// purpose: Rejects a shared read after a subtree ends its narrowing.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 15
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
function clear(h: Holder): void { h.c = null; }
export function main(): void {
  const h = new Holder();
  if (h.c !== null) {
    let i: i32 = 0;
    while (i < 2) {
      print(`${h.c.v}`);
      clear(h);
      i++;
    }
  }
}
