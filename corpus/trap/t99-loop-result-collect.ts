// corpus: trap/t99-loop-result-collect
// purpose: An unfinished call result does not root the previous loop result.
// exercises: counted-field, async-handle, collection, loop
// questions: compiler.md §172 rule 7, §116
// tsc: accepts
// js-comparable: no C8: Context collection has no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the second collect; message Error: lost

class Holder { task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }
async function fail(): Promise<void> { throw new Error("lost"); }
function step(i: i32): Holder {
  Context.collect();
  print(`step ${i}`);
  return new Holder(fail());
}
export function main(): void { for (let i: i32 = 0; i < 2; i++) { step(i); } }

// pin: 04963f81
// pin-dev-jit: Prints step 0 and step 1; no trap.
// pin-c-aot: Prints step 0 and step 1; no trap.
// pin-interpreter: Prints step 0 and step 1; no trap.
// round-7-before-fix-dev-jit: Traps at the second collect; prints step 0.
// round-7-before-fix-c-aot: Traps at the second collect; prints step 0.
// round-7-before-fix-interpreter: Prints step 0 and step 1; no trap.
// current-tree-dev-jit: Traps at the second collect; prints step 0.
// current-tree-c-aot: Traps at the second collect; prints step 0.
// current-tree-interpreter: Traps at the second collect; prints step 0.
// tsc-version: TypeScript 5.9.2; exit 0.
