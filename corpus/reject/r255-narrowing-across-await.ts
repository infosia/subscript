// corpus: reject/r255-narrowing-across-await
// purpose: Rejects a shared read after its narrowing ends.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 13
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
async function tick(h: Holder): Promise<void> { h.c = null; }
async function read(h: Holder): Promise<void> {
  if (h.c !== null) {
    await tick(h);
    print(`${h.c.v}`);
  }
}
