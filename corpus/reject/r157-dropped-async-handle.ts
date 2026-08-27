// corpus: reject/r157-dropped-async-handle
// purpose: Rejects a held async handle when no holder ever awaits its completion.
// exercises: async-handle, held-handle, dropped-handle
// questions: §70, C8
// tsc: accepts
// expected-error: S013 at the async call whose handle is dropped

async function work(): Promise<void> {
  await Context.suspend();
}

export async function main(): Promise<void> {
  const dropped = work();
  print("not awaited");
}
