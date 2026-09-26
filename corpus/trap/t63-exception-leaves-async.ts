// corpus: trap/t63-exception-leaves-async
// purpose: An exception that leaves an async body becomes the uncaught-exception trap at the body, so no handler of the caller catches it.
// exercises: throw, async-function, held-handle, uncaught-exception, array-for-each, try-catch
// questions: Q9, Q34
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the `throw` in `load`
async function load(value: i32): Promise<i32> {
  print(`load:${value}`);
  if (value > 1) {
    throw new Error(`load ${value} failed`);
  }
  return value;
}

export async function main(): Promise<void> {
  const first: i32 = await load(1);
  print(`first=${first}`);
  const values: i32[] = [2];
  const handles: Promise<i32>[] = [];
  try {
    // The body of `load` runs at the call (§92). The forEach call is a
    // raise site with a handler, so an exception that left the async
    // body would reach this catch.
    values.forEach((value: i32): void => {
      handles.push(load(value));
      print("created");
    });
  } catch {
    print("caught");
  }
  for (const handle of handles) {
    print(`awaited=${await handle}`);
  }
  print("unreached");
}
