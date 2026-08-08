// corpus: reject/r122-wire-enum-duplicate
// purpose: Rejects duplicate wire values across distinct CEnum members.
// exercises: CEnum, unique-wire-values
// questions: R23
// tsc-clean-standalone: stock TypeScript does not require property values to be unique.

type DuplicateWire = CEnum<{
  "m0": 7;
  "m1": 7;
}>;

export function main(): void {}
