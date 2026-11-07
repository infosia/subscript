// corpus: accept/a432-a-const-local-function-value-takes-an-adapter-that-captures-it
// purpose: A const local or a module const of a function type takes the adapter of its place, and the adapter captures the local by value.
// observable: each call prints, so the number of calls and the captured value are visible.
// exercises: function-value, lambda, capture, Map, forEach, conditional, async-function, await
// questions: compiler.md §246, §243, §240
// tsc: accepts; js-comparable: yes
type CB1 = (x: i32) => void;
type CB2 = (x: i32, y: i32) => void;

const G: CB1 = (x: i32): void => {
  print(`G ${x}`);
};

function run(cb: CB2): void {
  cb(1, 2);
}

function both(x: i32, y: i32): void {
  print(`both ${x} ${y}`);
}

function pick(b: boolean): CB1 | null {
  return b
    ? (x: i32): void => {
        print(`picked ${x}`);
      }
    : null;
}

async function step(): Promise<i32> {
  return 0;
}

async function later(): Promise<void> {
  const k2: i32 = 50;
  const fa = (x: i32): void => {
    print(`fa ${x + k2}`);
  };
  await step();
  run(fa);
  await step();
  const h: CB2 = fa;
  h(3, 4);
}

export async function main(): Promise<void> {
  const k: i32 = 10;
  const b: boolean = true;
  const fv = (x: i32): void => {
    print(`fv ${x + k}`);
  };
  const h: CB2 = fv;
  h(5, 6);
  run(fv);
  const fr = (x: i32): i32 => {
    print(`fr ${x + k}`);
    return x * 2;
  };
  const h1: CB1 = fr;
  h1(7);
  const m = new Map<string, i32>();
  m.set("a", 1);
  m.set("b", 2);
  m.forEach(fv);
  run(G);
  const plain = (x: i32): void => {
    print(`plain ${x}`);
  };
  run(plain);
  run(b ? fv : both);
  [8, 9].forEach(b ? fv : both);
  const handlers = new Map<string, CB1>();
  handlers.set("h", (x: i32): void => {
    print(`handler ${x}`);
  });
  const found = handlers.get("h");
  if (found !== undefined) {
    [20, 21].forEach(found);
  }
  const picked: CB1 | null = pick(true);
  if (picked !== null) {
    run(picked);
    const hp: CB2 = picked;
    hp(22, 23);
  }
  await later();
}
