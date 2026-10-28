// corpus: reject/r449-satisfies-mismatch
// purpose: `x satisfies T` requires the type of x to convert to T, as an assignment does.
// exercises: satisfies
// questions: compiler.md §204.1 rule 3
// tsc: rejects TS1360
// expected-error: S100, type mismatch
export function main(): void {
  const s = "a" satisfies i32;
  print(`${s}`);
}
