//! Q13 boundary mapping and `.d.ts` emission
//! (`specs/blocks/collisions.md` §2, `specs/blocks/compiler.md` §12.2).

use std::collections::HashMap;

use crate::cparse::{CField, Decl};

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

/// Emits the mirror text for the parsed declarations.
pub fn emit(decls: &[Decl]) -> String {
    let registry = classify(decls);
    let mut blocks: Vec<String> = Vec::new();
    let mut pending_fns: Vec<String> = Vec::new();

    let flush = |pending: &mut Vec<String>, blocks: &mut Vec<String>| {
        if !pending.is_empty() {
            blocks.push(pending.join("\n"));
            pending.clear();
        }
    };

    for decl in decls {
        match decl {
            Decl::Enum { name, members } => {
                flush(&mut pending_fns, &mut blocks);
                blocks.push(emit_enum(name, members));
            }
            Decl::FnPtr { name, ret, params } => {
                flush(&mut pending_fns, &mut blocks);
                blocks.push(emit_fn_ptr(name, ret, params, &registry));
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
                    blocks.push(emit_struct(name, fields, &registry));
                }
            },
            Decl::Func { name, ret, params } => {
                pending_fns.push(emit_func(name, ret, params, &registry));
            }
        }
    }
    flush(&mut pending_fns, &mut blocks);

    let mut out = String::from(HEADER);
    for block in &blocks {
        out.push('\n');
        out.push_str(block);
        out.push('\n');
    }
    out
}

/// Builds the name→role registry from the declarations.
fn classify(decls: &[Decl]) -> HashMap<String, Kind> {
    let mut reg = HashMap::new();
    for decl in decls {
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
    reg
}

/// A two-field `{ const P*; size_t; }` struct is a string view when `P`
/// is `char`, otherwise a `(pointer, count)` array-pair descriptor.
/// Everything else is a boundary struct.
fn classify_struct(fields: &[CField]) -> Kind {
    if fields.len() == 2
        && fields[0].pointer
        && fields[0].is_const
        && !fields[1].pointer
        && fields[1].base == "size_t"
    {
        if fields[0].base == "char" {
            return Kind::StringView;
        }
        if let Some(elem) = lang_scalar(&fields[0].base) {
            return Kind::ArrayPair(elem.to_string());
        }
    }
    Kind::Boundary
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

fn emit_fn_ptr(name: &str, ret: &CField, params: &[CField], reg: &HashMap<String, Kind>) -> String {
    format!(
        "type {name} = ({}) => {};",
        param_sig(params, reg),
        map_use(ret, reg)
    )
}

fn emit_struct(name: &str, fields: &[CField], reg: &HashMap<String, Kind>) -> String {
    let mut s = format!("declare class {name} {{\n");
    for f in fields {
        s.push_str(&format!("  {}: {};\n", f.name, map_use(f, reg)));
    }
    s.push_str(&format!("  constructor({});\n", param_sig(fields, reg)));
    s.push('}');
    s
}

fn emit_func(name: &str, ret: &CField, params: &[CField], reg: &HashMap<String, Kind>) -> String {
    format!(
        "declare function {name}({}): {};",
        param_sig(params, reg),
        map_use(ret, reg)
    )
}

fn param_sig(params: &[CField], reg: &HashMap<String, Kind>) -> String {
    params
        .iter()
        .map(|p| format!("{}: {}", p.name, map_use(p, reg)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Maps a C field/parameter/return type to its language boundary type
/// (Q13).
fn map_use(f: &CField, reg: &HashMap<String, Kind>) -> String {
    if let Some(n) = f.array_len {
        // Fixed C array `T[N]` → `FixedArray<T, N>`.
        return format!("FixedArray<{}, {}>", map_element(&f.base, reg), n);
    }
    if f.pointer {
        // `void*` userdata → `object | null`; struct pointer → `X | null`.
        if f.base == "void" {
            return "object | null".to_string();
        }
        return format!("{} | null", map_named(&f.base, reg));
    }
    if f.base == "void" {
        return "void".to_string();
    }
    if let Some(scalar) = lang_scalar(&f.base) {
        return scalar.to_string();
    }
    map_named(&f.base, reg)
}

/// Maps a by-value named type to its language spelling per its role.
fn map_named(base: &str, reg: &HashMap<String, Kind>) -> String {
    match reg.get(base) {
        Some(Kind::ArrayPair(elem)) => format!("{elem}[]"),
        Some(Kind::StringView) => "string".to_string(),
        // Enum, FnPtr, Handle, Boundary all use their declared name.
        Some(_) => base.to_string(),
        None => base.to_string(),
    }
}

/// Maps a fixed-array element type (scalar, or a named by-value type).
fn map_element(base: &str, reg: &HashMap<String, Kind>) -> String {
    lang_scalar(base)
        .map(str::to_string)
        .unwrap_or_else(|| map_named(base, reg))
}

/// Language spelling of a C scalar type, or `None` for a named type.
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
        _ => return None,
    })
}
