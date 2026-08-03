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
// @subscript-c-scalar-pair function="subDeviceSumBytes" parameter="data" element="uint8_t" const=true
// @subscript-c-scalar-pair function="subDeviceFillBytes" parameter="data" element="uint8_t" const=false
// @subscript-c-scalar-pair function="subDeviceFillShorts" parameter="data" element="uint16_t" const=false
// @subscript-c-scalar-pair function="subProbeQueueSubmitCheck" parameter="commands" element="SubDevice" const=true

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
declare function subDeviceSumBytes(data: u8[]): u32;
declare function subDeviceFillBytes(data: u8[]): void;
declare function subDeviceFillShorts(data: u16[]): void;

declare class SubBoundaryStringRecord {
  label: string;
  handle: u64;
  enabled: boolean;
  serial: u64;
  generation: u64;
  constructor(label: string, handle: u64, enabled: boolean, serial: u64, generation: u64);
}

declare function subBoundaryStringCheck(record: SubBoundaryStringRecord | null, selector: u32): u64;
declare function subBoundaryStringFill(record: SubBoundaryStringRecord | null, emptyLabel: boolean): void;

declare enum SGPUProbeFormat {
  SGPU_PROBE_FORMAT_RGBA8 = 11,
  SGPU_PROBE_FORMAT_BGRA8 = 29,
  SGPU_PROBE_FORMAT_DEPTH24 = 47,
}

declare class SGPUProbeExtent3D {
  width: u32;
  height: u32;
  depthOrArrayLayers: u32;
  constructor(width: u32, height: u32, depthOrArrayLayers: u32);
}

declare class SGPUProbeTextureDescriptor {
  label: string;
  extent: SGPUProbeExtent3D;
  viewFormats: SGPUProbeFormat[];
  format: SGPUProbeFormat;
  mipLevelCount: u32;
  sampleCount: u32;
  dimension: u32;
  usage: u32;
  constructor(label: string, extent: SGPUProbeExtent3D, viewFormats: SGPUProbeFormat[], format: SGPUProbeFormat, mipLevelCount: u32, sampleCount: u32, dimension: u32, usage: u32);
}

declare function subProbeTextureDescriptorCheck(descriptor: SGPUProbeTextureDescriptor | null, selector: u32): u64;
declare function subProbeTextureDescriptorFill(descriptor: SGPUProbeTextureDescriptor | null): void;

declare class SubProbePipelineLayoutDescriptor {
  label: string;
  bindGroupLayouts: SubDevice[];
  constructor(label: string, bindGroupLayouts: SubDevice[]);
}

declare function subProbePipelineLayoutCheck(descriptor: SubProbePipelineLayoutDescriptor | null, selector: u32): u64;

declare class SubProbeBindGroupEntry {
  binding: u32;
  buffer: SubDevice | null;
  sampler: SubDevice | null;
  textureView: SubDevice | null;
  constructor(binding: u32, buffer: SubDevice | null, sampler: SubDevice | null, textureView: SubDevice | null);
}

declare function subProbeBindGroupEntryCheck(entry: SubProbeBindGroupEntry | null): u32;
declare function subProbeBindGroupEntryFill(entry: SubProbeBindGroupEntry | null, selected: u32, handle: SubDevice): void;

declare class SGPUProbeComputeState {
  entryPoint: string;
  workgroupX: u32;
  workgroupY: u32;
  constantSeed: u64;
  constructor(entryPoint: string, workgroupX: u32, workgroupY: u32, constantSeed: u64);
}

declare class SGPUProbeComputePipelineDescriptor {
  label: string;
  compute: SGPUProbeComputeState;
  flags: u32;
  constructor(label: string, compute: SGPUProbeComputeState, flags: u32);
}

declare function subProbeComputePipelineCheck(descriptor: SGPUProbeComputePipelineDescriptor | null, selector: u32): u64;

declare class SGPUProbeVertexAttribute {
  shaderLocation: u32;
  format: u32;
  offset: u64;
  constructor(shaderLocation: u32, format: u32, offset: u64);
}

declare class SGPUProbeVertexBufferLayout {
  arrayStride: u64;
  stepMode: u32;
  attributes: SGPUProbeVertexAttribute[];
  constructor(arrayStride: u64, stepMode: u32, attributes: SGPUProbeVertexAttribute[]);
}

