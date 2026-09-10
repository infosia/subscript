// corpus: accept/a218-map-keys-to-array
// purpose: Collects the keys and the values of a Map into fresh arrays.
// observable: each array holds the Map in its insertion order.
// exercises: map-keys, map-values, for-of-views, array-push
// questions: Q30, compiler section 104
// tsc: accepts; js-comparable: yes
export function main(): void {
  const map: Map<i32, string> = new Map<i32, string>();
  map.set(7, "seven");
  map.set(8, "eight");

  const keys: i32[] = [];
  for (const key of map.keys()) {
    keys.push(key);
  }
  for (const key of keys) {
    print(`key:${key}`);
  }

  const values: string[] = [];
  for (const value of map.values()) {
    values.push(value);
  }
  for (const value of values) {
    print(`value:${value}`);
  }
}
