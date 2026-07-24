//! Q13 boundary mapping and `.d.ts` emission
//! (`specs/blocks/collisions.md` §2, `specs/blocks/compiler.md` §12.2,
//! §13.2).

use std::collections::{HashMap, HashSet};

use crate::clangfe::{Alias, Parsed};
use crate::cparse::{CField, Decl, ParseError};

/// The boundary role of a named C type, used to map its use sites.
#[derive(Debug, Clone)]
enum Kind {
    Enum,
    /// `(pointer, count)` descriptor → `elem[]` at use sites.
    ArrayPair(String),
    /// `{ const char*; size_t; }` → `string` at use sites.
    StringView,
    Handle,
    FnPtr,
    Boundary,
    /// A scalar `typedef` alias (flag typedef, §13.2): use sites spell the
    /// alias name; the alias itself is emitted as a `type X = <scalar>`.
    Alias,
}

const HEADER: &str = "\
// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from the
// pinned synthetic interop header (corpus/interop/interop.h). Hand edits
// are overwritten; the byte-identical regeneration test
// (specs/blocks/compiler.md §12.2) fails on drift. Fix the generator,
// never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.
";

/// Emits the mirror text for the parsed header.
///
/// # Errors
///
/// Returns [`ParseError`] when a boundary use site names a C type that is
/// neither a mapped scalar/builtin nor a registered named type
/// (struct/enum/handle/alias/array-pair/string-view): the emitter fails
/// loud rather than write an invalid mirror (`specs/blocks/compiler.md`
/// §13.2).
pub fn emit(parsed: &Parsed) -> Result<String, ParseError> {
    let registry = classify(parsed);
    let mut blocks: Vec<String> = Vec::new();
    let mut pending_fns: Vec<String> = Vec::new();

    let flush = |pending: &mut Vec<String>, blocks: &mut Vec<String>| {
        if !pending.is_empty() {
            blocks.push(pending.join("\n"));
            pending.clear();
        }
    };

    for decl in &parsed.decls {
        match decl {
            Decl::Enum { name, members } => {
                flush(&mut pending_fns, &mut blocks);
                blocks.push(emit_enum(name, members));
            }
            Decl::FnPtr { name, ret, params } => {
                flush(&mut pending_fns, &mut blocks);
                blocks.push(emit_fn_ptr(name, ret, params, &registry)?);
            }
            Decl::Handle { name } => {
                flush(&mut pending_fns, &mut blocks);
                blocks.push(emit_handle(name));
            }
            Decl::Struct { name, fields } => match registry.get(name) {
                // Array-pair descriptors and string views are absorbed
                // into `T[]` / `string` at use sites; no type is emitted.
                Some(Kind::ArrayPair(_)) | Some(Kind::StringView) => {}
                _ => {
                    flush(&mut pending_fns, &mut blocks);
                    blocks.push(emit_struct(name, fields, &registry)?);
                }
            },
            Decl::Func { name, ret, params } => {
                pending_fns.push(emit_func(name, ret, params, &registry)?);
            }
        }
    }
    flush(&mut pending_fns, &mut blocks);

    // Flag typedefs (§13.2): a scalar `typedef` alias emitted as a
    // `type X = <scalar>` plus its `static const` members as `declare const`
    // globals. The member value is folded into the mirror (a bare literal
    // initializer, which tsc accepts on an ambient const only without a
    // type annotation); the language types such a mirror member `u64` and
    // folds the value at each reference. Emitted after the declarations; TS
    // hoists type aliases, so a foreign signature using the alias may
    // precede it here.
    for alias in &parsed.aliases {
        let Some(scalar) = flag_alias_scalar(alias) else {
            continue;
        };
        let mut block = format!("type {} = {scalar};", alias.name);
        for c in &parsed.constants {
            if c.type_base == alias.name {
                // The mirror carries the value as a bare numeric literal,
                // which the language reads through the f64-exact integer
                // range (collisions.md C3: no i64/u64 literal surface above
                // 2^53-1). A larger flag value would truncate silently when
                // folded, so refuse to mirror it — fail loud at the source.
                if !exact_f64_integer(c.value) {
                    return Err(ParseError(format!(
                        "flag member `{}` has value {} outside the exactly-f64-\
                         representable integer range (|v| <= 2^53-1); the language \
                         has no integer-literal surface above 2^53 (collisions.md \
                         C3), so this flag cannot be mirrored as a folded constant",
                        c.name, c.value
                    )));
                }
                block.push_str(&format!("\ndeclare const {} = {};", c.name, c.value));
            }
        }
        blocks.push(block);
    }

    let mut out = String::from(HEADER);
    for block in &blocks {
        out.push('\n');
        out.push_str(block);
        out.push('\n');
    }
    Ok(out)
}

