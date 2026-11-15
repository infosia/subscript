// corpus: reject/r513-loop-unstable-local-store
// purpose: Rejects an unstable chain of local stores at the loop head.
// exercises: loop-head, nullable-store
// questions: compiler.md §304, §162
// tsc: rejects TS18047
// expected-error: S011 at line 14, The a = b dependency cannot preserve a when b receives a nullable value.

class A { x: i32 = 1; }
class H { f: A | null = null; }
function mk(): A | null { return new A(); }
export function main(): void {
  let a: A | null = new A(); let b: A | null = new A();
  for (let i: i32 = 0; i < 2; i++) {
    a.x;
    a = b; b = mk();
  }
}
