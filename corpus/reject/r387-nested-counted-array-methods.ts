// corpus: reject/r387-nested-counted-array-methods
// purpose: Counted elements, callback results, and accumulators reject array methods.
// exercises: counted-array, async-handle, ownership
// questions: compiler.md §171, §70, §116; stdlib.md §9
// tsc: accepts

// expected-error: S014 at find, sort, includes, map, reduce, and Map.groupBy

async function work(): Promise<void> { return; }
export async function main(): Promise<void> {
  const jobs: Promise<void>[][] = [[work()]];
  await jobs[0][0];
  jobs.find((row: Promise<void>[]): boolean => row.length > 0);
  jobs.sort((a: Promise<void>[], b: Promise<void>[]): i32 => a.length - b.length);
  jobs.includes(jobs[0]);
  const grouped: Map<i32, Promise<void>[][]> = Map.groupBy(
    jobs, (row: Promise<void>[]): i32 => row.length);
  const numbers: i32[] = [1];
  const output: Promise<void>[][] = numbers.map(
    (n: i32): Promise<void>[] => jobs[0]);
  const reduced: Promise<void>[] = numbers.reduce(
    (acc: Promise<void>[], n: i32): Promise<void>[] => acc, jobs[0]);
}

// pin: 96f46f35
// pin-dev-jit: Exit 2; counted-store LIR failure, not S014.
// pin-c-aot: Exit 1; counted-store LIR failure, not S014.
// pin-interpreter: Counted-store LIR failure before execution, not S014.
