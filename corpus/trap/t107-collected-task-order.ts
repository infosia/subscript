// corpus: trap/t107-collected-task-order
// purpose: Collection releases failed field owners in increasing task id.
// exercises: counted-map, counted-field, async-handle, ownership
// questions: compiler.md §177, §172, §171, §70, §116
// tsc: accepts
// js-comparable: no C8: Context collection and coroutine handles have no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at Context.collect; message Error: first

class Box { value: Promise<void>; constructor(value: Promise<void>) { this.value = value; } }
async function fail(message: string): Promise<void> { throw new Error(message); }
async function keep(skip: boolean, message: string): Promise<void> {
  const h = fail(message);
  if (!skip) { await h; }
  const box = new Box(h);
}
export async function main(): Promise<void> {
  await keep(true, "first");
  await keep(true, "second");
  await keep(true, "third");
  await keep(true, "fourth");
  await keep(true, "fifth");
  Context.collect();
  print("after");
}

// pin: dc3acdb4
// pin-dev-jit: Exit 3; uncaught-exception at 23:3; message Error: first; no stdout.
// pin-c-aot: Exit 3; uncaught-exception at 23:3; message Error: first; no stdout.
// pin-interpreter: uncaught-exception at 23:3; message Error: first; no stdout.
// round-1-release-interpreter: Red; message Error: third instead of Error: first; no stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
