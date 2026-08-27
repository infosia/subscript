// corpus: reject/r61-json-parse-date
// purpose: Rejects the unreachable untagged Date parse target.
// exercises: JSON, typed parse, Date
// questions: Q28, Q20
// tsc: accepts
// expected-error: S014 at the parse member
export function main(): void {
  JSON.parse<Date>('"2020-01-01T00:00:00.000Z"');
}
