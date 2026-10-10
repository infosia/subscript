// corpus: reject/r412-foreign-fill-embedded-external-view
// purpose: A fill of a struct that embeds a struct of another mirror reads a string view behind its pointer member.
// corpus-ambient: yes
// exercises: foreign-call, external-mirror-type, nested-boundary-aggregate, read-position
// questions: compiler.md §187, §48
// tsc: accepts
// expected-error: S100: foreign function `subReadBoxFill` parameter `out` reads `SubDescReadStr.label`, a string-view field with no read lowering; a struct that C writes and the script reads must not hold one (compiler.md §187)

// @subscript-c-header include="external-device.h"
// @subscript-c-external type="SubDescReadMutHolder"
declare class SubReadBox { k: i32; holder: SubDescReadMutHolder; constructor(k: i32, holder: SubDescReadMutHolder); }
declare function subReadBoxFill(out: SubReadBox | null): void;

// pin: ce01c7c7
// pin-fixture: the interop mirrors are bound by the pin binder from this tree's interop.h.
// pin-dev-jit: Accepted; empty stdout.
// pin-c-aot: Accepted; empty stdout.
// pin-interpreter: Accepted; empty stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
