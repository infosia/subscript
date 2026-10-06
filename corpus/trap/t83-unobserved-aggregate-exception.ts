// corpus: trap/t83-unobserved-aggregate-exception
// purpose: The last holder releases a failed aggregate without an observed completion.
// exercises: promise-all, held-handle, context-free, uncaught-exception
// questions: compiler.md §166, §116 rule 4
// tier-policy: both tiers trap
// js-comparable: no C8: An unobserved exception traps at the last release.
// expected-trap: uncaught-exception at the throw in fails, reported at the holder release

async function value(n: i32): Promise<i32> { return n; }
async function fails(): Promise<i32> { throw new Error("aggregate dropped"); }
class Holder {
  job: Promise<i32[]>;
  constructor(job: Promise<i32[]>) { this.job = job; }
}
export async function main(): Promise<void> {
  const jobs: Promise<i32>[] = [fails()];
  const holder: Holder = new Holder(Promise.all(jobs));
  await value(0);
  await value(0);
  print("release aggregate");
  Context.free(holder);
  print("unreached");
}
