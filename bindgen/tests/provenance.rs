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