declare class SGPUProbeVertexState {
  moduleId: u32;
  buffers: SGPUProbeVertexBufferLayout[];
  constructor(moduleId: u32, buffers: SGPUProbeVertexBufferLayout[]);
}

declare class SGPUProbeRenderPipelineDescriptor {
  label: string;
  vertex: SGPUProbeVertexState;
  primitive: u32;
  constructor(label: string, vertex: SGPUProbeVertexState, primitive: u32);
}

declare function subProbeRenderPipelineCheck(descriptor: SGPUProbeRenderPipelineDescriptor | null, selector: u32): u64;

declare class SGPUProbeConstantEntry {
  key: string;
  value: f64;
  constructor(key: string, value: f64);
}

declare class SGPUProbeProgrammableStage {
  constants: SGPUProbeConstantEntry[];
  stage: u32;
  constructor(constants: SGPUProbeConstantEntry[], stage: u32);
}

declare function subProbeProgrammableStageCheck(stage: SGPUProbeProgrammableStage | null, selector: u32): u64;

declare class SGPUProbeBlendState {
  colorOperation: u32;
  alphaOperation: u32;
  constructor(colorOperation: u32, alphaOperation: u32);
}

declare class SGPUProbeColorTargetState {
  format: u32;
  blend: SGPUProbeBlendState | null;
  writeMask: u32;
  constructor(format: u32, blend: SGPUProbeBlendState | null, writeMask: u32);
}

declare class SGPUProbeFragmentState {
  entryPoint: string;
  constants: SGPUProbeConstantEntry[];
  targets: SGPUProbeColorTargetState[];
  constructor(entryPoint: string, constants: SGPUProbeConstantEntry[], targets: SGPUProbeColorTargetState[]);
}

declare class SGPUProbeFullRenderPipelineDescriptor {
  label: string;
  fragment: SGPUProbeFragmentState | null;
  constructor(label: string, fragment: SGPUProbeFragmentState | null);
}

declare function subProbeFullRenderPipelineCheck(descriptor: SGPUProbeFullRenderPipelineDescriptor | null, selector: u32): u64;

declare class SGPUProbeHandleFragmentState {
  module: SubDevice | null;
  entryPoint: string;
  constants: SGPUProbeConstantEntry[];
  targets: SGPUProbeColorTargetState[];
  constructor(module: SubDevice | null, entryPoint: string, constants: SGPUProbeConstantEntry[], targets: SGPUProbeColorTargetState[]);
}

declare class SGPUProbeHandleRenderPipelineDescriptor {
  label: string;
  fragment: SGPUProbeHandleFragmentState | null;
  constructor(label: string, fragment: SGPUProbeHandleFragmentState | null);
}

declare function subProbeFullRenderPipelineWithHandleCheck(descriptor: SGPUProbeHandleRenderPipelineDescriptor | null, selector: u32): u64;

declare class SGPUProbeNestedBlendComponent {
  operation: u32;
  srcFactor: u32;
  dstFactor: u32;
  constructor(operation: u32, srcFactor: u32, dstFactor: u32);
}

declare class SGPUProbeNestedBlendState {
  color: SGPUProbeNestedBlendComponent;
  alpha: SGPUProbeNestedBlendComponent;
  constructor(color: SGPUProbeNestedBlendComponent, alpha: SGPUProbeNestedBlendComponent);
}

declare class SGPUProbeNestedColorTargetState {
  format: u32;
  blend: SGPUProbeNestedBlendState | null;
  writeMask: u32;
  constructor(format: u32, blend: SGPUProbeNestedBlendState | null, writeMask: u32);
}

declare class SGPUProbeNestedFragmentState {
  module: SubDevice | null;
  entryPoint: string;
  constants: SGPUProbeConstantEntry[];
  targets: SGPUProbeNestedColorTargetState[];
  constructor(module: SubDevice | null, entryPoint: string, constants: SGPUProbeConstantEntry[], targets: SGPUProbeNestedColorTargetState[]);
}

declare class SGPUProbeNestedRenderPipelineDescriptor {
  label: string;
  fragment: SGPUProbeNestedFragmentState | null;
  constructor(label: string, fragment: SGPUProbeNestedFragmentState | null);
}

declare function subProbeFullRenderPipelineWithNestedBlendCheck(descriptor: SGPUProbeNestedRenderPipelineDescriptor | null, selector: u32): u64;

