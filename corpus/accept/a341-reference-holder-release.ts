// corpus: accept/a341-reference-holder-release
// purpose: Map mutation and object collection preserve the values that live holders await.
// exercises: counted-map, counted-field, async-handle, ownership
// questions: compiler.md §172, §171, §70, §116
// tsc: accepts
// js-comparable: no C8: Context collection and coroutine handles have no JavaScript shim.

async function work(value: i32): Promise<i32> { return value; }
class Box<T> { value: T; constructor(value: T) { this.value = value; } }
function keep(): Map<i32, Promise<i32>> {
  const m: Map<i32, Promise<i32>> = new Map<i32, Promise<i32>>();
  const h = work(11);
  m.set(1, h);
  return m;
}
async function handles(): Promise<void> {
  const m = keep();
  const fallback = work(99); await fallback;
  const first = m.getOr(1, fallback);
  print(`${await first}`);
  const a = work(22); await a;
  m.set(1, a); m.set(1, a);
  for (const h of m.values()) { print(`${await h}`); }
  m.delete(1); m.clear(); Context.free(m);
}
async function arrays(): Promise<void> {
  const h = work(33); await h;
  const a: Promise<i32>[] = [h];
  const m: Map<i32, Promise<i32>[]> = new Map<i32, Promise<i32>[]>();
  m.set(1, a); m.set(1, a);
  const b: Promise<i32>[] = [work(44)]; await b[0];
  m.set(1, b);
  const read = m.getOr(1, a);
  print(`${await read[0]}`);
  for (const row of m.values()) { print(`${await row[0]}`); }
  m.delete(1); m.set(2, a); m.clear(); Context.free(m);
}
async function nested(): Promise<void> {
  const h = work(55); await h;
  const a: Promise<i32>[][] = [[h]];
  const m: Map<i32, Promise<i32>[][]> = new Map<i32, Promise<i32>[][]>();
  m.set(1, a); m.set(1, a);
  const b: Promise<i32>[][] = [[work(66)]]; await b[0][0];
  m.set(1, b);
  const read = m.getOr(1, a);
  print(`${await read[0][0]}`);
  for (const row of m.values()) { print(`${await row[0][0]}`); }
  m.delete(1); m.set(2, a); m.clear(); Context.free(m);
}
async function fixed(): Promise<void> {
  const h = work(77); await h;
  const a: FixedArray<Promise<i32>, 1> = [h];
  const m: Map<i32, FixedArray<Promise<i32>, 1>> = new Map<i32, FixedArray<Promise<i32>, 1>>();
  m.set(1, a); m.set(1, a);
  const b: FixedArray<Promise<i32>, 1> = [work(88)]; await b[0];
  m.set(1, b);
  const read = m.getOr(1, a);
  print(`${await read[0]}`);
  for (const row of m.values()) { print(`${await row[0]}`); }
  m.delete(1); m.set(2, a); m.clear(); Context.free(m);
}
async function field(): Promise<void> {
  const h = work(101); await h;
  const box = new Box<Promise<i32>>(h);
  print(`${await box.value}`);
}
export async function main(): Promise<void> {
  await handles(); await arrays(); await nested(); await fixed(); await field();
  Context.collect();
  const h = work(102); print(`${await h}`);
}

// pin: dbccd4d8
// pin-dev-jit: Expected output; 6 retained tasks, zero control 0.
// pin-c-aot: Expected output; 6 retained tasks, zero control 0.
// pin-interpreter: Expected output; 6 retained tasks, zero control 0.
// tsc-version: TypeScript 5.9.2; exit 0.
