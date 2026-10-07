// corpus: trap/t100-conditional-await-collect
// purpose: A suspend parameter does not root the previous loop iteration's holder.
// exercises: counted-field, async-handle, collection, conditional-await, loop
// questions: compiler.md §172 rule 7, §116
// tsc: accepts
// js-comparable: no C8: Context collection has no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the second collect; message Error: lost

class Holder { task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }
async function fail(): Promise<void> { throw new Error("lost"); }
async function tick(): Promise<void> {}
async function run(): Promise<void> {
  for (let i: i32 = 0; i < 2; i++) {
    const h = new Holder(fail());
    if (i == 0) { await tick(); }
    Context.collect();
    print(`step ${i}`);
    const t = h.task;
  }
}
export async function main(): Promise<void> { await run(); }

// pin: 9de0b638; preserved round 7 implementation before the round 8 fix.
// pin-dev-jit: Prints step 0 and step 1; no trap.
// pin-c-aot: Prints step 0 and step 1; no trap.
// pin-interpreter: Prints step 0 and step 1; no trap.
// current-tree-dev-jit: Traps at the second collect; prints step 0.
// current-tree-c-aot: Traps at the second collect; prints step 0.
// current-tree-interpreter: Traps at the second collect; prints step 0.
// tsc-version: TypeScript 5.9.2; exit 0.
