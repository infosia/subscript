// corpus: reject/r413-foreign-fill-embedded-external-userdata
// purpose: A fill of a struct that embeds a struct of another mirror reads its userdata slot.
// corpus-ambient: yes
// exercises: foreign-call, external-mirror-type, userdata-slot, read-position
// questions: compiler.md §187, §48
// tsc: accepts
// expected-error: S100: foreign function `subReadInfoBoxFill` parameter `out` reads `SubDescReadUd.ud`, a userdata field that C can write with no read lowering; a struct that C writes and the script reads must not hold one (compiler.md §187)

// @subscript-c-header include="external-device.h"
// @subscript-c-external type="SubDescReadUd"
declare class SubReadInfoBox { k: i32; ud: SubDescReadUd; constructor(k: i32, ud: SubDescReadUd); }
declare function subReadInfoBoxFill(out: SubReadInfoBox | null): void;

// pin: ce01c7c7
// pin-fixture: the interop mirrors are bound by the pin binder from this tree's interop.h and external-device.h.
// pin-dev-jit: Accepted; empty stdout.
// pin-c-aot: Checker accepts; the C compiler rejects the layout assertion: `SubReadInfoBox` is undeclared in the included headers.
// pin-interpreter: Accepted; empty stdout.
// tsc-version: TypeScript 5.9.2; exit 0.
