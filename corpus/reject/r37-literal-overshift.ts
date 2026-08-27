// corpus: reject/r37-literal-overshift
// purpose: Rejects a literal shift amount at least as wide as its operand.
// exercises: shift-width, contextual-literal, range-check
// questions: Q18, C4
// tsc: accepts
// expected-error: literal shift amount 8 is out of range for u8 width 8
const one: u8 = 1;
const value: u8 = one << 8;
