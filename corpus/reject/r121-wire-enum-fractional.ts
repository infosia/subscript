// corpus: reject/r121-wire-enum-fractional
// purpose: Rejects a fractional CEnum wire value.
// exercises: CEnum, integer-wire-values
// questions: R23
// tsc: accepts
// expected-error: S100 at the fractional wire value
type FractionalWire = CEnum<{
  "m0": 1.5;
}>;

export function main(): void {}
