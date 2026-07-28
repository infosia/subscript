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
