//! `specs/blocks/compiler.md` §187 rule 3: the checker reads every
//! parameter of every foreign function through the mirror classes, with
//! the `const` facts of the `@subscript-c-parameter` and
//! `@subscript-c-member` records. A pointer with no record counts as
//! non-`const`. `Lay` copies its bytes and holds a userdata slot, which
//! has no read lowering where the script reads C's bytes (§187 rule 8): in
//! a value, and in a struct whose bytes the call passes.
//!
//! Cost: each case checks two or three small in-memory sources; the file
//! runs in under 0.1 s.

use subscript_compiler::{check_program, RuleCode, SourceFile};

const TAIL: &str = "with no read lowering; a struct that C writes and the script reads must \
                    not hold one (compiler.md §187)";

/// A mirror that defines `Holder`, whose pointer member reaches a pair.
/// `member` is its member record line, or empty for no record.
fn holder_mirror(member: &str) -> SourceFile {
    SourceFile::ambient(
        "a.d.ts",
        format!(
            "// @subscript-c-header include=\"a.h\"\n{member}\n\
             declare class Lay {{ tag: i32; ud: object | null; constructor(tag: i32, ud: object | null); }}\n\
             declare class Holder {{ k: i32; lay: Lay | null; constructor(k: i32, lay: Lay | null); }}\n"
        ),
    )
}

/// A second mirror that takes `Holder` by value and through a `const`
/// pointer.
fn holder_user() -> SourceFile {
    SourceFile::ambient(
        "b.d.ts",
        "// @subscript-c-header include=\"b.h\"\n\
         // @subscript-c-external type=\"Holder\"\n\
         // @subscript-c-parameter function=\"holderTouchC\" parameter=\"h\" const=true\n\
         declare function holderTouch(h: Holder): void;\n\
         declare function holderTouchC(h: Holder | null): void;\n",
    )
}

fn main_file() -> SourceFile {
    SourceFile::new("main.ts", "export function main(): void {}\n")
}

fn messages(files: &[SourceFile]) -> Vec<String> {
    match check_program(files) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .into_iter()
            .inspect(|diagnostic| assert_eq!(diagnostic.code, RuleCode::S100))
            .map(|diagnostic| diagnostic.message)
            .collect(),
    }
}

/// A by-value root and a `const` input read the target of a mutable
/// pointer member (rule 1).
#[test]
fn a_mutable_pointer_member_is_read_at_a_value_and_a_const_root() {
    let through = |function: &str| {
        format!(
            "foreign function `{function}` parameter `h` through `Holder.lay` reads \
             `Lay.ud`, a userdata field that C can write {TAIL}"
        )
    };
    let expected = vec![through("holderTouch"), through("holderTouchC")];
    let mutable = "// @subscript-c-member aggregate=\"Holder\" member=\"lay\" const=false";
    assert_eq!(
        messages(&[holder_mirror(mutable), holder_user(), main_file()]),
        expected
    );
    // No record: the member counts as non-`const`.
    assert_eq!(
        messages(&[holder_mirror(""), holder_user(), main_file()]),
        expected
    );
    // Twin: a `const` member.
    let constant = "// @subscript-c-member aggregate=\"Holder\" member=\"lay\" const=true";
    assert!(messages(&[holder_mirror(constant), holder_user(), main_file()]).is_empty());
}

fn outer_mirror() -> SourceFile {
    SourceFile::ambient(
        "a.d.ts",
        "// @subscript-c-header include=\"a.h\"\n\
         declare class Lay { tag: i32; ud: object | null; constructor(tag: i32, ud: object | null); }\n\
         declare class Outer { k: i32; inner: Lay; constructor(k: i32, inner: Lay); }\n",
    )
}

fn info_mirror() -> SourceFile {
    SourceFile::ambient(
        "a.d.ts",
        "// @subscript-c-header include=\"a.h\"\n\
         // @subscript-c-callback typedef=\"Cb\"\n\
         type Cb = (message: string, userdata1: object | null, userdata2: object | null) => void;\n\
         declare class Info { callback: Cb; userdata1: object | null; userdata2: object | null; \
         constructor(callback: Cb, userdata1: object | null, userdata2: object | null); }\n",
    )
}

