// corpus: reject/r84-regex-off-literal
// feature: regex-off
// purpose: A regex literal is unavailable when the build omits P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  const regex = /x/;
  print(`${regex.test("x")}`);
}
