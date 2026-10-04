// corpus: reject/r356-this-capture-constructor
// purpose: Calls a receiver capture before a field holds a value.
// exercises: this, lambda, capture
// questions: compiler.md §108, compiler.md §157
// tsc: accepts
// expected-error: S100 at line 8, Calls a receiver capture before a field holds a value.

class C { n: i32; constructor() { const cb = (): i32 => this.n; cb(); this.n = 1; } }
export function main(): void {}
