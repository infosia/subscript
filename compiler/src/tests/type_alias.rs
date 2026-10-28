//! Transparent type aliases (compiler.md §202).
//!
//! Cost: nineteen checker calls on programs of one or two files, with
//! no code generation and no external process; about 0.03 s in a debug
//! build.

use super::*;
use crate::divergence::Divergence;

fn functions(src: &str) -> Vec<hir::Function> {
    check_program(&[SourceFile::entry("main.ts", src)])
        .expect("program checks")
        .functions
}

fn rejection(src: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::entry("main.ts", src)]).expect_err("program is rejected")
}

/// Each alias name has the length of its aliased type, so every
/// position in the function lines is the same in both programs. The
/// alias declarations follow the functions (rule 3).
const BODY: &str = "class Pt { x: i32 = 0; }\n\
     function f(a: Idx, b: Reals, c: Ptx, d: Fnx): Idx { return a + b.length + c.x + d(a); }\n\
     export function main(): void { print(`${f(1, [2], new Pt(), (v: i32): i32 => v)}`); }\n";

#[test]
fn an_alias_and_its_aliased_type_give_the_same_hir() {
    let aliased = functions(&format!(
        "{BODY}type Idx = i32; type Reals = f64[]; type Ptx = Pt; type Fnx = (v: i32) => i32;\n"
    ));
    let written = functions(
        &BODY
            .replace("a: Idx", "a: i32")
            .replace("): Idx", "): i32")
            .replace("b: Reals", "b: f64[]")
            .replace("c: Ptx", "c: Pt ")
            .replace("d: Fnx", "d: (v: i32) => i32"),
    );
    let f = |all: &[hir::Function]| {
        all.iter()
            .find(|function| function.name == "f")
            .cloned()
            .expect("f")
    };
    let (alias_f, written_f) = (f(&aliased), f(&written));
    assert_eq!(alias_f.params, written_f.params);
    assert_eq!(alias_f.ret, written_f.ret);
    // The written program spells `d` longer, so only `f`'s signature
    // and the body of `main` compare; `main` has no alias.
    let main = |all: &[hir::Function]| {
        all.iter()
            .find(|function| function.name == "main")
            .map(|function| format!("{:?}", function.body))
            .expect("main")
    };
    assert_eq!(main(&aliased), main(&written));

    // Control: a different aliased type gives a different signature.
    let other = functions(&format!(
        "{BODY}type Idx = i32; type Reals = f32[]; type Ptx = Pt; type Fnx = (v: i32) => i32;\n"
    ));
    assert_ne!(f(&other).params, written_f.params);
}

#[test]
fn an_alias_names_a_later_declaration() {
    let all = functions(
        "type Early = Later;\n\
         type Later = Box;\n\
         type Shade = Color;\n\
         class Box { v: i32 = 3; }\n\
         enum Color { Red, Green }\n\
         function read(b: Early, c: Shade): i32 { return c === Color.Green ? b.v : 0; }\n\
         export function main(): void { print(`${read(new Box(), Color.Green)}`); }\n",
    );
    let read = all
        .iter()
        .find(|function| function.name == "read")
        .expect("read");
    assert!(
        matches!(read.params[0].ty, Type::Class(_)),
        "{:?}",
        read.params[0].ty
    );
    assert!(
        matches!(read.params[1].ty, Type::Enum(_)),
        "{:?}",
        read.params[1].ty
    );
}

