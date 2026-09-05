// corpus: accept/a95-interop-async-await
// interpreter: no — calls the synthetic native interop library
// purpose: Exercises deterministic foreign polling from an async function.
// exercises: foreign-poll, Context.suspend, async-main
// questions: Q34, Q1, C8
// tsc: accepts; js-comparable: no C8 Q13: The host C boundary has no JavaScript shim.
export async function main(): Promise<void> {
  let attempt: i32 = 0;
  print("poll:start");
  while (subDevicePoll(attempt) === 0) {
    print(`poll:pending=${attempt}`);
    attempt += 1;
    await Context.suspend();
  }
  print(`poll:ready=${attempt}`);
}
