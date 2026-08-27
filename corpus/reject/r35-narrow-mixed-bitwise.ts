// corpus: reject/r35-narrow-mixed-bitwise
// purpose: Rejects narrow mixed-width bitwise operands without an explicit conversion.
// exercises: narrow-numerics, mixed-width-bitwise
// questions: Q18, Q23, C3
// tsc: accepts
// expected-error: mixed-width bitwise operands require an explicit as conversion
const left: u8 = 1;
const right: u16 = 2;
const value: u16 = left | right;
