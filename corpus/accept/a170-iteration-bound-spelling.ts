// corpus: accept/a170-iteration-bound-spelling
// purpose: The source spelling selects a live or fixed iteration bound.
// observable: for-of and Map.forEach visit appends; Array.forEach does not visit an append.
// exercises: live-for-of-bound, live-map-foreach-bound, fixed-array-foreach-bound
// questions: Q30
// tsc: accepts; js-comparable: yes
export function main(): void {
  const loopValues: i32[] = [1, 2, 3];
  for (const value of loopValues) {
    print(`for-of:${value}`);
    if (loopValues.length < 6) {
      loopValues.push(loopValues.length + 1);
    }
  }

  const mapValues: Map<i32, string> = new Map<i32, string>();
  mapValues.set(1, "one");
  mapValues.set(2, "two");
  mapValues.forEach((value: string, key: i32): void => {
    print(`map-for-each:${key}:${value}`);
    if (key === 1) {
      mapValues.set(3, "three");
    }
  });

  const callbackValues: i32[] = [1, 2, 3];
  callbackValues.forEach((value: i32): void => {
    print(`array-for-each:${value}`);
    if (value === 1) {
      callbackValues.push(4);
    }
  });
}
