// tsc: pass
// purpose: Rejects Date as a JSON.parse target because untagged ISO strings
//          cannot identify Date values, making the target unreachable.
// expected: S014 at parse

export function main(): void {
  JSON.parse<Date>('"2020-01-01T00:00:00.000Z"');
}
