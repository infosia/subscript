// corpus: accept/a447-retired-capture-escape-shapes
// purpose: Each retired C5 escape shape keeps its environment.
// exercises: lambda, generator, async, field, return, loop, indirect-call
// questions: compiler.md §274, collisions.md C5
// tsc: accepts; js-comparable: yes
function w0_makeAdder(offset: i32): (value: i32) => i32 {
  const local: i32 = offset;
  return (value: i32): i32 => value + local;
}

function w0_main(): void {
  const addFour: (value: i32) => i32 = w0_makeAdder(4);
  print(`${addFour(6)}`);
}
class w1_Holder {
  callback: (() => i32) | null = null;
}
function w1_store(holder: w1_Holder, cb: () => i32): void {
  holder.callback = cb;
}
function w1_main(): void {
  const holder = new w1_Holder();
  const value: i32 = 5;
  w1_store(holder, (): i32 => value);
}
function w2_forward(cb: () => i32): () => i32 {
  return cb;
}
function w2_main(): void {
  const value: i32 = 5;
  const callback = w2_forward((): i32 => value);
  print(`${callback()}`);
}
function w3_main(): void {
  const forward = (cb: () => i32): (() => i32) => cb;
  const value: i32 = 5;
  const callback = forward((): i32 => value);
  print(`${callback()}`);
}
function* w4_gen(cb: () => i32): Generator<i32> {
  yield cb();
  yield cb();
}
function w4_make(): Generator<i32> {
  const value: i32 = 5;
  return w4_gen((): i32 => value);
}
function w4_main(): void {
  for (const value of w4_make()) { print(`${value}`); }
}
async function w5_run(cb: () => i32): Promise<i32> {
  await Context.suspend();
  return cb();
}
async function w5_main(): Promise<void> {
  const value: i32 = 5;
  const handle = w5_run((): i32 => value);
  const result: i32 = await handle;
  print(`${result}`);
}
function* w6_gen(): Generator<() => i32> {
  const value: i32 = 5;
  yield (): i32 => value;
}
function w6_main(): void {
  for (const callback of w6_gen()) { print(`${callback()}`); }
}
function w7_named(): i32 { return 0; }
function w7_make(): () => i32 {
  const value: i32 = 5;
  let callback: () => i32 = w7_named;
  for (let i: i32 = 0; i < 2; i++) {
    if (i === 1) { return callback; }
    callback = (): i32 => value;
  }
  return w7_named;
}
function w7_main(): void { print(`${w7_make()()}`); }
function* w8_gen(cb: () => i32): Generator<i32> { yield cb(); }
function w8_store(values: Generator<i32>[], value: Generator<i32>): void {
  values.push(value);
}
function w8_main(): void {
  const value: i32 = 5;
  const values: Generator<i32>[] = [];
  w8_store(values, w8_gen((): i32 => value));
}
function w9_call(cb: () => i32): i32 { return cb(); }
function w9_main(): void {
  const value: i32 = 5;
  const indirect: (cb: () => i32) => i32 = w9_call;
  print(`${indirect((): i32 => value)}`);
}
export async function main():Promise<void> {
w0_main();
w1_main();
w2_main();
w3_main();
w4_main();
await w5_main();
w6_main();
w7_main();
w8_main();
w9_main();
}
