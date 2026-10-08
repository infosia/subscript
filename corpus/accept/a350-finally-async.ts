// corpus: accept/a350-finally-async
// purpose: Suspended finalizers keep independent exceptions and preserve identity across collection.
// exercises: finally, await, async-function, throw, identity, collect
// questions: C6, C11, Q9, Q34
// tsc: accepts
// js-comparable: no C11: Context.suspend and Context.collect use the host checkpoint and explicit collection APIs.
async function fail(tag: string): Promise<void> { throw new Error(tag); }
async function f(tag: string): Promise<void> {
  try { await fail(tag); }
  finally { print(`before ${tag}`); await Context.suspend(); print(`after ${tag}`); }
}
async function identity(): Promise<void> {
  const original = new Error("identity");
  try {
    try { throw original; }
    finally { await Context.suspend(); Context.collect(); print("finally"); }
  } catch (e) { if (e instanceof Error) { print(`${e === original}`); } }
}
async function withCatch(): Promise<void> {
  try { await fail("caught"); }
  catch (e) { if (e instanceof Error) { print(`catch ${e.message}`); } }
  finally { await Context.suspend(); print("catch finally"); }
}
export async function main(): Promise<void> {
  const a = f("A"); const b = f("B");
  try { await a; } catch (e) { if (e instanceof Error) { print(`a ${e.message}`); } }
  try { await b; } catch (e) { if (e instanceof Error) { print(`b ${e.message}`); } }
  await identity(); await withCatch();
}

// pin: 03974dd6
// pin-dev-jit: Rejected with S010 at finally; no stdout.
// pin-c-aot: Rejected with S010 at finally; no stdout.
// pin-interpreter: Rejected with S010 at finally; no stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
