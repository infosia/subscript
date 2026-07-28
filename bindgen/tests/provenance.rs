//! P25 mirror-provenance records (`specs/blocks/compiler.md` §23.3).
//! Each record kind is generated from a minimal header, and the const/mutable
//! descriptor pair proves provenance is keyed by parameter rather than
//! element type.

use subscript_bindgen::generate_for_header;

#[test]
fn header_record_uses_the_include_basename() {
    let mirror =
        generate_for_header("void engRun(void);", "engine.h").expect("minimal foreign header maps");
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
        typedef struct EngIds {
            const uint32_t *engItems;
            size_t engCount;
        } EngIds;
        void engRead(EngIds engIds);
    ";
    let mirror = generate_for_header(header, "ids.h").expect("descriptor maps");
    assert!(mirror.contains(
        "// @subscript-c-descriptor function=\"engRead\" parameter=\"engIds\" aggregate=\"EngIds\" element=\"uint32_t\" const=true"
    ), "{mirror}");
}

#[test]
fn string_view_record_names_parameter_and_c_aggregate() {
    let header = "
        #include <stddef.h>
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        void engName(EngText engText);
    ";
    let mirror = generate_for_header(header, "text.h").expect("string view maps");
    assert!(mirror.contains(
        "// @subscript-c-string-view function=\"engName\" parameter=\"engText\" aggregate=\"EngText\""
    ), "{mirror}");
}

#[test]
fn callback_record_names_the_c_function_pointer_typedef() {
    let header = "
        #include <stddef.h>
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngDone)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2);
        typedef struct EngSink {
            EngDone engCallback;
            void *engUserdata1;
            void *engUserdata2;
        } EngSink;
        void engSetSink(EngSink engSink);
    ";
    let mirror = generate_for_header(header, "sink.h").expect("callback maps");
    assert!(
        mirror.contains("// @subscript-c-callback typedef=\"EngDone\""),
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
        typedef struct EngItem {
            uint32_t engValue;
        } EngItem;
        typedef struct EngItemView {
            const EngItem *engItems;
            size_t engCount;
        } EngItemView;
        typedef struct EngItemOut {
            EngItem *engItems;
            size_t engCount;
        } EngItemOut;
        void engRead(EngItemView engItems);
        void engWrite(EngItemOut engItems);
    ";
    let mirror = generate_for_header(header, "items.h").expect("both descriptors map");
    assert!(mirror.contains(
        "// @subscript-c-descriptor function=\"engRead\" parameter=\"engItems\" aggregate=\"EngItemView\" element=\"EngItem\" const=true"
    ), "{mirror}");
    assert!(mirror.contains(
        "// @subscript-c-descriptor function=\"engWrite\" parameter=\"engItems\" aggregate=\"EngItemOut\" element=\"EngItem\" const=false"
    ), "{mirror}");
}

#[test]
fn header_without_foreign_functions_emits_no_provenance_records() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngValue {
            uint32_t engValue;
        } EngValue;
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngDone)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2);
    ";
    let mirror = generate_for_header(header, "types.h").expect("type-only header maps");
    assert!(!mirror.contains("@subscript-c-"), "{mirror}");
    assert!(!mirror.contains("type EngDone"), "{mirror}");
}

#[test]
fn unreachable_unsupported_callback_is_omitted_without_rejecting_header() {
    let header = "
        #include <stddef.h>
        typedef void (*EngAllocator)(size_t engSize, void *engUserdata);
        void engRun(void);
    ";
    let mirror = generate_for_header(header, "host.h")
        .expect("an unreachable host-only callback does not cross the boundary");
    assert!(mirror.contains("declare function engRun(): void;"), "{mirror}");
    assert!(!mirror.contains("type EngAllocator"), "{mirror}");
    assert!(
        !mirror.contains("@subscript-c-callback typedef=\"EngAllocator\""),
        "{mirror}"
    );
}

#[test]
fn callback_with_an_extra_parameter_is_rejected() {
    let header = "
        #include <stddef.h>
        #include <stdint.h>
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngExtra)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2,
            uint32_t engKind);
        typedef struct EngSink {
            EngExtra engCallback;
            void *engUserdata1;
            void *engUserdata2;
        } EngSink;
        void engInstall(EngSink engSink);
    ";
    let error =
        generate_for_header(header, "extra.h").expect_err("extra callback parameter must fail");
    assert!(
        error.to_string().contains("callback typedef `EngExtra`"),
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
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngShort)(
            EngText engMessage,
            void *engUserdata1);
        typedef struct EngSink {
            EngShort engCallback;
            void *engUserdata1;
            void *engUserdata2;
        } EngSink;
        void engInstall(EngSink engSink);
    ";
    let error =
        generate_for_header(header, "short.h").expect_err("missing userdata slot must fail");
    assert!(
        error.to_string().contains("callback typedef `EngShort`"),
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
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef int32_t (*EngReturning)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2);
        typedef struct EngSink {
            EngReturning engCallback;
            void *engUserdata1;
            void *engUserdata2;
        } EngSink;
        void engInstall(EngSink engSink);
    ";
    let error =
        generate_for_header(header, "return.h").expect_err("non-void callback return must fail");
    assert!(
        error
            .to_string()
            .contains("callback typedef `EngReturning`"),
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
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        EngText engWorldName(void);
    ";
    let error =
        generate_for_header(header, "return.h").expect_err("string-view return must fail");
    assert!(
        error
            .to_string()
            .contains("foreign function `engWorldName` returns string-view aggregate `EngText`"),
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
        typedef struct EngIds {
            const uint32_t *engItems;
            size_t engCount;
        } EngIds;
        EngIds engWorldIds(void);
    ";
    let error =
        generate_for_header(header, "return.h").expect_err("descriptor return must fail");
    assert!(
        error
            .to_string()
            .contains("foreign function `engWorldIds` returns descriptor aggregate `EngIds`"),
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
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngDone)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2);
        void engInstall(EngDone engCallback);
    ";
    let error =
        generate_for_header(header, "direct.h").expect_err("direct callback parameter must fail");
    assert!(
        error.to_string().contains(
            "foreign function `engInstall` parameter `engCallback` uses callback typedef \
             `EngDone` directly"
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
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngDone)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2);
        EngDone engCallback(void);
    ";
    let error =
        generate_for_header(header, "direct.h").expect_err("direct callback return must fail");
    assert!(
        error.to_string().contains(
            "foreign function `engCallback` returns callback typedef `EngDone` directly"
        ),
        "{error}"
    );
}

#[test]
fn callback_field_without_a_foreign_function_is_rejected() {
    let header = "
        #include <stddef.h>
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngDone)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2);
        typedef struct EngSink {
            EngDone engCallback;
            void *engUserdata1;
            void *engUserdata2;
        } EngSink;
    ";
    let error = generate_for_header(header, "types.h")
        .expect_err("a callback field without foreign provenance must fail");
    assert!(
        error.to_string().contains(
            "struct `EngSink` field `engCallback` uses callback typedef `EngDone`"
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
        typedef struct EngText {
            const char *engData;
            size_t engLen;
        } EngText;
        typedef void (*EngDone)(
            EngText engMessage,
            void *engUserdata1,
            void *engUserdata2);
        typedef struct EngCallbacks {
            EngDone *engItems;
            size_t engCount;
        } EngCallbacks;
        void engInstall(EngCallbacks engCallbacks);
    ";
    let error = generate_for_header(header, "callbacks.h")
        .expect_err("a callback descriptor element must fail");
    assert!(
        error.to_string().contains(
            "descriptor struct `EngCallbacks` has callback-typedef element `EngDone`"
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
