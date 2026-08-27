// corpus: reject/r115-unreachable-as-value
// purpose: Keeps unreachable() as a diverging call statement rather than a value-producing expression.
// exercises: unreachable, value-position, never
// questions: R15
// tsc: accepts
// expected-error: S100 at unreachable() in value position

export function main(): void {
  const value: i32 = unreachable();
  print(`${value}`);
}
