// corpus: reject/r105-floating-async-method-call
// purpose: Requires every async method call to appear immediately under await.
// exercises: floating-async-method-call, no-Promise-values
// questions: R13, Q34, C8
// tsc-clean-standalone: verified with node_modules/.bin/tsc against prelude/lang.d.ts; stock TypeScript permits ignoring an async method return value.
// expected-error: S013 at the floating async method call

class Worker {
  async work(): Promise<void> {
    await Context.suspend();
  }
}

export function main(): void {
  const worker: Worker = new Worker();
  worker.work();
}
