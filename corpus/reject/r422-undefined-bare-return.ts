// corpus: reject/r422-undefined-bare-return
// purpose: A bare return in a function whose result is T | undefined is rejected.
// exercises: undefined, return
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, a function whose result is `T | undefined` returns with `return undefined;`
class Box {
  v: i32 = 1;
}
function pick(f: boolean): Box | undefined {
  if (f) {
    return new Box();
  }
  return;
}

export function main(): void {
  print(`${pick(false) === undefined}`);
}
