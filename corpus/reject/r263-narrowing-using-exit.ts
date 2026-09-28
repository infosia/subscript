// corpus: reject/r263-narrowing-using-exit
// purpose: Rejects a shared read after a subtree ends its narrowing.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 15
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
const h: Holder = new Holder();
class Cleaner { [Symbol.dispose](): void { h.c = null; } }
export function main(): void {
  const cleaner = new Cleaner();
  if (h.c !== null) {
    { using active = cleaner; }
    print(`${h.c.v}`);
  }
}
