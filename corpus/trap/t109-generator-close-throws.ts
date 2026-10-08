// corpus: trap/t109-generator-close-throws
// purpose: An exception from a close finalizer traps at its throw.
// exercises: generator, finally, break, throw
// questions: C6, C8, Q9
// tsc: accepts
// js-comparable: no C8: An exception that leaves a generator body traps.
// tier-policy: all three tiers trap
// expected-trap: uncaught-exception at 11:13; message Error: close
function* values(): Generator<i32> {
  try { yield 1; print("unreachable"); }
  finally { throw new Error("close"); }
}
export function main(): void {
  for (const value of values()) { print(`${value}`); break; }
  print("after");
}

// pin: 03974dd6
// pin-dev-jit: Rejected S010 at 11:11; no stdout.
// pin-c-aot: Rejected S010 at 11:11; no stdout.
// pin-interpreter: Rejected S010 at 11:11; no stdout.
