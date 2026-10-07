// corpus: reject/r388-counted-map-for-each
// purpose: A callback collection cannot carry a counted value.
// exercises: counted-map, counted-field, async-handle, ownership
// questions: compiler.md §172, §171, §70, §116
// tsc: accepts
// js-comparable: no C8: Context collection and coroutine handles have no JavaScript shim.
// expected-error: S014 at Map.forEach

let stored: Promise<void>[] = [];
async function work(): Promise<void> { return; }
export async function main(): Promise<void> {
  const h = work(); await h;
  const m: Map<i32, Promise<void>> = new Map<i32, Promise<void>>();
  m.set(1, h);
  m.forEach((value: Promise<void>, key: i32): void => { stored = [value]; });
  stored = [];
}

// pin: dbccd4d8
// pin-dev-jit: Exit 2; counted-store LIR error, not S014.
// pin-c-aot: Exit 1; counted-store LIR error, not S014.
// pin-interpreter: Counted-store LIR error before execution, not S014.
// tsc-version: TypeScript 5.9.2; exit 0.
