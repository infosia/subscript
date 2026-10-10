//! `specs/blocks/compiler.md` §187 rule 3, end to end: the binder binds
//! each header to its mirror, and the checker reads the generated mirrors
//! as the total scan. Each case pairs a form that C writes with a `const`
//! twin of the same shape.
//!
//! Cost: each case writes three small headers to a temporary directory,
//! binds them in memory with libclang (one parse each), and checks the
//! mirrors in memory. No child process starts. The file runs in under
//! 0.2 s.

use std::fs;
use std::path::PathBuf;

use subscript_bindgen::generate_for_header;
use subscript_compiler::{check_program, SourceFile};

const TAIL: &str = "with no read lowering; a struct that C writes and the script reads must \
                    not hold one (compiler.md §187)";

/// A temporary directory for one case's headers.
struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "subscript-s187-records-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create directory");
        Self(path)
    }

    /// Writes `header` and returns the `#include` line that names it.
    fn header(&self, name: &str, text: &str) -> String {
        let path = self.0.join(name);
        fs::write(&path, text).expect("write header");
        format!("#include \"{}\"\n", path.display())
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Binds each `(name, header)` and checks the mirrors with an empty
/// program. Returns the diagnostic messages.
fn bind_and_check(headers: &[(&str, String)]) -> Vec<String> {
    let mut files: Vec<SourceFile> = headers
        .iter()
        .map(|(name, text)| {
            let mirror = generate_for_header(text, name)
                .unwrap_or_else(|error| panic!("{name}: {}", error.0));
            SourceFile::ambient(format!("{name}.d.ts"), mirror)
        })
        .collect();
    files.push(SourceFile::new(
        "main.ts",
        "export function main(): void {}\n",
    ));
    match check_program(&files) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect(),
    }
}

/// `Lay` holds a userdata slot, which has no read lowering where C writes
/// it (§187 rule 8); a pair has a read lowering at every reach of a fill
/// (rule 11).
const LAY: &str = "#include <stdint.h>\n#include <stddef.h>\n\
                   typedef struct Lay { int32_t tag; void *ud; } Lay;\n\
                   int32_t layTag(const Lay *lay);\n";

/// A struct of one mirror points to a struct that a third header defines,
/// and a second header uses it through an include. The checker reads the
/// fill and the by-value root through the mirror classes.
#[test]
fn an_external_reached_through_an_include_is_read() {
    let case = |name: &str, qualifier: &str| {
        let directory = Directory::new(name);
        let c = LAY.to_string();
        let a = format!(
            "#include <stdint.h>\n/* @subscript-external Lay */\n\
             typedef struct Outer {{ int32_t k; {qualifier}struct Lay *lay; }} Outer;\n"
        );
        let b = format!(
            "{}/* @subscript-external Outer */\n\
             void outerFill(Outer *out);\nvoid outerTouch(Outer o);\nint32_t outerUse(const Outer *o);\n",
            directory.header("a.h", &a)
        );
        bind_and_check(&[("c.h", c), ("a.h", a), ("b.h", b)])
    };
    assert_eq!(
        case("include", ""),
        [
            format!(
                "foreign function `outerFill` parameter `out` reads `Lay.ud`, a userdata field \
                 that C can write {TAIL}"
            ),
            format!(
                "foreign function `outerTouch` parameter `o` through `Outer.lay` reads \
                 `Lay.ud`, a userdata field that C can write {TAIL}"
            ),
            format!(
                "foreign function `outerUse` parameter `o` through `Outer.lay` reads \
                 `Lay.ud`, a userdata field that C can write {TAIL}"
            ),
        ]
    );
    // Twin: `const struct Lay *lay`. C only reads `Lay`, at the inputs and
    // under the fill; the fill keeps the script link (§187 rule 7).
    assert!(case("include-const", "const ").is_empty());
}

/// A declaration-only header records the `const` of its members, so a
/// `const` member of an input stays accepted.
#[test]
fn a_declaration_only_header_records_its_members() {
    let case = |qualifier: &str| {
        let types =
            format!("{LAY}typedef struct Holder {{ int32_t k; {qualifier}Lay *lay; }} Holder;\n")
                .replace("int32_t layTag(const Lay *lay);\n", "");
        let user = "#include <stdint.h>\n/* @subscript-external Holder */\n\
                    int32_t holderUse(Holder h);\n"
            .to_string();
        bind_and_check(&[("types.h", types), ("use.h", user)])
    };
    assert!(case("const ").is_empty());
    assert_eq!(
        case(""),
        [format!(
            "foreign function `holderUse` parameter `h` through `Holder.lay` reads `Lay.ud`, \
             a userdata field that C can write {TAIL}"
        )]
    );
}

/// A hand-written mirror with no `@subscript-c-parameter` record counts its
/// pointer parameter as non-`const`: a fill that reads a nested userdata slot is
/// rejected.
#[test]
fn a_hand_written_mirror_with_no_record_fails_closed() {
    let check = |records: &str| {
        let files = [
            SourceFile::ambient(
                "a.d.ts",
                "// @subscript-c-header include=\"a.h\"\n\
                 declare class Lay { tag: i32; ud: object | null; constructor(tag: i32, ud: object | null); }\n\
                 declare class Outer { k: i32; inner: Lay; constructor(k: i32, inner: Lay); }\n",
            ),
            SourceFile::ambient(
                "b.d.ts",
                format!(
                    "// @subscript-c-header include=\"b.h\"\n{records}\
                     declare function outerFill(out: Outer | null): void;\n"
                ),
            ),
            SourceFile::new("main.ts", "export function main(): void {}\n"),
        ];
        match check_program(&files) {
            Ok(_) => Vec::new(),
            Err(diagnostics) => diagnostics.into_iter().map(|d| d.message).collect(),
        }
    };
    let rejected = vec![format!(
        "foreign function `outerFill` parameter `out` reads `Lay.ud`, a userdata field that \
         C can write {TAIL}"
    )];
    assert_eq!(check(""), rejected);
    assert_eq!(
        check("// @subscript-c-parameter function=\"outerFill\" parameter=\"out\" const=false\n"),
        rejected
    );
    assert!(check(
        "// @subscript-c-parameter function=\"outerFill\" parameter=\"out\" const=true\n"
    )
    .is_empty());
}
