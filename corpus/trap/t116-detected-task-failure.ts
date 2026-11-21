// corpus: trap/t116-detected-task-failure
// purpose: The outermost exit reports a failed task with an unreachable holder, with no explicit collection.
// exercises: counted-field, async-handle, ownership
// questions: compiler.md §342, §177, §116, §70
// tsc: accepts
// js-comparable: no C8: Context collection and coroutine handles have no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw; message Error: lost

class Box { value: Promise<void>; constructor(value: Promise<void>) { this.value = value; } }
async function fail(message: string): Promise<void> { throw new Error(message); }
async function keep(skip: boolean, message: string): Promise<void> {
  const h = fail(message);
  if (!skip) { await h; }
  const box = new Box(h);
}
export async function main(): Promise<void> {
  await keep(true, "lost");
  print("after");
}

// pin: ca3fc9ed
// pin-dev-jit: Prints after; no trap.
// pin-c-aot: Prints after; no trap.
// pin-interpreter: Prints after; no trap.
// tsc-version: TypeScript 5.9.2; exit 0.
