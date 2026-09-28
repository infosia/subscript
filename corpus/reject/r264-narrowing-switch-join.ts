// corpus: reject/r264-narrowing-switch-join
// purpose: A switch exit intersects the facts from every break edge.
// exercises: shared-location-narrowing, switch
// questions: compiler.md §124
// tsc: rejects TS18047
// expected-error: S011 at line 11
class Cell { v: i32 = 7; }
function read(local: Cell | null, k: i32): void {
  switch (k) {
    case 0: if (local === null) { return; } break;
    case 1: print(`${local.v}`); break;
  }
}
export function main(): void { read(null, 1); }
