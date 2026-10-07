// corpus: reject/r396-counted-capture-copy
// purpose: A local outside a counted binding block cannot receive its capture.
// exercises: capture-block, counted-binding, copy
// questions: compiler.md §175, C5
// tsc: accepts
// expected-error: S009: captured binding `h` is released when its block exits; local `f` is declared outside that block

async function work(n: i32): Promise<i32> { return n; }
export async function main(): Promise<void> {
  let f: () => Promise<i32> = () => work(0);
  { const h = work(2); const g = () => h; f = g; }
  print(`${await f()}`);
}

// pin: d19ed912
// pin-dev-jit: Checker accepts; exit 1, use-after-delete trap.
// pin-c-aot: Checker accepts; exit 3, async resume without completion trap.
// pin-interpreter: Checker accepts; internal async resume without completion trap.
