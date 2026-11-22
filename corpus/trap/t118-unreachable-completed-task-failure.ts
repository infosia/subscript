// corpus: trap/t118-unreachable-completed-task-failure
// purpose: A collection ends a completed task that only unreachable objects hold, and the outermost exit reports the failed task that it held.
// exercises: counted-field, async-handle, completed-task, ownership
// questions: compiler.md §348 rules 1, 3, and 4, §342 rules 12 to 15, §70, §116
// tsc: accepts
// js-comparable: no C8: Context collection and coroutine handles have no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw; message Error: cycle

class Box {
  failing: Promise<void>;
  self: Promise<Box | null>;
  constructor() { this.failing = done(); this.self = none(); }
}
async function done(): Promise<void> {}
async function none(): Promise<Box | null> { return null; }
async function keep(box: Box): Promise<Box | null> { return box; }
async function fail(): Promise<void> { await Context.suspend(); throw new Error("cycle"); }
function start(): void {
  const box: Box = new Box();
  box.failing = fail();
  box.self = keep(box);
}
export async function main(): Promise<void> {
  start();
  for (let k: i32 = 0; k < 3; k += 1) { await Context.suspend(); }
  print("after");
}

// pin: 3a74c396
// pin-dev-jit: Prints after; no trap.
// pin-c-aot: Prints after; no trap.
// pin-interpreter: Prints after; no trap.
// tsc-version: TypeScript 5.9.2; exit 0.
