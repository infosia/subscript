//! P25 mirror-provenance records (`specs/blocks/compiler.md` §23.3).
//! Each record kind is generated from a minimal header, and the const/mutable
//! descriptor pair proves provenance is keyed by parameter rather than
//! element type.

use subscript_bindgen::generate_for_header;

#[test]
fn header_record_uses_the_include_basename() {
    let mirror =
        generate_for_header("void engineRun(void);", "engine.h").expect("minimal foreign header maps");
    assert!(
        mirror.contains("// @subscript-c-header include=\"engine.h\""),
        "{mirror}"
    );
    assert!(
        mirror.contains("produced by this project's `bindgen` from\n// `engine.h`."),
        "{mirror}"
    );
    assert!(!mirror.contains("corpus/interop/interop.h"), "{mirror}");
}

#[test]
fn descriptor_record_names_parameter_aggregate_element_and_constness() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineIds {
            const uint32_t *engineItems;
            size_t engineCount;
        } EngineIds;
        void engineRead(EngineIds engineIds);
    ";
    let mirror = generate_for_header(header, "ids.h").expect("descriptor maps");
    assert!(mirror.contains(
        "// @subscript-c-descriptor function=\"engineRead\" parameter=\"engineIds\" aggregate=\"EngineIds\" element=\"uint32_t\" const=true"
    ), "{mirror}");
}

#[test]
fn string_view_record_names_parameter_and_c_aggregate() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        void engineName(EngineText engineText);
    ";
    let mirror = generate_for_header(header, "text.h").expect("string view maps");
    assert!(mirror.contains(
        "// @subscript-c-string-view function=\"engineName\" parameter=\"engineText\" aggregate=\"EngineText\""
    ), "{mirror}");
}

#[test]
fn callback_record_names_the_c_function_pointer_typedef() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineDone)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2);
        typedef struct EngineSink {
            EngineDone engineCallback;
            void *engineUserdata1;
            void *engineUserdata2;
        } EngineSink;
        void engineSetSink(EngineSink engineSink);
    ";
    let mirror = generate_for_header(header, "sink.h").expect("callback maps");
    assert!(
        mirror.contains("// @subscript-c-callback typedef=\"EngineDone\""),
        "{mirror}"
    );
    assert!(
        !mirror.contains("@subscript-c-string-view callback="),
        "{mirror}"
    );
}

#[test]
fn const_borrow_and_mutable_out_array_over_one_element_differ() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineItem {
            uint32_t engineValue;
        } EngineItem;
        typedef struct EngineItemView {
            const EngineItem *engineItems;
            size_t engineCount;
        } EngineItemView;
        typedef struct EngineItemOut {
            EngineItem *engineItems;
            size_t engineCount;
        } EngineItemOut;
        void engineRead(EngineItemView engineItems);
        void engineWrite(EngineItemOut engineItems);
    ";
    let mirror = generate_for_header(header, "items.h").expect("both descriptors map");
    assert!(mirror.contains(
        "// @subscript-c-descriptor function=\"engineRead\" parameter=\"engineItems\" aggregate=\"EngineItemView\" element=\"EngineItem\" const=true"
    ), "{mirror}");
    assert!(mirror.contains(
        "// @subscript-c-descriptor function=\"engineWrite\" parameter=\"engineItems\" aggregate=\"EngineItemOut\" element=\"EngineItem\" const=false"
    ), "{mirror}");
}

#[test]
fn header_without_foreign_functions_emits_no_provenance_records() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineValue {
            uint32_t engineValue;
        } EngineValue;
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineDone)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2);
    ";
    let mirror = generate_for_header(header, "types.h").expect("type-only header maps");
    assert!(!mirror.contains("@subscript-c-"), "{mirror}");
    assert!(!mirror.contains("type EngineDone"), "{mirror}");
}

#[test]
fn unreachable_unsupported_callback_is_omitted_without_rejecting_header() {
    let header = "
        #include <stddef.h>
        typedef void (*EngineAllocator)(size_t engineSize, void *engineUserdata);
        void engineRun(void);
    ";
    let mirror = generate_for_header(header, "host.h")
        .expect("an unreachable host-only callback does not cross the boundary");
    assert!(mirror.contains("declare function engineRun(): void;"), "{mirror}");
    assert!(!mirror.contains("type EngineAllocator"), "{mirror}");
    assert!(
        !mirror.contains("@subscript-c-callback typedef=\"EngineAllocator\""),
        "{mirror}"
    );
}

