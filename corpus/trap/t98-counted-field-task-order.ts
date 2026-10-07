// corpus: trap/t98-counted-field-task-order
// purpose: Collection releases failed field owners in increasing task id.
// exercises: counted-map, counted-field, async-handle, ownership
// questions: compiler.md §172, §171, §70, §116
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
  Context.collect();
  print("after");
}

// pin: dbccd4d8
// pin-dev-jit: Exit 0; prints after; no trap.
// pin-c-aot: Exit 0; prints after; no trap.
// pin-interpreter: Prints after; no trap.
// tsc-version: TypeScript 5.9.2; exit 0.
