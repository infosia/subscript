// corpus: reject/r353-this-capture-return
// purpose: Returns a lambda that captures the receiver.
// exercises: this, lambda, capture
// questions: C5, compiler.md §157
// tsc: accepts
// expected-error: S009 at line 8, Returns a lambda that captures the receiver.

class C { n: i32 = 1; read(): () => i32 { return (): i32 => this.n; } }
export function main(): void {}
