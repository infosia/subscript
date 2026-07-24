// corpus: reject/r19-date-local-accessor
// purpose: Rejects local-time Date accessors; the accepted subset is
//          UTC-only (getUTC…).
// exercises: rejected-date-subset, date-intrinsics
// questions: Q20
// expected-error: getFullYear reads local time; use getUTCFullYear

export function main(): void {
  const d: Date = new Date(0);
  const y: i32 = d.getFullYear();
  print(`${y}`);
}
