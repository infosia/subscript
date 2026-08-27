// corpus: reject/r36-f16-arithmetic
// purpose: Rejects arithmetic in the storage-only binary16 type.
// exercises: f16-storage-only, rejected-arithmetic
// questions: Q23
// tsc: accepts
// expected-error: arithmetic on f16 is not supported; compute via as f32
const left: f16 = 1.0;
const right: f16 = 2.0;
const value: f16 = left + right;
