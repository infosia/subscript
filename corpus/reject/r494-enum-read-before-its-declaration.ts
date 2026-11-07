// corpus: reject/r494-enum-read-before-its-declaration
// purpose: Rejects a value read of an enum before its declaration, when the read is evaluated at once.
// exercises: enum, read-before-declaration
// questions: compiler.md §241.1 rule 1
// tsc: rejects TS2450
// expected-error: S100 at line 8, `E` is used before its declaration

const e: E = E.A;

enum E {
  A,
  B,
}

export function main(): void {
  print(`${e === E.A}`);
}
