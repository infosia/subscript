// corpus: accept/a81-array-literal-spread
// purpose: P22 array-literal spread battery over every spreadable container.
// observable: fresh arrays preserve prefix/suffix and fused traversal order.
// exercises: array-spread, multi-spread, fixed-array, map-keys, set, unicode-string
// questions: Q30
// tsc: accepts
export function main(): void {
  const xs: i32[] = [1, 2];
  const ys: i32[] = [3, 4];
  const copy: i32[] = [...xs];
  const framed: i32[] = [0, ...xs, 9];
  const joined: i32[] = [...xs, ...ys];
  for (const value of copy) {
    print(`copy:${value}`);
  }
  for (const value of framed) {
    print(`framed:${value}`);
  }
  for (const value of joined) {
    print(`joined:${value}`);
  }

  const fixed: FixedArray<i32, 2> = [5, 6];
  const fixedCopy: i32[] = [...fixed];
  for (const value of fixedCopy) {
    print(`fixed:${value}`);
  }

  const map: Map<i32, string> = new Map<i32, string>();
  map.set(7, "seven");
  map.set(8, "eight");
  const mapKeys = [...map];
  for (const key of mapKeys) {
    print(`map:${key}`);
  }

  const set: Set<i32> = new Set<i32>();
  set.add(9);
  set.add(10);
  const setValues: i32[] = [...set];
  for (const value of setValues) {
    print(`set:${value}`);
  }

  const chars: string[] = [..."é🙂"];
  for (const value of chars) {
    print(`string:${value}`);
  }
}
