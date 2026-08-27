// corpus: reject/r123-wire-enum-out-of-range
// purpose: Rejects a CEnum wire value outside the i32 range.
// exercises: CEnum, i32-wire-range
// questions: R23
// tsc: accepts
// expected-error: S100 at the out-of-range wire value
type WideWire = CEnum<{
  "m0": 2147483648;
}>;

export function main(): void {}
