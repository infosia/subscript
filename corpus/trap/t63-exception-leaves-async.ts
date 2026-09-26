// corpus: trap/t63-exception-leaves-async
// purpose: An exception that leaves a host-kicked async export becomes the uncaught-exception trap at its completion, because the host is its only holder.
// exercises: throw, async-function, host-entry, uncaught-exception
// questions: Q9, Q34
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the `throw` in `main`
async function load(value: i32): Promise<i32> {
  print(`load:${value}`);
  return value;
}

export async function main(): Promise<void> {
  const first: i32 = await load(1);
  print(`first=${first}`);
  if (first > 0) {
    throw new Error(`main failed after ${first}`);
  }
  print("unreached");
}