/// A second mirror whose struct embeds a struct of the first mirror, passed
/// through a pointer with the given `const` record.
fn box_user(external: &str, record: &str) -> SourceFile {
    let field = external.to_lowercase();
    SourceFile::ambient(
        "b.d.ts",
        format!(
            "// @subscript-c-header include=\"b.h\"\n\
             // @subscript-c-external type=\"{external}\"\n{record}\
             declare class Box {{ k: i32; {field}: {external}; constructor(k: i32, {field}: {external}); }}\n\
             declare function boxUse(box: Box | null): void;\n"
        ),
    )
}

/// A fill reads every member of the struct (rule 1); a `const` input of
/// the same shape writes nothing that C changes.
#[test]
fn a_fill_reads_an_embedded_struct_and_a_const_input_does_not() {
    let fill = "// @subscript-c-parameter function=\"boxUse\" parameter=\"box\" const=false\n";
    let input = "// @subscript-c-parameter function=\"boxUse\" parameter=\"box\" const=true\n";
    let pair = vec![format!(
        "foreign function `boxUse` parameter `box` reads `Lay.ud`, a userdata field that C \
         can write {TAIL}"
    )];
    assert_eq!(
        messages(&[outer_mirror(), box_user("Outer", fill), main_file()]),
        pair
    );
    // No record: the pointer counts as non-`const`.
    assert_eq!(
        messages(&[outer_mirror(), box_user("Outer", ""), main_file()]),
        pair
    );
    assert!(messages(&[outer_mirror(), box_user("Outer", input), main_file()]).is_empty());
    // §187 rule 8: `Box` embeds the scratch struct `Info`, whose copy-back
    // skips the callback field and its userdata slots.
    assert!(messages(&[info_mirror(), box_user("Info", fill), main_file()]).is_empty());
    assert!(messages(&[info_mirror(), box_user("Info", input), main_file()]).is_empty());
}

/// A handle passed by value is no struct, so the scan reads nothing.
#[test]
fn a_handle_by_value_is_no_struct() {
    let owner = SourceFile::ambient(
        "a.d.ts",
        "// @subscript-c-header include=\"a.h\"\n\
         interface SubDevice { readonly __sub_handle_SubDevice: never; }\n\
         declare function deviceMake(): SubDevice;\n",
    );
    let user = SourceFile::ambient(
        "b.d.ts",
        "// @subscript-c-header include=\"b.h\"\n\
         // @subscript-c-external type=\"SubDevice\"\n\
         declare function deviceTag(device: SubDevice): u32;\n",
    );
    assert!(messages(&[owner, user, main_file()]).is_empty());
    // Control: the same position with a struct that has a mutable pointer
    // member.
    let user = SourceFile::ambient(
        "b.d.ts",
        "// @subscript-c-header include=\"b.h\"\n\
         // @subscript-c-external type=\"Holder\"\n\
         declare function deviceTag(device: Holder): u32;\n",
    );
    assert_eq!(messages(&[holder_mirror(""), user, main_file()]).len(), 1);
}

/// A hand-written mirror can declare a callback parameter of a foreign
/// function whose argument is a struct. The mirror rule rejects a direct
/// callback parameter; the read scan is total, so it also reads the
/// argument as a value (§187 rule 1) and names its innermost member.
#[test]
fn a_callback_parameter_of_a_foreign_function_is_read() {
    let user = |callback: &str| {
        SourceFile::ambient(
            "b.d.ts",
            format!(
                "// @subscript-c-header include=\"b.h\"\n\
                 // @subscript-c-external type=\"Lay\"\n\
                 // @subscript-c-external type=\"Outer\"\n\
                 declare function layEach(each: {callback}): void;\n"
            ),
        )
    };
    let direct = "mirror `b.d.ts` foreign function `layEach` parameter `each` is a direct \
                  callback; callbacks are supported only as fields of mirrored boundary structs"
        .to_string();
    assert_eq!(
        messages(&[outer_mirror(), user("(lay: Lay) => void"), main_file()]),
        vec![
            direct.clone(),
            format!(
                "callback typedef `layEach.each` parameter `#0` reads `Lay.ud`, a userdata \
                 field that C can write {TAIL}"
            )
        ]
    );
    // Twin: the same callback with a scalar argument.
    assert_eq!(
        messages(&[outer_mirror(), user("(tag: i32) => void"), main_file()]),
        vec![direct]
    );
}
