// corpus: reject/r115-unreachable-as-value
// purpose: Keeps unreachable() as a diverging call statement rather than a value-producing expression.
// exercises: unreachable, value-position, never
// questions: R15
// tsc-clean-standalone: verified with node_modules/.bin/tsc --noEmit --strict --target es2022 --lib es2022 against prelude/lang.d.ts; stock TypeScript accepts never in value position.
// expected-error: S100 at unreachable() in value position

export function main(): void {
  const value: i32 = unreachable();
  print(`${value}`);
}
