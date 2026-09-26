// corpus: trap/t67-exception-exit-releases-its-handle
// purpose: An exception exit releases an unobserved failed handle.
// exercises: throw, async-function, held-handle, uncaught-exception
// questions: Q9, Q34
// tier-policy: both tiers trap
// js-comparable: no C8
// expected-trap: uncaught-exception at the throw in fails
async function fails(): Promise<i32> { throw new Error("dropped"); }
async function inner(skip: boolean): Promise<i32> {
  const h: Promise<i32> = fails();
  if (skip) { throw new TypeError("skipped"); }
  return await h;
}
export async function main(): Promise<void> {
  try { print(`inner ${await inner(true)}`); } catch (e) { if (e instanceof Error) { print(`caught ${e.name} ${e.message}`); } }
  print("end");
}
