//! §187 rule 3: a struct that another mirror declares (§48) is classified
//! by its definition in the binder's own parse of the header.
//!
//! Cost: the test writes one small header to a temporary directory and
//! parses each `b.h` form in memory with libclang (two parses for each
//! position, one for the unmodeled case). The record tests parse one
//! header each in memory, with no file. No test starts a child process.

use std::fs;
use std::path::PathBuf;

use subscript_bindgen::generate_for_header;

const A_H: &str = "
#ifndef A_H
#define A_H
#include <stdint.h>
#include <stddef.h>
typedef struct View { const char *data; size_t len; } View;
typedef void (*Cb)(View message, void *userdata1, void *userdata2);
typedef struct Lay { int32_t tag; void *ud; } Lay;
typedef struct Outer { int32_t k; Lay inner; } Outer;
typedef struct Holder { int32_t k; Lay *lay; } Holder;
typedef struct HolderC { int32_t k; const Lay *lay; } HolderC;
typedef struct Info { Cb callback; void *userdata1; void *userdata2; } Info;
typedef union Bits { int32_t i; float f; } Bits;
typedef struct Odd { int32_t k; Bits bits; } Odd;
#endif
";

const TAIL: &str =
    "with no read lowering; a struct that C writes and the script reads must not hold one \
     (compiler.md §187)";

/// A directory that holds `a.h` for the duration of the test.
struct Directory(PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn b_h(directory: &Directory, external: &str, declaration: &str) -> String {
    format!(
        "#include \"{}\"\n/* @subscript-external {external} */\n{declaration}\n",
        directory.0.join("a.h").display()
    )
}

#[test]
fn an_external_struct_is_read_through_its_included_definition() {
    let directory = Directory(
        std::env::temp_dir().join(format!("subscript-s187-split-{}", std::process::id())),
    );
    fs::create_dir_all(&directory.0).expect("create directory");
    fs::write(directory.0.join("a.h"), A_H).expect("write a.h");

    // (external, violating declaration, twin external, accepted twin,
    // diagnostic prefix)
    let cases = [
        (
            "Outer",
            "void outerFill(Outer *out);",
            "Outer",
            "int32_t outerRead(const Outer *in);",
            "foreign function `outerFill` parameter `out` reads `Lay.ud`, a userdata field \
             that C can write",
        ),
        (
            "Holder",
            "int32_t holderTouch(Holder h);",
            "HolderC",
            "int32_t holderTouch(HolderC h);",
            "foreign function `holderTouch` parameter `h` through `Holder.lay` reads \
             `Lay.ud`, a userdata field that C can write",
        ),
        // §187 rule 8: a fill of a scratch struct skips its callback
        // field, so the callback is read only where the script reads C's
        // bytes: a result.
        (
            "Info",
            "Info infoGet(void);",
            "Info",
            "void infoFill(Info *out);",
            "foreign function `infoGet` result reads `Info.callback`, a callback field",
        ),
    ];
    for (external, bad, twin_external, good, message) in cases {
        let twin = b_h(&directory, twin_external, good);
        generate_for_header(&twin, "b.h").unwrap_or_else(|error| panic!("{good}: {}", error.0));
        let error = generate_for_header(&b_h(&directory, external, bad), "b.h").expect_err(bad);
        assert_eq!(error.0, format!("{message} {TAIL}"), "{bad}");
    }

    // An external that reaches an unmodeled included typedef fails loud.
    let error = generate_for_header(&b_h(&directory, "Odd", "void oddFill(Odd *out);"), "b.h")
        .expect_err("unmodeled definition");
    assert!(
        error.0.contains("external type `") && error.0.contains("does not model"),
        "{}",
        error.0
    );
}

/// Every header records the `const` of each struct-pointer parameter and
/// member, also for an external with no definition in the parse: the
/// checker reads the struct from the mirror that declares it.
#[test]
fn every_pointer_parameter_and_member_records_its_const() {
    let mirror = generate_for_header(
        "#include <stdint.h>\n\
         /* @subscript-external Outer */\n\
         typedef struct Box { int32_t k; Outer outer; } Box;\n\
         typedef struct Ref { int32_t k; Outer *outer; } Ref;\n\
         Outer *outerGet(void);\n\
         void outerFill(Outer *out);\n\
         void outerUse(const Outer *outer);\n\
         void outerTake(Outer outer);\n\
         void boxFill(Box *out);\n\
         void refTake(Ref ref);\n\
         int32_t plain(int32_t value);\n",
        "b.h",
    )
    .expect("an undefined external binds");
    let records: Vec<&str> = mirror
        .lines()
        .filter(|line| {
            line.starts_with("// @subscript-c-parameter ")
                || line.starts_with("// @subscript-c-member ")
        })
        .collect();
    assert_eq!(
        records,
        [
            "// @subscript-c-member aggregate=\"Ref\" member=\"outer\" const=false",
            "// @subscript-c-parameter function=\"outerFill\" parameter=\"out\" const=false",
            "// @subscript-c-parameter function=\"outerUse\" parameter=\"outer\" const=true",
            "// @subscript-c-parameter function=\"boxFill\" parameter=\"out\" const=false",
        ]
    );
}

/// The defining mirror records the `const` of each struct-pointer member
/// and each pair of struct elements, and no other member.
#[test]
fn the_defining_mirror_records_member_const() {
    let mirror = generate_for_header(
        "#include <stdint.h>\n#include <stddef.h>\n\
         typedef struct Obj { int32_t v; } Obj;\n\
         typedef struct Host { int32_t k; Obj *owned; const Obj *shared; Obj embedded; } Host;\n\
         typedef struct Pairs { size_t objsCount; Obj *objs; size_t valuesCount; \
         const uint32_t *values; } Pairs;\n\
         int32_t hostUse(const Host *host);\n\
         int32_t pairsUse(const Pairs *pairs);\n",
        "a.h",
    )
    .expect("bind");
    let records: Vec<&str> = mirror
        .lines()
        .filter(|line| line.starts_with("// @subscript-c-member "))
        .collect();
    assert_eq!(
        records,
        [
            "// @subscript-c-member aggregate=\"Host\" member=\"owned\" const=false",
            "// @subscript-c-member aggregate=\"Host\" member=\"shared\" const=true",
            "// @subscript-c-member aggregate=\"Pairs\" member=\"objs\" const=false",
        ]
    );
}
