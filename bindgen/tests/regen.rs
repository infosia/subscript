//! Byte-identical regeneration test (`specs/blocks/compiler.md` §12.2)
//! plus Q13 mapping checks. Running the generator on the pinned header
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

#[test]
fn committed_mirror_is_byte_identical_to_regeneration() {
    let generated = subscript_bindgen::generate(&header()).expect("generate mirror");
    let committed = fs::read_to_string(repo().join("corpus/interop/interop.generated.d.ts"))
        .expect("read committed mirror");
    assert_eq!(
        generated, committed,
        "the committed mirror drifted from the generator output; regenerate with \
         `subscript-bindgen --header corpus/interop/interop.h \
         -o corpus/interop/interop.generated.d.ts` (never hand-edit the generated file)"
    );
}

#[test]
fn q13_rules_are_reflected_in_the_mirror() {
    let m = subscript_bindgen::generate(&header()).expect("generate");

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
    assert!(!m.contains("SubBufferView"));

    // Length-carrying string view → `string`, no named type.
    assert!(m.contains("label: string"));
    assert!(!m.contains("SubStringView"));

    // Function-pointer typedef → a `type` alias; callback userdata → `object | null`.
    assert!(m.contains("type SubLogCallback = (message: string, userdata: object | null) => void;"));
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

    // No external library is named; every synthetic type is `Sub`-prefixed.
    assert!(!m.to_lowercase().contains("vulkan"));
    assert!(!m.to_lowercase().contains("webgpu"));
}
