// corpus: reject/r519-mirror-generator
// purpose: A host cannot supply a generator through the C ABI.
// corpus-ambient: yes
// exercises: mirror, generator, foreign-result
// questions: compiler.md §314, collisions.md C24
// tsc: accepts
// expected-error: S100 at line 10, the C ABI has no generator

// @subscript-c-header include="host.h"
declare function hostGenerator(): Generator<i32>;

// pin: 166c3006feeea635767c378a9063683f6d69c030
// pin-checker: Accepted.
// tsc-version: TypeScript 5.9.2; exit 0.