/// Builds the name→role registry from the declarations and scalar aliases.
fn classify(parsed: &Parsed) -> HashMap<String, Kind> {
    let mut reg = HashMap::new();
    for decl in &parsed.decls {
        match decl {
            Decl::Enum { name, .. } => {
                reg.insert(name.clone(), Kind::Enum);
            }
            Decl::Handle { name } => {
                reg.insert(name.clone(), Kind::Handle);
            }
            Decl::FnPtr { name, .. } => {
                reg.insert(name.clone(), Kind::FnPtr);
            }
            Decl::Struct { name, fields } => {
                reg.insert(name.clone(), classify_struct(fields));
            }
            Decl::Func { .. } => {}
        }
    }
    // A scalar typedef alias resolves to its own name at use sites (the
    // emitted `type X = <scalar>` alias). Only aliases whose typedef chain
    // bottoms out in a mapped integer are registered; others are left
    // unmapped so a use site fails loud (§14.1).
    for alias in &parsed.aliases {
        if alias_scalar(alias).is_some() {
            reg.insert(alias.name.clone(), Kind::Alias);
        }
    }
    reg
}

/// A two-field `{ P*; size_t; }` struct is a string view when `P` is a
/// `const char`, a scalar `(pointer, count)` array-pair descriptor when `P`
/// is a `const` scalar, or a boundary-struct array-pair descriptor when `P`
/// is a named struct (§14.5). For a struct element the pointer may be const
/// (a borrow) or non-const (a callee-written out-array, §14.3) — both map
/// to `T[]`, since the marshaling is identical and const-ness is the C
/// callee's concern. Everything else is a boundary struct (embedded array
/// pairs inside it are collapsed field-by-field at emission, not here —
/// §13.2).
fn classify_struct(fields: &[CField]) -> Kind {
    if fields.len() == 2
        && fields[0].pointer
        && fields[0].array_len.is_none()
        && !fields[1].pointer
        && fields[1].base == "size_t"
    {
        // Const scalar / char descriptor (const borrow, §12).
        if fields[0].is_const {
            if fields[0].base == "char" {
                return Kind::StringView;
            }
            if let Some(elem) = lang_scalar(&fields[0].base) {
                return Kind::ArrayPair(elem.to_string());
            }
        }
        // Named-struct element → a boundary-struct array pair (§14.5); the
        // element spelling is the struct's own name. `void` is not an
        // element type (an untyped bulk pointer takes the §13.2 facade).
        if lang_scalar(&fields[0].base).is_none()
            && fields[0].base != "char"
            && fields[0].base != "void"
        {
            return Kind::ArrayPair(fields[0].base.clone());
        }
    }
    Kind::Boundary
}

/// The language spelling of a flag typedef's element scalar, when the
/// alias is over a mapped scalar; `None` otherwise (that alias is skipped).
fn flag_alias_scalar(alias: &Alias) -> Option<&'static str> {
    alias_scalar(alias)
}

/// Resolves a scalar/flag typedef alias to its language integer by
/// following its typedef chain to the first spelling that maps (§14.1):
/// `typedef uint32_t B; typedef B X;` resolves `X` to `u32`. A chain that
/// never reaches a mapped integer resolves to `None` (unregistered → its
/// use site fails loud). The immediate underlying is checked first, then
/// each deeper link, so a stdint spelling (`int64_t`, `size_t`) is honored
/// before its target-dependent canonical builtin (`long`, `unsigned long`)
/// which is deliberately unmapped.
fn alias_scalar(alias: &Alias) -> Option<&'static str> {
    if let Some(s) = lang_scalar(&alias.underlying) {
        return Some(s);
    }
    alias.chain.iter().find_map(|s| lang_scalar(s))
}

/// True when `v` is exactly representable as an `f64` integer
/// (`|v| <= 2^53 - 1`) — the range a bare numeric literal survives without
/// silent rounding (collisions.md C3).
fn exact_f64_integer(v: i64) -> bool {
    v.unsigned_abs() <= (1u64 << 53) - 1
}

