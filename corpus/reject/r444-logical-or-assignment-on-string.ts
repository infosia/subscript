// corpus: reject/r444-logical-or-assignment-on-string
// purpose: `||=` tests truth, so it needs a boolean place.
// exercises: logical-assignment, truth-test
// questions: compiler.md §199.1 rule 6
// tsc: accepts
// expected-error: S100, `||=` requires a boolean operand, got `string`
export function main(): void {
  let s: string = "";
  s ||= "d";
  print(s);
}