#[test]
fn callback_with_an_extra_parameter_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineExtra)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2,
            uint32_t engineKind);
        typedef struct EngineSink {
            EngineExtra engineCallback;
            void *engineUserdata1;
            void *engineUserdata2;
        } EngineSink;
        void engineInstall(EngineSink engineSink);
    ";
    let error =
        generate_for_header(header, "extra.h").expect_err("extra callback parameter must fail");
    assert!(
        error.to_string().contains("callback typedef `EngineExtra`"),
        "{error}"
    );
    assert!(
        error.to_string().contains(
            "supported shape is `void Callback(StringView message, void *userdata1, void *userdata2)`"
        ),
        "{error}"
    );
}

#[test]
fn callback_with_one_userdata_slot_is_rejected() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineShort)(
            EngineText engineMessage,
            void *engineUserdata1);
        typedef struct EngineSink {
            EngineShort engineCallback;
            void *engineUserdata1;
            void *engineUserdata2;
        } EngineSink;
        void engineInstall(EngineSink engineSink);
    ";
    let error =
        generate_for_header(header, "short.h").expect_err("missing userdata slot must fail");
    assert!(
        error.to_string().contains("callback typedef `EngineShort`"),
        "{error}"
    );
    assert!(
        error.to_string().contains(
            "supported shape is `void Callback(StringView message, void *userdata1, void *userdata2)`"
        ),
        "{error}"
    );
}

#[test]
fn callback_with_non_void_return_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef int32_t (*EngineReturning)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2);
        typedef struct EngineSink {
            EngineReturning engineCallback;
            void *engineUserdata1;
            void *engineUserdata2;
        } EngineSink;
        void engineInstall(EngineSink engineSink);
    ";
    let error =
        generate_for_header(header, "return.h").expect_err("non-void callback return must fail");
    assert!(
        error
            .to_string()
            .contains("callback typedef `EngineReturning`"),
        "{error}"
    );
    assert!(
        error.to_string().contains(
            "supported shape is `void Callback(StringView message, void *userdata1, void *userdata2)`"
        ),
        "{error}"
    );
}

#[test]
fn by_value_string_view_return_is_rejected() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        EngineText engineWorldName(void);
    ";
    let error =
        generate_for_header(header, "return.h").expect_err("string-view return must fail");
    assert!(
        error
            .to_string()
            .contains("foreign function `engineWorldName` returns string-view aggregate `EngineText`"),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("boundary provenance vocabulary cannot express string-view returns"),
        "{error}"
    );
}

#[test]
fn by_value_descriptor_return_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineIds {
            const uint32_t *engineItems;
            size_t engineCount;
        } EngineIds;
        EngineIds engineWorldIds(void);
    ";
    let error =
        generate_for_header(header, "return.h").expect_err("descriptor return must fail");
    assert!(
        error
            .to_string()
            .contains("foreign function `engineWorldIds` returns descriptor aggregate `EngineIds`"),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("boundary provenance vocabulary cannot express descriptor returns"),
        "{error}"
    );
}

#[test]
fn direct_callback_parameter_is_rejected() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineDone)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2);
        void engineInstall(EngineDone engineCallback);
    ";
    let error =
        generate_for_header(header, "direct.h").expect_err("direct callback parameter must fail");
    assert!(
        error.to_string().contains(
            "foreign function `engineInstall` parameter `engineCallback` uses callback typedef \
             `EngineDone` directly"
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("callbacks are bindable only as mirrored struct fields"),
        "{error}"
    );
}

#[test]
fn direct_callback_return_is_rejected() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineDone)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2);
        EngineDone engineCallback(void);
    ";
    let error =
        generate_for_header(header, "direct.h").expect_err("direct callback return must fail");
    assert!(
        error.to_string().contains(
            "foreign function `engineCallback` returns callback typedef `EngineDone` directly"
        ),
        "{error}"
    );
}

#[test]
fn callback_field_without_a_foreign_function_is_rejected() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineDone)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2);
        typedef struct EngineSink {
            EngineDone engineCallback;
            void *engineUserdata1;
            void *engineUserdata2;
        } EngineSink;
    ";
    let error = generate_for_header(header, "types.h")
        .expect_err("a callback field without foreign provenance must fail");
    assert!(
        error.to_string().contains(
            "struct `EngineSink` field `engineCallback` uses callback typedef `EngineDone`"
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("header declares no foreign function"),
        "{error}"
    );
}

