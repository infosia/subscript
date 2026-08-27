// corpus: reject/r96-new-promise
// purpose: Rejects construction of Promise objects.
// exercises: Promise-constructor, async-boundary
// questions: Q34, C8
// tsc: accepts
// expected-error: S013 at the `new Promise` expression
export function main(): void {
  const pending = new Promise<i32>((resolve) => resolve(1));
  print(`${pending}`);
}