/// Descriptor-embedded `(count, pointer)` array pairs within a struct
/// (§13.2): the index of each `const T*` pointer field mapped to its
/// element scalar, and the set of `size_t` count-field indices to elide.
#[derive(Default)]
struct EmbeddedPairs {
    /// Pointer field index → element scalar spelling (`u32`, `f32`, …).
    ptr_elem: HashMap<usize, String>,
    /// Count field indices to omit from the mirror.
    count_idx: HashSet<usize>,
}

/// Recognizes embedded `(count, pointer)` array pairs (§13.2), and ONLY
/// the one shape both lowerings reconstruct: a `size_t` count field named
/// `<n>Count` or `<n>_count` **immediately followed** by a `const T*`
/// pointer field named `<n>` with a scalar element (count-first,
/// contiguous). The pointer field index maps to `T[]`; the count field
/// index is elided. Any other spelling — pointer-first, non-adjacent
/// count, a struct-element pointer — is deliberately NOT an embedded array:
/// the pointer then falls through to `map_use`, where a lone scalar
/// pointer has no boundary type and fails loud. Both lowerings marshal the
/// pair count-immediately-before-pointer, contiguous, so recognizing any
/// looser shape would silently mismarshal on both tiers (no `-Werror` on
/// the AOT tier; the JIT never compiles the C struct).
fn embedded_array_pairs(fields: &[CField]) -> EmbeddedPairs {
    let mut pairs = EmbeddedPairs::default();
    for i in 0..fields.len().saturating_sub(1) {
        let count = &fields[i];
        let ptr = &fields[i + 1];
        if count.pointer || count.array_len.is_some() || count.base != "size_t" {
            continue;
        }
        if !(ptr.pointer && ptr.is_const && ptr.array_len.is_none()) {
            continue;
        }
        let Some(elem) = lang_scalar(&ptr.base) else {
            continue;
        };
        let want1 = format!("{}Count", ptr.name);
        let want2 = format!("{}_count", ptr.name);
        if count.name == want1 || count.name == want2 {
            pairs.count_idx.insert(i);
            pairs.ptr_elem.insert(i + 1, elem.to_string());
        }
    }
    pairs
}

fn emit_enum(name: &str, members: &[(String, i64)]) -> String {
    let mut s = format!("declare enum {name} {{\n");
    for (member, value) in members {
        s.push_str(&format!("  {member} = {value},\n"));
    }
    s.push('}');
    s
}

fn emit_handle(name: &str) -> String {
    // Branded empty interface: a phantom `never` property with a
    // per-handle-unique name makes distinct handles non-cross-assignable
    // under tsc, and no value can inhabit it (opaque). `unique symbol`
    // is not permitted on interface properties (tsc TS1332), so a named
    // `never` brand is used instead.
    format!(
        "interface {name} {{\n  readonly __sub_handle_{name}: never;\n}}"
    )
}

fn emit_fn_ptr(
    name: &str,
    ret: &CField,
    params: &[CField],
    reg: &HashMap<String, Kind>,
) -> Result<String, ParseError> {
    Ok(format!(
        "type {name} = ({}) => {};",
        param_sig(params, reg)?,
        map_use(ret, reg)?
    ))
}

fn emit_struct(
    name: &str,
    fields: &[CField],
    reg: &HashMap<String, Kind>,
) -> Result<String, ParseError> {
    let pairs = embedded_array_pairs(fields);
    let mut s = format!("declare class {name} {{\n");
    let mut ctor: Vec<String> = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        if pairs.count_idx.contains(&i) {
            continue;
        }
        let ty = match pairs.ptr_elem.get(&i) {
            Some(elem) => format!("{elem}[]"),
            None => map_use(f, reg)?,
        };
        s.push_str(&format!("  {}: {};\n", f.name, ty));
        ctor.push(format!("{}: {}", f.name, ty));
    }
    s.push_str(&format!("  constructor({});\n", ctor.join(", ")));
    s.push('}');
    Ok(s)
}

fn emit_func(
    name: &str,
    ret: &CField,
    params: &[CField],
    reg: &HashMap<String, Kind>,
) -> Result<String, ParseError> {
    Ok(format!(
        "declare function {name}({}): {};",
        param_sig(params, reg)?,
        map_use(ret, reg)?
    ))
}

fn param_sig(params: &[CField], reg: &HashMap<String, Kind>) -> Result<String, ParseError> {
    let mut out = Vec::with_capacity(params.len());
    for p in params {
        out.push(format!("{}: {}", p.name, map_use(p, reg)?));
    }
    Ok(out.join(", "))
}

