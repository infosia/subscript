// corpus: reject/r21-date-multiarg-ctor
// purpose: Rejects the multi-argument Date constructor; the lib defines
//          it in local time, so accepting it with UTC semantics would
//          silently change meaning. Write new Date(Date.UTC(…)).
// exercises: rejected-date-subset, date-intrinsics
// questions: Q20
// tsc: accepts
// expected-error: new Date(y, m, ...) is local time; use Date.UTC
export function main(): void {
  const d: Date = new Date(2020, 0, 1);
  print(d.toISOString());
}
