// corpus: reject/r02-eval
// purpose: Rejects dynamic source evaluation.
// exercises: rejected-eval
// questions: none
// expected-error: no dynamic code evaluation

export function main(): void {
  eval("print('dynamic')");
}
