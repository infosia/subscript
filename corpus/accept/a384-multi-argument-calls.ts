// corpus: accept/a384-multi-argument-calls
// purpose: push, Math.max, Math.min, and Array.of take any fixed argument count; push evaluates every argument before the first append.
// exercises: array-push, math-intrinsics, array-of, evaluation-order
// questions: compiler.md §201
// tsc: accepts; js-comparable: yes
class Item {
  id: i32;
  constructor(id: i32) {
    this.id = id;
  }
}

let reads: i32 = 0;

function next(v: i32): i32 {
  reads += 1;
  return v;
}

let picks: i32 = 0;

function pick(a: i32[], b: i32[]): i32[] {
  picks += 1;
  return a;
}

export function main(): void {
  const xs: i32[] = [1];
  const n = xs.push(xs.length, xs.length);
  print(`${n} ${xs.join(",")}`);

  const ys: i32[] = [];
  print(`${ys.push()} ${ys.push(next(5), next(6), next(7))} ${reads} ${ys.join(",")}`);

  const other: i32[] = [];
  pick(ys, other).push(8, 9);
  print(`${picks} ${ys.join(",")} ${other.length}`);

  const items: Item[] = [];
  items.push(new Item(1), new Item(2), new Item(3));
  let sum: i32 = 0;
  for (const item of items) {
    sum += item.id;
  }
  print(`${items.length} ${sum}`);

  print(`${Math.max()} ${Math.min()} ${Math.max(2.5)} ${Math.min(-1.5)}`);
  print(`${Math.max(1, 2)} ${Math.min(1, 2)} ${Math.max(3, 9, -4, 7)} ${Math.min(3, 9, -4, 7)}`);
  print(`${Math.max(1, NaN, 3)} ${Math.min(NaN, 1, 2)}`);
  print(`${1 / Math.max(-0, 0, -0)} ${1 / Math.min(0, -0, 0)} ${1 / Math.max(-0)}`);

  const zs = Array.of(4, 5, 6);
  const ws: string[] = Array.of("a", "b");
  print(`${zs.length} ${zs.join(",")} ${ws.join("")}`);
}
