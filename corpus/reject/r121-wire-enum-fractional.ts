// corpus: reject/r121-wire-enum-fractional
// purpose: Rejects a fractional CEnum wire value.
// exercises: CEnum, integer-wire-values
// questions: R23
// tsc-clean-standalone: stock TypeScript accepts numeric literal types with fractional values.

type FractionalWire = CEnum<{
  "m0": 1.5;
}>;

export function main(): void {}
