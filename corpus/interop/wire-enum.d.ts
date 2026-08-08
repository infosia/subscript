// Hand-authored R23 boundary mirror. `subscript bind` does not emit CEnum
// aliases in this slice, so this file is intentionally not generated.

// @subscript-c-header include="wire-enum.h"

type SubWireMode = CEnum<{
  "m0": 0x10;
  "m1": 23;
  "m2": -7;
}>;

declare function subWireModeNext(): SubWireMode;
declare function subWireModeEcho(value: SubWireMode): i32;
declare function subWireModeUnknown(): SubWireMode;
