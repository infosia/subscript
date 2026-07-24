// corpus: reject/r23-date-zero-arg-ctor
// purpose: Rejects the zero-argument Date constructor; the lib means
//          "now in local time". Write new Date(Date.now()).
// exercises: rejected-date-subset, date-intrinsics
// questions: Q20
// expected-error: new Date() is out of subset; use new Date(Date.now())

export function main(): void {
  const d: Date = new Date();
  print(d.toISOString());
}
