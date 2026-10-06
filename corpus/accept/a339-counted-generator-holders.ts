// corpus: accept/a339-counted-generator-holders
// purpose: Yielded counted values survive normal generator completion.
// exercises: counted-array, async-handle, ownership, generator
// questions: compiler.md §171, §70, §116
// tsc: accepts
// js-comparable: yes

async function work(value: i32): Promise<i32> { return value; }
let stored: Promise<i32>[] = [];
function consume(job: Promise<i32>): void { stored.push(job); }
function* handle(): Generator<Promise<i32>> {
  const job = work(7);
  consume(job);
  stored.pop();
  yield job;
}
function* array(): Generator<Promise<i32>[]> {
  const jobs: Promise<i32>[] = [work(8)];
  consume(jobs[0]);
  stored.pop();
  yield jobs;
}
function* nested(): Generator<Promise<i32>[][]> {
  const jobs: Promise<i32>[][] = [[work(9)]];
  consume(jobs[0][0]);
  stored.pop();
  yield jobs;
}
function* fixed(): Generator<FixedArray<Promise<i32>, 1>> {
  const jobs: FixedArray<Promise<i32>, 1> = [work(10)];
  consume(jobs[0]);
  stored.pop();
  yield jobs;
}
export async function main(): Promise<void> {
  const h = handle();
  const first = h.next();
  if (!first.done) {
    print(`handle before ${await first.value}`);
    print(`handle done ${h.next().done}`);
    print(`reuse ${await work(99)}`);
    print(`handle after ${await first.value}`);
  }
  const a = array();
  const second = a.next();
  if (!second.done) {
    print(`array before ${await second.value[0]}`);
    print(`array done ${a.next().done}`);
    print(`reuse ${await work(99)}`);
    print(`array after ${await second.value[0]}`);
  }
  const n = nested();
  const third = n.next();
  if (!third.done) {
    print(`nested before ${await third.value[0][0]}`);
    print(`nested done ${n.next().done}`);
    print(`reuse ${await work(99)}`);
    print(`nested after ${await third.value[0][0]}`);
  }
  const f = fixed();
  const fourth = f.next();
  if (!fourth.done) {
    print(`fixed before ${await fourth.value[0]}`);
    print(`fixed done ${f.next().done}`);
    print(`reuse ${await work(99)}`);
    print(`fixed after ${await fourth.value[0]}`);
  }
  for (const job of handle()) { print(`for of ${await job}`); }
}

// node: Exit 0; stdout matches the complete golden.

// pin: 96f46f35
// pin-dev-jit: Exit 1; internal trap after reuse 99 in the handle probe.
// pin-c-aot: Exit 3; internal trap after the same output.
// pin-interpreter: expected async handle, found Null after handle before 7.
