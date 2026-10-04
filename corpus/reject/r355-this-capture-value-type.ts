// corpus: reject/r355-this-capture-value-type
// purpose: Rejects a copy of a ValueType receiver.
// exercises: this, lambda, capture
// questions: C2, C24, compiler.md §157
// tsc: accepts
// expected-error: S100 at line 8, Rejects a copy of a ValueType receiver.

@ValueType class C { n: i32 = 1; read(): i32 { const cb = (): i32 => this.n; return cb(); } }
export function main(): void {}
