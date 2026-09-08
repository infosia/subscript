// corpus: trap/t54-async-start-fault
// purpose: A started async body traps before the caller receives its handle.
// exercises: async-call, held-handle, trap-stop
// questions: Q34, C6
// expected-trap: unreachable-reached at the callee's unreachable() call

async function fail(): Promise<void> {
  print("callee:before");
  unreachable();
}

export async function main(): Promise<void> {
  print("caller:before");
  const handle: Promise<void> = fail();
  print("caller:after-create");
  await handle;
  print("caller:after-await");
}
