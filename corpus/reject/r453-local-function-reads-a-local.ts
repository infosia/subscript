// corpus: reject/r453-local-function-reads-a-local
// purpose: A local function that reads a local of its enclosing function is not a module function, so it is rejected; a const lambda is the spelling.
// exercises: local-functions, capture
// questions: compiler.md §207.1 rule 4
// tsc: accepts
// expected-error: S100, local function `addK` reads the local `k` of its enclosing function
export function main(): void {
  const k: i32 = 10;
  function addK(x: i32): i32 {
    return x + k;
  }
  print(`${addK(1)}`);
}
