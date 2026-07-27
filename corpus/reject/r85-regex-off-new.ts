// corpus: reject/r85-regex-off-new
// feature: regex-off
// purpose: The RegExp constructor is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  const regex = new RegExp("x");
  print(`${regex.test("x")}`);
}
