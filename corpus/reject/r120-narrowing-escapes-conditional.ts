// corpus: reject/r120-narrowing-escapes-conditional
// purpose: Keeps a conditional arm's null narrowing from escaping to a later expression.
// exercises: conditional-expression, flow-narrowing, narrowing-scope, nullable-reference
// questions: R19, C7
// tsc-status: exit 2 (TS2345 at 22:14) verified with node_modules/.bin/tsc --noEmit --strict --target es2022 --lib es2022 corpus/reject/r120-narrowing-escapes-conditional.ts prelude/lang.d.ts; stock TypeScript also keeps the arm narrowing from escaping.
// expected-error: S005 at the post-conditional argument

class EscapingValue {
  value: u32;

  constructor(value: u32) {
    this.value = value;
  }
}

function use(value: EscapingValue): u32 {
  return value.value;
}

function escaped(value: EscapingValue | null): u32 {
  const observed: u32 = value !== null ? use(value) : 0;
  return use(value);
}

export function main(): void {
  print(`${escaped(new EscapingValue(7))}`);
}
