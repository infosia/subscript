//! Unit tests of the §187 read predicate and the read-position scan.
//!
//! Cost: each test parses its headers in memory with libclang and starts
//! no child process. The predicate test parses one header. The scan test
//! parses two headers for each case: the violating form and an accepted
//! twin of the same shape, which proves that the rejection comes from the
//! member, not from the shape.

use super::*;
use crate::emit::classify;

/// The walk of [`first_unreadable_in`] over a view of `parsed`.
fn first_unreadable_member(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    aggregate: &str,
    root: ReadRoot,
    reach: Reach,
) -> Option<Unreadable> {
    first_unreadable_in(
        &HeaderStructs::new(parsed, registry),
        registry,
        aggregate,
        root,
        reach,
    )
}

const LEAVES: &str = "
#include <stdint.h>
#include <stddef.h>
typedef struct View { const char *data; size_t len; } View;
typedef void (*Cb)(void *ud);
typedef struct H_T *H;
typedef struct Obj { int32_t v; } Obj;
typedef struct Arr { const Obj *items; size_t count; } Arr;
typedef struct Pair { int32_t tag; size_t itemsCount; const uint32_t *items; } Pair;
typedef struct ObjPair { size_t objsCount; const Obj *objs; } ObjPair;
typedef struct Str { int32_t n; View label; } Str;
typedef struct Call { Cb cb; void *ud; } Call;
typedef struct Desc { Arr arr; } Desc;
typedef struct Plain { int32_t a; H h; void *ud; uint32_t fixed[4]; const Obj *ptr; Obj obj; } Plain;
typedef struct MutObjs { size_t objsCount; Obj *objs; } MutObjs;
typedef struct MutCalls { size_t callsCount; Call *calls; } MutCalls;
typedef struct Node { int32_t v; const struct Node *next; } Node;
";

/// The leaf structs, each wrapped at depth 1 (`Emb*`, embedded) and at
/// depth 2 (`Ptr*`, a pointer member to the embedding struct).
const NAMES: [&str; 6] = ["Pair", "ObjPair", "Str", "Call", "Desc", "Plain"];

fn header() -> String {
    let mut text = LEAVES.to_string();
    for leaf in NAMES {
        text.push_str(&format!(
            "typedef struct Emb{leaf} {{ int32_t k; {leaf} inner; }} Emb{leaf};\n\
             typedef struct Ptr{leaf} {{ int32_t k; const Emb{leaf} *inner; }} Ptr{leaf};\n"
        ));
    }
    text
}

fn found(
    owner: &str,
    member: &str,
    kind: UnreadableKind,
) -> Option<(String, String, UnreadableKind)> {
    Some((owner.to_string(), member.to_string(), kind))
}

#[test]
fn predicate_finds_each_member_kind_at_each_depth() {
    let parsed = crate::clangfe::parse(&header()).expect("parse");
    let registry = classify(&parsed);
    let read = |aggregate: &str, root| {
        first_unreadable_member(&parsed, &registry, aggregate, root, Reach::Root)
            .map(|found| (found.owner, found.member, found.kind))
    };
    let value = ReadRoot::Value;
    let fill = ReadRoot::Fill(StructPass::Scratch);
    let input = ReadRoot::Input(StructPass::Scratch);
    // A value is read at every depth: by embedding and behind a pointer.
    let copy = [
        ("Pair", found("Pair", "items", UnreadableKind::Pair)),
        ("ObjPair", found("ObjPair", "objs", UnreadableKind::Pair)),
        ("Str", found("Str", "label", UnreadableKind::StringView)),
        ("Call", found("Call", "cb", UnreadableKind::Callback)),
        (
            "Desc",
            found("Desc", "arr", UnreadableKind::DescriptorAggregate),
        ),
        // §187 rule 8: a userdata slot that C writes in a result.
        ("Plain", found("Plain", "ud", UnreadableKind::Userdata)),
    ];
    for (leaf, expected) in &copy {
        for root in [leaf.to_string(), format!("Emb{leaf}"), format!("Ptr{leaf}")] {
            assert_eq!(read(&root, value), *expected, "{root}");
        }
    }
    // A fill of a scratch struct: the copy-back keeps the script array of
    // a direct pair (§187 rule 4), materializes a string view at any depth
    // of by-value embedding (rule 2), and skips a userdata slot and a
    // callback field (rule 8). `Plain` copies its bytes, so embedded in a
    // scratch struct its userdata slot is C's bytes (rule 8). The `const`
    // pointer of `Ptr*` gives a target that C only reads.
    let userdata = found("Plain", "ud", UnreadableKind::Userdata);
    let scratch_direct = [
        ("Pair", None),
        ("ObjPair", None),
        ("Str", None),
        ("Call", None),
        (
            "Desc",
            found("Desc", "arr", UnreadableKind::DescriptorAggregate),
        ),
        ("Plain", None),
    ];
    for ((leaf, direct), (_, nested)) in scratch_direct.iter().zip(&copy) {
        assert_eq!(read(leaf, fill), *direct, "{leaf}");
        // Rule 11: a pair keeps its script array at every reach.
        let embedded = match *leaf {
            "Plain" => userdata.clone(),
            "Str" | "Pair" | "ObjPair" | "Call" => None,
            _ => nested.clone(),
        };
        assert_eq!(read(&format!("Emb{leaf}"), fill), embedded, "Emb{leaf}");
        assert_eq!(read(&format!("Ptr{leaf}"), fill), None, "Ptr{leaf}");
        // An input: C writes nothing that it holds.
        for root in [leaf.to_string(), format!("Emb{leaf}"), format!("Ptr{leaf}")] {
            assert_eq!(read(&root, input), None, "{root}");
        }
    }
    // A mutable direct pair: C writes elements that copy their bytes in
    // place; a pair of scratch elements needs a write-back of each element,
    // so it has no read lowering (rule 11).
    assert_eq!(read("MutObjs", fill), None);
    for root in [fill, input] {
        assert_eq!(
            read("MutCalls", root),
            found("MutCalls", "calls", UnreadableKind::WrittenBackPair)
        );
    }
    assert_eq!(
        read("MutObjs", value),
        found("MutObjs", "objs", UnreadableKind::Pair)
    );
    // A root that C writes and that is itself absorbed has no read
    // lowering.
    assert_eq!(
        read("Arr", value),
        found("Arr", "items", UnreadableKind::DescriptorAggregate)
    );
    assert_eq!(
        read("View", value),
        found("View", "data", UnreadableKind::StringView)
    );
    assert_eq!(read("View", input), None);
    // A pointer cycle ends.
    assert_eq!(read("Node", value), None);
    assert_eq!(read("Node", fill), None);
}

