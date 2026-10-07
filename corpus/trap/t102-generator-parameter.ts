// corpus: trap/t102-generator-parameter
// purpose: A generator parameter stays rooted until completion.
// exercises: counted-field, coroutine, collection, liveness
// questions: compiler.md §172 rule 7, §116
// tsc: accepts
// js-comparable: no C8: Context collection has no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception; message Error: lost

class Holder { task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }
async function fail(): Promise<void> { throw new Error("lost"); }
let kept: Holder | null = null;
function* work(h: Holder): Generator<i32> {
    const t = h.task;
    yield 1;
    Context.collect();
    print("after");
}
export async function main(): Promise<void> {
    const g = work(new Holder(fail()));
    g.next();
    await Context.suspend();
    g.next();
    Context.collect();
}

// pin: 1dcaafb8 with the preserved round 8 implementation.
// pin-dev-jit: Prints after; no trap.
// pin-c-aot: Prints after; no trap.
// pin-interpreter: Prints after; then traps at the generator exit.
// tsc-version: TypeScript 5.9.2; exit 0.
