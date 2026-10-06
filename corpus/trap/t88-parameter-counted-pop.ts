// corpus: trap/t88-parameter-counted-pop
// purpose: A parameter alias does not retain a removed element.
// exercises: counted-array, async-handle, ownership
// questions: compiler.md §171, §70, §116
// tsc: accepts
// js-comparable: no C8: Async handles have no Promise assimilation.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw in fail

async function fail(): Promise<void> { throw new Error("lost"); }
function remove(jobs: Promise<void>[]): Promise<void> {
  const job: Promise<void> = jobs[0];
  jobs.pop();
  return job;
}
async function use(skip: boolean): Promise<void> {
  const jobs: Promise<void>[] = [fail()];
  const removed: Promise<void> = remove(jobs);
  if (!skip) { await removed; }
}
export async function main(): Promise<void> { await use(true); }

// pin: 96f46f35
// pin-dev-jit: Exit 0; no uncaught-exception trap.
// pin-c-aot: Exit 0; no uncaught-exception trap.
// pin-interpreter: Empty stdout; no uncaught-exception trap.
