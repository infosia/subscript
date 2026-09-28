// corpus: reject/r257-narrowing-loop-alias-store
// purpose: Rejects a shared read after a subtree ends its narrowing.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 14
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
export function main(): void {
  const h = new Holder();
  const alias = h;
  if (h.c !== null) {
    for (let i: i32 = 0; i < 2; i++) {
      print(`${h.c.v}`);
      alias.c = null;
    }
  }
}
