// corpus: trap/t93-counted-fill-release
// purpose: A counted fill releases an unobserved failed task before the next statement.
// exercises: counted-array, async-handle, ownership
// questions: compiler.md §171, §116
// tsc: accepts
// js-comparable: no C8: Async handles have no Promise assimilation.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw in fail

async function fail(): Promise<void> { throw new Error("lost"); }
async function work(): Promise<void> { return; }
async function use(skip: boolean): Promise<void> {
  const a = work();
  await a;
  const jobs: Promise<void>[] = [fail()];
  if (!skip) { await jobs[0]; }
  jobs.fill(a);
  print("after");
}
export async function main(): Promise<void> { await use(true); }

// pin: 96f46f35
// pin-dev-jit: Prints after; later use-after-delete trap.
// pin-c-aot: Prints after; no uncaught-exception trap.
// pin-interpreter: Prints after; no uncaught-exception trap.
