// corpus: reject/r254-global-narrowing-across-call
// purpose: Rejects a shared read after its narrowing ends.
// exercises: shared-location-narrowing
// questions: compiler.md §124, C17
// tsc: accepts
// expected-error: S011 at line 13
class Cell { v: i32 = 7; }
let shared: Cell | null = new Cell();
function clear(): void { shared = null; }
function read(): void {
  if (shared !== null) {
    clear();
    print(`${shared.v}`);
  }
}
