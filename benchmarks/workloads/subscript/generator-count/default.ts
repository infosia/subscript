type A = Generator<i32>;

function* gen(): Generator<i32> {
  yield 1;
}

function pick(x: A): A {
  return x;
}

function take(a: A, k: i32 = 1): i32 {
  return k;
}

export function main(): void {
  const g: A = gen();
  let s: i32 = 0;
  for (let r: i32 = 0; r < 10000000; r++) {
    s += take(pick(g));
  }
  print(`${s}`);
}
