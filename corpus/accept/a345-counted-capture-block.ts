// corpus: accept/a345-counted-capture-block
// purpose: Counted captures stay in the block of each captured binding.
// exercises: capture-block, counted-binding, local-flow
// questions: compiler.md §175, C5
// tsc: accepts
// js-comparable: yes

class Box { v: i32 = 5; }
async function work(n: i32): Promise<i32> { return n; }
async function call(g: () => Promise<i32>): Promise<i32> { return await g(); }
async function param(h: Promise<i32>): Promise<i32> {
  const p = h;
  let f: () => Promise<i32> = () => work(0);
  { f = () => p; }
  return await f();
}
export async function main(): Promise<void> {
  {
    const h = work(1);
    let f: () => Promise<i32> = () => h;
    const g = () => h;
    f = g;
    const k = () => g();
    let m: () => Promise<i32> = k;
    print(`${await f()} ${await m()} ${await call(k)}`);
  }
  for (let i: i32 = 0; i < 2; i++) {
    const h = work(i);
    let f: () => Promise<i32> = () => work(9);
    f = () => h;
    print(`${await f()}`);
  }
  const outer = work(2);
  let f2: () => Promise<i32> = () => work(0);
  { f2 = () => outer; }
  print(`${await f2()}`);
  let fi: () => i32 = () => 0;
  let fb: () => i32 = () => 0;
  { const x: i32 = 3; const b = new Box(); fi = () => x; fb = () => b.v; }
  print(`${fi()} ${fb()} ${await param(work(4))}`);
}

// pin: d19ed912
// pin-dev-jit: Exit 0; output matches the golden.
// pin-c-aot: Exit 0; output matches the golden.
// pin-interpreter: Output matches the golden.
