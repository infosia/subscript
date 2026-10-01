// corpus: reject/r289-regex-literal-v-flag
// purpose: Rejects the v flag in a regex literal under the ES2022 target.
// exercises: RegExp-literal, unicodeSets, check-time-rejection
// questions: compiler.md §141
// tsc: rejects TS1501
// expected-error: S100 at line 8, the v flag in the regex literal
export function main(): void {
  const regex: RegExp = /a/v;
  print(`${regex.test("a")}`);
}
