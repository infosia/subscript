// corpus: accept/a340-counted-for-of-hold
// purpose: The loop holds its counted subject after the body resets the global.
// exercises: counted-array, async-handle, ownership, for-of
// questions: compiler.md §171, §70
// tsc: accepts
// js-comparable: yes

let queue: Promise<i32>[] = [];
async function work(value: i32): Promise<i32> { return value; }
function reset(): void { queue = []; }
async function use(): Promise<void> {
  const a = work(11); await a;
  const b = work(22); await b;
  queue = [a, b];
  for (const job of queue) {
    reset();
    const c = work(33); await c;
    const d = work(44); await d;
    queue = [c, d];
    print(`${await job}`);
  }
}
export async function main(): Promise<void> { await use(); }

// pin: 96f46f35
// pin-dev-jit: Prints 11 and 22; no last-holder storage free.
// pin-c-aot: Prints 11 and 22; no last-holder storage free.
// pin-interpreter: Prints 11 and 22; no last-holder storage free.