/// Maps a C field/parameter/return type to its language boundary type
/// (Q13).
fn map_use(f: &CField, reg: &HashMap<String, Kind>) -> Result<String, ParseError> {
    if let Some(n) = f.array_len {
        // Fixed C array `T[N]` → `FixedArray<T, N>`.
        return Ok(format!("FixedArray<{}, {}>", map_element(&f.base, reg)?, n));
    }
    if f.pointer {
        // `void*` userdata → `object | null`; struct pointer → `X | null`.
        if f.base == "void" {
            return Ok("object | null".to_string());
        }
        return Ok(format!("{} | null", map_named(&f.base, reg)?));
    }
    if f.base == "void" {
        return Ok("void".to_string());
    }
    if let Some(scalar) = lang_scalar(&f.base) {
        return Ok(scalar.to_string());
    }
    map_named(&f.base, reg)
}

/// Maps a by-value named type to its language spelling per its role.
/// Fails loud on an unregistered name (§13.2): never emits a literal
/// unknown spelling into the mirror.
fn map_named(base: &str, reg: &HashMap<String, Kind>) -> Result<String, ParseError> {
    match reg.get(base) {
        Some(Kind::ArrayPair(elem)) => Ok(format!("{elem}[]")),
        Some(Kind::StringView) => Ok("string".to_string()),
        // Enum, FnPtr, Handle, Boundary, Alias all use their declared name.
        Some(_) => Ok(base.to_string()),
        None => Err(unmapped(base)),
    }
}

/// Maps a fixed-array element type (scalar, or a named by-value type).
fn map_element(base: &str, reg: &HashMap<String, Kind>) -> Result<String, ParseError> {
    match lang_scalar(base) {
        Some(s) => Ok(s.to_string()),
        None => map_named(base, reg),
    }
}

/// The fail-loud error for a boundary use site naming an unmapped C type.
fn unmapped(base: &str) -> ParseError {
    ParseError(format!(
        "unmapped C type `{base}` at a boundary use site: it is neither a mapped \
         scalar/builtin nor a registered named type (struct/enum/handle/alias/\
         array-pair/string-view). Refusing to emit an invalid mirror; add a \
         mapping or a typedef for this type."
    ))
}

