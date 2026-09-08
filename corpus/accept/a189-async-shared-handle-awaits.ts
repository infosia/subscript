// corpus: accept/a189-async-shared-handle-awaits
// purpose: Two parents await one handle, and the holder awaits its completed result twice.
// exercises: async-call, held-handle, shared-handle, repeated-await, completion-cache, reference-result
// questions: §94, §70, Q34
// tsc: accepts; js-comparable: yes
class Box {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

async function tick(): Promise<void> {
  print("tick");
}

async function boxed(): Promise<Box> {
  print("boxed:start");
  await tick();
  print("boxed:end");
  return new Box(42);
}

async function watcher(tag: string, handle: Promise<Box>): Promise<i32> {
  print(`${tag}:before`);
  const box: Box = await handle;
  print(`${tag}:after=${box.value}`);
  return box.value;
}

export async function main(): Promise<void> {
  const shared: Promise<Box> = boxed();
  const w1: Promise<i32> = watcher("w1", shared);
  const w2: Promise<i32> = watcher("w2", shared);
  print("main:held");
  print(`w1=${await w1}`);
  print(`w2=${await w2}`);
  const first: Box = await shared;
  print(`first=${first.value}`);
  const second: Box = await shared;
  print(`second=${second.value}`);
}
