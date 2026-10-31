// corpus: accept/a408-fixed-array-traversal-forms
// purpose: A traversal of a place works with a handle that only the traversal keeps, with a pattern, with continue, break, return, and try in the body, with an inner traversal, at the top level, and with each element kind of the callback methods.
// exercises: for-of, fixed-array, destructuring, try-catch, continue, break, Context.collect, forEach, map, filter, reduce, reduceRight, function-value
// questions: compiler.md §220.1 rules 1, 2, 10
// tsc: accepts; js-comparable: yes
class C { n: i32; constructor(n: i32) { this.n = n; } }
class R {
  names: FixedArray<string, 2> = ["a", "b"];
  cs: FixedArray<C, 2> = [new C(1), new C(2)];
}
function make(): R { return new R(); }
class P { x: i32 = 1; y: i32 = 2; }
function pts(): FixedArray<P, 2> { return [new P(), new P()]; }
let counter: i32 = 0;
function next(): i32 { counter += 1; return 0; }

function temporaryHandle(): void {
  for (const x of make().names) { Context.collect(); print(x); }
  for (const c of make().cs) { Context.collect(); print(`${c.n}`); }
}
function patterns(): void {
  const ps = pts();
  ps[1].x = 8;
  for (const { x, y } of ps) { ps[1].y = 9; print(`${x} ${y}`); }
  const grid: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];
  for (const [a, b] of grid) { print(`${a} ${b}`); }
}
function control(): i32 {
  const b: FixedArray<i32, 4> = [1, 2, 3, 4];
  let t: i32 = 0;
  for (const x of b) {
    if (x === 1) { continue; }
    if (x === 3) { break; }
    try { if (x === 2) { throw new Error("e"); } } catch (e) { t += 10; }
    t += x;
  }
  for (const x of b) { if (x === 4) { return t + x; } }
  return t;
}
function inner(): void {
  const b: FixedArray<i32, 2> = [1, 2];
  for (const x of b) {
    for (const y of b) { b[1] = 5; print(`${x} ${y}`); }
    let i: i32 = 0;
    while (i < 2) { i += 1; }
  }
}
function viaIndexFn(): void {
  const m: FixedArray<FixedArray<i32, 2>, 2> = [[1, 2], [3, 4]];
  for (const x of m[next()]) { print(`${x}`); }
  print(`${counter}`);
  m[0].forEach((x: i32): void => { print(`${x}`); });
}
function throwing(): void {
  const b: FixedArray<i32, 2> = [1, 2];
  try {
    b.forEach((x: i32): void => { if (x === 2) { throw new Error("cb"); } print(`${x}`); });
  } catch (e) { print("caught"); }
}
function str(): void {
  const b: FixedArray<string, 2> = ["q", "r"];
  print(b.map((s: string, i: i32): string => s + `${i}`).join(","));
  print(b.filter((s: string): boolean => s === "r").join(","));
  print(b.reduce((a: string, s: string): string => a + s, ""));
  print(b.reduceRight((a: string, s: string, i: i32): string => a + s + `${i}`, "<"));
  const f = (a: string, s: string): string => a + s;
  print(b.reduceRight(f, ">"));
  const n: FixedArray<i32, 2> = [1, 2];
  const widened: i64 = n.reduce((a: i64, x: i32): i64 => a + (x as i64), 0 as i64);
  print(`${widened}`);
}
const tb: FixedArray<i32, 2> = [1, 2];
for (const x of tb) { tb[1] = 3; print(`top ${x}`); }
export function main(): void {
  temporaryHandle();
  patterns();
  print(`${control()}`);
  inner();
  viaIndexFn();
  throwing();
  str();
}
