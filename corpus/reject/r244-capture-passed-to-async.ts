// corpus: reject/r244-capture-passed-to-async
// purpose: Reject a coroutine capture escape (compiler.md §118).
// exercises: coroutine, escape
// questions: Q10
// tsc: accepts
// expected-error: S009 at escape boundary
async function run(cb: () => i32): Promise<i32> {
  await Context.suspend();
  return cb();
}
export async function main(): Promise<void> {
  const value: i32 = 5;
  const handle = run((): i32 => value);
  const result: i32 = await handle;
  print(`${result}`);
}
