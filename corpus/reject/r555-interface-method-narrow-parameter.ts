// corpus: reject/r555-interface-method-narrow-parameter
// purpose: An interface call can pass null, so its class method must take null.
// exercises: interface, nullable-reference
// questions: compiler.md §334 rule 1
// tsc: accepts
// expected-error: S100 at line 9, the interface requires
class C { v: i32 = 7; }
interface I { m(x: C | null): i32; }
class K implements I { m(x: C): i32 { return x.v; } }
export function main(): void {}
