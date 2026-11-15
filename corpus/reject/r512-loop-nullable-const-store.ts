// corpus: reject/r512-loop-nullable-const-store
// purpose: Rejects a nullable const copy that ends a loop-head non-null fact.
// exercises: loop-head, nullable-store
// questions: compiler.md §304, §162
// tsc: rejects TS18047
// expected-error: S011 at line 14, Copying nullable const n into a ends its loop-head non-null fact.

class A { x: i32 = 1; }
class H { f: A | null = null; }
function mk(): A | null { return new A(); }
export function main(): void {
  let a: A | null = new A();
  for (let i: i32 = 0; i < 2; i++) {
    a.x;
    const n = mk(); a = n;
  }
}
