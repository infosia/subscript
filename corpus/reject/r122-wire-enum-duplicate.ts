// corpus: reject/r122-wire-enum-duplicate
// purpose: Rejects duplicate wire values across distinct CEnum members.
// exercises: CEnum, unique-wire-values
// questions: R23
// tsc: accepts
// expected-error: S100 at the second duplicate wire value
type DuplicateWire = CEnum<{
  "m0": 7;
  "m1": 7;
}>;

export function main(): void {}
