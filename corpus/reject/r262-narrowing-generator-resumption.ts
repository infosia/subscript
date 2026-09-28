// corpus: reject/r262-narrowing-generator-resumption
// purpose: Rejects a shared read after a subtree ends its narrowing.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 15
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
function* values(h: Holder): Generator<i32> { h.c = null; yield 1; }
export function main(): void {
  const h = new Holder();
  const sequence = values(h);
  if (h.c !== null) {
    for (const value of sequence) {
      print(`${h.c.v}`);
    }
  }
}
