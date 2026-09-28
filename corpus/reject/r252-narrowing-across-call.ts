// corpus: reject/r252-narrowing-across-call
// purpose: Rejects a shared read after its narrowing ends.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 13
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
function clear(h: Holder): void { h.c = null; }
function read(h: Holder): void {
  if (h.c !== null) {
    clear(h);
    print(`${h.c.v}`);
  }
}
