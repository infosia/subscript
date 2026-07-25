// corpus: reject/r34-narrow-mixed-arithmetic
// purpose: Rejects narrow mixed-width arithmetic without an explicit conversion.
// exercises: narrow-numerics, mixed-width-arithmetic
// questions: Q23, C3
// expected-error: mixed-width arithmetic requires an explicit as conversion

const left: i8 = 1;
const right: i16 = 2;
const value: i16 = left + right;
