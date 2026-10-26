// corpus: reject/r423-undefined-fall-through
// purpose: A body end that control reaches in a function whose result is T | undefined is rejected.
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
}

export function main(): void {
  print(`${pick(false) === undefined}`);
}