const SCAN_TYPES: &str = "
#include <stdint.h>
#include <stddef.h>
typedef struct View { const char *data; size_t len; } View;
typedef void (*Cb)(View message, void *userdata1, void *userdata2);
typedef struct Obj { int32_t v; } Obj;
typedef struct Pair { int32_t tag; size_t itemsCount; const uint32_t *items; } Pair;
typedef struct ObjPair { int32_t tag; size_t objsCount; const Obj *objs; } ObjPair;
typedef struct Call { Cb cb; void *ud; } Call;
typedef struct UdBox { int32_t k; void *ud; } UdBox;
typedef struct Outer { int32_t k; UdBox inner; } Outer;
";

const TAIL: &str =
    "with no read lowering; a struct that C writes and the script reads must not hold one \
     (compiler.md §187)";

/// One read position: the violating declarations, an accepted twin of the
/// same shape, and the diagnostic prefix.
struct Case {
    bad: &'static str,
    good: &'static str,
    message: &'static str,
}

#[test]
fn scan_rejects_each_read_position() {
    let cases = [
        Case {
            bad: "Pair get(void);",
            good: "Obj get(void);",
            message: "foreign function `get` result reads `Pair.items`, a count-first pair field",
        },
        Case {
            bad: "const Pair *get(void);",
            good: "const Obj *get(void);",
            message: "foreign function `get` result reads `Pair.items`, a count-first pair field",
        },
        // An out-parameter that the call fills with no scratch struct: C
        // writes its userdata slot in script memory (§187 rule 8). A
        // callback field and a userdata slot of a scratch struct are
        // accepted (`fillCall`).
        Case {
            bad: "typedef struct PB { int32_t k; void *ud; } PB; void fill(PB *out);",
            good: "typedef struct PB { int32_t k; void *ud; } PB; void fill(const PB *out); \
                   void fillCall(Call *out); \
                   typedef struct PS { View label; void *ud; } PS; void fillPs(PS *out);",
            message: "foreign function `fill` parameter `out` reads `PB.ud`, a userdata field \
                      that C can write",
        },
        // An out-parameter that embeds a struct: the member is read at
        // the embedded reach. Rule 11 keeps a pair at every reach, of
        // scalar and of struct elements.
        Case {
            bad: "void fill(Outer *out);",
            good: "void fill(Pair *out); void fillObjs(ObjPair *out); \
                   typedef struct ScalarOuter { int32_t k; Pair inner; } ScalarOuter; \
                   void fillScalar(ScalarOuter *out); \
                   typedef struct ObjOuter { int32_t k; ObjPair inner; } ObjOuter; \
                   void fillObjOuter(ObjOuter *out);",
            message: "foreign function `fill` parameter `out` reads `UdBox.ud`, a userdata field \
                      that C can write",
        },
        // §187 rule 8: a userdata slot that C can write, in a scratch
        // struct and in a struct whose bytes the call copies.
        Case {
            bad: "typedef struct PU { int32_t k; void *ud; } PU; void fill(PU *out);",
            good: "typedef struct PU { int32_t k; void *ud; } PU; void fill(const PU *out);",
            message: "foreign function `fill` parameter `out` reads `PU.ud`, a userdata field \
                      that C can write",
        },
        Case {
            bad: "typedef struct PU { int32_t k; void *ud; } PU; \
                  typedef struct PE { View label; PU inner; } PE; void fill(PE *out);",
            good: "typedef struct PU { int32_t k; void *ud; } PU; \
                   typedef struct PE { View label; PU inner; } PE; void fill(const PE *out);",
            message: "foreign function `fill` parameter `out` reads `PU.ud`, a userdata field \
                      that C can write",
        },
        // §187 rule 7: a pointer target that C can write is written back,
        // so a string view below it is read; behind a `const` pointer C
        // only reads it.
        Case {
            bad: "typedef struct T { View label; } T; \
                  typedef struct PS { int32_t k; T *t; } PS; void fill(PS *out);",
            good: "typedef struct T { View label; } T; \
                   typedef struct PS { int32_t k; const T *t; } PS; void fill(PS *out);",
            message: "foreign function `fill` parameter `out` reads `T.label`, a string-view \
                      field",
        },
        // A by-value root and a pointer root reach mutable pair elements.
        Case {
            bad: "typedef struct List { size_t laysCount; UdBox *lays; } List; \
                  void use(List list);",
            good: "typedef struct List { size_t laysCount; const UdBox *lays; } List; \
                   void use(List list);",
            message: "foreign function `use` parameter `list` through `List.lays` reads \
                      `UdBox.ud`, a userdata field that C can write",
        },
        Case {
            bad: "typedef struct List { size_t laysCount; UdBox *lays; } List; \
                  void use(const List *list);",
            good: "typedef struct List { size_t laysCount; const UdBox *lays; } List; \
                   void use(const List *list);",
            message: "foreign function `use` parameter `list` through `List.lays` reads \
                      `UdBox.ud`, a userdata field that C can write",
        },
        // §187 rule 9: a struct cycle at a pointer root, a by-value root,
        // and through a struct that reaches it. The twin links a struct
        // that copies its bytes.
        Case {
            bad: "typedef struct N { int32_t v; struct N *next; } N; void use(const N *n);",
            good: "typedef struct N { int32_t v; struct N *next; } N; void use(const Obj *n);",
            message: "foreign function `use` parameter `n` passes `N.next`, a member through \
                      which a struct reaches itself; a call cannot build a struct cycle as \
                      scratch copies (compiler.md §187)",
        },
        Case {
            bad: "typedef struct A { int32_t k; struct B *b; } A; \
                  typedef struct B { int32_t k; A *a; } B; void use(A a);",
            good: "typedef struct A { int32_t k; struct B *b; } A; \
                   typedef struct B { int32_t k; Obj *a; } B; void use(A a);",
            message: "foreign function `use` parameter `a` passes `B.a`, a member through which \
                      a struct reaches itself; a call cannot build a struct cycle as scratch \
                      copies (compiler.md §187)",
        },
        // A non-`const` struct-pointer member of an input struct.
        Case {
            bad: "typedef struct Holder { int32_t k; UdBox *box; } Holder; \
                  void use(const Holder *h);",
            good: "typedef struct Holder { int32_t k; const UdBox *box; } Holder; \
                   void use(const Holder *h);",
            message: "foreign function `use` parameter `h` through `Holder.box` reads \
                      `UdBox.ud`, a userdata field that C can write",
        },
    ];
    for case in cases {
        let make =
            |declarations: &str| format!("{SCAN_TYPES}\nvoid keep(const Call *c);\n{declarations}");
        crate::generate_for_header(&make(case.good), "t.h")
            .unwrap_or_else(|error| panic!("{}: {}", case.good, error.0));
        let error = crate::generate_for_header(&make(case.bad), "t.h").expect_err(case.bad);
        let expected = if case.message.ends_with("(compiler.md §187)") {
            case.message.to_string()
        } else {
            format!("{} {TAIL}", case.message)
        };
        assert_eq!(error.0, expected, "{}", case.bad);
    }
}

