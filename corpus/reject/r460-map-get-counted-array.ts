// corpus: reject/r460-map-get-counted-array
// purpose: `Map.get` on a counted array has no owner for the alias, so a later `set` releases the handles that the alias holds.
// exercises: map-get, async-handle, counted-array
// questions: compiler.md §217
// tsc: accepts
// expected-error: S014, `get(key)` is rejected: The value type has no `| null` form of the map's value representation; use `getOr`
async function work(value: i32): Promise<i32> {
  return value;
}

export async function main(): Promise<void> {
  const table = new Map<string, Promise<i32>[]>();
  table.set("a", [work(1)]);
  const held = table.get("a");
  const replacement: Promise<i32>[] = [work(2)];
  table.set("a", replacement);
  if (held !== undefined) {
    print(`${await held[0]}`);
  }
  print(`${await replacement[0]}`);
}
