// corpus: reject/r392-abstract-property
// purpose: Pin the fact split of compiler section 173.
// exercises: rejection-class, divergence-block
// questions: compiler.md §173
// tsc: accepts
// expected-error: S100: abstract member `x` of `P` is not supported because class inheritance is rejected

abstract class P { abstract x: i32; get(): i32 { return this.x; } }
export function main(): void { print("ok"); }
