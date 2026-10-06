// corpus: accept/a338-counted-array-holders
// purpose: An array owns each counted element across aliases and mutations.
// exercises: counted-array, async-handle, ownership
// questions: compiler.md §171, §70, §116
// tsc: accepts
// js-comparable: yes

async function work(value: i32): Promise<i32> { return value; }
let globalJobs: Promise<i32>[] = [];
function remove(jobs: Promise<i32>[]): Promise<i32> {
  const job: Promise<i32> = jobs[jobs.length - 1];
  jobs.pop();
  return job;
}
function identity(jobs: Promise<i32>[]): Promise<i32>[] { return jobs; }
async function use(jobs: Promise<i32>[]): Promise<void> {
  print(`async parameter ${await jobs[0]}`);
  jobs.pop();
}
async function makeHandle(): Promise<Promise<i32>> { return work(70); }
async function makeArray(): Promise<Promise<i32>[]> { return [work(80)]; }
export async function main(): Promise<void> {
  const jobs: Promise<i32>[] = [work(1), work(2)];
  print(`literal ${await jobs[0]} ${await jobs[1]}`);
  const alias: Promise<i32>[] = jobs;
  const removedAlias = remove(alias);
  print(`synchronous parameter ${await removedAlias}`);
  jobs.push(work(3));
  print(`push ${await jobs[1]}`);
  const shifted = jobs.shift();
  print(`shift ${await shifted}`);
  const live: Promise<i32> = work(4);
  jobs.unshift(live);
  print(`unshift ${await jobs[0]}`);
  jobs[1] = live;
  print(`index ${await jobs[1]}`);
  jobs.fill(live);
  print(`fill ${await jobs[0]} ${await jobs[1]}`);
  const preserved: Promise<i32> = work(41);
  {
    const replaced: Promise<i32> = work(5);
    const copied: Promise<i32>[] = [replaced, preserved];
    print(`before copy ${await replaced}`);
    copied.copyWithin(0, 1);
    print(`copyWithin ${await copied[0]} ${await preserved}`);
  }
  print(`reuse ${await work(99)}`);
  print(`live after copy ${await preserved}`);
  alias.push(live);
  print(`alias push ${await jobs[2]} ${await live}`);
  const receiver: Promise<i32>[] = jobs.reverse();
  print(`reverse ${await receiver[0]}`);
  const filled: Promise<i32>[] = jobs.fill(live, 0, 1);
  print(`fill receiver ${await filled[0]}`);
  const within: Promise<i32>[] = jobs.copyWithin(0, 1, 2);
  print(`copy receiver ${await within[0]}`);
  const sliced: Promise<i32>[] = jobs.slice(0, 1);
  const spread: Promise<i32>[] = [...sliced];
  const from: Promise<i32>[] = Array.from(spread);
  const of: Promise<i32>[] = Array.of(live);
  const combined: Promise<i32>[] = from.concat(of);
  print(`new arrays ${await combined[0]} ${await combined[1]}`);
  const removed: Promise<i32>[] = combined.splice(0, 1);
  print(`splice ${await removed[0]}`);
  combined.splice(0, 1);
  const read = jobs.at(0);
  print(`at ${await read}`);
  jobs.at(0);
  print(`length ${jobs.length}`);
  for (const job of jobs) { print(`for of ${await job}`); }
  const returned: Promise<i32>[] = identity(jobs);
  print(`return ${await returned[0]}`);
  globalJobs = jobs;
  print(`global ${await globalJobs[0]}`);
  globalJobs.pop();
  globalJobs = [];
  const parameter: Promise<i32>[] = [work(60)];
  await use(parameter);
  const nested: Promise<i32>[][] = [[work(61)]];
  const nestedAlias: Promise<i32>[] = nested[0];
  print(`nested ${await nestedAlias[0]}`);
  nestedAlias.pop();
  const fixed: FixedArray<Promise<i32>, 2> = [work(62), work(63)];
  const fixedCopy: FixedArray<Promise<i32>, 2> = fixed;
  print(`fixed ${await fixed[0]} ${await fixedCopy[1]}`);
  const outerHandle = makeHandle();
  const firstHandle = await outerHandle;
  print(`completion handle ${await firstHandle}`);
  const secondHandle = await outerHandle;
  print(`completion handle again ${await secondHandle}`);
  const outerArray = makeArray();
  const firstArray = await outerArray;
  print(`completion array ${await firstArray[0]}`);
  const secondArray = await outerArray;
  print(`completion array again ${await secondArray[0]}`);
  firstArray.pop();
  print("end");
}

// pin: 96f46f35
// pin-dev-jit: Exit 1; internal trap after reuse 99.
// pin-c-aot: Exit 3; internal trap after reuse 99.
// pin-interpreter: Complete golden stdout.
