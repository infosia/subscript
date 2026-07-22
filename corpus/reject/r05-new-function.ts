// corpus: reject/r05-new-function
// purpose: Rejects dynamic function construction.
// exercises: rejected-new-function
// questions: none
// expected-error: no dynamic code evaluation

export function main(): void {
  const build = new Function("print('dynamic')");
}
