// corpus: accept/a166-resume-parameter-interference
// purpose: Keeps an earlier loop lambda when a resume parameter carries a later lambda into conditional storage.
// exercises: async-await, lambda-environment, loop, resume-parameter, interference
// questions: §68
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
export async function main(): Promise<void> {
  let keep = (): i32 => 0;
  let i: i32 = 0;
  while (i < 2) {
    const factor: i32 = i + 1;
    const fn = (): i32 => factor * 10;
    await Context.suspend();
    if (i === 0) {
      keep = fn;
    }
    i = i + 1;
  }
  print(`resume-parameter=${keep()}`);
}