#[test]
fn scan_rejects_a_completion_result() {
    let make = |text: &str| {
        format!(
            "#include <stdint.h>\n#include <stddef.h>\n\
             typedef struct {{ uint64_t context_id; uint64_t operation_id; }} \
             subscript_rt_completion;\n\
             typedef struct {{ const char *data; size_t length; }} SubString;\n\
             typedef struct Res {{ {text} text; int32_t v; }} Res;\n\
             void read(int32_t id, subscript_rt_completion endpoint);\n"
        )
    };
    let options = crate::BindOptions::new().with_completion("read", "Res");
    crate::generate_with_options(&make("int32_t"), "t.h", &options).expect("scalar twin");
    let error = crate::generate_with_options(&make("SubString"), "t.h", &options)
        .expect_err("string-view field");
    assert_eq!(
        error.0,
        format!(
            "foreign function `read` completion result reads `Res.text`, a string-view field \
             {TAIL}"
        )
    );
}

/// The callback shape rule admits no struct parameter today, so the scan
/// reads this position on a parse that has not passed that rule.
#[test]
fn scan_rejects_a_callback_parameter() {
    let make = |parameter: &str| {
        format!(
            "{SCAN_TYPES}\ntypedef void (*OnPair)(const {parameter} *p, void *ud);\n\
             typedef struct Reg {{ OnPair cb; void *ud; }} Reg;\nvoid reg(const Reg *r);\n"
        )
    };
    let reachable = HashSet::from(["OnPair".to_string()]);
    let scan = |parameter: &str| {
        let parsed = crate::clangfe::parse(&make(parameter)).expect("parse");
        validate(&parsed, &reachable)
    };
    scan("Obj").expect("plain twin");
    let error = scan("Pair").expect_err("pair parameter");
    assert_eq!(
        error.0,
        format!(
            "callback typedef `OnPair` parameter `p` reads `Pair.items`, a count-first pair \
             field {TAIL}"
        )
    );
}

