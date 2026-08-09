//! Byte-identical regeneration test (`specs/blocks/compiler.md` §12.2)
//! plus Q13/P14 mapping checks. Running the generator on the pinned header
//! reproduces the committed mirror byte-for-byte; drift fails the test.
//! This is how "generated code is never hand-edited" (CLAUDE.md core
//! principle 6) is enforced for the mirror.

use std::fs;
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn header() -> String {
    fs::read_to_string(repo().join("corpus/interop/interop.h")).expect("read interop.h")
}

fn external_header() -> String {
    fs::read_to_string(repo().join("corpus/interop/external-device.h"))
        .expect("read external-device.h")
}

fn wire_enum_header() -> String {
    fs::read_to_string(repo().join("corpus/interop/wire-enum.h")).expect("read wire-enum.h")
}

#[test]
fn committed_mirror_is_byte_identical_to_regeneration() {
    let generated =
        subscript_bindgen::generate_for_header(&header(), "interop.h").expect("generate mirror");
    let committed = fs::read_to_string(repo().join("corpus/interop/interop.generated.d.ts"))
        .expect("read committed mirror");
    assert_eq!(
        generated, committed,
        "the committed mirror drifted from the generator output; regenerate with \
         `subscript bind --header corpus/interop/interop.h \
         -o corpus/interop/interop.generated.d.ts` (never hand-edit the generated file)"
    );
}

#[test]
fn external_mirror_is_byte_identical_to_regeneration() {
    let generated = subscript_bindgen::generate_for_header(&external_header(), "external-device.h")
        .expect("generate external mirror");
    let committed =
        fs::read_to_string(repo().join("corpus/interop/external-device.generated.d.ts"))
            .expect("read committed external mirror");
    assert_eq!(
        generated, committed,
        "the committed external mirror drifted from the generator output; regenerate with \
         `subscript bind --header corpus/interop/external-device.h \
         -o corpus/interop/external-device.generated.d.ts` (never hand-edit the generated file)"
    );
}

#[test]
fn wire_enum_mirror_is_byte_identical_to_regeneration() {
    let generated = subscript_bindgen::generate_for_header(&wire_enum_header(), "wire-enum.h")
        .expect("generate wire-enum mirror");
    let committed = fs::read_to_string(repo().join("corpus/interop/wire-enum.generated.d.ts"))
        .expect("read committed wire-enum mirror");
    assert_eq!(
        generated, committed,
        "the committed wire-enum mirror drifted from the generator output; regenerate with \
         `subscript bind corpus/interop/wire-enum.h \
         -o corpus/interop/wire-enum.generated.d.ts` (never hand-edit the generated file)"
    );
}

