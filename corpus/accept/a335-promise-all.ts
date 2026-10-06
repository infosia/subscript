// corpus: accept/a335-promise-all
// purpose: An aggregate observes every input and preserves result and queue order.
// exercises: async-function, held-handle, promise-all, try-catch, array-store
// questions: compiler.md §166, §70, §94, §116
// tsc: accepts; js-comparable: yes

async function value(n: i32): Promise<i32> { return n; }
async function later(n: i32, turns: i32): Promise<i32> {
  for (let i: i32 = 0; i < turns; i++) { await value(0); }
  print(`value ${n}`);
  return n;
}
async function fails(tag: string, turns: i32): Promise<i32> {
  for (let i: i32 = 0; i < turns; i++) { await value(0); }
  print(`fail ${tag}`);
  throw new Error(tag);
}
async function turns(): Promise<void> {
  for (let i: i32 = 0; i < 5; i++) {
    print(`turn ${i}`);
    await value(0);
  }
}
async function success(): Promise<void> {
  const jobs: Promise<i32>[] = [later(10, 1), later(20, 2)];
  const xs: i32[] = await Promise.all(jobs);
  print(`success ${xs[0]} ${xs[1]}`);
}
async function quiet(): Promise<void> { await value(0); }

export async function main(): Promise<void> {
  const work: Promise<void> = success();
  const clock: Promise<void> = turns();
  await work;
  await clock;

  const failures: Promise<i32>[] = [fails("input-first", 8), fails("completion-first", 1)];
  try { await Promise.all(failures); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  print("after catch");
  for (let i: i32 = 0; i < 10; i++) { await value(0); }
  print("after later failure");

  const empty: Promise<i32>[] = [];
  const emptyResult: i32[] = await Promise.all(empty);
  print(`empty ${emptyResult.length}`);

  const duplicate: Promise<i32> = later(30, 1);
  const duplicates: Promise<i32>[] = [duplicate, duplicate];
  const duplicateResult: i32[] = await Promise.all(duplicates);
  print(`duplicate ${duplicateResult[0]} ${duplicateResult[1]}`);

  const jobs: Promise<i32>[] = [later(40, 2), value(50)];
  const snapshot: Promise<i32[]> = Promise.all(jobs);
  const replacement: Promise<i32> = value(99);
  jobs[0] = replacement;
  const snapshotResult: i32[] = await snapshot;
  print(`snapshot ${snapshotResult[0]} ${snapshotResult[1]}`);
  print(`replacement ${await replacement}`);

  const completed: Promise<i32> = value(60);
  const completedJobs: Promise<i32>[] = [completed];
  const completedResult: i32[] = await Promise.all(completedJobs);
  print(`completed ${completedResult[0]}`);
  print(`also awaited ${await completed}`);

  const voidJobs: Promise<void>[] = [quiet(), quiet()];
  await Promise.all(voidJobs);
  const heldVoid: Promise<void[]> = Promise.all(voidJobs);
  await heldVoid;
  print("void complete");
}
