// corpus: reject/r389-error-outside-surface
// purpose: Pin the fact split of compiler section 173.
// exercises: rejection-class, divergence-block
// questions: compiler.md §173
// tsc: accepts
// expected-error: S018: `Error` has no member `stack`

export function main(): void { const e = new Error("x"); print(`${e.stack}`); print(`${e.cause}`); }
