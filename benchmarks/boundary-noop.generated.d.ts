// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `boundary-noop.h`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

// @subscript-c-header include="boundary-noop.h"
// @subscript-c-descriptor function="bnSetBindGroup" parameter="offsets" aggregate="BnOffsets" element="uint32_t" const=true

interface BnBindGroup {
  readonly __sub_handle_BnBindGroup: never;
}

declare function bnBindGroupCreate(): BnBindGroup;
declare function bnBindGroupRelease(g: BnBindGroup): void;
declare function bnSetBindGroup(index: u32, group: BnBindGroup, offsets: u32[]): void;
declare function bnDraw(a: u32, b: u32, c: u32, d: u32): void;
declare function bnNow(): i64;
declare function bnMoreSamples(): i32;
declare function bnRecordSample(t0: i64, t1: i64): void;
declare function bnReport(): void;