#[test]
fn binding_rules_are_reflected_in_the_mirror() {
    let m = subscript_bindgen::generate_for_header(&header(), "interop.h").expect("generate");

    // Opaque handle → branded empty interface.
    assert!(m.contains("interface SubDevice {"));
    assert!(m.contains("readonly __sub_handle_SubDevice: never;"));

    // Enum → ambient enum carrying its constant values.
    assert!(m.contains("declare enum SubChainKind {"));
    assert!(m.contains("SUB_CHAIN_KIND_EXT_B = 2,"));

    // Struct pointer / value-class-with-null → `X | null`.
    assert!(m.contains("next: SubChainHeader | null;"));
    assert!(m.contains("subDeviceCreate(chain: SubChainHeader | null): SubDevice;"));

    // (pointer,count) array-pair descriptor → `T[]`, no named type.
    assert!(m.contains("commands: u32[]"));
    assert!(!m.contains("declare class SubBufferView"));

    // Length-carrying string view → `string`, no named type.
    assert!(m.contains("label: string"));
    assert!(!m.contains("declare class SubStringView"));

    // Function-pointer typedef → a `type` alias; callback userdata → `object | null`.
    // Two-userdata callback (§14.4): the callback type carries both slots.
    assert!(m.contains(
        "type SubLogCallback = (message: string, userdata1: object | null, userdata2: object | null) => void;"
    ));
    assert!(m.contains("userdata: object | null;"));

    // Fixed C array → FixedArray<T, N>.
    assert!(m.contains("basis: FixedArray<f32, 16>;"));

    // P6.2 (§13.2). Descriptor-embedded (count, pointer) array: the count
    // is elided, the pointer field becomes `T[]`.
    assert!(m.contains("draws: u32[];"));
    assert!(!m.contains("drawsCount"));
    assert!(m.contains("constructor(layer: u32, draws: u32[]);"));

    // Flag typedef: a `u64` alias plus `declare const` members whose C
    // values are folded into the mirror.
    assert!(m.contains("type SubAccess = u64;"));
    assert!(m.contains("declare const SUB_ACCESS_READ = 1;"));
    assert!(m.contains("declare const SUB_ACCESS_EXEC = 4;"));

    // Untyped bulk-data API (`void*` + byte size) plus its typed facade.
    assert!(m.contains("subBulkConsume(data: object | null, size: u64): i32;"));
    assert!(m.contains("subBulkConsumeF32(data: f32[]): i32;"));

    // P14 (§16). The production-shaped packet proves the scalar blocker
    // is removed: byte, short, and binary16 fields retain exact widths,
    // and each narrow typed descriptor collapses to a zero-copy `T[]`.
    assert!(m.contains("type SubFloat16 = f16;"));
    assert!(m.contains("kind: u8;"));
    assert!(m.contains("delta: i16;"));
    assert!(m.contains("weight: SubFloat16;"));
    assert!(m.contains("bias: i8;"));
    assert!(m.contains("count: u16;"));
    assert!(m.contains("subSliceChecksumU8(data: u8[]): i32;"));
    assert!(m.contains("subSliceChecksumF16(data: SubFloat16[]): i32;"));

    // P7.2 (§14.5). Mutable (pointer, count) descriptor over a value class
    // → `SubWaitEntry[]`; the descriptor struct (SubWaitList) is absorbed,
    // never emitted as a named type.
    assert!(m.contains("subDeviceWait(device: SubDevice, waits: SubWaitEntry[]): void;"));
    assert!(!m.contains("declare class SubWaitList"));
    // R5 (§27). Adjacent scalar count/pointer function parameters collapse
    // to one array in both const-input and mutable-fill directions.
    assert!(m.contains("subDeviceSumBytes(data: u8[]): u32;"));
    assert!(m.contains("subDeviceFillBytes(data: u8[]): void;"));
    assert!(m.contains("subDeviceFillShorts(data: u16[]): void;"));
    // R11 (§34). The final parameter-pair cell reuses the same provenance
    // path for const registered-handle elements; neither half of the old
    // leaked-count/nullable-pointer mirror may survive.
    assert!(m.contains(
        "subProbeQueueSubmitCheck(queue: SubDevice, commands: SubDevice[], selector: u32): u64;"
    ));
    assert!(!m.contains("commandsCount: u64"));
    assert!(!m.contains("commands: SubDevice | null"));
    // R12 (§35). A direct registered-handle foreign-function parameter
    // retains null while the leading unqualified handle remains non-null.
    assert!(
        m.contains("subProbeSetBindGroupCheck(encoder: SubDevice, group: SubDevice | null): u32;")
    );
    // R21 (§49). The host-owned fixture is an opaque handle with explicit
    // lifecycle functions and a non-owning accessor used by script code.
    assert!(m.contains("interface SubHostOwnedState {"));
    assert!(!m.contains("subHostOwnedStateCreate"));
    assert!(!m.contains("subHostOwnedStateDestroy"));
    assert!(m.contains("subHostOwnedStateBorrow(): SubHostOwnedState;"));
    assert!(m.contains("subHostOwnedStateAdvance(state: SubHostOwnedState): i32;"));
    // R7 (§30). A direct string field can share its pointer scratch with a
    // recursively plain embedded aggregate and a collapsed enum-element
    // count-first pair. The count and nullable-pointer evidence shape must
    // never survive in the mirror.
    assert!(m.contains("extent: SGPUProbeExtent3D;"));
    assert!(m.contains("viewFormats: SGPUProbeFormat[];"));
    assert!(!m.contains("viewFormatsCount"));
    assert!(!m.contains("SGPUProbeFormat | null"));
    // R8 (§31). A const registered-handle pair collapses input-only, and
    // direct `_Nullable` handle fields retain null in the mirror.
    assert!(m.contains("bindGroupLayouts: SubDevice[];"));
    assert!(!m.contains("bindGroupLayoutsCount"));
    assert!(m.contains("buffer: SubDevice | null;"));
    assert!(m.contains("sampler: SubDevice | null;"));
    assert!(m.contains("textureView: SubDevice | null;"));
    // R10 (§33). Pointer-reachable fragment state lowers through its
    // string/pairs, and each target's nullable plain blend pointer remains
    // explicit in the mirror.
    assert!(m.contains("fragment: SGPUProbeFragmentState | null;"));
    assert!(m.contains("entryPoint: string;"));
    assert!(m.contains("constants: SGPUProbeConstantEntry[];"));
    assert!(m.contains("targets: SGPUProbeColorTargetState[];"));
    assert!(m.contains("blend: SGPUProbeBlendState | null;"));
    // OBS-3 (§44). The scalar nullable handle remains a distinct field
    // beside both collapsed pairs in the pointer-reachable fragment.
    assert!(m.contains("fragment: SGPUProbeHandleFragmentState | null;"));
    assert!(m.contains("module: SubDevice | null;"));
    assert!(m.contains(
        "constructor(module: SubDevice | null, entryPoint: string, constants: SGPUProbeConstantEntry[], targets: SGPUProbeColorTargetState[]);"
    ));
    // OBS-3 round 2 (§44.5). The target element's nullable pointer reaches
    // a blend aggregate whose color and alpha fields are embedded structs.
    assert!(m.contains("blend: SGPUProbeNestedBlendState | null;"));
    assert!(m.contains("color: SGPUProbeNestedBlendComponent;"));
    assert!(m.contains("alpha: SGPUProbeNestedBlendComponent;"));
    assert!(m.contains(
        "constructor(module: SubDevice | null, entryPoint: string, constants: SGPUProbeConstantEntry[], targets: SGPUProbeNestedColorTargetState[]);"
    ));
    // OBS-3 round 3 (§44.6). A plain, count-less pointer to a registered
    // boundary struct has the same nullable mirror shape as the annotated
    // spellings above; the adjacent enum and u64 alias remain exact.
    assert!(m.contains("format: SGPUProbeUnmarkedTextureFormat;"));
    assert!(m.contains("blend: SGPUProbeUnmarkedBlendState | null;"));
    assert!(m.contains("writeMask: SGPUProbeUnmarkedColorWriteMask;"));
    assert!(m.contains(
        "constructor(module: SubDevice | null, entryPoint: string, constants: SGPUProbeConstantEntry[], targets: SGPUProbeUnmarkedColorTargetState[]);"
    ));
    // OBS-3 round 4 (§44.7). Both count-less pointer members remain
    // independently reachable across the by-value primitive aggregate, and
    // both pointed-to count/pointer pairs collapse without leaking counts.
    assert!(m.contains("depthStencil: SGPUProbeBreadthDepthStencilState | null;"));
    assert!(m.contains("fragment: SGPUProbeBreadthFragmentState | null;"));
    assert!(m.contains("primitive: SGPUProbeBreadthPrimitiveState;"));
    assert!(m.contains("biases: u32[];"));
    assert!(m.contains("constants: u32[];"));
    assert!(!m.contains("biasesCount"));
    assert!(!m.contains("constantsCount"));
    // OBS-3 round 5 (§44.8). The wide descriptor keeps both count-less
    // pointers independently reachable while every nested pair collapses,
    // including pairs in array elements and behind element pointers.
    assert!(m.contains("vertex: SGPUProbeWideVertexState;"));
    assert!(m.contains("buffers: SGPUProbeWidePairEntry[];"));
    assert!(m.contains("depthStencil: SGPUProbeWideDepthStencilState | null;"));
    assert!(m.contains("fragment: SGPUProbeWideFragmentState | null;"));
    assert!(m.contains("payload: SGPUProbeWidePayload | null;"));
    assert!(m.contains("constants: SGPUProbeWidePairEntry[];"));
    assert!(m.contains("elements: SGPUProbeWidePointerElement[];"));
    assert!(m.contains("values: u32[];"));
    assert!(!m.contains("buffersCount"));
    assert!(!m.contains("elementsCount"));
    assert!(!m.contains("valuesCount"));
    // Async op returning a future by value while taking the two-userdata
    // callback-info.
    assert!(m.contains(
        "subDeviceKickAsync(device: SubDevice, request: u32, info: SubCallbackInfo): SubFuture;"
    ));

    // No external library is named; every synthetic type is `Sub`-prefixed.
    assert!(!m.to_lowercase().contains("vulkan"));
    assert!(!m.to_lowercase().contains("webgpu"));
}
