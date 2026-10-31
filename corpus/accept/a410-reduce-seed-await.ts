// corpus: accept/a410-reduce-seed-await
// purpose: reduce and reduceRight with a lambda accept await in the seed, on a FixedArray receiver and on a dynamic array receiver.
// exercises: reduce, reduceRight, fixed-array, async, await, lambda, function-value
// questions: compiler.md §220.1 rule 10b
// tsc: accepts; js-comparable: yes
class R {
  a: FixedArray<i32, 2> = [1, 2];
}

async function seed(): Promise<i32> {
  await Context.suspend();
  return 100;
}

function pair(): FixedArray<i32, 2> {
  return [3, 4];
}

function plus(q: i32, x: i32): i32 {
  return q + x;
}

export async function main(): Promise<void> {
  const r = new R();
  print(`${r.a.reduce((q: i32, x: i32): i32 => q + x, await seed())}`);
  print(`${r.a.reduceRight((q: i32, x: i32): i32 => q * 2 + x, await seed())}`);
  print(`${pair().reduce((q: i32, x: i32): i32 => q + x, await seed())}`);
  const d: i32[] = [1, 2];
  print(`${d.reduce((q: i32, x: i32): i32 => q + x, await seed())}`);
  print(`${d.reduceRight((q: i32, x: i32, i: i32): i32 => q * 2 + x + i, await seed())}`);
  print(`${r.a.reduce(plus, await seed())}`);
  print(`${d.reduce(plus, await seed())}`);
  const f: (q: i32, x: i32) => i32 = plus;
  print(`${r.a.reduce(f, await seed())}`);
  print(`${d.reduce(f, await seed())}`);
}