#[test]
fn callback_typedef_descriptor_element_is_rejected() {
    let header = "
        #include <stddef.h>
        typedef struct EngineText {
            const char *engineData;
            size_t engineLen;
        } EngineText;
        typedef void (*EngineDone)(
            EngineText engineMessage,
            void *engineUserdata1,
            void *engineUserdata2);
        typedef struct EngineCallbacks {
            EngineDone *engineItems;
            size_t engineCount;
        } EngineCallbacks;
        void engineInstall(EngineCallbacks engineCallbacks);
    ";
    let error = generate_for_header(header, "callbacks.h")
        .expect_err("a callback descriptor element must fail");
    assert!(
        error.to_string().contains(
            "descriptor struct `EngineCallbacks` has callback-typedef element `EngineDone`"
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("callback typedefs cannot be descriptor elements"),
        "{error}"
    );
}

#[test]
fn by_value_string_field_boundary_struct_parameter_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineText { const char *data; size_t len; } EngineText;
        typedef struct EngineRecord { EngineText label; uint64_t serial; } EngineRecord;
        void engineCheck(EngineRecord record);
    ";
    let error = generate_for_header(header, "record.h")
        .expect_err("a by-value string-field struct parameter must fail");
    assert!(
        error.to_string().contains(
            "foreign function `engineCheck` parameter `record` passes string-field boundary \
             struct `EngineRecord` by value"
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("only a direct pointer parameter has a string-field lowering"),
        "{error}"
    );
}

#[test]
fn by_value_string_field_boundary_struct_return_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineText { const char *data; size_t len; } EngineText;
        typedef struct EngineRecord { EngineText label; uint64_t serial; } EngineRecord;
        EngineRecord engineRead(void);
    ";
    let error = generate_for_header(header, "record.h")
        .expect_err("a by-value string-field struct return must fail");
    assert!(
        error.to_string().contains(
            "foreign function `engineRead` returns string-field boundary struct \
             `EngineRecord` by value"
        ),
        "{error}"
    );
}

#[test]
fn string_field_boundary_struct_array_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineText { const char *data; size_t len; } EngineText;
        typedef struct EngineRecord { EngineText label; uint64_t serial; } EngineRecord;
        typedef struct EngineRecords { const EngineRecord *items; size_t count; } EngineRecords;
        void engineCheck(EngineRecords records);
    ";
    let error = generate_for_header(header, "records.h")
        .expect_err("an array of string-field structs must fail");
    assert!(
        error.to_string().contains(
            "descriptor struct `EngineRecords` forms an array of string-field boundary \
             struct `EngineRecord`"
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("arrays of string-field structs have no boundary lowering"),
        "{error}"
    );
}

#[test]
fn count_pointer_array_of_string_field_boundary_struct_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineText { const char *data; size_t len; } EngineText;
        typedef struct EngineRecord { EngineText label; uint64_t serial; } EngineRecord;
        void engineCheck(size_t recordCount, const EngineRecord *records);
    ";
    let error = generate_for_header(header, "records.h")
        .expect_err("a count/pointer array of string-field structs must fail");
    assert!(
        error.to_string().contains(
            "foreign function `engineCheck` parameters `recordCount` and `records` form an \
             array of string-field boundary struct `EngineRecord`"
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("arrays of string-field structs have no boundary lowering"),
        "{error}"
    );
}