#[test]
fn an_alias_that_reaches_itself_is_one_diagnostic() {
    // A cycle of direct references: `tsc` rejects it (TS2456).
    let diagnostics = rejection(
        "type A = B;\ntype B = A;\n\
         export function main(): void { const a: A = 1; const b: B = 2; }\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].message, "type alias `A` refers to itself");
    assert_eq!(diagnostics[0].divergence, None);

    // A cycle through an array element: `tsc` accepts it.
    for src in [
        "type L = L[];\nexport function main(): void { const l: L = []; }\n",
        "type C = D[];\ntype D = C;\nexport function main(): void { const c: C = []; }\n",
    ] {
        let diagnostics = rejection(src);
        assert_eq!(diagnostics.len(), 1, "{src}: {diagnostics:?}");
        assert!(
            diagnostics[0].message.ends_with("refers to itself"),
            "{src}"
        );
        assert_eq!(
            diagnostics[0].divergence,
            Some(Divergence::SourceAliasRecursiveForm),
            "{src}"
        );
    }

    // Control: the same shapes with no cycle check cleanly.
    functions("type L = i32[];\nexport function main(): void { const l: L = []; }\n");
    functions("type C = D[];\ntype D = i32;\nexport function main(): void { const c: C = []; }\n");
}

#[test]
fn a_rejected_aliased_type_is_one_diagnostic_at_the_alias() {
    let diagnostics = rejection(
        "type Num = i32 | f64;\n\
         type Nums = Num[];\n\
         export function main(): void { const n: Num = 1; const m: Nums = []; }\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(diagnostics[0].pos.line, 1);
}

#[test]
fn an_async_result_alias_of_promise_stays_rejected() {
    let diagnostics = rejection(
        "type Result = Promise<void>;\nasync function f(): Result {}\n\
         export function main(): void {}\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "async functions must return an explicitly annotated builtin `Promise<T>`"
    );
}

#[test]
fn an_alias_of_a_literal_union_is_the_same_closed_set() {
    let all = functions(
        "type Label = \"ready\" | \"done\";\n\
         type State = Label;\n\
         function tag(s: State): string {\n\
           switch (s) { case \"ready\": return \"r\"; case \"done\": return \"d\"; }\n\
         }\n\
         export function main(): void { print(tag(\"done\")); }\n",
    );
    let tag = all
        .iter()
        .find(|function| function.name == "tag")
        .expect("tag");
    assert!(
        matches!(tag.params[0].ty, Type::StringAlias(_)),
        "{:?}",
        tag.params[0].ty
    );
}

#[test]
fn an_alias_of_a_generic_instance_resolves_before_its_members() {
    // A member names a later alias.
    let module = check_program(&[SourceFile::entry(
        "main.ts",
        "type Stack = Pile<i32>;\ntype Count = i32;\n\
         class Pile<T> { n: Count = 0; }\n\
         export function main(): void { const s: Stack = new Pile<i32>(); print(`${s.n}`); }\n",
    )])
    .expect("an instance member can name a later alias");
    let n = module
        .classes
        .iter()
        .flat_map(|class| &class.fields)
        .find(|field| field.name == "n")
        .expect("n");
    assert_eq!(n.ty, Type::I32);
    // A member names the alias that names its own instance.
    let module = check_program(&[SourceFile::entry(
        "main.ts",
        "type List = Link<i32>;\n\
         class Link<T> { v: T; next: List | null = null; constructor(v: T) { this.v = v; } }\n\
         export function main(): void { const a: List = new Link<i32>(1); print(`${a.v}`); }\n",
    )])
    .expect("an instance member can name the alias of that instance");
    let link = module
        .classes
        .iter()
        .find(|class| class.fields.iter().any(|field| field.name == "next"))
        .expect("Link<i32>");
    let next = link
        .fields
        .iter()
        .find(|field| field.name == "next")
        .expect("next");
    assert!(
        !format!("{:?}", next.ty).contains("Error"),
        "member type {:?}",
        next.ty
    );
}

#[test]
fn a_type_argument_on_an_alias_is_rejected() {
    let diagnostics =
        rejection("type Id = i32;\nexport function main(): void { const x: Id<i32> = 1; }\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].message, "type alias `Id` is not generic");
    assert_eq!(diagnostics[0].divergence, None);
    // A mirror alias follows the same rule.
    let mirror = SourceFile::ambient(
        "flags.d.ts",
        "// @subscript-c-header include=\"flags.h\"\ntype Flags = u64;\n",
    );
    let diagnostics = check_program(&[
        mirror.clone(),
        SourceFile::entry(
            "main.ts",
            "export function main(): void { const f: Flags<i32> = 1; }\n",
        ),
    ])
    .expect_err("type argument on a mirror alias");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].message, "type alias `Flags` is not generic");
    // Control: the alias with no type argument checks.
    check_program(&[
        mirror,
        SourceFile::entry(
            "main.ts",
            "export function main(): void { const f: Flags = 1; print(`${f}`); }\n",
        ),
    ])
    .expect("mirror alias with no type argument");
}

#[test]
fn constructing_an_alias_is_a_value_use() {
    for src in [
        "class Point { x: i32 = 0; }\ntype P = Point;\nexport function main(): void { new P(); }\n",
        "class Point { static make(): i32 { return 1; } }\ntype P = Point;\nexport function main(): void { P.make(); }\n",
    ] {
        let diagnostics = rejection(src);
        assert_eq!(diagnostics.len(), 1, "{src}: {diagnostics:?}");
        assert_eq!(diagnostics[0].message, "type alias `P` used as a value", "{src}");
    }
}
