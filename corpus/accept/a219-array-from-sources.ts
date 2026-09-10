// corpus: accept/a219-array-from-sources
// purpose: Array.from over every accepted source, and the fresh result it returns.
// observable: each result keeps the source order, and a later write to the array source does not reach it.
// exercises: array-from, fixed-array, set, unicode-string, fresh-array, explicit-type-argument
// questions: Q22, compiler section 105
// tsc: accepts; js-comparable: yes
export function main(): void {
  const xs: i32[] = [1, 2, 3];
  const fromArray: i32[] = Array.from(xs);
  xs[0] = 99;
  xs.push(4);
  for (const value of fromArray) {
    print(`array:${value}`);
  }
  print(`array-length:${fromArray.length} source-length:${xs.length}`);
  print(`array-head:${fromArray[0]} source-head:${xs[0]}`);

  const fixed: FixedArray<i32, 2> = [5, 6];
  const fromFixed: i32[] = Array.from(fixed);
  for (const value of fromFixed) {
    print(`fixed:${value}`);
  }

  const set: Set<i32> = new Set<i32>();
  set.add(9);
  set.add(10);
  const fromSet: i32[] = Array.from(set);
  for (const value of fromSet) {
    print(`set:${value}`);
  }

  const fromString: string[] = Array.from("añ😀");
  print(`string-length:${fromString.length}`);
  for (const value of fromString) {
    print(`string:${value}`);
  }

  const empty: i32[] = [];
  const fromEmpty: i32[] = Array.from(empty);
  print(`empty:${fromEmpty.length}`);

  const typed: i32[] = Array.from<i32>([]);
  print(`typed:${typed.length}`);
}
