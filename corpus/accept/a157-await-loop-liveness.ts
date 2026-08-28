// corpus: accept/a157-await-loop-liveness
// purpose: Keeps an await result through a loop and a later suspension.
// exercises: async-await, loop, suspension-liveness
// questions: §68, C8
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
async function get(seed: i32): Promise<i32> {
  await Context.suspend();
  return seed;
}
export async function main(): Promise<void> {
  const first: i32 = await get(1);
  let index: i32 = 0;
  while (index < 4) {
    index = index + 1;
  }
  const second: i32 = await get(3);
  print(`${first},${second}`);
}
