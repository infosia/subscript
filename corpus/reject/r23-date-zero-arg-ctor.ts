// corpus: reject/r23-date-zero-arg-ctor
// purpose: Rejects the zero-argument Date constructor: it reads the
//          nondeterministic current time. Write new Date(Date.now()).
// exercises: rejected-date-subset, date-intrinsics
// questions: Q20
// tsc: accepts
// expected-error: new Date() is out of subset; use new Date(Date.now())
export function main(): void {
  const d: Date = new Date();
  print(d.toISOString());
}