#[test]
fn downstream_texture_descriptor_shape_collapses_enum_pair_and_accepts_extent() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct SGPUStringView { const char *data; size_t len; } SGPUStringView;
        typedef enum SGPUProbeFormat {
            SGPU_PROBE_FORMAT_RGBA8 = 11,
            SGPU_PROBE_FORMAT_BGRA8 = 29
        } SGPUProbeFormat;
        typedef struct SGPUProbeExtent3D {
            uint32_t width;
            uint32_t height;
            uint32_t depthOrArrayLayers;
        } SGPUProbeExtent3D;
        typedef struct SGPUProbeTextureDescriptor {
            SGPUStringView label;
            SGPUProbeExtent3D extent;
            size_t viewFormatsCount;
            const SGPUProbeFormat *viewFormats;
            uint32_t mipLevelCount;
            uint32_t sampleCount;
        } SGPUProbeTextureDescriptor;
        void sgpuProbeTextureCheck(const SGPUProbeTextureDescriptor *descriptor);
    ";
    let mirror = generate_for_header(header, "probe.h").expect("R7 texture shape maps");
    let block = mirror
        .split("declare class SGPUProbeTextureDescriptor {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .expect("texture descriptor declare block");
    assert!(block.contains("label: string;"), "{mirror}");
    assert!(block.contains("extent: SGPUProbeExtent3D;"), "{mirror}");
    assert!(block.contains("viewFormats: SGPUProbeFormat[];"), "{mirror}");
    assert!(!block.contains("viewFormatsCount"), "{mirror}");
    assert!(!block.contains("SGPUProbeFormat | null"), "{mirror}");
}

#[test]
fn non_adjacent_registered_enum_struct_pair_fails_loud() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef enum EngineFormat { ENGINE_FORMAT_A = 1 } EngineFormat;
        typedef struct EngineTexture {
            size_t viewFormatsCount;
            uint32_t marker;
            const EngineFormat *viewFormats;
        } EngineTexture;
    ";
    let error = generate_for_header(header, "texture.h")
        .expect_err("non-adjacent enum pair must fail before mirror emission");
    assert!(
        error.to_string().contains(
            "struct `EngineTexture` fields `viewFormatsCount` and `viewFormats` form a \
             count/pointer pair but are not the supported adjacent count-first shape"
        ),
        "{error}"
    );
    assert!(error.to_string().contains("bare count"), "{error}");
}

#[test]
fn every_emitted_struct_array_field_is_a_collapsed_pair() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef enum EngineFormat { ENGINE_FORMAT_A = 1 } EngineFormat;
        typedef struct EngineExtent { uint32_t width; uint32_t height; } EngineExtent;
        typedef struct EngineTexture {
            size_t viewFormatsCount;
            const EngineFormat *viewFormats;
            uint32_t usage;
        } EngineTexture;
        typedef struct EngineValues {
            size_t values_count;
            uint16_t *values;
        } EngineValues;
        typedef struct EngineExtents {
            size_t extentsCount;
            const EngineExtent *extents;
        } EngineExtents;
    ";
    let mirror = generate_for_header(header, "pairs.h").expect("all lowered pairs map");
    let array_fields: Vec<&str> = mirror
        .lines()
        .filter(|line| line.starts_with("  ") && line.ends_with("[];"))
        .collect();
    assert_eq!(
        array_fields,
        vec![
            "  viewFormats: EngineFormat[];",
            "  values: u16[];",
            "  extents: EngineExtent[];",
        ],
        "every emitted array field must be one recognized collapsed pair:\n{mirror}"
    );
    assert!(!mirror.contains("viewFormatsCount"), "{mirror}");
    assert!(!mirror.contains("values_count"), "{mirror}");
    assert!(!mirror.contains("extentsCount"), "{mirror}");
    assert!(!mirror.contains("EngineFormat | null"), "{mirror}");

    for (name, declaration, expected) in [
        (
            "wrong count width",
            "typedef enum EngineFormat { ENGINE_FORMAT_A = 1 } EngineFormat;\n\
             typedef struct EngineBad { uint32_t formatsCount; const EngineFormat *formats; } EngineBad;",
            "count type is `uint32_t` instead of `size_t`",
        ),
        (
            "mismatched adjacent names",
            "typedef enum EngineFormat { ENGINE_FORMAT_A = 1 } EngineFormat;\n\
             typedef struct EngineBad { size_t formatsCount; const EngineFormat *items; } EngineBad;",
            "names do not collapse",
        ),
        (
            "unsupported adjacent element",
            "typedef struct EngineBad { size_t namesCount; const char *names; } EngineBad;",
            "unsupported element `char`",
        ),
        (
            "nested descriptor aggregate",
            "typedef struct EngineWords { const uint32_t *items; size_t count; } EngineWords;\n\
             typedef struct EngineBad { EngineWords words; uint32_t tag; } EngineBad;",
            "emitted array fields are reserved for collapsed adjacent count/pointer pairs",
        ),
    ] {
        let source = format!("#include <stddef.h>\n#include <stdint.h>\n{declaration}");
        let error = match generate_for_header(&source, "audit.h") {
            Ok(mirror) => panic!("{name}: uncollapsed pair emitted:\n{mirror}"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(expected), "{name}: {error}");
    }
}

