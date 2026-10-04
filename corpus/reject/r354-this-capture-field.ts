// corpus: reject/r354-this-capture-field
// purpose: Stores a receiver capture in a field.
// exercises: this, lambda, capture
// questions: C5, compiler.md §157
// tsc: accepts
// expected-error: S009 at line 8, Stores a receiver capture in a field.

class C { n: i32 = 1; cb: () => i32 = (): i32 => 0; store(): void { this.cb = (): i32 => this.n; } }
export function main(): void {}
