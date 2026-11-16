// corpus: reject/r538-static-readonly-stored-lambda
// purpose: A stored lambda does not keep a static readonly field fact.
// exercises: static-readonly, null-narrowing, lambda
// questions: compiler.md §322 rule 1
// tsc: rejects TS18047
// expected-error: S011 at line 12
class A { x: i32 = 1; }
function mk(): A | null { return new A(); }
class K { static readonly R: A | null = mk(); }
export function main(): void {
  if (K.R !== null) {
    const read = (): i32 => K.R.x;
    print(`${read()}`);
  }
}

// tsc-version: TypeScript 5.9.2; TS18047 at line 12, measured.
