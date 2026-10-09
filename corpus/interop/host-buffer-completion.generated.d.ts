// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `host-buffer-completion.h`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

// @subscript-c-header include="host-buffer-completion.h"
// @subscript-c-external type="SubDevice"
// @subscript-c-completion function="subCompletionText" result="string"
// @subscript-c-completion function="subCompletionBytes" result="u8[]"
// @subscript-c-completion function="subCompletionTextError" result="string"

declare function subCompletionText(device: SubDevice, empty: i32): Promise<string>;
declare function subCompletionBytes(device: SubDevice, empty: i32): Promise<u8[]>;
declare function subCompletionTextError(device: SubDevice): Promise<string>;
