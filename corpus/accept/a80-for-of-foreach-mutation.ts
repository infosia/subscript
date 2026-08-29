// corpus: accept/a80-for-of-foreach-mutation
// purpose: Mutation traversal follows the bound rule of each source spelling.
// observable: removals shorten each visit; Map appends extend for-of and forEach traversal.
// exercises: for-of-foreach-parity, mutation-during-iteration, live-association-bound
// questions: Q30
// tsc: accepts; js-comparable: yes
function visitArray(value: i32): void {
  print(`array:${value}`);
}

export function main(): void {
  const plain: i32[] = [1, 2, 3];
  plain.forEach(visitArray);
  for (const value of plain) {
    visitArray(value);
  }

  const viaCallback: i32[] = [1, 2, 3, 4];
  viaCallback.forEach((value: i32): void => {
    print(`mut-array:${value}`);
    if (value === 1) {
      viaCallback.pop();
      viaCallback.pop();
    }
  });
  const viaLoop: i32[] = [1, 2, 3, 4];
  for (const value of viaLoop) {
    print(`mut-array:${value}`);
    if (value === 1) {
      viaLoop.pop();
      viaLoop.pop();
    }
  }

  const callbackMap: Map<i32, string> = new Map<i32, string>();
  callbackMap.set(1, "one");
  callbackMap.set(2, "two");
  callbackMap.set(3, "three");
  callbackMap.forEach((value: string, key: i32): void => {
    print(`mut-map:${key}`);
    if (key === 1) {
      callbackMap.delete(2);
      callbackMap.set(4, "four");
    }
  });

  const loopMap: Map<i32, string> = new Map<i32, string>();
  loopMap.set(1, "one");
  loopMap.set(2, "two");
  loopMap.set(3, "three");
  for (const key of loopMap.keys()) {
    print(`mut-map:${key}`);
    if (key === 1) {
      loopMap.delete(2);
      loopMap.set(4, "four");
    }
  }
}