/// Language spelling of a C scalar or raw builtin, or `None` for a named
/// type. The stdint typedefs and the width-stable raw builtins map to the
/// sized numerics (`specs/blocks/compiler.md` §13.2). Two deliberate
/// exclusions fail loud rather than mirror a target-dependent width:
///
/// - `long`/`unsigned long` — 64-bit on LP64 (Unix) but 32-bit on LLP64
///   (Windows); an ABI-stable header must spell a 64-bit int
///   `int64_t`/`long long`, so a bare `long` is unmapped.
/// - `char`/`signed char`/`unsigned char` — the string-view element only;
///   a standalone `char` scalar has no language type.
///
/// `int`/`unsigned int` (32-bit on every supported target) and `long
/// long`/`unsigned long long` (64-bit everywhere) are width-stable and
/// mapped.
fn lang_scalar(base: &str) -> Option<&'static str> {
    Some(match base {
        "bool" => "boolean",
        "float" => "f32",
        "double" => "f64",
        "int32_t" => "i32",
        "uint32_t" => "u32",
        "int64_t" => "i64",
        "uint64_t" => "u64",
        "size_t" => "u64",
        // Width-stable raw C builtins.
        "int" => "i32",
        "unsigned int" => "u32",
        "long long" => "i64",
        "unsigned long long" => "u64",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(base: &str, pointer: bool, is_const: bool, name: &str) -> CField {
        CField {
            base: base.into(),
            is_const,
            pointer,
            array_len: None,
            name: name.into(),
        }
    }

    fn parsed_with(decls: Vec<Decl>) -> Parsed {
        let mut p = Parsed::default();
        p.decls = decls;
        p
    }

    #[test]
    fn raw_builtins_map_to_sized_numerics() {
        // A struct whose fields are raw C builtins (no stdint typedef).
        let decls = vec![Decl::Struct {
            name: "SubRaw".into(),
            fields: vec![
                field("int", false, false, "a"),
                field("unsigned int", false, false, "b"),
                field("long long", false, false, "c"),
                field("unsigned long long", false, false, "d"),
                field("float", false, false, "e"),
                field("double", false, false, "f"),
            ],
        }];
        let m = emit(&parsed_with(decls)).expect("raw builtins map cleanly");
        assert!(m.contains("a: i32;"), "{m}");
        assert!(m.contains("b: u32;"), "{m}");
        assert!(m.contains("c: i64;"), "{m}");
        assert!(m.contains("d: u64;"), "{m}");
        assert!(m.contains("e: f32;"), "{m}");
        assert!(m.contains("f: f64;"), "{m}");
    }

    #[test]
    fn bare_char_scalar_field_is_a_clean_err() {
        // A standalone `char` scalar has no language type: fail loud, do
        // not emit a literal `char` into the mirror.
        let decls = vec![Decl::Struct {
            name: "SubHasChar".into(),
            fields: vec![field("char", false, false, "c")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("bare char must fail loud");
        assert!(err.0.contains("char"), "message names the type: {}", err.0);
    }

    #[test]
    fn double_pointer_field_is_a_clean_err() {
        // A double pointer surfaces as a pointer to an unnamed pointer type
        // (`SubThing *`), which is not a registered named type → fail loud.
        let decls = vec![Decl::Struct {
            name: "SubHasPP".into(),
            fields: vec![field("SubThing *", true, false, "pp")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("double pointer must fail loud");
        assert!(err.0.contains("SubThing"), "message names the type: {}", err.0);
    }

    #[test]
    fn anonymous_inline_struct_field_is_a_clean_err() {
        // An anonymous inline struct field carries an unnamed record
        // spelling that is not in the registry → fail loud.
        let decls = vec![Decl::Struct {
            name: "SubHasAnon".into(),
            fields: vec![field("SubOuter::(unnamed at header.h:3:5)", false, false, "inner")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("anonymous struct must fail loud");
        assert!(err.0.contains("unnamed"), "message names the type: {}", err.0);
    }

    #[test]
    fn embedded_count_pointer_pair_collapses_to_array() {
        // `uint32_t layer; size_t drawsCount; const uint32_t* draws;` →
        // the count field is elided and the pointer becomes `u32[]`.
        let decls = vec![Decl::Struct {
            name: "SubDrawList".into(),
            fields: vec![
                field("uint32_t", false, false, "layer"),
                field("size_t", false, false, "drawsCount"),
                field("uint32_t", true, true, "draws"),
            ],
        }];
        let m = emit(&parsed_with(decls)).expect("embedded pair maps");
        assert!(m.contains("layer: u32;"), "{m}");
        assert!(m.contains("draws: u32[];"), "{m}");
        assert!(!m.contains("drawsCount"), "count field is elided: {m}");
        assert!(
            m.contains("constructor(layer: u32, draws: u32[]);"),
            "constructor drops the count: {m}"
        );
    }

    #[test]
    fn pointer_first_embedded_shape_fails_loud() {
        // Pointer-before-count inside a larger struct is NOT the shape both
        // lowerings reconstruct (count immediately before pointer), so the
        // pointer stays a lone scalar pointer → fail loud. (A bare two-field
        // `{const T*; size_t}` is instead the standalone descriptor absorbed
        // to `T[]`, a26/a31 — a different, correct path; hence the third
        // field here forces the embedded interpretation.)
        let decls = vec![Decl::Struct {
            name: "SubPtrFirst".into(),
            fields: vec![
                field("uint32_t", false, false, "layer"),
                field("uint32_t", true, true, "draws"),
                field("size_t", false, false, "drawsCount"),
            ],
        }];
        let err = emit(&parsed_with(decls)).expect_err("pointer-first must fail loud");
        assert!(err.0.contains("uint32_t"), "{}", err.0);
    }

    #[test]
    fn non_adjacent_count_fails_loud() {
        // A count separated from the pointer by another field is not an
        // embedded pair; the pointer then fails loud as a lone scalar ptr.
        let decls = vec![Decl::Struct {
            name: "SubGap".into(),
            fields: vec![
                field("size_t", false, false, "drawsCount"),
                field("uint32_t", false, false, "layer"),
                field("uint32_t", true, true, "draws"),
            ],
        }];
        let err = emit(&parsed_with(decls)).expect_err("non-adjacent count must fail loud");
        assert!(err.0.contains("uint32_t"), "{}", err.0);
    }

    #[test]
    fn lone_scalar_pointer_field_fails_loud() {
        // A `const uint32_t*` field with no paired count is not an array
        // descriptor and has no boundary type → fail loud, not `u32 | null`.
        let decls = vec![Decl::Struct {
            name: "SubLonePtr".into(),
            fields: vec![field("uint32_t", true, true, "items")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("lone scalar pointer must fail loud");
        assert!(err.0.contains("uint32_t"), "{}", err.0);
    }

    #[test]
    fn bare_long_is_unmapped_and_fails_loud() {
        // `long`/`unsigned long` are target-width-dependent (LP64 vs LLP64)
        // and dropped from the builtin map.
        for spelling in ["long", "unsigned long"] {
            let decls = vec![Decl::Struct {
                name: "SubLong".into(),
                fields: vec![field(spelling, false, false, "n")],
            }];
            let err = emit(&parsed_with(decls)).expect_err("bare long must fail loud");
            assert!(err.0.contains("long"), "{spelling}: {}", err.0);
        }
    }

    #[test]
    fn flag_value_above_f64_exact_range_fails_loud() {
        let mut p = Parsed::default();
        p.aliases = vec![Alias {
            name: "SubBig".into(),
            underlying: "uint64_t".into(),
            chain: vec!["uint64_t".into()],
        }];
        p.constants = vec![crate::clangfe::Constant {
            name: "SUB_BIG_ONE".into(),
            type_base: "SubBig".into(),
            value: (1i64 << 53) + 1,
        }];
        let err = emit(&p).expect_err("flag value above 2^53 must fail loud");
        assert!(err.0.contains("SUB_BIG_ONE"), "{}", err.0);
        assert!(err.0.contains("2^53"), "{}", err.0);
    }

    #[test]
    fn flag_typedef_emits_alias_and_folded_members() {
        let mut p = Parsed::default();
        p.aliases = vec![Alias {
            name: "SubAccess".into(),
            underlying: "uint64_t".into(),
            chain: vec!["uint64_t".into()],
        }];
        p.constants = vec![
            Constant("SUB_ACCESS_READ", "SubAccess", 1),
            Constant("SUB_ACCESS_WRITE", "SubAccess", 2),
        ]
        .into_iter()
        .map(|c| c.into())
        .collect();
        let m = emit(&p).expect("flags emit");
        assert!(m.contains("type SubAccess = u64;"), "{m}");
        assert!(m.contains("declare const SUB_ACCESS_READ = 1;"), "{m}");
        assert!(m.contains("declare const SUB_ACCESS_WRITE = 2;"), "{m}");
    }

    #[test]
    fn two_level_flag_alias_resolves_through_the_chain() {
        // `typedef uint32_t B; typedef B X;` — the immediate underlying is
        // the intermediate typedef `B` (not a mapped integer); the emitter
        // follows the chain to `uint32_t` → `u32` (§14.1).
        let mut p = Parsed::default();
        p.aliases = vec![Alias {
            name: "SubStageFlags".into(),
            underlying: "SubStageBits".into(),
            chain: vec![
                "SubStageBits".into(),
                "uint32_t".into(),
                "unsigned int".into(),
            ],
        }];
        p.constants = vec![Constant("SUB_STAGE_VERTEX", "SubStageFlags", 1)]
            .into_iter()
            .map(|c| c.into())
            .collect();
        let m = emit(&p).expect("two-level alias resolves");
        assert!(m.contains("type SubStageFlags = u32;"), "{m}");
        assert!(m.contains("declare const SUB_STAGE_VERTEX = 1;"), "{m}");
    }

    #[test]
    fn alias_chain_not_reaching_an_integer_fails_loud_at_use() {
        // A typedef chain that never bottoms out in a mapped integer is not
        // registered, so a boundary use site of it fails loud (§14.1) —
        // never a silently wrong mirror.
        let mut p = Parsed::default();
        p.aliases = vec![Alias {
            name: "SubOpaqueId".into(),
            underlying: "SubBase".into(),
            chain: vec!["SubBase".into(), "SubThing".into()],
        }];
        p.decls = vec![Decl::Func {
            name: "subUse".into(),
            ret: field("void", false, false, ""),
            params: vec![field("SubOpaqueId", false, false, "id")],
        }];
        let err = emit(&p).expect_err("unresolvable alias chain must fail loud");
        assert!(err.0.contains("SubOpaqueId"), "{}", err.0);
    }

    // Tiny helper to keep the flag test terse.
    struct Constant(&'static str, &'static str, i64);
    impl From<Constant> for crate::clangfe::Constant {
        fn from(c: Constant) -> Self {
            crate::clangfe::Constant {
                name: c.0.into(),
                type_base: c.1.into(),
                value: c.2,
            }
        }
    }
}
