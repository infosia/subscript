// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `interop.h`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

// @subscript-c-header include="interop.h"
// @subscript-c-callback typedef="SubLogCallback"
// @subscript-c-descriptor function="subDeviceSubmit" parameter="commands" aggregate="SubBufferView" element="uint32_t" const=true
// @subscript-c-string-view function="subDeviceSetLabel" parameter="label" aggregate="SubStringView"
// @subscript-c-descriptor function="subSliceChecksumF32" parameter="data" aggregate="SubSliceF32" element="float" const=true
// @subscript-c-descriptor function="subSliceChecksumI32" parameter="data" aggregate="SubSliceI32" element="int32_t" const=true
// @subscript-c-descriptor function="subSliceChecksumF64" parameter="data" aggregate="SubSliceF64" element="double" const=true
// @subscript-c-descriptor function="subSliceChecksumI64" parameter="data" aggregate="SubSliceI64" element="int64_t" const=true
// @subscript-c-descriptor function="subSliceChecksumU8" parameter="data" aggregate="SubSliceU8" element="uint8_t" const=true
// @subscript-c-descriptor function="subSliceChecksumI8" parameter="data" aggregate="SubSliceI8" element="int8_t" const=true
// @subscript-c-descriptor function="subSliceChecksumU16" parameter="data" aggregate="SubSliceU16" element="uint16_t" const=true
// @subscript-c-descriptor function="subSliceChecksumI16" parameter="data" aggregate="SubSliceI16" element="int16_t" const=true
// @subscript-c-descriptor function="subSliceChecksumF16" parameter="data" aggregate="SubSliceF16" element="SubFloat16" const=true
// @subscript-c-descriptor function="subBulkConsumeF32" parameter="data" aggregate="SubSliceF32" element="float" const=true
// @subscript-c-descriptor function="subDeviceWait" parameter="waits" aggregate="SubWaitList" element="SubWaitEntry" const=false

declare enum SubChainKind {
  SUB_CHAIN_KIND_BASE = 0,
  SUB_CHAIN_KIND_EXT_A = 1,
  SUB_CHAIN_KIND_EXT_B = 2,
}

declare class SubChainHeader {
  sType: SubChainKind;
  next: SubChainHeader | null;
  constructor(sType: SubChainKind, next: SubChainHeader | null);
}

declare class SubChainExtA {
  header: SubChainHeader;
  intensity: f32;
  flags: u32;
  constructor(header: SubChainHeader, intensity: f32, flags: u32);
}

declare class SubChainExtB {
  header: SubChainHeader;
  scale: f64;
  level: i32;
  constructor(header: SubChainHeader, scale: f64, level: i32);
}

declare function subChainPayloadValue(chain: SubChainHeader | null): i32;

type SubLogCallback = (message: string, userdata1: object | null, userdata2: object | null) => void;

declare class SubCallbackInfo {
  callback: SubLogCallback;
  userdata: object | null;
  userparam: object | null;
  constructor(callback: SubLogCallback, userdata: object | null, userparam: object | null);
}

declare class SubTransform {
  basis: FixedArray<f32, 16>;
  bone: i32;
  weight: f64;
  visible: boolean;
  constructor(basis: FixedArray<f32, 16>, bone: i32, weight: f64, visible: boolean);
}

declare class SubSample {
  a: boolean;
  b: f64;
  c: i32;
  d: f32;
  constructor(a: boolean, b: f64, c: i32, d: f32);
}

interface SubDevice {
  readonly __sub_handle_SubDevice: never;
}

declare function subDeviceCreate(chain: SubChainHeader | null): SubDevice;
declare function subDeviceRetain(device: SubDevice): void;
declare function subDeviceRelease(device: SubDevice): void;
declare function subDeviceSubmit(device: SubDevice, commands: u32[]): void;
declare function subDeviceSetLogger(device: SubDevice, logger: SubCallbackInfo): void;
declare function subDeviceSetLabel(device: SubDevice, label: string): void;
declare function subDevicePoll(attempt: i32): i32;
declare function subSliceChecksumF32(data: f32[]): i32;
declare function subSliceChecksumI32(data: i32[]): i32;
declare function subSliceChecksumF64(data: f64[]): i32;
declare function subSliceChecksumI64(data: i64[]): i32;
declare function subSliceChecksumU8(data: u8[]): i32;
declare function subSliceChecksumI8(data: i8[]): i32;
declare function subSliceChecksumU16(data: u16[]): i32;
declare function subSliceChecksumI16(data: i16[]): i32;
declare function subSliceChecksumF16(data: SubFloat16[]): i32;
declare function subAccessMatches(mask: SubAccess, required: SubAccess): i32;

declare class SubDrawList {
  layer: u32;
  draws: u32[];
  constructor(layer: u32, draws: u32[]);
}

declare function subDrawListTotal(list: SubDrawList): i32;
declare function subBulkConsume(data: object | null, size: u64): i32;
declare function subBulkConsumeF32(data: f32[]): i32;

declare class SubCompletionInfo {
  callback: SubLogCallback;
  userdata: object | null;
  constructor(callback: SubLogCallback, userdata: object | null);
}

declare function subDeviceOnComplete(device: SubDevice, info: SubCompletionInfo): void;
declare function subDevicePump(device: SubDevice): void;

declare class SubVec2 {
  x: f32;
  y: f32;
  constructor(x: f32, y: f32);
}

