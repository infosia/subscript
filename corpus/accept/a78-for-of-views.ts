// corpus: accept/a78-for-of-views
// purpose: P22 subject-only keys()/values() fusion on Array, Map, and Set.
// observable: views print indices, keys, or values without becoming iterator values.
// exercises: for-of-views, array-keys-values, map-keys-values, set-keys-values
// questions: Q30
// tsc: accepts
export function main(): void {
  const value: string = "outer";
  const array: i32[] = [11, 12];
  for (const index of array.keys()) {
    print(`array-key:${index}`);
  }
  for (const value of array.values()) {
    print(`array-value:${value}`);
  }

  const map: Map<i32, string> = new Map<i32, string>();
  map.set(1, "one");
  map.set(2, "two");
  for (const key of map.keys()) {
    print(`map-key:${key}`);
  }
  for (const value of map.values()) {
    print(`map-value:${value}`);
  }

  const set: Set<i32> = new Set<i32>();
  set.add(21);
  set.add(22);
  for (const key of set.keys()) {
    print(`set-key:${key}`);
  }
  for (const value of set.values()) {
    print(`set-value:${value}`);
  }
  print(`after:${value}`);
}
