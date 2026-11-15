// corpus: reject/r515-let-global-lambda-narrowing
// purpose: Rejects a narrowed mutable global in a lambda.
// exercises: module-global, lambda, null-narrowing
// questions: compiler.md §304, §162, collisions.md C17
// tsc: rejects TS18047
// expected-error: S011 at line 13, The mutable global is not narrowed in the lambda.

class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
let GL: A | null = mk();
export function main(): void {
  if (GL === null) { return; }
  const g = (): i32 => GL.x;
  GL = null;
  print(`${g()}`);
}
