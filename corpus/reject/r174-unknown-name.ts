// corpus: reject/r174-unknown-name
// purpose: Rejects an unknown value name.
// exercises: unknown-name
// questions: §82.2
// tsc: rejects TS2304
// expected-error: S016 at the unknown name
const n: i32 = zz;
