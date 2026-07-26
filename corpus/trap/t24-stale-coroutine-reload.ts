// corpus: trap/t24-stale-coroutine-reload
// purpose: Reload-mode JIT traps when a suspended coroutine's body was replaced.
// exercises: coroutine, hot-reload, stale-coroutine, dev-tier-only-mode
// questions: none
// tier-policy: reload-mode dev-JIT only; shipped C has no hot-reload mode
// expected-trap: stale-coroutine at live.next() after the body replacement

function* counting() {
  let i: i32 = 0;
  while (i < 100) {
    yield i;
    i += 1;
  }
}

let live: Generator<i32> = counting();

export function main(): void {
  const step = live.next();
  print(`${step.value}`);
}
