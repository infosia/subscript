// corpus: reject/r65-cstruct-field-offset-layout-too-large
// purpose: Rejects a CStruct whose accumulated field layout exceeds the byte limit.
// exercises: CStruct field offsets, final aggregate size
// questions: Q2, Q3
// tsc: rejects TS2564
// expected-error: S100 at the field that crosses the limit
@CStruct
class Accumulated {
  prefix: FixedArray<u8, 2147483640>;
  tail: u64;
}

export function main(): void {}
