// corpus: reject/r410-foreign-result-callback-field
// purpose: A by-value foreign result of a struct that holds a callback field has no read lowering.
// corpus-ambient: yes
// exercises: foreign-call, external-mirror-type, callback-field, read-position
// questions: compiler.md §187, §48
// tsc: accepts
// expected-error: S100: foreign function `subReadCallbackGet` result reads `SubCallbackInfo.callback`, a callback field with no read lowering; a struct that C writes and the script reads must not hold one (compiler.md §187)

// @subscript-c-header include="external-device.h"
// @subscript-c-external type="SubCallbackInfo"
declare function subReadCallbackGet(): SubCallbackInfo;

// pin: ce01c7c7
// pin-fixture: the interop mirrors are bound by the pin binder from this tree's interop.h.
// pin-dev-jit: Accepted; empty stdout.
// pin-c-aot: Accepted; empty stdout.
// pin-interpreter: Accepted; empty stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
