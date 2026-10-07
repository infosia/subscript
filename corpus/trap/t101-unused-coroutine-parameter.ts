// corpus: trap/t101-unused-coroutine-parameter
// purpose: An unused coroutine parameter stays rooted until completion.
// exercises: counted-field, coroutine, collection, liveness
// questions: compiler.md §172 rule 7, §116
// tsc: accepts
// js-comparable: no C8: Context collection has no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception; message Error: lost

class Holder { task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }
async function fail(): Promise<void> { throw new Error("lost"); }
async function tick(): Promise<void> {}
async function work(h: Holder): Promise<void> {
    print("start");
    await tick();
    Context.collect();
    print("after");
}
export async function main(): Promise<void> { await work(new Holder(fail())); Context.collect(); }

// pin: 1dcaafb8 with the preserved round 8 implementation.
// pin-dev-jit: Prints start and after; no trap.
// pin-c-aot: Prints start and after; no trap.
// pin-interpreter: Traps at collect; prints start.
// tsc-version: TypeScript 5.9.2; exit 0.
