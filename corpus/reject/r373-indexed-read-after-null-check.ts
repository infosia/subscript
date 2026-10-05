// corpus: reject/r373-indexed-read-after-null-check
// purpose: An indexed null check does not narrow the element read.
// exercises: nullable-reference, indexed-read
// questions: compiler.md §163, collisions.md C24 row 32
// tsc: accepts
// expected-error: S011 at line 11, copy the element to a `const` local and test the local

class A { x: i32 = 1; }
export function main(): void {
  const xs: (A | null)[] = [new A()];
  if (xs[0] !== null) { print(`${xs[0].x}`); }
}
