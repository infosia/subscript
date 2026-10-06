// corpus: reject/r378-promise-all-non-handle-array
// purpose: An aggregate requires an array of async handles.
// exercises: promise-all, array, await
// questions: compiler.md §166 rule 12, collisions.md C8
// tsc: accepts
// expected-error: S013, Promise.all requires Promise<T>[]

export async function main(): Promise<void> {
  const values: i32[] = [1, 2];
  await Promise.all(values);
}
