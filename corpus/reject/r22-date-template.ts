// corpus: reject/r22-date-template
// purpose: Rejects interpolating a Date into a template literal; the
//          lib's implicit string form is local-time toString. Format
//          with toISOString().
// exercises: rejected-date-subset, date-intrinsics, q14-formatting
// questions: Q20
// expected-error: a Date cannot be interpolated; use toISOString()

export function main(): void {
  const d: Date = new Date(0);
  print(`now: ${d}`);
}
