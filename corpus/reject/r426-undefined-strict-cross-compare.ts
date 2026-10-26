// corpus: reject/r426-undefined-strict-cross-compare
// purpose: A strict comparison of two values with different absence spellings is rejected; node gives false.
// exercises: undefined, equality
// questions: compiler.md §195
// tsc: accepts
// expected-error: S012, `===` compares a `null` and an `undefined` absence; use `==`
class Box {
  v: i32 = 1;
}
function pick(): Box | undefined {
  return undefined;
}

export function main(): void {
  const n: Box | null = null;
  print(`${pick() === n}`);
}
