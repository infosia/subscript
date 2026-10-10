// corpus: reject/r409-foreign-result-pointer-member-string
// purpose: A foreign result whose pointer member reaches a string view has no read lowering.
// corpus-ambient: yes
// exercises: foreign-call, external-mirror-type, string-view-field, read-position
// questions: compiler.md §187, §48
// tsc: accepts
// expected-error: S100: foreign function `subReadStrHolderGet` result reads `SubDescReadStr.label`, a string-view field with no read lowering; a struct that C writes and the script reads must not hold one (compiler.md §187)

// @subscript-c-header include="external-device.h"
// @subscript-c-external type="SubDescReadStrHolder"
declare function subReadStrHolderGet(): SubDescReadStrHolder;

// pin: ce01c7c7
// pin-fixture: the interop mirrors are bound by the pin binder from this tree's interop.h.
// pin-dev-jit: Accepted; empty stdout.
// pin-c-aot: Accepted; empty stdout.
// pin-interpreter: Accepted; empty stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
