// corpus: reject/r65-valuetype-field-offset-layout-too-large
// purpose: Rejects a ValueType whose accumulated field layout exceeds the byte limit.
// exercises: ValueType field offsets, final aggregate size
// questions: Q2, Q3
// tsc: accepts
// expected-error: S100 at the field that crosses the limit
@ValueType
class Accumulated {
  prefix: FixedArray<u8, 2147483640>;
  tail: u64;

  constructor(prefix: FixedArray<u8, 2147483640>, tail: u64) {
    this.prefix = prefix;
    this.tail = tail;
  }
}

export function main(): void {}
