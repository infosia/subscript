// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from the
// pinned synthetic interop header (corpus/interop/interop.h). Hand edits
// are overwritten; the byte-identical regeneration test
// (specs/blocks/compiler.md §12.2) fails on drift. Fix the generator,
// never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.

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

type SubLogCallback = (message: string, userdata: object | null) => void;

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
