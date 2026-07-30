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
// @subscript-c-callback typedef="EngineEventCallback"
// @subscript-c-string-view function="engineWorldSetName" parameter="engineName" aggregate="EngineStringView"
// @subscript-c-descriptor function="engineWorldReplaceEntities" parameter="engineStates" aggregate="EngineEntityStateView" element="EngineEntityState" const=true
// @subscript-c-descriptor function="engineWorldReadEntities" parameter="engineStates" aggregate="EngineEntityStateOut" element="EngineEntityState" const=false

declare enum EngineWorldOptionKind {
  ENGINE_WORLD_OPTION_TICK = 1,
  ENGINE_WORLD_OPTION_ENTITY_LIMIT = 2,
}

declare class EngineWorldOption {
  engineKind: EngineWorldOptionKind;
  engineNext: EngineWorldOption | null;
  constructor(engineKind: EngineWorldOptionKind, engineNext: EngineWorldOption | null);
}

declare class EngineTickOption {
  engineHeader: EngineWorldOption;
  engineTicksPerFrame: u32;
  constructor(engineHeader: EngineWorldOption, engineTicksPerFrame: u32);
}

declare class EngineEntityLimitOption {
  engineHeader: EngineWorldOption;
  engineMaximumEntities: u32;
  constructor(engineHeader: EngineWorldOption, engineMaximumEntities: u32);
}

declare class EngineTransform {
  engineInheritScale: boolean;
  engineX: f32;
  engineY: f32;
  engineRotation: f32;
  engineLayer: u16;
  constructor(engineInheritScale: boolean, engineX: f32, engineY: f32, engineRotation: f32, engineLayer: u16);
}

declare class EngineEntityState {
  engineId: u32;
  engineTransform: EngineTransform;
  engineFlags: EngineEntityFlags;
  constructor(engineId: u32, engineTransform: EngineTransform, engineFlags: EngineEntityFlags);
}

declare enum EngineEventKind {
  ENGINE_EVENT_WORLD_READY = 0,
  ENGINE_EVENT_ENTITY_CHANGED = 1,
  ENGINE_EVENT_FRAME_STEPPED = 2,
}

type EngineEventCallback = (engineMessage: string, engineUserdata1: object | null, engineUserdata2: object | null) => void;

declare class EngineEventSink {
  engineCallback: EngineEventCallback;
  engineUserdata1: object | null;
  engineUserdata2: object | null;
  constructor(engineCallback: EngineEventCallback, engineUserdata1: object | null, engineUserdata2: object | null);
}

interface EngineWorld {
  readonly __sub_handle_EngineWorld: never;
}

declare class EngineEntityBatch {
  engineFlags: EngineEntityFlags;
  engineEntityIds: u32[];
  constructor(engineFlags: EngineEntityFlags, engineEntityIds: u32[]);
}

declare function engineWorldCreate(engineOptions: EngineWorldOption | null): EngineWorld;
declare function engineWorldRetain(engineWorld: EngineWorld): void;
declare function engineWorldRelease(engineWorld: EngineWorld): void;
declare function engineWorldSetName(engineWorld: EngineWorld, engineName: string): void;
declare function engineWorldSetTransform(engineWorld: EngineWorld, engineEntityId: u32, engineTransform: EngineTransform): void;
declare function engineWorldReplaceEntities(engineWorld: EngineWorld, engineStates: EngineEntityState[]): void;
declare function engineWorldReadEntities(engineWorld: EngineWorld, engineStates: EngineEntityState[]): u64;
declare function engineWorldApplyFlags(engineWorld: EngineWorld, engineBatch: EngineEntityBatch): u64;
declare function engineWorldSetEventSink(engineWorld: EngineWorld, engineSink: EngineEventSink): void;
declare function engineWorldPump(engineWorld: EngineWorld): void;
declare function engineWorldLastEvent(engineWorld: EngineWorld): EngineEventKind;
declare function engineWorldStep(engineWorld: EngineWorld, engineFixedStep: f32): void;
declare function engineFrameBegin(engineWorld: EngineWorld, engineFixedStep: f32): void;
declare function engineFrameWorld(): EngineWorld;
declare function engineFrameFixedStep(): f32;
declare function engineFrameIndex(): u64;

type EngineEntityFlags = u64;
declare const ENGINE_ENTITY_FLAG_NONE = 0;
declare const ENGINE_ENTITY_FLAG_ACTIVE = 1;
declare const ENGINE_ENTITY_FLAG_VISIBLE = 2;
