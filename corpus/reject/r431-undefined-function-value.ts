// corpus: reject/r431-undefined-function-value
// purpose: A function whose result is T | undefined is read as a value; it may only be called.
// exercises: undefined, function value
// questions: compiler.md §195
// tsc: accepts
// expected-error: S100, a function whose result is `T | undefined` may only be called
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}

export function main(): void {
  const f = pick;
  print(`${f() === undefined}`);
}
