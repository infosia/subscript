// corpus: reject/r13-undefined
// purpose: Rejects undefined in the language's single null story.
// exercises: rejected-undefined, nullable-value
// questions: none
// tsc: accepts
// expected-error: S012, `T | undefined` is legal only as the result type of a function declaration or a class method
let maybe: i32 | undefined = undefined;

export function main(): void {
  if (maybe === undefined) {
    print("missing");
    return;
  }
  print(`${maybe}`);
}