declare enum SGPUProbeUnmarkedTextureFormat {
  SGPU_PROBE_UNMARKED_TEXTURE_FORMAT_RGBA8 = 101,
  SGPU_PROBE_UNMARKED_TEXTURE_FORMAT_BGRA8 = 202,
}

declare class SGPUProbeUnmarkedBlendState {
  colorOperation: u32;
  alphaOperation: u32;
  constructor(colorOperation: u32, alphaOperation: u32);
}

declare class SGPUProbeUnmarkedColorTargetState {
  format: SGPUProbeUnmarkedTextureFormat;
  blend: SGPUProbeUnmarkedBlendState | null;
  writeMask: SGPUProbeUnmarkedColorWriteMask;
  constructor(format: SGPUProbeUnmarkedTextureFormat, blend: SGPUProbeUnmarkedBlendState | null, writeMask: SGPUProbeUnmarkedColorWriteMask);
}

declare class SGPUProbeUnmarkedFragmentState {
  module: SubDevice | null;
  entryPoint: string;
  constants: SGPUProbeConstantEntry[];
  targets: SGPUProbeUnmarkedColorTargetState[];
  constructor(module: SubDevice | null, entryPoint: string, constants: SGPUProbeConstantEntry[], targets: SGPUProbeUnmarkedColorTargetState[]);
}

declare class SGPUProbeUnmarkedRenderPipelineDescriptor {
  label: string;
  fragment: SGPUProbeUnmarkedFragmentState | null;
  constructor(label: string, fragment: SGPUProbeUnmarkedFragmentState | null);
}

declare function subProbeFullRenderPipelineWithUnmarkedBlendCheck(descriptor: SGPUProbeUnmarkedRenderPipelineDescriptor | null, selector: u32): u64;

declare class SGPUProbeBreadthNestedState {
  first: u32;
  second: u32;
  constructor(first: u32, second: u32);
}

declare class SGPUProbeBreadthDepthStencilState {
  limits: SGPUProbeBreadthNestedState;
  biases: u32[];
  constructor(limits: SGPUProbeBreadthNestedState, biases: u32[]);
}

declare class SGPUProbeBreadthFragmentState {
  stage: SGPUProbeBreadthNestedState;
  constants: u32[];
  constructor(stage: SGPUProbeBreadthNestedState, constants: u32[]);
}

declare class SGPUProbeBreadthPrimitiveState {
  topology: u32;
  stripIndexFormat: u32;
  constructor(topology: u32, stripIndexFormat: u32);
}

declare class SGPUProbeBreadthRenderPipelineDescriptor {
  label: string;
  depthStencil: SGPUProbeBreadthDepthStencilState | null;
  primitive: SGPUProbeBreadthPrimitiveState;
  fragment: SGPUProbeBreadthFragmentState | null;
  constructor(label: string, depthStencil: SGPUProbeBreadthDepthStencilState | null, primitive: SGPUProbeBreadthPrimitiveState, fragment: SGPUProbeBreadthFragmentState | null);
}

declare function subProbeBreadthRenderPipelineCheck(descriptor: SGPUProbeBreadthRenderPipelineDescriptor | null, selector: u32): u64;

declare class SGPUProbeWidePairEntry {
  key: string;
  values: u32[];
  constructor(key: string, values: u32[]);
}

declare class SGPUProbeWideVertexState {
  entryPoint: string;
  buffers: SGPUProbeWidePairEntry[];
  constructor(entryPoint: string, buffers: SGPUProbeWidePairEntry[]);
}

declare class SGPUProbeWidePrimitiveState {
  topology: u32;
  stripIndexFormat: u32;
  constructor(topology: u32, stripIndexFormat: u32);
}

declare class SGPUProbeWideMultisampleState {
  count: u32;
  mask: u32;
  alphaToCoverage: u32;
  constructor(count: u32, mask: u32, alphaToCoverage: u32);
}

declare class SGPUProbeWidePayload {
  label: string;
  values: u32[];
  constructor(label: string, values: u32[]);
}

declare class SGPUProbeWidePointerElement {
  kind: u32;
  payload: SGPUProbeWidePayload | null;
  constructor(kind: u32, payload: SGPUProbeWidePayload | null);
}

declare class SGPUProbeWideDepthStencilState {
  constants: SGPUProbeWidePairEntry[];
  elements: SGPUProbeWidePointerElement[];
  constructor(constants: SGPUProbeWidePairEntry[], elements: SGPUProbeWidePointerElement[]);
}

