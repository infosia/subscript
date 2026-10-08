use subscript_boundary::{c_kind, language_kind, Carrier, Extension, Leaf, KINDS};

#[test]
fn every_kind_has_the_contract_facts() {
    let expected = [
        (
            "int8_t",
            "i8",
            1,
            Carrier::General,
            Extension::Signed,
            Leaf::Integer,
        ),
        (
            "uint8_t",
            "u8",
            1,
            Carrier::General,
            Extension::Unsigned,
            Leaf::Integer,
        ),
        (
            "int16_t",
            "i16",
            2,
            Carrier::General,
            Extension::Signed,
            Leaf::Integer,
        ),
        (
            "uint16_t",
            "u16",
            2,
            Carrier::General,
            Extension::Unsigned,
            Leaf::Integer,
        ),
        (
            "int32_t",
            "i32",
            4,
            Carrier::General,
            Extension::None,
            Leaf::Integer,
        ),
        (
            "uint32_t",
            "u32",
            4,
            Carrier::General,
            Extension::None,
            Leaf::Integer,
        ),
        (
            "int64_t",
            "i64",
            8,
            Carrier::General,
            Extension::None,
            Leaf::Integer,
        ),
        (
            "uint64_t",
            "u64",
            8,
            Carrier::General,
            Extension::None,
            Leaf::Integer,
        ),
        (
            "bool",
            "boolean",
            1,
            Carrier::General,
            Extension::Unsigned,
            Leaf::Integer,
        ),
        (
            "_Float16",
            "f16",
            2,
            Carrier::Simd,
            Extension::None,
            Leaf::Half,
        ),
        (
            "float",
            "f32",
            4,
            Carrier::Simd,
            Extension::None,
            Leaf::Float,
        ),
        (
            "double",
            "f64",
            8,
            Carrier::Simd,
            Extension::None,
            Leaf::Double,
        ),
    ];
    assert_eq!(KINDS.len(), expected.len());
    for (c, language, size, carrier, extension, leaf) in expected {
        let kind = c_kind(c).expect("C kind");
        assert_eq!(language_kind(language), Some(kind));
        assert_eq!(
            (
                kind.c_type,
                kind.language,
                kind.size,
                kind.align,
                kind.carrier,
                kind.extension,
                kind.leaf
            ),
            (c, language, size, size, carrier, extension, leaf)
        );
    }
    for (c, lang) in [
        ("signed char", "i8"),
        ("unsigned char", "u8"),
        ("short", "i16"),
        ("unsigned short int", "u16"),
        ("int", "i32"),
        ("unsigned int", "u32"),
        ("long long", "i64"),
        ("unsigned long long", "u64"),
        ("size_t", "u64"),
    ] {
        assert_eq!(c_kind(c), language_kind(lang));
    }
    for unsupported in [
        "long",
        "char",
        "__fp16",
        "missing",
        "void*",
        "_Bool",
        "signed int",
        "long long int",
    ] {
        assert_eq!(c_kind(unsupported), None);
    }
    assert_eq!(language_kind("missing"), None);
}

#[cfg(not(all(windows, target_env = "msvc")))]
#[test]
fn storage_matches_the_native_c_compiler() {
    use std::fmt::Write;
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("subscript-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temporary directory");
    let mut source = String::from("#include <stdint.h>\n#include <stdbool.h>\n");
    for kind in KINDS {
        writeln!(
            source,
            "_Static_assert(sizeof({}) == {}, \"size: {}\");",
            kind.c_type, kind.size, kind.c_type
        )
        .expect("size assertion");
        writeln!(
            source,
            "_Static_assert(_Alignof({}) == {}, \"alignment: {}\");",
            kind.c_type, kind.align, kind.c_type
        )
        .expect("alignment assertion");
    }
    let file = dir.join("probe.c");
    let object = dir.join("probe.o");
    std::fs::write(&file, source).expect("C source");
    let output = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()))
        .arg("-std=c11")
        .arg("-c")
        .arg(&file)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("C compiler");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(dir).expect("remove probe");
}
