// corpus: reject/r13-undefined
// purpose: Rejects undefined in the language's single null story.
// exercises: rejected-undefined, nullable-value
// questions: none
// expected-error: single null story: use null

let maybe: i32 | undefined = undefined;

export function main(): void {
  if (maybe === undefined) {
    print("missing");
    return;
  }
  print(`${maybe}`);
}
