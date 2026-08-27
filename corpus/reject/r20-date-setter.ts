// corpus: reject/r20-date-setter
// purpose: Rejects Date setters; a Date is an immutable value.
// exercises: rejected-date-subset, date-intrinsics
// questions: Q20
// tsc: accepts
// expected-error: setTime mutates; construct a new Date instead
export function main(): void {
  const d: Date = new Date(0);
  d.setTime(1000);
  print("unreachable");
}
