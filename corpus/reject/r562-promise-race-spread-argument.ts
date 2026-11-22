// corpus: reject/r562-promise-race-spread-argument
// purpose: A race takes one Promise<T>[] argument; a spread argument is rejected, as for Promise.all.
// exercises: promise-race, spread, await
// questions: compiler.md §350 rule 1, §166, collisions.md C8
// tsc: accepts
// expected-error: S013, Promise.race takes one Promise<T>[] argument, not a spread argument

async function value(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> {
  const jobs: Promise<i32>[] = [value(1), value(2)];
  print(`${await Promise.race(...[jobs])}`);
  for (const job of jobs) { print(`${await job}`); }
}
