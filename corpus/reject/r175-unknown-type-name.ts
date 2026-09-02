// corpus: reject/r175-unknown-type-name
// purpose: Rejects an unknown type name.
// exercises: unknown-type-name
// questions: §82.2
// tsc: rejects TS2304
// expected-error: S016 at the unknown type name
const t: Q = new S();
