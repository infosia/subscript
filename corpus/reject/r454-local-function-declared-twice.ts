// corpus: reject/r454-local-function-declared-twice
// purpose: A local function name follows the duplicate-declaration rule of its block.
// exercises: local-functions, duplicate-declarations
// questions: compiler.md §207.1 rule 2
// tsc: rejects TS2393
// expected-error: S017, duplicate declaration of `f` in one scope
export function main(): void {
  function f(): i32 {
    return 1;
  }
  function f(): i32 {
    return 2;
  }
  print(`${f()}`);
}
