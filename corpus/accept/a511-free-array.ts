// corpus: accept/a511-free-array
// purpose: Context.free releases arrays of numbers, references, a grown array, an empty array, and an array in a field.
// exercises: Context.free, dynamic-array, manual-free, array-growth
// questions: Q4, Q6, compiler.md §358
// tsc: accepts; js-comparable: no Q6: The Context memory API has no JavaScript shim.
class Point {
  x: i32;
  constructor(x: i32) {
    this.x = x;
  }
}

class Holder {
  values: i32[];
  constructor(values: i32[]) {
    this.values = values;
  }
}

function sum(values: i32[]): i32 {
  let total: i32 = 0;
  for (const value of values) {
    total += value;
  }
  return total;
}

export function main(): void {
  const numbers: f64[] = [1.5, 2.5, 3.0];
  print(`${numbers.length} ${numbers[2]}`);
  Context.free(numbers);

  const first: Point = new Point(3);
  const second: Point = new Point(4);
  const points: Point[] = [first, second];
  print(`${points[0].x + points[1].x}`);
  Context.free(points);
  print(`${first.x} ${second.x}`);

  const grown: i32[] = [];
  for (let i: i32 = 0; i < 100; i++) {
    grown.push(i);
  }
  print(`${grown.length} ${sum(grown)}`);
  Context.free(grown);

  const empty: string[] = [];
  print(`${empty.length}`);
  Context.free(empty);

  const holder: Holder = new Holder([5, 6, 7]);
  print(`${sum(holder.values)}`);
  Context.free(holder.values);
  Context.free(holder);
  Context.free(first);
  Context.free(second);
  print("done");
}
