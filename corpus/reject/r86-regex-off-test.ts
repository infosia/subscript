// corpus: reject/r86-regex-off-test
// feature: regex-off
// purpose: RegExp.test is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print(`${/x/.test("x")}`);
}
