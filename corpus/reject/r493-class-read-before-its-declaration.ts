// corpus: reject/r493-class-read-before-its-declaration
// purpose: Rejects a value read of a class before its declaration, when the read is evaluated at once.
// exercises: class, static-method, function-value, read-before-declaration
// questions: compiler.md §241.1 rule 1
// tsc: rejects TS2449
// expected-error: S100 at line 9, `B` is used before its declaration

type CB = (x: i32) => void;
const early: CB = B.m;

class B {
  static m(x: i32): void {
    print(`m${x}`);
  }
}

export function main(): void {
  early(1);
}
