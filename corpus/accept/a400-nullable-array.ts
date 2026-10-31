// corpus: accept/a400-nullable-array
// purpose: A nullable array is a `Ref | null` form in each position, narrows as a class does, and has the `node` JSON result.
// exercises: nullable-array, nullable-parameter, nullable-field, null-narrowing, nullish-coalescing, nullish-assignment, optional-chain, generic-class, generic-function, Map, json, Context.collect
// questions: compiler.md §217, C7
// tsc: accepts; js-comparable: yes
class Point {
  x: i32 = 1;
}

class Holder {
  items: i32[] | null = null;
  names: string[] | null = ["a", "b"];
  n: i32 = 3;
}

class Box<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}

let shared: i32[] | null = null;

function sizeOf(values: i32[] | null): i32 {
  if (values === null) {
    return -1;
  }
  return values.length;
}

function maybe(n: i32): i32[] | null {
  if (n < 0) {
    return null;
  }
  return [n, n + 1];
}

function sizeOfAny<T>(values: T[] | null): i32 {
  return values?.length ?? -1;
}

function orEmpty(values: i32[] | null): i32[] {
  return values ?? [];
}

function fillShared(): void {
  shared = [7, 8, 9];
}

export function main(): void {
  // A parameter, a result, and a local.
  print(`${sizeOf(maybe(1))} ${sizeOf(maybe(-1))} ${sizeOf(null)}`);
  let local: i32[] | null = null;
  print(`${local === null} ${local == null}`);
  local = [4];
  print(`${local.length} ${local !== null}`);

  // A field, and a collection with a null field.
  const holder = new Holder();
  const filled = new Holder();
  filled.items = [1, 2, 3];
  Context.collect();
  print(`${holder.items === null} ${sizeOf(filled.items)}`);
  print(JSON.stringify(holder));
  print(JSON.stringify(filled));
  const parsed = JSON.parse<Holder>("{\"items\":null,\"names\":[\"q\"],\"n\":4}");
  print(`${parsed.items === null} ${parsed.names?.length ?? -1} ${parsed.n}`);

  // An array element and a Map value.
  const rows: (i32[] | null)[] = [[1], null, [2, 3]];
  let total = 0;
  for (const row of rows) {
    total += row?.length ?? 100;
  }
  print(`${total} ${rows[1] === null}`);
  const table = new Map<string, i32[] | null>();
  table.set("a", [1, 2]);
  table.set("b", null);
  print(`${table.size} ${table.has("b")}`);

  // A global.
  print(`${shared === null}`);
  fillShared();
  if (shared !== null) {
    print(`${shared.length}`);
  }
  shared = null;
  print(`${shared === null}`);

  // A generic argument, an inferred element, and an array of classes.
  const empty = new Box<i32[] | null>(null);
  const full = new Box<i32[] | null>([5, 6]);
  print(`${empty.value === null} ${sizeOf(full.value)}`);
  print(`${sizeOfAny<i32>(null)} ${sizeOfAny([1, 2])}`);
  const points: Point[] | null = [new Point()];
  print(`${sizeOfAny(points)} ${points[0].x}`);

  // `??`, `??=`, and `?.` with a member and a call.
  print(`${orEmpty(null).length} ${orEmpty([1]).length}`);
  let pending: i32[] | null = null;
  pending ??= [9];
  print(`${pending[0]}`);
  const text: i32[] | null = maybe(2);
  print(text?.join("+") ?? "none");
  text?.push(4);
  print(`${text?.length ?? -1}`);
}