#[test]
fn every_mirror_accepted_string_field_position_has_a_lowering() {
    struct Position {
        name: &'static str,
        declaration: &'static str,
        lowered: bool,
    }

    let positions = [
        Position {
            name: "mutable pointer parameter",
            declaration: "void engineUse(EngineRecord *record);",
            lowered: true,
        },
        Position {
            name: "const pointer parameter",
            declaration: "void engineUse(const EngineRecord *record);",
            lowered: true,
        },
        Position {
            name: "by-value parameter",
            declaration: "void engineUse(EngineRecord record);",
            lowered: false,
        },
        Position {
            name: "by-value return",
            declaration: "EngineRecord engineUse(void);",
            lowered: false,
        },
        Position {
            name: "pointer return",
            declaration: "EngineRecord *engineUse(void);",
            lowered: false,
        },
    ];

    for position in positions {
        let header = format!(
            "#include <stddef.h>\n#include <stdint.h>\n\
             typedef struct EngineText {{ const char *data; size_t len; }} EngineText;\n\
             typedef struct EngineRecord {{ EngineText label; uint64_t serial; }} EngineRecord;\n\
             {}",
            position.declaration
        );
        let result = generate_for_header(&header, "audit.h");
        let accepted = result.is_ok();
        let detail = result.as_ref().err().map(ToString::to_string);
        assert_eq!(
            accepted,
            position.lowered,
            "{}: accepted={} but lowered={}: {:?}",
            position.name,
            accepted,
            position.lowered,
            detail
        );
        if let Ok(mirror) = result {
            assert!(mirror.contains("label: string;"), "{}: {mirror}", position.name);
            assert!(
                mirror.contains("record: EngineRecord | null"),
                "{}: accepted position did not retain its pointer lowering: {mirror}",
                position.name
            );
        }
    }

    // §30.1 adds one recursively plain aggregate beside the direct string
    // field to the same pointer scratch construction.
    let aggregate_header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngineText { const char *data; size_t len; } EngineText;
        typedef struct EngineExtent {
            uint32_t width;
            uint32_t height;
            uint32_t depth;
        } EngineExtent;
        typedef struct EngineRecord {
            EngineText label;
            EngineExtent extent;
            uint64_t serial;
        } EngineRecord;
        void engineUse(EngineRecord *record);
    ";
    let aggregate_mirror = generate_for_header(aggregate_header, "audit.h")
        .expect("plain nested aggregate has pointer-scratch lowering");
    assert!(aggregate_mirror.contains("label: string;"), "{aggregate_mirror}");
    assert!(
        aggregate_mirror.contains("extent: EngineExtent;"),
        "{aggregate_mirror}"
    );

    // Aggregate positions not expressible by the one direct-pointer rule.
    for (name, declaration) in [
        (
            "array descriptor element",
            "typedef struct EngineRecords { const EngineRecord *items; size_t count; } EngineRecords;\n\
             void engineUse(EngineRecords records);",
        ),
        (
            "count/pointer array parameters",
            "void engineUse(size_t recordCount, const EngineRecord *records);",
        ),
        (
            "nested aggregate field",
            "typedef struct EngineEnvelope { EngineRecord record; } EngineEnvelope;\n\
             void engineUse(EngineEnvelope *envelope);",
        ),
        (
            "callback parameter",
            "typedef void (*EngineCallback)(const EngineRecord *record);\n\
             typedef struct EngineCallbackInfo { EngineCallback callback; } EngineCallbackInfo;\n\
             void engineUse(const EngineRecord *record);",
        ),
        (
            "no foreign pointer use",
            "void engineUnrelated(uint32_t value);",
        ),
    ] {
        let header = format!(
            "#include <stddef.h>\n#include <stdint.h>\n\
             typedef struct EngineText {{ const char *data; size_t len; }} EngineText;\n\
             typedef struct EngineRecord {{ EngineText label; uint64_t serial; }} EngineRecord;\n\
             {declaration}"
        );
        let result = generate_for_header(&header, "audit.h");
        assert!(
            result.is_err(),
            "{name}: unlowered position was accepted:\n{}",
            result.expect("accepted mirror")
        );
    }
}
