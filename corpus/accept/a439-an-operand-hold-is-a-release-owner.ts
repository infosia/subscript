// corpus: accept/a439-an-operand-hold-is-a-release-owner
// purpose: An operand of a counted type that a later operand with user code follows is held to the end of its expression, on the normal path and on each exception edge.
// observable: each call prints its result or "caught", so each path runs to its end.
// exercises: async-function, await, held-handle, generator, for-of, lambda, exception, default-parameter, index
// questions: compiler.md §255, §236
// tsc: accepts; js-comparable: yes
async function v(): Promise<i32> {
  return 1;
}

async function two(a: Promise<i32>, b: Promise<i32>): Promise<i32> {
  return (await a) + (await b);
}

function boom(): Promise<i32> {
  throw new Error("boom");
}

async function bad(): Promise<i32> {
  throw new Error("bad");
}

function arr(): Promise<i32>[] {
  return [v()];
}

async function takeArr(xs: Promise<i32>[], b: Promise<i32>): Promise<i32> {
  let total = await b;
  for (const h of xs) {
    total += await h;
  }
  return total;
}

function* gen(): Generator<i32> {
  yield 10;
  yield 20;
}

function* gens(): Generator<Generator<i32>> {
  yield gen();
}

async function takeGen(g: Generator<i32>, b: Promise<i32>): Promise<i32> {
  let total = await b;
  for (const n of g) {
    total += n;
  }
  return total;
}

function start(i: i32): Promise<i32> {
  if (i > 1) {
    throw new Error("start");
  }
  return v();
}

function it(i: i32): i32 {
  if (i > 5) {
    throw new Error("it");
  }
  return i;
}

function idx(): i32 {
  return 0;
}

function boomi(): i32 {
  throw new Error("boomi");
}

async function withDefault(a: Promise<i32>, b: i32 = boomi()): Promise<i32> {
  return (await a) + b;
}

export async function main(): Promise<void> {
  print(`plain ${await two(v(), v())}`);
  try {
    print(`throws ${await two(v(), boom())}`);
  } catch (e) {
    print("throws caught");
  }
  try {
    print(`rejects ${await two(bad(), v())}`);
  } catch (e) {
    print("rejects caught");
  }
  const a = v();
  const b = v();
  print(`held ${await two(a, b)}`);
  print(`array ${await takeArr(arr(), v())}`);
  print(`generator ${await takeGen(gen(), v())}`);
  const hs = [v()];
  for (const h of hs) {
    print(`for-of ${await two(h, v())}`);
  }
  for (const g of gens()) {
    print(`generator const ${await takeGen(g, v())}`);
  }
  for (let g of gens()) {
    print(`generator let ${await takeGen(g, v())}`);
  }
  const c = v();
  const f = (): Promise<i32> => two(c, boom());
  try {
    print(`capture ${await f()}`);
  } catch (e) {
    print("capture caught");
  }
  print(`after ${await c}`);
  let tasks: Promise<i32>[] = [];
  try {
    for (let i = 0; i < 4; i++) {
      tasks.push(start(i));
    }
  } catch (e) {
    print(`push caught ${tasks.length}`);
  }
  for (const t of tasks) {
    print(`task ${await t}`);
  }
  let ps: Promise<i32>[] = [v()];
  try {
    print(`read ${await ps[it(9)]}`);
  } catch (e) {
    print("read caught");
  }
  try {
    ps[idx()] = boom();
  } catch (e) {
    print("store caught");
  }
  print(`ps ${await ps[it(0)]}`);
  try {
    print(`default ${await withDefault(v())}`);
  } catch (e) {
    print("default caught");
  }
  let fas: FixedArray<Promise<i32>, 2>[] = [[v(), v()]];
  for (const h of fas[it(0)]) {
    print(`fixed ${await h}`);
  }
  try {
    for (const h of fas[it(9)]) {
      print(`fixed ${await h}`);
    }
  } catch (e) {
    print("fixed caught");
  }
}
