// corpus: reject/r463-string-pop-needs-a-test
// purpose: A member read on a `pop` result of `string[]` needs a test for `undefined` first.
// exercises: nullable-string, undefined, Array.pop
// questions: compiler.md §219
// tsc: rejects TS18048
// expected-error: S011, may be null here
export function main(): void {
  const xs: string[] = ["a"];
  const last = xs.pop();
  print(`${last.length}`);
}
