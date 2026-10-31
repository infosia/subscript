// corpus: accept/a407-fixed-array-traversal-suspension
// purpose: A traversal of a place of string elements reads the place after a yield and after an await, and the kept operands survive each suspension.
// exercises: for-of, fixed-array, generator, async, await, yield, Context.suspend, Context.collect, field, element-place, forEach
// questions: compiler.md §220.1 rules 2, 3, 11
// tsc: accepts; js-comparable: yes
class Names {
  a: FixedArray<string, 2> = ["a", "b"];
}

function built(a: string, b: string): string {
  return a + b;
}

function* walkField(r: Names): Generator<string> {
  for (const x of r.a) {
    yield x;
    Context.collect();
    r.a[1] = built("y", "ield");
  }
}

function* walkElement(m: FixedArray<string, 2>[]): Generator<string> {
  let i: i32 = 0;
  for (const x of m[i]) {
    yield x;
    i = 1;
    m[0][1] = built("e", "lement");
  }
}

function* walkLocal(): Generator<string> {
  const b: FixedArray<string, 2> = [built("l", "1"), built("l", "2")];
  for (const x of b) {
    yield x;
    b[1] = built("lo", "cal");
  }
}

async function awaitField(r: Names): Promise<void> {
  for (const x of r.a) {
    await Context.suspend();
    Context.collect();
    print(x);
    r.a[1] = built("aw", "ait");
  }
}

async function awaitElement(m: FixedArray<string, 2>[]): Promise<void> {
  for (const x of m[0]) {
    await Context.suspend();
    Context.collect();
    print(x);
    m[0][1] = built("el", "ement");
  }
}

async function awaitLocal(): Promise<void> {
  const b: FixedArray<string, 2> = [built("l", "3"), built("l", "4")];
  let last: string = "";
  for (const x of b) {
    await Context.suspend();
    Context.collect();
    last = x;
    print(x);
    b[1] = built("lo", "cal");
  }
  print(last);
}

async function tick(): Promise<i32> {
  return 4;
}

async function counted(): Promise<void> {
  const hs: FixedArray<Promise<i32>, 2> = [tick(), tick()];
  for (const h of hs) {
    print(`${await h}`);
  }
  let t: FixedArray<Promise<i32>, 1> = [tick()];
  for (const h of t) {
    t = [tick()];
    print(`${await h}`);
  }
  print(`${await t[0]}`);
}

export async function main(): Promise<void> {
  for (const v of walkField(new Names())) {
    print(v);
  }
  const m: FixedArray<string, 2>[] = [["p", "q"], ["r", "s"]];
  for (const v of walkElement(m)) {
    print(v);
  }
  for (const v of walkLocal()) {
    print(v);
  }
  await awaitField(new Names());
  await awaitElement([["t", "u"]]);
  await awaitLocal();
  await counted();
}
