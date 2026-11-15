// corpus: reject/r514-loop-nullable-field-store
// purpose: Rejects a nullable field store that ends a loop-head non-null fact.
// exercises: loop-head, nullable-store
// questions: compiler.md §304, §162
// tsc: rejects TS18047
// expected-error: S011 at line 14, Storing nullable mk() into h.f ends its loop-head non-null fact.

class A { x: i32 = 1; }
class H { f: A | null = null; }
function mk(): A | null { return new A(); }
export function main(): void {
  const h = new H(); h.f = new A();
  for (let i: i32 = 0; i < 2; i++) {
    h.f.x;
    h.f = mk();
  }
}
