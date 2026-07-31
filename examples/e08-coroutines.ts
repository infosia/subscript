// example: e08-coroutines
// teaches: Advance a function* coroutine once per frame and observe each suspended value.
// differs-from-typescript: C8/Q34 provide host-driven coroutines and poll-driven async; no event loop or Promise objects.
// see: corpus/accept/a20-coroutine-generator.ts, corpus/accept/a79-for-of-generator.ts, corpus/reject/r14-async.ts, collisions.md C8, compiler.md §7

function* updates(): Generator<i32> {
  let position: i32 = 0;
  for (let step: i32 = 1; step <= 3; step += 1) {
    position += step * 2;
    yield position;
  }
}

// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // C8 and compiler.md §7: invoking a generator allocates a Context-owned
  // frame for its suspended control state and live locals.
  const update: Generator<i32> = updates();

  for (let frame: i32 = 0; frame < 4; frame += 1) {
    // C8: one next call advances exactly one suspension, matching a
    // host-owned frame loop instead of draining the coroutine.
    const result = update.next();
    if (result.done) {
      print(`frame=${frame},done=true`);
    } else {
      print(`frame=${frame},value=${result.value}`);
    }
  }

  // Rejected alternative: async function is S013, "`async` requires an
  // event loop; the language has none (use coroutines)";
  // corpus/reject/r14-async.ts pins it.
}
