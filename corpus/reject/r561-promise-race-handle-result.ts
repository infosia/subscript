// corpus: reject/r561-promise-race-handle-result
// purpose: A race result type that holds a handle is rejected, as for Promise.all.
// exercises: promise-race, held-handle, await
// questions: compiler.md §350 rule 2, §166 rule 12, collisions.md C8
// tsc: accepts
// expected-error: S013, Promise.race cannot store a counted result

async function value(n: i32): Promise<i32> { return n; }
async function wrap(n: i32): Promise<Promise<i32>[]> {
  const h: Promise<i32> = value(n);
  const xs: Promise<i32>[] = [h];
  await h;
  return xs;
}
export async function main(): Promise<void> {
  const jobs: Promise<Promise<i32>[]>[] = [wrap(1), wrap(2)];
  const inner: Promise<i32>[] = await Promise.race(jobs);
  print(`${await inner[0]}`);
}
