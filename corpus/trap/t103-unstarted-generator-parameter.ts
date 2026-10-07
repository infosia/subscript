// corpus: trap/t103-unstarted-generator-parameter
// purpose: An unstarted generator roots its unused parameter.
// exercises: counted-field, coroutine, collection, liveness
// questions: compiler.md §172 rule 7, §116
// tsc: accepts
// js-comparable: no C8: Context collection has no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception; message Error: lost

class Holder { task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }
async function fail(): Promise<void> { throw new Error("lost"); }
function* work(h: Holder): Generator<i32> {
    yield 1;
}
export async function main(): Promise<void> {
    const g = work(new Holder(fail()));
    await Context.suspend();
    Context.collect();
    print("after");
    g.next();
    g.next();
    Context.collect();
}

// pin: 1dcaafb8 with the preserved round 8 implementation.
// pin-dev-jit: Prints after; no trap.
// pin-c-aot: Prints after; no trap.
// pin-interpreter: Traps at collect; prints nothing.
// tsc-version: TypeScript 5.9.2; exit 0.
