// corpus: accept/a152-lambda-env-per-iteration
// purpose: Keeps one coroutine loop iteration's lambda environment distinct from later iterations.
// exercises: closures, lambda-environment, coroutine, loop-iteration, live-range-storage
// questions: §68
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
export async function main(): Promise<void> {
  let keep = (): i32 => 0;
  let i: i32 = 0;
  while (i < 3) {
    const factor: i32 = i + 1;
    const f = (): i32 => factor * 10;
    if (i === 0) {
      keep = f;
    }
    await Context.suspend();
    i = i + 1;
  }
  print(`async-keep=${keep()}`);
}
