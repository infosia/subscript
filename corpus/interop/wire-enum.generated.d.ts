// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `wire-enum.h`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

// @subscript-c-header include="wire-enum.h"
// @subscript-c-cenum typedef="SubWireModeC" alias="SubWireMode"
// @subscript-c-cenum typedef="SubBindToneC" alias="SubBindTone"

declare class SubWireModeRecord {
  tag: i32;
  mode: SubWireMode;
  tone: SubBindTone;
  modes: SubWireMode[];
  serial: u32;
  constructor(tag: i32, mode: SubWireMode, tone: SubBindTone, modes: SubWireMode[], serial: u32);
}

declare function subWireModeNext(): SubWireMode;
declare function subWireModeEcho(value: SubWireMode): i32;
declare function subWireModeUnknown(): SubWireMode;
declare function subBindToneNext(): SubBindTone;
declare function subBindToneEcho(value: SubBindTone): i32;
declare function subWireModeRecordEchoMode(value: SubWireModeRecord | null): i32;
declare function subWireModeRecordEchoTone(value: SubWireModeRecord): i32;
declare function subWireModeRecordEchoElement(value: SubWireModeRecord | null, index: u32): i32;
declare function subWireModeRecordFill(value: SubWireModeRecord | null): void;
declare function subWireModeRecordFillUnknown(value: SubWireModeRecord | null): void;
