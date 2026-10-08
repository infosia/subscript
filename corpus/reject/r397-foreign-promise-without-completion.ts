// corpus: reject/r397-foreign-promise-without-completion
// purpose: A foreign Promise result requires completion provenance.
// corpus-ambient: yes
// exercises: foreign-call, promise-result, completion-provenance
// questions: compiler.md §178, Q13
// tsc: accepts
// expected-error: S100: foreign function `read` returns Promise<T> without a `@subscript-c-completion` directive

// @subscript-c-header include="host.h"
declare function read(): Promise<i32>;

// pin: 09a1a889
// pin-dev-jit: Accepted; empty stdout.
// pin-c-aot: Accepted; empty stdout.
// pin-interpreter: Accepted; empty stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
