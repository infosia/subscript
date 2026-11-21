// corpus: trap/t117-detected-task-order
// purpose: The outermost exit reports the failed task with the lowest task id of two unreachable ones.
// exercises: counted-field, async-handle, ownership
// questions: compiler.md §342, §177, §116, §70
// tsc: accepts
// js-comparable: no C8: Context collection and coroutine handles have no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw; message Error: first

class Box { value: Promise<void>; constructor(value: Promise<void>) { this.value = value; } }
async function fail(message: string): Promise<void> { throw new Error(message); }
function hold(message: string): void {
  const box = new Box(fail(message));
}
export function main(): void {
  hold("first");
  hold("second");
  print("after");
}

// pin: ca3fc9ed
// pin-dev-jit: Prints after; no trap.
// pin-c-aot: Prints after; no trap.
// pin-interpreter: Prints after; no trap.
// tsc-version: TypeScript 5.9.2; exit 0.
