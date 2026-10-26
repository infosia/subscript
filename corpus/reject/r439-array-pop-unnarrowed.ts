// corpus: reject/r439-array-pop-unnarrowed
// purpose: A member read on a pop result needs a test for undefined first.
// exercises: undefined, nullable, Array.pop
// questions: compiler.md §196
// tsc: rejects TS2532
// expected-error: S011, may be null here
class Job {
  id: i32 = 1;
}
export function main(): void {
  const jobs: Job[] = [new Job()];
  print(`${jobs.pop().id}`);
}
