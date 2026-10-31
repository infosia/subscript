// corpus: reject/r459-nullable-array-of-handles
// purpose: A nullable array of a counted element has no holder count, so the handles in the array have no owner.
// exercises: nullable-array, async-handle, counted-array
// questions: compiler.md §217, C7
// tsc: accepts
// expected-error: S011, unions are limited to `Ref | null`; `Promise<void>[] | null` is not a reference type union
function take(jobs: Promise<void>[] | null): i32 {
  return jobs === null ? 0 : jobs.length;
}

export function main(): void {
  print(`${take(null)}`);
}
