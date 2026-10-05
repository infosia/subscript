// corpus: reject/r374-indexed-read-without-null-check
// purpose: An indexed read without a null check needs a local copy.
// exercises: nullable-reference, indexed-read
// questions: compiler.md §163, collisions.md C24 row 32
// tsc: rejects TS2531
// expected-error: S011 at line 11, copy the element to a `const` local and test the local

class A { x: i32 = 1; }
export function main(): void {
  const xs: (A | null)[] = [new A()];
  print(`${xs[0].x}`);
}
