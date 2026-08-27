// corpus: reject/r100-floating-async-call
// purpose: Rejects a dropped free-function async handle; holding the handle for a later await is legal.
// exercises: async-call-statement, dropped-async-handle
// questions: §70, Q34, C8
// tsc: accepts
// expected-error: S013 at the dropped async call

async function work(): Promise<void> {
  await Context.suspend();
}

export async function main(): Promise<void> {
  work();
}
