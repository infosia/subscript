// corpus: trap/t94-counted-map-delete
// purpose: Map removal releases the sole owner of an unobserved failed task.
// exercises: counted-map, counted-field, async-handle, ownership
// questions: compiler.md §172, §171, §70, §116
// tsc: accepts
// js-comparable: no C8: Context collection and coroutine handles have no JavaScript shim.
// tier-policy: both tiers trap
// expected-trap: uncaught-exception at the throw in fail; release occurs at the Map mutation

async function fail(): Promise<void> { throw new Error("lost"); }
async function work(): Promise<void> { return; }
async function keep(skip: boolean): Promise<Map<i32, Promise<void>>> {
  const m: Map<i32, Promise<void>> = new Map<i32, Promise<void>>();
  const h = fail();
  if (!skip) { await h; }
  m.set(1, h);
  return m;
}
async function use(skip: boolean): Promise<void> {
  const fallback = work(); await fallback;
  const m = await keep(skip);
  m.delete(1);
  print("after");
}
export async function main(): Promise<void> { await use(true); }

// pin: dbccd4d8
// pin-dev-jit: Exit 0; prints after; no trap.
// pin-c-aot: Exit 0; prints after; no trap.
// pin-interpreter: Prints after; no trap.
// tsc-version: TypeScript 5.9.2; exit 0.
