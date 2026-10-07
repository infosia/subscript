// corpus: trap/t104-coroutine-closure-environment
// purpose: A coroutine retains its closure environments until completion.
// exercises: counted-field, coroutine, collection, liveness
// questions: compiler.md §172 rule 7, §116
// tsc: accepts
// js-comparable: no C8: Context collection has no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception; message Error: lost

class Holder { task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }
async function fail(): Promise<void> { throw new Error("lost"); }
async function tick(): Promise<void> {}
async function work(): Promise<void> {
    const h = new Holder(fail());
    const f = (): void => { const t = h.task; };
    f();
    await tick();
    Context.collect();
    print("after");
}
export async function main(): Promise<void> { await work(); Context.collect(); }

// pin: 1dcaafb8 with the preserved round 8 implementation.
// pin-dev-jit: Prints after; no trap.
// pin-c-aot: Prints after; no trap.
// pin-interpreter: Traps at collect; prints nothing.
// tsc-version: TypeScript 5.9.2; exit 0.
