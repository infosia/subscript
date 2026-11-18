// corpus: reject/r541-c-callback-named-function-parameter-type
// purpose: A named function must take the first C callback parameter type.
// exercises: C-callback, named-function, parameter-type
// questions: compiler.md §328 rule 3
// tsc: rejects TS2345
// expected-error: S100 at line 13

function wrongParameter(message: i32): void {
  print(`${message}`);
}

export function main(): void {
  new SubCallbackInfo(wrongParameter, null, null);
}
