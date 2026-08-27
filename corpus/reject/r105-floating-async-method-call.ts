// corpus: reject/r105-floating-async-method-call
// purpose: Rejects a dropped async-method handle; holding the handle for a later await is legal.
// exercises: dropped-async-method-handle
// questions: §70, R13, Q34, C8
// tsc: accepts
// expected-error: S013 at the dropped async method call

class Worker {
  async work(): Promise<void> {
    await Context.suspend();
  }
}

export async function main(): Promise<void> {
  const worker: Worker = new Worker();
  worker.work();
}
