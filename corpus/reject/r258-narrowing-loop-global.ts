// corpus: reject/r258-narrowing-loop-global
// purpose: Rejects a shared read after a subtree ends its narrowing.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 15
class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
let current: Cell | null = new Cell();
function clear(): void { current = null; }
export function main(): void {
  if (current !== null) {
    let i: i32 = 0;
    while (i < 2) {
      print(`${current.v}`);
      clear();
      i++;
    }
  }
}