declare class SGPUProbeWideFragmentState {
  module: SubDevice | null;
  entryPoint: string;
  constants: SGPUProbeWidePairEntry[];
  elements: SGPUProbeWidePointerElement[];
  constructor(module: SubDevice | null, entryPoint: string, constants: SGPUProbeWidePairEntry[], elements: SGPUProbeWidePointerElement[]);
}

declare class SGPUProbeWideRenderPipelineDescriptor {
  label: string;
  layout: SubDevice | null;
  vertex: SGPUProbeWideVertexState;
  primitive: SGPUProbeWidePrimitiveState;
  depthStencil: SGPUProbeWideDepthStencilState | null;
  multisample: SGPUProbeWideMultisampleState;
  fragment: SGPUProbeWideFragmentState | null;
  constructor(label: string, layout: SubDevice | null, vertex: SGPUProbeWideVertexState, primitive: SGPUProbeWidePrimitiveState, depthStencil: SGPUProbeWideDepthStencilState | null, multisample: SGPUProbeWideMultisampleState, fragment: SGPUProbeWideFragmentState | null);
}

declare function subProbeWideRenderPipelineCheck(descriptor: SGPUProbeWideRenderPipelineDescriptor | null, selector: u32): u64;
declare function subProbeQueueSubmitCheck(queue: SubDevice, commands: SubDevice[], selector: u32): u64;
declare function subProbeSetBindGroupCheck(encoder: SubDevice, group: SubDevice | null): u32;

declare class SubByValueI32One {
  a: i32;
  constructor(a: i32);
}

declare class SubByValueI32Pair {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32);
}

declare class SubByValueI32Triple {
  a: i32;
  b: i32;
  c: i32;
  constructor(a: i32, b: i32, c: i32);
}

declare class SubByValueI16I16I32 {
  a: i16;
  b: i16;
  c: i32;
  constructor(a: i16, b: i16, c: i32);
}

declare class SubByValueU8Four {
  a: u8;
  b: u8;
  c: u8;
  d: u8;
  constructor(a: u8, b: u8, c: u8, d: u8);
}

declare class SubByValueI64Pair {
  a: i64;
  b: i64;
  constructor(a: i64, b: i64);
}

declare class SubByValueF32Hfa2 {
  a: f32;
  b: f32;
  constructor(a: f32, b: f32);
}

declare class SubByValueF32Hfa4 {
  a: f32;
  b: f32;
  c: f32;
  d: f32;
  constructor(a: f32, b: f32, c: f32, d: f32);
}

declare class SubByValueI32F32 {
  a: i32;
  b: f32;
  constructor(a: i32, b: f32);
}

declare class SubByValueI32I64 {
  a: i32;
  b: i64;
  constructor(a: i32, b: i64);
}

declare class SubByValueI64Triple {
  a: i64;
  b: i64;
  c: i64;
  constructor(a: i64, b: i64, c: i64);
}

declare function subByValueI32OneReport(report: SubByValueI32One | null, value: SubByValueI32One): void;
declare function subByValueI32PairReport(report: SubByValueI32Pair | null, value: SubByValueI32Pair): void;
declare function subByValueI32TripleReport(report: SubByValueI32Triple | null, value: SubByValueI32Triple): void;
declare function subByValueI16I16I32Report(report: SubByValueI16I16I32 | null, value: SubByValueI16I16I32): void;
declare function subByValueU8FourReport(report: SubByValueU8Four | null, value: SubByValueU8Four): void;
declare function subByValueI64PairReport(report: SubByValueI64Pair | null, value: SubByValueI64Pair): void;
declare function subByValueF32Hfa2Report(report: SubByValueF32Hfa2 | null, value: SubByValueF32Hfa2): void;
declare function subByValueF32Hfa4Report(report: SubByValueF32Hfa4 | null, value: SubByValueF32Hfa4): void;
declare function subByValueI32F32Report(report: SubByValueI32F32 | null, value: SubByValueI32F32): void;
declare function subByValueI32I64Report(report: SubByValueI32I64 | null, value: SubByValueI32I64): void;
declare function subByValueI64TripleReport(report: SubByValueI64Triple | null, value: SubByValueI64Triple): void;

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

type SGPUProbeUnmarkedColorWriteMask = u64;
