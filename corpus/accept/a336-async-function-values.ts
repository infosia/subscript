// corpus: accept/a336-async-function-values
// purpose: Async function values preserve call-time start, queue order, and exception delivery.
// exercises: async-function-value, async-arrow, held-handle, array, field, global, try-catch
// questions: compiler.md §167, §70, §92, §94, §116, §118
// tsc: accepts; js-comparable: yes

async function value(n: i32): Promise<i32> { return n; }
async function twice(n: i32): Promise<i32> {
  print(`twice start ${n}`);
  await value(0);
  print(`twice end ${n}`);
  return n * 2;
}
const globalJob: (n: i32) => Promise<i32> = twice;
class Jobs {
  job: (n: i32) => Promise<i32> = twice;
}
async function invoke(job: (n: i32) => Promise<i32>, n: i32): Promise<i32> {
  await value(0);
  return await job(n);
}
async function turns(): Promise<void> {
  for (let i: i32 = 0; i < 8; i++) {
    print(`turn ${i}`);
    await value(0);
  }
}
async function work(): Promise<void> {
  print(`direct ${await (async (n: i32): Promise<i32> => {
    print(`direct start ${n}`);
    await value(0);
    return n + 1;
  })(10)}`);

  print(`parameter ${await invoke(async (n: i32): Promise<i32> => {
    print(`parameter start ${n}`);
    await value(0);
    return n + 2;
  }, 20)}`);

  const named: (n: i32) => Promise<i32> = twice;
  const held: Promise<i32> = named(30);
  print("after held call");
  print(`held ${await held}`);

  const jobs: ((n: i32) => Promise<i32>)[] = [twice];
  print(`array ${await jobs[0](40)}`);
  const owner = new Jobs();
  print(`field ${await owner.job(50)}`);
  print(`global ${await globalJob(60)}`);

  const inferred = async (n: i32) => n + 3;
  print(`inferred ${await inferred(70)}`);
  const contextual: (n: i32) => Promise<i32> = async (n) => {
    await value(0);
    return n + 4;
  };
  print(`contextual ${await contextual(80)}`);

  const fails = async (): Promise<i32> => {
    await value(0);
    throw new Error("arrow failure");
  };
  try { await fails(); }
  catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
  print("work done");
}
export async function main(): Promise<void> {
  const task: Promise<void> = work();
  const clock: Promise<void> = turns();
  await task;
  await clock;
}
