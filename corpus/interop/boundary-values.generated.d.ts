// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `boundary-values.h`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

// @subscript-c-header include="boundary-values.h"
// @subscript-c-scalar-pair function="subBoundaryScriptSize" parameter="data" element="uint8_t" const=true

declare class SubBoundaryHalf2 {
  x: f16;
  y: f16;
  constructor(x: f16, y: f16);
}

declare class SubBoundaryFloat2 {
  x: f32;
  y: f32;
  constructor(x: f32, y: f32);
}

declare class SubBoundaryBoolPadded {
  a: boolean;
  b: i32;
  c: boolean;
  constructor(a: boolean, b: i32, c: boolean);
}

declare function subBoundaryNarrow(value: i8): i32;
declare function subBoundaryHalfCheck(value: f16): i32;
declare function subBoundaryHalfReturn(): f16;
declare function subBoundaryHalf2Check(value: SubBoundaryHalf2): i32;
declare function subBoundaryHalf2Return(): SubBoundaryHalf2;
declare function subBoundaryFloat2Return(): SubBoundaryFloat2;
declare function subBoundaryBoolReturn(): SubBoundaryBoolPadded;
declare function subBoundaryScriptSize(data: u8[]): u64;
