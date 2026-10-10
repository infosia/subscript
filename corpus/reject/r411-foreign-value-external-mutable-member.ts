// corpus: reject/r411-foreign-value-external-mutable-member
// purpose: A by-value struct of another mirror reaches a string view through a mutable pointer member that C can write.
// corpus-ambient: yes
// exercises: foreign-call, external-mirror-type, mutable-pointer-member, read-position
// questions: compiler.md §187, §48
// tsc: accepts
// expected-error: S100: foreign function `subReadMutHolderTouch` parameter `holder` through `SubDescReadMutHolder.inner` reads `SubDescReadStr.label`, a string-view field with no read lowering; a struct that C writes and the script reads must not hold one (compiler.md §187)

// @subscript-c-header include="external-device.h"
// @subscript-c-external type="SubDescReadMutHolder"
declare function subReadMutHolderTouch(holder: SubDescReadMutHolder): void;

// pin: ce01c7c7
// pin-fixture: the interop mirrors are bound by the pin binder from this tree's interop.h.
// pin-dev-jit: Accepted; empty stdout.
// pin-c-aot: Accepted; empty stdout.
// pin-interpreter: Accepted; empty stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