declare class SubVec3 {
  x: f32;
  y: f32;
  z: f32;
  constructor(x: f32, y: f32, z: f32);
}

declare class SubVec4 {
  x: f32;
  y: f32;
  z: f32;
  w: f32;
  constructor(x: f32, y: f32, z: f32, w: f32);
}

declare class SubRect {
  x: i32;
  y: i32;
  width: u32;
  height: u32;
  constructor(x: i32, y: i32, width: u32, height: u32);
}

declare class SubRange {
  offset: u64;
  size: u64;
  constructor(offset: u64, size: u64);
}

declare class SubColor {
  r: f32;
  g: f32;
  b: f32;
  a: f32;
  constructor(r: f32, g: f32, b: f32, a: f32);
}

declare class SubTimings {
  cpu: f64;
  gpu: f64;
  frame: i32;
  constructor(cpu: f64, gpu: f64, frame: i32);
}

declare class SubMixed {
  enabled: boolean;
  id: i64;
  visible: boolean;
  ratio: f32;
  constructor(enabled: boolean, id: i64, visible: boolean, ratio: f32);
}

declare class SubPadB {
  head: i32;
  mid: boolean;
  tail: f64;
  constructor(head: i32, mid: boolean, tail: f64);
}

declare class SubNarrowPacket {
  kind: u8;
  delta: i16;
  weight: SubFloat16;
  serial: u64;
  bias: i8;
  count: u16;
  scale: f32;
  constructor(kind: u8, delta: i16, weight: SubFloat16, serial: u64, bias: i8, count: u16, scale: f32);
}

declare class SubExtent {
  width: u32;
  height: u32;
  depth: u32;
  constructor(width: u32, height: u32, depth: u32);
}

declare class SubImageInfo {
  extent: SubExtent;
  mipLevels: u32;
  usage: SubAccess;
  constructor(extent: SubExtent, mipLevels: u32, usage: SubAccess);
}

declare class SubBounds {
  min: SubVec3;
  max: SubVec3;
  constructor(min: SubVec3, max: SubVec3);
}

declare class SubViewport {
  rect: SubRect;
  depth: SubRange;
  constructor(rect: SubRect, depth: SubRange);
}

declare class SubNodeInfo {
  bounds: SubBounds;
  id: u32;
  tint: SubColor;
  constructor(bounds: SubBounds, id: u32, tint: SubColor);
}

declare class SubChainExtC {
  header: SubChainHeader;
  offset: SubVec3;
  flags: u32;
  constructor(header: SubChainHeader, offset: SubVec3, flags: u32);
}

declare class SubChainExtD {
  header: SubChainHeader;
  scale: f64;
  level: i64;
  active: boolean;
  constructor(header: SubChainHeader, scale: f64, level: i64, active: boolean);
}

declare class SubEventHeader {
  kind: i32;
  next: SubEventHeader | null;
  constructor(kind: i32, next: SubEventHeader | null);
}

declare class SubEventKey {
  header: SubEventHeader;
  code: u32;
  pressed: boolean;
  constructor(header: SubEventHeader, code: u32, pressed: boolean);
}

declare class SubEventMove {
  header: SubEventHeader;
  dx: f32;
  dy: f32;
  constructor(header: SubEventHeader, dx: f32, dy: f32);
}

declare class SubPassInfo {
  access: SubAccess;
  width: u32;
  height: u32;
  constructor(access: SubAccess, width: u32, height: u32);
}

declare class SubResourceDesc {
  usage: SubAccess;
  range: SubRange;
  count: u32;
  constructor(usage: SubAccess, range: SubRange, count: u32);
}

declare class SubCommandBuffer {
  queue: u32;
  commands: u32[];
  constructor(queue: u32, commands: u32[]);
}

declare function subCommandBufferTotal(buf: SubCommandBuffer): i32;
declare function subStageMatches(mask: SubStageFlags, required: SubStageFlags): i32;

declare class SubFuture {
  id: u64;
  constructor(id: u64);
}

declare class SubStats {
  submitted: u64;
  completed: u64;
  pending: u64;
  constructor(submitted: u64, completed: u64, pending: u64);
}

declare function subFutureMake(request: u32): SubFuture;
declare function subStatsMake(base: u32): SubStats;

declare class SubQueryStatus {
  future: u64;
  completed: i32;
  constructor(future: u64, completed: i32);
}

declare function subDeviceQuery(device: SubDevice, request: u32, status: SubQueryStatus | null): void;

declare class SubWaitEntry {
  future: SubFuture;
  completed: i32;
  constructor(future: SubFuture, completed: i32);
}

declare function subDeviceKickAsync(device: SubDevice, request: u32, info: SubCallbackInfo): SubFuture;
declare function subDeviceWait(device: SubDevice, waits: SubWaitEntry[]): void;

type SubFloat16 = f16;

type SubAccess = u64;
declare const SUB_ACCESS_NONE = 0;
declare const SUB_ACCESS_READ = 1;
declare const SUB_ACCESS_WRITE = 2;
declare const SUB_ACCESS_EXEC = 4;

type SubStageBits = u64;

type SubStageFlags = u64;
declare const SUB_STAGE_NONE = 0;
declare const SUB_STAGE_VERTEX = 1;
declare const SUB_STAGE_FRAGMENT = 2;
declare const SUB_STAGE_COMPUTE = 4;