/// §187 rule 2: under a fill that builds a scratch struct, a string view
/// at depth 1 or 2 of by-value embedding has a read lowering. The same
/// view behind a non-`const` pointer member has none.
#[test]
fn a_fill_reads_an_embedded_string_view_and_not_one_behind_a_pointer() {
    let make = |declarations: &str| {
        format!(
            "#include <stdint.h>\n#include <stddef.h>\n\
             typedef struct View {{ const char *data; size_t len; }} View;\n\
             typedef struct Str {{ int32_t n; View label; }} Str;\n{declarations}"
        )
    };
    let embedded = [
        "typedef struct D1 { int32_t k; Str s; } D1; void fill(D1 *out);",
        "typedef struct D1 { int32_t k; Str s; } D1; \
         typedef struct D2 { int32_t k; D1 d; } D2; void fill(D2 *out);",
    ];
    for declarations in embedded {
        crate::generate_for_header(&make(declarations), "t.h")
            .unwrap_or_else(|error| panic!("{declarations}: {}", error.0));
    }
    let pointer = [
        "typedef struct D1 { int32_t k; Str *s; } D1; void fill(D1 *out);",
        "typedef struct D1 { int32_t k; Str s; } D1; \
         typedef struct D2 { int32_t k; D1 *d; } D2; void fill(D2 *out);",
    ];
    for declarations in pointer {
        let error = crate::generate_for_header(&make(declarations), "t.h").expect_err(declarations);
        assert_eq!(
            error.0,
            "foreign function `fill` parameter `out` reads `Str.label`, a string-view field \
             with no read lowering; a struct that C writes and the script reads must not hold \
             one (compiler.md §187)",
            "{declarations}"
        );
    }
}

/// The binder's pass decision for each struct and pointer member: a
/// struct that copies its bytes, a scratch struct whose pointer target
/// copies its bytes, a scratch struct whose target is a scratch struct,
/// and an embedded-header link. The cli test `read_root.rs` compares the
/// same decision with code generation.
#[test]
fn boundary_passes_gives_each_struct_and_pointer_its_pass() {
    use subscript_boundary::{PointerPass, StructPass};
    let header = "
#include <stdint.h>
#include <stddef.h>
typedef struct View { const char *data; size_t len; } View;
typedef struct P { int32_t x; } P;
typedef struct T { View label; } T;
typedef struct Holder { int32_t k; P *p; T *t; } Holder;
typedef struct Chain { int32_t kind; struct Chain *next; } Chain;
typedef struct Ext { Chain chain; int32_t extra; } Ext;
void use(const Holder *holder, const T *t, const Ext *ext, const Chain *chain);
";
    let passes = crate::boundary_passes(header).expect("parse");
    let summary: Vec<_> = passes
        .iter()
        .map(|entry| (entry.name.as_str(), entry.pass, entry.pointers.clone()))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("P", StructPass::Bytes, vec![]),
            ("T", StructPass::Scratch, vec![]),
            (
                "Holder",
                StructPass::Scratch,
                vec![
                    ("p".to_string(), PointerPass::ScriptMemory),
                    ("t".to_string(), PointerPass::ScratchWrittenBack),
                ],
            ),
            (
                "Chain",
                StructPass::Bytes,
                vec![("next".to_string(), PointerPass::ScriptMemory)],
            ),
            ("Ext", StructPass::Bytes, vec![]),
        ]
    );
}
