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
  const h: A = gen();
  const f = (): i32 => use2(h, pick(g));
  let s: i32 = 0;
  for (let r: i32 = 0; r < 10000000; r++) {
    s += f();
  }
  print(`${s}`);
}
