// corpus: reject/r377-promise-all-void-value
// purpose: An aggregate of void handles supplies no array value.
// exercises: promise-all, void-array, await
// questions: compiler.md §166 rule 2, collisions.md C24 row 34
// tsc: accepts
// expected-error: S013, the language has no void[] value

async function quiet(): Promise<void> {}
export async function main(): Promise<void> {
  const jobs: Promise<void>[] = [quiet()];
  const xs = await Promise.all(jobs);
  print(`${xs.length}`);
}
