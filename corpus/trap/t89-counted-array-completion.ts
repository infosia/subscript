// corpus: trap/t89-counted-array-completion
// purpose: Frame free and result exit release the failed completion element.
// exercises: counted-array, async-handle, ownership
// questions: compiler.md §171, §70, §116
// tsc: accepts
// js-comparable: no C8: Async handles have no Promise assimilation.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw in fail

async function fail(): Promise<void> { throw new Error("lost"); }
async function make(): Promise<Promise<void>[]> { return [fail()]; }
async function use(skip: boolean): Promise<void> {
  const jobs: Promise<void>[] = await make();
  if (!skip) { await jobs[0]; }
}
export async function main(): Promise<void> { await use(true); }

// pin: 96f46f35
// pin-dev-jit: Exit 0; no uncaught-exception trap.
// pin-c-aot: Exit 0; no uncaught-exception trap.
// pin-interpreter: Empty stdout; no uncaught-exception trap.
