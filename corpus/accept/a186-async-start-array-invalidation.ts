// corpus: accept/a186-async-start-array-invalidation
// purpose: The caller sees the grown array because the async body runs at the call.
// exercises: async-call, array-growth, array-invalidation, held-handle
// questions: Q34
// tsc: accepts; js-comparable: yes
async function grow(a: i32[]): Promise<i32> {
  for (let i: i32 = 0; i < 64; i += 1) {
    a.push(100 + i);
  }
  return a[0];
}

export async function main(): Promise<void> {
  const a: i32[] = [1, 2];
  a[0] = 5;
  const h: Promise<i32> = grow(a);
  print(`a0=${a[0]} len=${a.length} last=${a[a.length - 1]}`);
  const v: i32 = await h;
  print(`v=${v}`);
}
