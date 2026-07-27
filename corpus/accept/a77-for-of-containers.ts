// corpus: accept/a77-for-of-containers
// purpose: P22 closed-container for-of battery, including Unicode code points.
// observable: each accepted subject prints its values in the contracted order.
// exercises: for-of, arrays, fixed-arrays, map-keys, set-order, unicode-code-points

export function main(): void {
  const array: i32[] = [1, 2, 3];
  for (const value of array) {
    print(`array:${value}`);
  }

  const fixed: FixedArray<i32, 2> = [4, 5];
  for (const value of fixed) {
    print(`fixed:${value}`);
  }

  const map: Map<i32, string> = new Map<i32, string>();
  map.set(7, "seven");
  map.set(8, "eight");
  for (const key of map) {
    print(`map:${key}`);
  }

  const set: Set<i32> = new Set<i32>();
  set.add(9);
  set.add(10);
  for (const value of set) {
    print(`set:${value}`);
  }

  const text: string = "Aé🙂";
  for (const codePoint of text) {
    print(`string:${codePoint}`);
  }
}
