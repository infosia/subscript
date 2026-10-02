// corpus: reject/r334-bare-yield-nonvoid
// purpose: Rejects a value-less suspension in a generator of i32.
// exercises: generator, bare-yield, declared-element-type
// questions: C8, compiler section 151
// tsc: rejects TS2322
// expected-error: S100
function* numbers(): Generator<i32> { yield; }
export function main(): void {}
