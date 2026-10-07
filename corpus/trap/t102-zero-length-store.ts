// corpus: trap/t102-zero-length-store
// purpose: A clear releases a failed unobserved task at the store.
// exercises: array-clear, counted-array, async-handle
// questions: compiler.md §174, §171, §116
// tsc: accepts
// js-comparable: no C8: Async handles have no Promise assimilation.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw in fail

async function fail(): Promise<void> { throw new Error("lost"); }
async function use(skip: boolean): Promise<void> {
  const jobs: Promise<void>[] = [fail()];
  if (skip) { jobs.length = 0; }
  else { await jobs[0]; }
}
export async function main(): Promise<void> { await use(true); }

// pin: 69212b29
// pin-dev-jit: Exit 1; S100 at ArrayUnknownMember with no divergence block.
// pin-c-aot: Exit 1; S100 at ArrayUnknownMember with no divergence block.
// pin-interpreter: Checker rejects with S100 before execution.
