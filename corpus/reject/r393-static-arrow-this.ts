// corpus: reject/r393-static-arrow-this
// purpose: Pin the fact split of compiler section 173.
// exercises: rejection-class, divergence-block
// questions: compiler.md §173
// tsc: accepts
// expected-error: S100: a static method must name its class instead of `this`; use `ClassName.member`

class N { static f(): void { const g = (): void => { const self = this; }; g(); } }
export function main(): void { N.f(); print("ok"); }
