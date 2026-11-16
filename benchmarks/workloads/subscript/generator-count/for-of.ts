type A = Generator<i32>;

function* gen(): Generator<i32> {
  yield 1;
}

function pick(x: A): A {
  return x;
}

function use2(a: A, b: A): i32 {
  return 1;
}

export function main(): void {
  const g: A = gen();
  const hs: A[] = [];
  for (let i: i32 = 0; i < 1000; i++) {
    hs.push(g);
  }
  let s: i32 = 0;
  for (let r: i32 = 0; r < 10000; r++) {
    for (const h of hs) {
      s += use2(h, pick(g));
    }
  }
  print(`${s}`);
}
