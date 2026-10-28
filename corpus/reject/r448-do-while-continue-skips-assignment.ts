// corpus: reject/r448-do-while-continue-skips-assignment
// purpose: A continue goes to the test, so it skips the assignment that follows it in the body.
// exercises: do-while, definite-assignment
// questions: compiler.md §203.1 rule 3
// tsc: rejects (TS2454)
// expected-error: S100, local `n` is read before assignment
export function main(): void {
  const skip = true;
  let n: i32;
  do {
    if (skip) {
      continue;
    }
    n = 1;
  } while (false);
  print(`${n}`);
}
