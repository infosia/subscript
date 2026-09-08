// corpus: trap/t55-async-trap-after-settled-await
// purpose: A body that faults after a settled await reports when its continuation runs, after the caller's later effects.
// exercises: async-call, held-handle, settled-await, continuation-queue, trap-stop
// questions: §94, Q34, C6
// expected-trap: unreachable-reached at the callee's unreachable() call
async function settled(): Promise<i32> {
  print("settled");
  return 7;
}

async function fail(): Promise<i32> {
  print("callee:before");
  const value: i32 = await settled();
  print(`callee:after=${value}`);
  unreachable();
  return value;
}

export async function main(): Promise<void> {
  print("caller:before");
  const handle: Promise<i32> = fail();
  print("caller:after-create");
  print(`v=${await handle}`);
  print("caller:after-await");
}
