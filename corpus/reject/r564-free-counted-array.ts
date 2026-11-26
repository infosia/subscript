// corpus: reject/r564-free-counted-array
// purpose: Context.free of an array of a counted element type is rejected; the last holder releases the array.
// exercises: Context.free, dynamic-array, counted-array, async-handle
// questions: Q6, compiler.md §358 rule 5, §171
// tsc: accepts
// expected-error: S014, `Context.free` cannot release an array of the counted type `Promise<void>`

async function job(): Promise<void> {}
export async function main(): Promise<void> {
  const jobs: Promise<void>[] = [job()];
  Context.free(jobs);
}
