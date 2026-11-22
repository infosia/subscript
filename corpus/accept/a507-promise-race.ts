// corpus: accept/a507-promise-race
// purpose: A race completes with the first completion that it reads, and the inputs that lost run on and stay awaitable.
// exercises: promise-race, async-function, held-handle, try-catch, array-store, nullable
// questions: compiler.md §350 rules 1, 3, 4, 5, and 12, §166, §70
// tsc: accepts; js-comparable: yes
//
// `turn` advances once for each job of `counter`. A direct `await` of a
// task resumes 3 jobs after its start. An `await` of a race or of
// Promise.all over the same task resumes one job later (rule 4). The
// input `lose` fails after it lost, and the program ends with no trap
// (rule 5).
class Box {
  n: i32;
  constructor(n: i32) { this.n = n; }
}

let turn: i32 = 0;

async function nothing(): Promise<void> { return; }
async function wait(turns: i32): Promise<void> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
}
async function counter(): Promise<void> {
  for (let k: i32 = 0; k < 40; k += 1) { turn += 1; await nothing(); }
}
async function value(n: i32, turns: i32): Promise<i32> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
  return n;
}
async function fails(message: string, turns: i32): Promise<i32> {
  for (let i: i32 = 0; i < turns; i += 1) { await nothing(); }
  throw new Error(message);
}
async function boxed(n: i32, turns: i32): Promise<Box> {
  await wait(turns);
  return new Box(n);
}
async function maybe(n: i32, turns: i32): Promise<Box | null> {
  await wait(turns);
  return n < 0 ? null : new Box(n);
}
async function text(s: string, turns: i32): Promise<string> {
  await wait(turns);
  return `${s}!`;
}
async function step(tag: string, turns: i32): Promise<void> {
  await wait(turns);
  print(`step ${tag}`);
}

export async function main(): Promise<void> {
  print(`first ${await Promise.race([value(1, 3), value(2, 1)])}`);
  try { await Promise.race([value(1, 3), fails("first", 1)]); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }

  const lose: Promise<i32> = fails("late", 4);
  print(`lost ${await Promise.race([value(5, 0), lose])}`);

  const a: Promise<i32> = value(7, 0);
  const b: Promise<i32> = value(8, 0);
  print(`order ${await Promise.race([a, b])} ${await Promise.race([b, a])}`);

  const pending: Promise<i32> = value(21, 1);
  const done: Promise<i32> = value(22, 0);
  print(`same turn ${await Promise.race([pending, done])} ${await pending}`);

  const slow: Promise<i32> = value(10, 2);
  print(`loser ${await Promise.race([slow, value(11, 0)])} ${await slow}`);

  const c: Promise<void> = counter();
  const s0: i32 = turn;
  await value(0, 2);
  const n0: i32 = turn - s0;
  const s1: i32 = turn;
  await Promise.race([value(0, 2)]);
  const n1: i32 = turn - s1;
  const s2: i32 = turn;
  await Promise.all([value(0, 2)]);
  const n2: i32 = turn - s2;
  print(`turns ${n0} ${n1} ${n2}`);
  await c;

  await Promise.race([step("v1", 2), step("v2", 1)]);
  await wait(4);
  print(`class ${(await Promise.race([boxed(4, 2), boxed(5, 1)])).n}`);
  const m: Box | null = await Promise.race([maybe(6, 2), maybe(-1, 1)]);
  print(`nullable ${m === null ? "null" : "box"}`);
  const m2: Box | null = await Promise.race([maybe(7, 1), maybe(-1, 2)]);
  print(`nullable ${m2 === null ? "null" : `${m2.n}`}`);
  print(`string ${await Promise.race([text("a", 2), text("b", 1)])}`);

  const held: Promise<i32> = Promise.race([value(8, 2), value(9, 1)]);
  await wait(4);
  print(`held ${await held}`);

  const jobs: Promise<i32>[] = [value(10, 3), value(11, 2)];
  print(`array ${await Promise.race(jobs)} ${await jobs[0]} ${await jobs[1]}`);

  const twice: Promise<i32> = value(12, 1);
  print(`duplicate ${await Promise.race([twice, twice])} ${await twice}`);
  print("end");
}
