// corpus: reject/r33-narrow-literal-overflow
// purpose: Rejects a contextual integer literal outside its narrow type.
// exercises: narrow-numerics, contextual-literal, range-check
// questions: Q23, C4
// expected-error: integer literal 128 is out of range for i8

const value: i8 = 128;
