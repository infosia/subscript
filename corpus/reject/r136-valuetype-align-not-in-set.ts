// corpus: reject/r136-valuetype-align-not-in-set
// purpose: Rejects a value-class alignment outside the supported set.
// exercises: ValueType-alignment, decorator-options
// questions: R33
// tsc: rejects TS1238, TS2322
// expected-error: the alignment is not 2, 4, 8, or 16
@ValueType({ align: 3 })
class InvalidAlignment {
  value: f32 = 0.0;
}

export function main(): void {}
