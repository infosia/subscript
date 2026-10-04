// corpus: reject/r358-this-capture-default-annotated
// purpose: Rejects a receiver capture in a parameter default.
// exercises: this, lambda, capture, default
// questions: C5, compiler.md §157
// tsc: accepts
// expected-error: S009 at line 8, a lambda in a parameter default cannot capture `this`

class C { d: i32 = 7; m(k: () => i32 = (): i32 => this.d): i32 { return k(); } }
export function main(): void { new C().m(); }
