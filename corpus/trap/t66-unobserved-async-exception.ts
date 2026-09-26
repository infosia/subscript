// corpus: trap/t66-unobserved-async-exception
// purpose: A handle that holds an exception that no await raised traps when its count reaches zero, with the position of the throw.
// exercises: throw, async-function, held-handle, context-free, uncaught-exception
// questions: Q9, Q34
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the `throw` in `fails`, reported at the release in `main`
async function fails(tag: string): Promise<i32> {
  print(`fails:start ${tag}`);
  throw new Error(`never observed ${tag}`);
}

class Holder {
  job: Promise<i32>;

  constructor(job: Promise<i32>) {
    this.job = job;
  }
}

export async function main(): Promise<void> {
  const holder: Holder = new Holder(fails("held"));
  print("main:created");
  Context.free(holder);
  print("unreached");
}
