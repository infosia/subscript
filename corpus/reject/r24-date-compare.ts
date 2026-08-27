// corpus: reject/r24-date-compare
// purpose: Rejects comparing Date values directly; compare the
//          getTime() milliseconds instead.
// exercises: rejected-date-subset, date-intrinsics
// questions: Q20
// tsc: accepts
// expected-error: Dates do not compare directly; compare getTime()
export function main(): void {
  const a: Date = new Date(0);
  const b: Date = new Date(1);
  if (a === b) {
    print("same");
  }
}
