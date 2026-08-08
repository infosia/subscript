// corpus: reject/r123-wire-enum-out-of-range
// purpose: Rejects a CEnum wire value outside the i32 range.
// exercises: CEnum, i32-wire-range
// questions: R23
// tsc-clean-standalone: stock TypeScript accepts this numeric literal type.

type WideWire = CEnum<{
  "m0": 2147483648;
}>;

export function main(): void {}
