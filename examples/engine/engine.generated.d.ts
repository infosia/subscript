// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `engine.h`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

// @subscript-c-header include="engine.h"
// @subscript-c-callback typedef="EngEventCallback"
// @subscript-c-string-view function="engWorldSetName" parameter="engName" aggregate="EngStringView"
// @subscript-c-descriptor function="engWorldReplaceEntities" parameter="engStates" aggregate="EngEntityStateView" element="EngEntityState" const=true
// @subscript-c-descriptor function="engWorldReadEntities" parameter="engStates" aggregate="EngEntityStateOut" element="EngEntityState" const=false

declare enum EngWorldOptionKind {
  ENG_WORLD_OPTION_TICK = 1,
  ENG_WORLD_OPTION_ENTITY_LIMIT = 2,
}

declare class EngWorldOption {
  engKind: EngWorldOptionKind;
  engNext: EngWorldOption | null;
  constructor(engKind: EngWorldOptionKind, engNext: EngWorldOption | null);
}

declare class EngTickOption {
  engHeader: EngWorldOption;
  engTicksPerFrame: u32;
  constructor(engHeader: EngWorldOption, engTicksPerFrame: u32);
}

declare class EngEntityLimitOption {
  engHeader: EngWorldOption;
  engMaximumEntities: u32;
  constructor(engHeader: EngWorldOption, engMaximumEntities: u32);
}

declare class EngTransform {
  engInheritScale: boolean;
  engX: f32;
  engY: f32;
  engRotation: f32;
  engLayer: u16;
  constructor(engInheritScale: boolean, engX: f32, engY: f32, engRotation: f32, engLayer: u16);
}

declare class EngEntityState {
  engId: u32;
  engTransform: EngTransform;
  engFlags: EngEntityFlags;
  constructor(engId: u32, engTransform: EngTransform, engFlags: EngEntityFlags);
}

declare enum EngEventKind {
  ENG_EVENT_WORLD_READY = 0,
  ENG_EVENT_ENTITY_CHANGED = 1,
  ENG_EVENT_FRAME_STEPPED = 2,
}

type EngEventCallback = (engMessage: string, engUserdata1: object | null, engUserdata2: object | null) => void;

declare class EngEventSink {
  engCallback: EngEventCallback;
  engUserdata1: object | null;
  engUserdata2: object | null;
  constructor(engCallback: EngEventCallback, engUserdata1: object | null, engUserdata2: object | null);
}

interface EngWorld {
  readonly __sub_handle_EngWorld: never;
}

declare class EngEntityBatch {
  engFlags: EngEntityFlags;
  engEntityIds: u32[];
  constructor(engFlags: EngEntityFlags, engEntityIds: u32[]);
}

declare function engWorldCreate(engOptions: EngWorldOption | null): EngWorld;
declare function engWorldRetain(engWorld: EngWorld): void;
declare function engWorldRelease(engWorld: EngWorld): void;
declare function engWorldSetName(engWorld: EngWorld, engName: string): void;
declare function engWorldSetTransform(engWorld: EngWorld, engEntityId: u32, engTransform: EngTransform): void;
declare function engWorldReplaceEntities(engWorld: EngWorld, engStates: EngEntityState[]): void;
declare function engWorldReadEntities(engWorld: EngWorld, engStates: EngEntityState[]): u64;
declare function engWorldApplyFlags(engWorld: EngWorld, engBatch: EngEntityBatch): u64;
declare function engWorldSetEventSink(engWorld: EngWorld, engSink: EngEventSink): void;
declare function engWorldPump(engWorld: EngWorld): void;
declare function engWorldLastEvent(engWorld: EngWorld): EngEventKind;
declare function engWorldStep(engWorld: EngWorld, engFixedStep: f32): void;
declare function engFrameBegin(engWorld: EngWorld, engFixedStep: f32): void;
declare function engFrameWorld(): EngWorld;
declare function engFrameFixedStep(): f32;
declare function engFrameIndex(): u64;

type EngEntityFlags = u64;
declare const ENG_ENTITY_FLAG_NONE = 0;
declare const ENG_ENTITY_FLAG_ACTIVE = 1;
declare const ENG_ENTITY_FLAG_VISIBLE = 2;
