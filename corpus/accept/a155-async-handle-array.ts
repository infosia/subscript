// corpus: accept/a155-async-handle-array
// purpose: Stores async handles in an array and awaits each element in index order.
// exercises: async-handle, dynamic-array, indexed-read, delayed-await
// questions: §70, C8
// tsc: accepts; js-comparable: no C8: The coroutine API has no JavaScript shim.
async function work(value: i32): Promise<i32> {
  print(`start=${value}`);
  await Context.suspend();
  print(`resume=${value}`);
  return value * 10;
}

export async function main(): Promise<void> {
  const handles: Promise<i32>[] = [work(1), work(2), work(3)];
  print(`held=${handles.length}`);
  let total: i32 = 0;
  for (let i: i32 = 0; i < handles.length; i += 1) {
    const value: i32 = await handles[i];
    print(`value=${value}`);
    total += value;
  }
  print(`total=${total}`);
}
