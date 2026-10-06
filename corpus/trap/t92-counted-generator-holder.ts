// corpus: trap/t92-counted-generator-holder
// purpose: IterResult exit releases an unobserved failed yielded handle.
// exercises: counted-array, async-handle, ownership, generator
// questions: compiler.md §171, §70, §116
// tsc: accepts
// js-comparable: no C8: Async handles have no Promise assimilation.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw in fail

async function fail(): Promise<void> { throw new Error("lost"); }
let stored: Promise<void>[] = [];
function consume(job: Promise<void>): void { stored.push(job); }
function* rows(): Generator<Promise<void>> {
  const job = fail();
  consume(job);
  stored.pop();
  yield job;
}
async function use(skip: boolean): Promise<void> {
  const source = rows();
  const first = source.next();
  if (!first.done && !skip) { await first.value; }
  print("yielded");
  source.next();
  print("completed");
}
export async function main(): Promise<void> { await use(true); }

// pin: 96f46f35
// pin-dev-jit: Exit 1; uncaught-exception before completed.
// pin-c-aot: Exit 3; uncaught-exception before completed.
// pin-interpreter: uncaught-exception before completed.
