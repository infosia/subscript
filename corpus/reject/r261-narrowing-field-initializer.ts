// corpus: reject/r261-narrowing-field-initializer
// purpose: Rejects a shared read after a subtree ends its narrowing.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 15
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
const h: Holder = new Holder();
function clear(): i32 { h.c = null; return 0; }
class Trigger { value: i32 = clear(); }
export function main(): void {
  if (h.c !== null) {
    const trigger = new Trigger();
    print(`${h.c.v}`);
  }
}
