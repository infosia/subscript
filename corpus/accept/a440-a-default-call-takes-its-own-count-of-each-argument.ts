// corpus: accept/a440-a-default-call-takes-its-own-count-of-each-argument
// purpose: A call that passes a counted value to a default function and to the callee gives each pass a count of its own.
// observable: each call prints its result, so each form runs to its end.
// exercises: async-function, await, default-parameter, generator, method, held-handle
// questions: compiler.md §256, §57
// tsc: accepts; js-comparable: yes
async function v(): Promise<i32> {
  return 1;
}

function* gen(): Generator<i32> {
  yield 10;
}

async function w(p: Promise<i32>, k: i32 = 2): Promise<i32> {
  return (await p) + k;
}

class C {
  async w(p: Promise<i32>, k: i32 = 2): Promise<i32> {
    return (await p) + k;
  }
}

async function all(xs: Promise<i32>[] = [], k: i32 = 0): Promise<i32> {
  let total = k;
  for (const h of xs) {
    total += await h;
  }
  return total;
}

function take(g: Generator<i32>, k: i32 = 1): i32 {
  let total = k;
  for (const n of g) {
    total += n;
  }
  return total;
}

function take3(g: Generator<i32>, k: i32 = 1, m: i32 = 2): i32 {
  let total = k + m;
  for (const n of g) {
    total += n;
  }
  return total;
}

async function cnt(xs: Promise<i32>[], n: i32 = xs.length): Promise<i32> {
  let total = n;
  for (const h of xs) {
    total += await h;
  }
  return total;
}

export async function main(): Promise<void> {
  print(`w ${await w(v())}`);
  const c = new C();
  print(`method ${await c.w(v())}`);
  const ps: Promise<i32>[] = [];
  ps.push(w(v()));
  for (const p of ps) {
    print(`push ${await p}`);
  }
  print(`all ${await all()}`);
  print(`all empty ${await all([])}`);
  print(`all one ${await all([v()])}`);
  print(`take ${take(gen())}`);
  print(`take3 ${take3(gen(), 5)}`);
  print(`cnt ${await cnt([v(), v()])}`);
}
