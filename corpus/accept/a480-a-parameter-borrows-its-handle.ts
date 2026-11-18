// corpus: accept/a480-a-parameter-borrows-its-handle
// purpose: Parameters borrow handles, and yield and built-in method results transfer their origins.
// observable: Each form prints its result after each caller awaits every handle.
// exercises: async-function, await, parameter, default-parameter, generator, method, lambda, array
// questions: compiler.md §329, §188, §257, §317
// tsc: accepts; js-comparable: yes
async function work(v: i32): Promise<i32> { return v; }
async function many(): Promise<Promise<i32>[]> { return [work(1), work(2)]; }
function count(xs: Promise<i32>[]): i32 { return xs.length; }
class Counter {
  n: i32;
  constructor(jobs: Promise<i32>[]) { this.n = jobs.length; }
  count(jobs: Promise<i32>[]): i32 { return jobs.length; }
}
function* gen(): Generator<Promise<i32>> { yield work(1); yield work(2); }
async function waitAll(xs: Promise<i32>[]): Promise<i32> {
  return (await Promise.all(xs)).length;
}
async function earlier(p: Promise<i32>, q: Promise<i32> = p): Promise<i32> { return await q; }
async function choose(c: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { return await (c ? p : q); }
async function defaulted(p: Promise<i32> = work(7)): Promise<i32> { return await p; }
export async function main(): Promise<void> {
  const xs = [work(1), work(2)];
  print(`function ${count(xs)} ${(await Promise.all(xs)).length}`);
  const jobs = [work(3), work(4)];
  const c = new Counter(jobs);
  print(`constructor ${c.n} ${(await Promise.all(jobs)).length}`);
  let total: i32 = 0;
  for (const h of gen()) { total += await h; }
  print(`yield ${total}`);
  const ps = (await many()).slice(0);
  print(`slice ${(await Promise.all(ps)).length}`);
  print(`control ${await waitAll([work(1), work(2)])}`);
  const methodJobs = [work(5)];
  print(`method ${c.count(methodJobs)} ${(await Promise.all(methodJobs)).length}`);
  const lambda = (handles: Promise<i32>[]): i32 => handles.length;
  const f: (handles: Promise<i32>[]) => i32 = lambda;
  const lambdaJobs = [work(6)];
  print(`lambda ${f(lambdaJobs)} ${(await Promise.all(lambdaJobs)).length}`);
  const p = work(8);
  const q = work(9);
  print(`earlier ${await earlier(p, q)} ${await p} ${await q}`);
  const omitted = work(12);
  print(`earlier-default ${await earlier(omitted)} ${await omitted}`);
  const a = work(10);
  const b = work(11);
  print(`conditional ${await choose(true, a, b)} ${await a} ${await b}`);
  print(`default ${await defaulted()}`);
}
