// corpus: reject/r98-promise-static
// purpose: Rejects Promise static functions.
// exercises: Promise.resolve, Promise-object-surface
// questions: Q34, C8
// tsc: accepts
// expected-error: S013 at the Promise static call
export function main(): void {
  const pending = Promise.resolve(1);
  print(`${pending}`);
}
