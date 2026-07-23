//! P6.1 production-C parse test (`specs/blocks/compiler.md` §13.1). Proves
//! the libclang frontend ingests a neutral header carrying the real-C
//! features the P5 synthetic fixture lacked: object-like and
//! function-like macros, an attribute (visibility) macro applied to a
//! declaration, a nullability attribute macro that expands to nothing,
//! `/** doc comments */`, `static const` integer constants, a flag
//! typedef with `static const` members, nested structs, a function-
//! pointer typedef, and an intrusive-chain struct.
//!
//! P6.1 asserts the parser extracts these; mapping the new shapes to the
//! boundary mirror is P6.2, so no golden mirror is produced here.

use std::fs;
use std::path::PathBuf;

use subscript_bindgen::{parse, CField, Decl, Parsed};

fn prodfix() -> Parsed {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop/prodfix.h");
    let src = fs::read_to_string(path).expect("read prodfix.h");
    parse(&src).expect("libclang frontend parses prodfix.h without error")
}

fn count<F: Fn(&Decl) -> bool>(p: &Parsed, pred: F) -> usize {
    p.decls.iter().filter(|d| pred(d)).count()
}

#[test]
fn declaration_counts_match() {
    let p = prodfix();
    assert_eq!(
        count(&p, |d| matches!(d, Decl::Enum { .. })),
        1,
        "one enum (SubStatus)"
    );
    assert_eq!(
        count(&p, |d| matches!(d, Decl::Struct { .. })),
        3,
        "three structs (SubExtent, SubImageInfo, SubNodeHeader)"
    );
    assert_eq!(
        count(&p, |d| matches!(d, Decl::FnPtr { .. })),
        1,
        "one function-pointer typedef (SubAllocCallback)"
    );
    assert_eq!(
        count(&p, |d| matches!(d, Decl::Func { .. })),
        3,
        "three functions (subImageCreate, subImageDestroy, subExtentVolume)"
    );
    assert_eq!(
        count(&p, |d| matches!(d, Decl::Handle { .. })),
        0,
        "no opaque handles in this fixture"
    );
}

#[test]
fn flag_typedef_and_members_extracted() {
    let p = prodfix();
    // The flag typedef surfaces as a scalar alias over uint64_t.
    let flags = p
        .aliases
        .iter()
        .find(|a| a.name == "SubFlags")
        .expect("SubFlags flag typedef parsed as an alias");
    assert_eq!(flags.underlying, "uint64_t");

    // Its `static const SubFlags …` members are the SubFlags-typed
    // constants; there are four, with the expected bit values.
    let members: Vec<_> = p
        .constants
        .iter()
        .filter(|c| c.type_base == "SubFlags")
        .collect();
    assert_eq!(members.len(), 4, "four SubFlags members");
    let read = members
        .iter()
        .find(|c| c.name == "SUB_FLAG_READ")
        .expect("SUB_FLAG_READ");
    assert_eq!(read.value, 1);
    let exec = members
        .iter()
        .find(|c| c.name == "SUB_FLAG_EXEC")
        .expect("SUB_FLAG_EXEC");
    assert_eq!(exec.value, 4);
}

#[test]
fn static_const_scalar_constants_extracted() {
    let p = prodfix();
    let max = p
        .constants
        .iter()
        .find(|c| c.name == "SUB_MAX_ATTACHMENTS")
        .expect("SUB_MAX_ATTACHMENTS constant");
    assert_eq!(max.type_base, "int");
    assert_eq!(max.value, 8);

    let mask = p
        .constants
        .iter()
        .find(|c| c.name == "SUB_DEFAULT_MASK")
        .expect("SUB_DEFAULT_MASK constant");
    assert_eq!(mask.type_base, "uint32_t");
    assert_eq!(mask.value, 0xFF);
}

#[test]
fn object_like_and_function_like_macros_extracted() {
    let p = prodfix();
    let version = p
        .macros
        .iter()
        .find(|m| m.name == "SUB_VERSION")
        .expect("SUB_VERSION object-like macro");
    assert!(!version.function_like);

    let make = p
        .macros
        .iter()
        .find(|m| m.name == "SUB_MAKE_VERSION")
        .expect("SUB_MAKE_VERSION function-like macro");
    assert!(make.function_like);

    // The attribute macros are seen at the preprocessor layer too.
    assert!(p.macros.iter().any(|m| m.name == "SUB_EXPORT"));
    assert!(p.macros.iter().any(|m| m.name == "SUB_NULLABLE"));
}

#[test]
fn doc_commented_attribute_tagged_function_captured_with_macros_stripped() {
    let p = prodfix();

    // The visibility and nullability macros are stripped by preprocessing:
    // the extracted signature carries neither macro's text, only the clean
    // C types.
    let Decl::Func { name, ret, params } = p
        .decls
        .iter()
        .find(|d| matches!(d, Decl::Func { name, .. } if name == "subImageCreate"))
        .expect("subImageCreate function")
    else {
        unreachable!()
    };
    assert_eq!(name, "subImageCreate");
    assert_eq!(ret.base, "SubStatus");
    assert!(!ret.pointer);

    assert_eq!(
        params,
        &vec![
            CField {
                base: "SubImageInfo".into(),
                is_const: true,
                pointer: true,
                array_len: None,
                name: "info".into(),
            },
            CField {
                base: "SubImageInfo".into(),
                is_const: false,
                pointer: true,
                array_len: None,
                name: "out".into(),
            },
        ]
    );

    // The `/** … */` doc comment is captured, keyed by the function name.
    let (_, doc) = p
        .docs
        .iter()
        .find(|(n, _)| n == "subImageCreate")
        .expect("doc comment captured for subImageCreate");
    assert!(doc.contains("Creates an image"));
    // Macro text never leaks into the captured signature or doc.
    assert!(!doc.contains("SUB_EXPORT"));
}

#[test]
fn nested_and_chain_structs_extracted() {
    let p = prodfix();

    // Nested struct: SubImageInfo embeds SubExtent by value.
    let Decl::Struct { fields, .. } = p
        .decls
        .iter()
        .find(|d| matches!(d, Decl::Struct { name, .. } if name == "SubImageInfo"))
        .expect("SubImageInfo struct")
    else {
        unreachable!()
    };
    assert_eq!(fields[0].name, "extent");
    assert_eq!(fields[0].base, "SubExtent");
    assert!(!fields[0].pointer);
    assert_eq!(fields[2].base, "SubFlags");

    // Intrusive chain: SubNodeHeader's `next` is a self pointer.
    let Decl::Struct { fields, .. } = p
        .decls
        .iter()
        .find(|d| matches!(d, Decl::Struct { name, .. } if name == "SubNodeHeader"))
        .expect("SubNodeHeader struct")
    else {
        unreachable!()
    };
    let next = &fields[1];
    assert_eq!(next.name, "next");
    assert_eq!(next.base, "SubNodeHeader");
    assert!(next.pointer);
}
