// corpus: reject/r405-foreign-result-pair
// purpose: A by-value foreign result of a struct that holds a count-first pair has no read lowering.
// corpus-ambient: yes
// exercises: foreign-call, external-mirror-type, struct-scalar-pair, read-position
// questions: compiler.md §187, §48
// tsc: accepts
// expected-error: S100: foreign function `subReadLayGet` result reads `SubDescReadLay.items`, a count-first pair field with no read lowering; a struct that C writes and the script reads must not hold one (compiler.md §187)

// @subscript-c-header include="external-device.h"
// @subscript-c-external type="SubDescReadLay"
declare function subReadLayGet(): SubDescReadLay;

// pin: ce01c7c7
// pin-fixture: the interop mirrors are bound by the pin binder from this tree's interop.h.
// pin-dev-jit: Accepted; empty stdout.
// pin-c-aot: Accepted; empty stdout.
// pin-interpreter: Accepted; empty stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
