// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `host-completion.h`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

// @subscript-c-header include="host-completion.h"
// @subscript-c-external type="SubDevice"
// @subscript-c-completion function="subCompletionI32" result="int32_t"
// @subscript-c-completion function="subCompletionStruct" result="SubCompletionValue"
// @subscript-c-completion function="subCompletionVoid" result="void"
// @subscript-c-completion function="subCompletionImmediate" result="int32_t"
// @subscript-c-completion function="subCompletionSeven" result="int32_t"

declare class SubCompletionValue {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32);
}

declare function subCompletionI32(device: SubDevice, request: i32): Promise<i32>;
declare function subCompletionStruct(device: SubDevice, request: i32): Promise<SubCompletionValue>;
declare function subCompletionVoid(device: SubDevice, request: i32): Promise<void>;
declare function subCompletionImmediate(device: SubDevice, value: i32): Promise<i32>;
declare function subCompletionSeven(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32): Promise<i32>;
declare function subCompletionPump(device: SubDevice, request: i32, fail: i32): i32;
