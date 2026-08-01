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

fn header(include_spelling: &str) -> String {
    format!(
        "\
// GENERATED FILE — DO NOT EDIT.
//
// Ambient boundary mirror produced by this project's `bindgen` from
// `{include_spelling}`. Hand edits are overwritten; the byte-identical
// regeneration test (specs/blocks/compiler.md §12.2) fails on drift. Fix
// the generator, never this file (CLAUDE.md core principle 6).
//
// Boundary typing follows the Q13 rules (specs/blocks/collisions.md §2):
// opaque handles are branded interfaces; struct pointers and
// value-class-with-null are `X | null`; (pointer,count) descriptors are
// `T[]`; length-carrying string views are `string`; callback userdata
// slots are `object | null`. These declarations are global ambient (no
// import/export), like the language prelude.
"
    )
}

/// Emits the mirror text for the parsed header.
///
/// # Errors
///
/// Returns [`ParseError`] when a boundary use site names a C type that is
/// neither a mapped scalar/builtin nor a registered named type
/// (struct/enum/handle/alias/array-pair/string-view): the emitter fails
/// loud rather than write an invalid mirror (`specs/blocks/compiler.md`
/// §13.2).
#[cfg(test)]
fn emit(parsed: &Parsed) -> Result<String, ParseError> {
    emit_for_header(parsed, "header.h")
}

/// Emits a mirror with provenance naming `include_spelling`.
///
/// # Errors
///
/// Returns [`ParseError`] under the same conditions as [`emit`].
pub fn emit_for_header(parsed: &Parsed, include_spelling: &str) -> Result<String, ParseError> {
    let registry = classify(parsed);
    validate_nullable_positions(parsed, &registry)?;
    validate_boundary_positions(parsed, &registry)?;
    let reachable_callbacks = reachable_callbacks(parsed, &registry);
    validate_callback_shapes(parsed, &registry, &reachable_callbacks)?;
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
                if !reachable_callbacks.contains(name) {
                    continue;
                }
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

    let mut out = header(include_spelling);
    if let Some(provenance) =
        emit_provenance(parsed, &registry, &reachable_callbacks, include_spelling)?
    {
        out.push('\n');
        out.push_str(&provenance);
        out.push('\n');
    }
    for block in &blocks {
        out.push('\n');
        out.push_str(block);
        out.push('\n');
    }
    Ok(out)
}

const NULLABLE_POSITION_HELP: &str = "only direct registered opaque-handle fields, \
boundary-struct pointer fields, and direct registered opaque-handle foreign-function \
parameters support `_Nullable`";

/// Rejects every visible `_Nullable` position except a direct registered
/// opaque-handle field (§31.2), a boundary-struct pointer field (§33), or a
/// direct registered opaque-handle foreign-function parameter (§35). The
/// frontend records qualifiers at every nested type layer, so a pair element
/// or an outer pointer qualifier reaches this check too instead of being
/// erased.
fn validate_nullable_positions(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
) -> Result<(), ParseError> {
    for decl in &parsed.decls {
        match decl {
            Decl::Struct { name, fields } => {
                for field in fields.iter().filter(|field| field.nullable) {
                    let nullable_handle = is_direct_registered_handle(field, registry);
                    let nullable_struct_pointer = field.pointer
                        && field.array_len.is_none()
                        && matches!(registry.get(&field.base), Some(Kind::Boundary));
                    let lowered = nullable_handle || nullable_struct_pointer;
                    if !lowered {
                        return Err(ParseError(format!(
                            "`_Nullable` on struct `{name}` field `{}` of type `{}` has no \
                             boundary lowering; {NULLABLE_POSITION_HELP}",
                            field.name, field.base,
                        )));
                    }
                }
            }
            Decl::Func { name, ret, params } => {
                if ret.nullable {
                    return Err(ParseError(format!(
                        "`_Nullable` on foreign function `{name}` return type `{}` has no \
                         boundary lowering; {NULLABLE_POSITION_HELP}",
                        ret.base,
                    )));
                }
                for param in params.iter().filter(|param| param.nullable) {
                    if !is_direct_registered_handle(param, registry) {
                        return Err(ParseError(format!(
                            "`_Nullable` on foreign function `{name}` parameter `{}` of type \
                             `{}` has no boundary lowering; {NULLABLE_POSITION_HELP}",
                            param.name, param.base,
                        )));
                    }
                }
            }
            Decl::FnPtr { name, ret, params } => {
                if ret.nullable {
                    return Err(ParseError(format!(
                        "`_Nullable` on callback typedef `{name}` return type `{}` has no \
                         boundary lowering; {NULLABLE_POSITION_HELP}",
                        ret.base,
                    )));
                }
                if let Some(param) = params.iter().find(|param| param.nullable) {
                    return Err(ParseError(format!(
                        "`_Nullable` on callback typedef `{name}` parameter `{}` of type `{}` \
                         has no boundary lowering; {NULLABLE_POSITION_HELP}",
                        param.name, param.base,
                    )));
                }
            }
            Decl::Enum { .. } | Decl::Handle { .. } => {}
        }
    }
    Ok(())
}

fn is_direct_registered_handle(field: &CField, registry: &HashMap<String, Kind>) -> bool {
    !field.pointer
        && field.array_len.is_none()
        && matches!(registry.get(&field.base), Some(Kind::Handle))
}

/// Rejects C positions that the mirror vocabulary or either lowering
/// cannot represent without losing ABI information.
fn validate_boundary_positions(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
) -> Result<(), ParseError> {
    let callback_names: HashSet<&str> = parsed
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::FnPtr { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let has_foreign_function = parsed
        .decls
        .iter()
        .any(|decl| matches!(decl, Decl::Func { .. }));

    // A direct string-view field is lowered only when its owning boundary
    // struct crosses through a pointer parameter (§28). Keep this owner set
    // explicit so every other appearance can fail at bind time rather than
    // reaching two lowerings that store the one-word language string handle
    // where C expects the two-word view.
    let string_field_structs: HashSet<String> = parsed
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Struct { name, fields }
                if matches!(registry.get(name), Some(Kind::Boundary))
                    && fields.iter().any(|field| {
                        matches!(registry.get(&field.base), Some(Kind::StringView))
                    }) =>
            {
                Some(name.clone())
            }
            _ => None,
        })
        .collect();
    let mut pointer_lowered = HashSet::new();

    for decl in &parsed.decls {
        let Decl::Struct { name, fields } = decl else {
            continue;
        };
        if !string_field_structs.contains(name) {
            continue;
        }
        for field in fields {
            if matches!(registry.get(&field.base), Some(Kind::StringView)) {
                if field.pointer || field.array_len.is_some() {
                    return Err(ParseError(format!(
                        "string-field boundary struct `{name}` field `{}` uses string-view \
                         aggregate `{}` outside the supported direct by-value field position",
                        field.name, field.base
                    )));
                }
                continue;
            }
            if field.array_len.is_some() {
                return Err(ParseError(format!(
                    "string-field boundary struct `{name}` field `{}` is a fixed array; \
                     nested aggregate fields are not lowered in the pointer scratch struct",
                    field.name
                )));
            }
            match registry.get(&field.base) {
                Some(Kind::ArrayPair(_)) => {
                    return Err(ParseError(format!(
                        "string-field boundary struct `{name}` field `{}` uses descriptor \
                         aggregate `{}`; combined absorbed aggregate fields are not lowered",
                        field.name, field.base
                    )));
                }
                Some(Kind::FnPtr) => {
                    return Err(ParseError(format!(
                        "string-field boundary struct `{name}` field `{}` uses callback \
                         typedef `{}`; callback fields are not lowered in a string-field \
                         pointer scratch struct",
                        field.name, field.base
                    )));
                }
                Some(Kind::Boundary) => {
                    validate_lowerable_boundary_aggregate(
                        parsed,
                        registry,
                        name,
                        &field.base,
                        &mut HashSet::new(),
                    )?;
                }
                Some(
                    Kind::Enum
                    | Kind::Handle
                    | Kind::Alias
                    | Kind::StringView,
                )
                | None => {}
            }
        }
    }

    for decl in &parsed.decls {
        match decl {
            Decl::Struct { name, fields } => {
                if let Some(Kind::ArrayPair(element)) = registry.get(name) {
                    if callback_names.contains(element.as_str()) {
                        return Err(ParseError(format!(
                            "descriptor struct `{name}` has callback-typedef element \
                             `{element}`; callback typedefs cannot be descriptor elements"
                        )));
                    }
                }
                if !matches!(
                    registry.get(name),
                    Some(Kind::ArrayPair(_) | Kind::StringView)
                ) {
                    // A descriptor aggregate used as one emitted struct
                    // field would also spell `T[]` in the mirror, but the
                    // struct lowering reconstructs an adjacent count-first
                    // pair, not a nested pointer-first descriptor. Reserve
                    // every emitted array field for a proven adjacency.
                    if let Some(field) = fields.iter().find(|field| {
                        !field.pointer
                            && field.array_len.is_none()
                            && matches!(registry.get(&field.base), Some(Kind::ArrayPair(_)))
                    }) {
                        return Err(ParseError(format!(
                            "struct `{name}` field `{}` embeds descriptor aggregate `{}`; \
                             emitted array fields are reserved for collapsed adjacent \
                             count/pointer pairs, and this aggregate position has no \
                             struct-field lowering",
                            field.name, field.base
                        )));
                    }
                }
                if !has_foreign_function
                    && !matches!(
                        registry.get(name),
                        Some(Kind::ArrayPair(_) | Kind::StringView)
                    )
                {
                    if let Some(field) = fields
                        .iter()
                        .find(|field| callback_names.contains(field.base.as_str()))
                    {
                        return Err(ParseError(format!(
                            "struct `{name}` field `{}` uses callback typedef `{}`, but \
                             the header declares no foreign function from which callback \
                             provenance can be emitted",
                            field.name, field.base
                        )));
                    }
                }
            }
            Decl::Func { name, ret, params } => {
                for param in params {
                    if param.pointer
                        && !param.is_const
                        && param.array_len.is_none()
                        && matches!(registry.get(&param.base), Some(Kind::Boundary))
                        && boundary_aggregate_needs_scratch(
                            parsed,
                            registry,
                            &param.base,
                            &mut HashSet::new(),
                        )?
                    {
                        if let Some((inner_owner, inner_member)) =
                            first_recursive_lowered_member(
                                parsed,
                                registry,
                                &param.base,
                                0,
                                &mut HashSet::new(),
                            )?
                        {
                            return Err(ParseError(format!(
                                "foreign function `{name}` parameter `{}` may read recursively-\
                                 lowered member `{inner_owner}.{inner_member}`; recursive \
                                 positions support script-to-C scratch construction only",
                                param.name
                            )));
                        }
                    }
                    if !param.pointer && param.array_len.is_none() {
                        if let Some(Kind::ArrayPair(element)) = registry.get(&param.base) {
                            let element_pointer_const = parsed.decls.iter().find_map(|decl| match decl {
                                Decl::Struct { name, fields } if name == &param.base => {
                                    fields.first().map(|field| field.is_const)
                                }
                                _ => None,
                            });
                            if element_pointer_const == Some(false) {
                                if let Some((inner_owner, inner_member)) =
                                    first_recursive_lowered_member(
                                        parsed,
                                        registry,
                                        element,
                                        1,
                                        &mut HashSet::new(),
                                    )?
                                {
                                    return Err(ParseError(format!(
                                        "foreign function `{name}` parameter `{}` may read recursively-\
                                         lowered pair-element member `{inner_owner}.{inner_member}`; \
                                         recursive positions support script-to-C scratch construction only",
                                        param.name
                                    )));
                                }
                            }
                        }
                    }
                }
                if string_field_structs.contains(&ret.base) {
                    let position = if ret.pointer {
                        "through a returned pointer"
                    } else {
                        "by value"
                    };
                    return Err(ParseError(format!(
                        "foreign function `{name}` returns string-field boundary struct \
                         `{}` {position}; only pointer parameters have a string-field lowering",
                        ret.base
                    )));
                }
                if !ret.pointer && ret.array_len.is_none() {
                    match registry.get(&ret.base) {
                        Some(Kind::StringView) => {
                            return Err(ParseError(format!(
                                "foreign function `{name}` returns string-view aggregate \
                                 `{}` by value; the boundary provenance vocabulary cannot \
                                 express string-view returns",
                                ret.base
                            )));
                        }
                        Some(Kind::ArrayPair(_)) => {
                            return Err(ParseError(format!(
                                "foreign function `{name}` returns descriptor aggregate \
                                 `{}` by value; the boundary provenance vocabulary cannot \
                                 express descriptor returns",
                                ret.base
                            )));
                        }
                        Some(Kind::FnPtr) => {
                            return Err(ParseError(format!(
                                "foreign function `{name}` returns callback typedef `{}` \
                                 directly; callbacks are bindable only as mirrored struct \
                                 fields",
                                ret.base
                            )));
                        }
                        Some(
                            Kind::Enum
                            | Kind::Handle
                            | Kind::Boundary
                            | Kind::Alias,
                        )
                        | None => {}
                    }
                }
                for (param_index, param) in params.iter().enumerate() {
                    if string_field_structs.contains(&param.base) {
                        if param.pointer && param.array_len.is_none() {
                            let adjacent_count = param_index
                                .checked_sub(1)
                                .and_then(|index| params.get(index))
                                .filter(|count| {
                                    count.base == "size_t"
                                        && !count.pointer
                                        && count.array_len.is_none()
                                })
                                .or_else(|| {
                                    params.get(param_index + 1).filter(|count| {
                                        count.base == "size_t"
                                            && !count.pointer
                                            && count.array_len.is_none()
                                    })
                                });
                            if let Some(count) = adjacent_count {
                                return Err(ParseError(format!(
                                    "foreign function `{name}` parameters `{}` and `{}` form \
                                     an array of string-field boundary struct `{}`; arrays of \
                                     string-field structs have no boundary lowering",
                                    count.name, param.name, param.base
                                )));
                            }
                            pointer_lowered.insert(param.base.clone());
                        } else {
                            let position = if param.array_len.is_some() {
                                "as an array"
                            } else {
                                "by value"
                            };
                            return Err(ParseError(format!(
                                "foreign function `{name}` parameter `{}` passes \
                                 string-field boundary struct `{}` {position}; only a \
                                 direct pointer parameter has a string-field lowering",
                                param.name, param.base
                            )));
                        }
                    }
                    if matches!(registry.get(&param.base), Some(Kind::FnPtr)) {
                        return Err(ParseError(format!(
                            "foreign function `{name}` parameter `{}` uses callback typedef \
                             `{}` directly; callbacks are bindable only as mirrored struct \
                             fields",
                            param.name, param.base
                        )));
                    }
                }
            }
            Decl::FnPtr { name, ret, params } => {
                if string_field_structs.contains(&ret.base) {
                    return Err(ParseError(format!(
                        "callback typedef `{name}` returns string-field boundary struct `{}`; \
                         callback aggregate positions have no string-field lowering",
                        ret.base
                    )));
                }
                if let Some(param) = params
                    .iter()
                    .find(|param| string_field_structs.contains(&param.base))
                {
                    return Err(ParseError(format!(
                        "callback typedef `{name}` parameter `{}` uses string-field boundary \
                         struct `{}`; callback aggregate positions have no string-field lowering",
                        param.name, param.base
                    )));
                }
            }
            Decl::Enum { .. } | Decl::Handle { .. } => {}
        }
    }
    // §32 widens the direct §28 owner set to every string owner reached
    // recursively from a script→C position through embedding or a collapsed-
    // pair element. §33 adds boundary-struct pointer members to the same
    // traversal. Keep collecting the direct owners so the standing audit
    // below still proves that no mirror-visible string field exists without
    // a call-site lowering.
    for decl in &parsed.decls {
        let Decl::Func { params, .. } = decl else {
            continue;
        };
        for param in params {
            let root = if param.pointer
                && param.array_len.is_none()
                && matches!(registry.get(&param.base), Some(Kind::Boundary))
            {
                Some(param.base.as_str())
            } else if !param.pointer && param.array_len.is_none() {
                match registry.get(&param.base) {
                    Some(Kind::ArrayPair(element)) => Some(element.as_str()),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(root) = root {
                let root_needs_scratch = boundary_aggregate_needs_scratch(
                    parsed,
                    registry,
                    root,
                    &mut HashSet::new(),
                )?;
                if root_needs_scratch {
                    validate_lowerable_boundary_aggregate(
                        parsed,
                        registry,
                        root,
                        root,
                        &mut HashSet::new(),
                    )?;
                }
                collect_recursive_string_owners(
                    parsed,
                    registry,
                    root,
                    &string_field_structs,
                    &mut pointer_lowered,
                    &mut HashSet::new(),
                    root_needs_scratch,
                )?;
            }
        }
    }
    for owner in &string_field_structs {
        if !pointer_lowered.contains(owner) {
            let field = parsed.decls.iter().find_map(|decl| match decl {
                Decl::Struct { name, fields } if name == owner => fields.iter().find(|field| {
                    matches!(registry.get(&field.base), Some(Kind::StringView))
                }),
                _ => None,
            });
            return Err(ParseError(format!(
                "string-field boundary struct `{owner}` field `{}` is mirror-visible but \
                 the struct is not passed through any foreign pointer parameter; no \
                 string-field lowering exists for this construct",
                field.map_or("<unknown>", |field| field.name.as_str())
            )));
        }
    }
    Ok(())
}

/// Proves that an aggregate nested in the §28 scratch construction has a
/// write-direction lowering (§32/§33). Absorbed strings, collapsed pairs,
/// and boundary-struct pointer members recurse; callbacks, fixed arrays, and
/// absorbed descriptor aggregates remain fail-loud at the innermost
/// unsupported member.
fn validate_lowerable_boundary_aggregate(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    root: &str,
    aggregate: &str,
    visiting: &mut HashSet<String>,
) -> Result<(), ParseError> {
    if !visiting.insert(aggregate.to_string()) {
        return Ok(());
    }
    let fields = parsed.decls.iter().find_map(|decl| match decl {
        Decl::Struct { name, fields } if name == aggregate => Some(fields.as_slice()),
        _ => None,
    });
    let Some(fields) = fields else {
        return Err(ParseError(format!(
            "boundary scratch root `{root}` reaches aggregate `{aggregate}`, but its \
             declaration is unavailable for recursive C-layout validation"
        )));
    };
    let pairs = embedded_array_pairs(aggregate, fields, registry)?;
    for (index, field) in fields.iter().enumerate() {
        if field.array_len.is_some() {
            return Err(ParseError(format!(
                "boundary scratch root `{root}` recursively reaches `{aggregate}.{}` which \
                 is a fixed array and has no write-direction scratch lowering",
                field.name
            )));
        }
        if pairs.count_idx.contains(&index) {
            continue;
        }
        if pairs.ptr_elem.contains_key(&index) {
            if matches!(registry.get(&field.base), Some(Kind::Boundary)) {
                if visiting.contains(&field.base) {
                    return Err(ParseError(format!(
                        "boundary scratch root `{root}` recursively reaches \
                         `{aggregate}.{}` which closes a pair-element type cycle through \
                         `{}` and has no finite write-direction scratch lowering",
                        field.name, field.base
                    )));
                }
                validate_lowerable_boundary_aggregate(
                    parsed,
                    registry,
                    root,
                    &field.base,
                    visiting,
                )?;
            }
            continue;
        }
        if field.pointer {
            if matches!(registry.get(&field.base), Some(Kind::Boundary)) {
                if visiting.contains(&field.base) {
                    return Err(ParseError(format!(
                        "boundary scratch root `{root}` recursively reaches \
                         `{aggregate}.{}` which closes a struct-pointer type cycle through \
                         `{}` and has no finite write-direction scratch lowering",
                        field.name, field.base
                    )));
                }
                validate_lowerable_boundary_aggregate(
                    parsed,
                    registry,
                    root,
                    &field.base,
                    visiting,
                )?;
            }
            continue;
        }
        match registry.get(&field.base) {
            Some(Kind::StringView) => {}
            Some(Kind::ArrayPair(_)) => {
                return Err(ParseError(format!(
                    "boundary scratch root `{root}` recursively reaches `{aggregate}.{}` \
                     which is an absorbed descriptor aggregate and has no write-direction \
                     struct-field lowering",
                    field.name
                )));
            }
            Some(Kind::FnPtr) => {
                return Err(ParseError(format!(
                    "boundary scratch root `{root}` recursively reaches `{aggregate}.{}` \
                     which is a callback and has no write-direction scratch lowering",
                    field.name
                )));
            }
            Some(Kind::Boundary) => validate_lowerable_boundary_aggregate(
                parsed,
                registry,
                root,
                &field.base,
                visiting,
            )?,
            Some(Kind::Enum | Kind::Handle | Kind::Alias) | None => {}
        }
    }
    visiting.remove(aggregate);
    Ok(())
}

/// Whether a pointer-passed boundary aggregate must be rebuilt in actual C
/// layout. A pointer member makes its parent require scratch when the target
/// itself has an absorbed lowering. Once scratch construction is active for
/// any reason, §33 still rebuilds plain pointer targets encountered beside or
/// below that lowering.
fn boundary_aggregate_needs_scratch(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    aggregate: &str,
    visiting: &mut HashSet<String>,
) -> Result<bool, ParseError> {
    if !visiting.insert(aggregate.to_string()) {
        return Ok(false);
    }
    let fields = parsed.decls.iter().find_map(|decl| match decl {
        Decl::Struct { name, fields } if name == aggregate => Some(fields.as_slice()),
        _ => None,
    });
    let Some(fields) = fields else {
        visiting.remove(aggregate);
        return Ok(false);
    };
    let pairs = embedded_array_pairs(aggregate, fields, registry)?;
    for (index, field) in fields.iter().enumerate() {
        let needs = pairs.ptr_elem.contains_key(&index)
            || matches!(registry.get(&field.base), Some(Kind::StringView))
            || (field.pointer
                && field.array_len.is_none()
                && matches!(registry.get(&field.base), Some(Kind::Boundary))
                && boundary_aggregate_needs_scratch(
                    parsed,
                    registry,
                    &field.base,
                    visiting,
                )?)
            || (!field.pointer
                && field.array_len.is_none()
                && matches!(registry.get(&field.base), Some(Kind::Boundary))
                && boundary_aggregate_needs_scratch(
                    parsed,
                    registry,
                    &field.base,
                    visiting,
                )?);
        if needs {
            visiting.remove(aggregate);
            return Ok(true);
        }
    }
    visiting.remove(aggregate);
    Ok(false)
}

/// Adds every direct string-field struct reached through recursively lowered
/// by-value aggregates, collapsed-pair element positions, and boundary-
/// struct pointer members. The traversal uses the exact pair recognizer used
/// by emission, extending both §30 audits through every §33 axis instead of
/// maintaining a second approximation.
fn collect_recursive_string_owners(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    aggregate: &str,
    direct_string_owners: &HashSet<String>,
    lowered: &mut HashSet<String>,
    visiting: &mut HashSet<String>,
    scratch_active: bool,
) -> Result<(), ParseError> {
    if !visiting.insert(aggregate.to_string()) {
        return Ok(());
    }
    if direct_string_owners.contains(aggregate) {
        lowered.insert(aggregate.to_string());
    }
    let fields = parsed.decls.iter().find_map(|decl| match decl {
        Decl::Struct { name, fields } if name == aggregate => Some(fields.as_slice()),
        _ => None,
    });
    let Some(fields) = fields else {
        visiting.remove(aggregate);
        return Ok(());
    };
    let pairs = embedded_array_pairs(aggregate, fields, registry)?;
    for (index, field) in fields.iter().enumerate() {
        let pair_element = pairs.ptr_elem.contains_key(&index);
        let embedded_value = !field.pointer
            && field.array_len.is_none()
            && matches!(registry.get(&field.base), Some(Kind::Boundary));
        let pointer_member = field.pointer
            && field.array_len.is_none()
            && !pair_element
            && matches!(registry.get(&field.base), Some(Kind::Boundary));
        if (pair_element || embedded_value || pointer_member)
            && matches!(registry.get(&field.base), Some(Kind::Boundary))
        {
            if pair_element && !field.is_const {
                if let Some((inner_owner, inner_member)) = first_recursive_lowered_member(
                    parsed,
                    registry,
                    &field.base,
                    1,
                    &mut HashSet::new(),
                )? {
                    return Err(ParseError(format!(
                        "struct `{aggregate}` field `{}` has mutable recursively-lowered \
                         pair elements and may read `{inner_owner}.{inner_member}`; recursive \
                         pair-element positions support script-to-C scratch construction only",
                        field.name
                    )));
                }
            }
            if scratch_active && pointer_member && !field.is_const {
                let offender = first_recursive_lowered_member(
                    parsed,
                    registry,
                    &field.base,
                    1,
                    &mut HashSet::new(),
                )?
                .unwrap_or_else(|| (aggregate.to_string(), field.name.clone()));
                return Err(ParseError(format!(
                    "struct `{aggregate}` field `{}` has a mutable recursively-lowered \
                     pointer target and may read `{}.{}`; recursive pointer-member \
                     positions support script-to-C scratch construction only",
                    field.name, offender.0, offender.1
                )));
            }
            collect_recursive_string_owners(
                parsed,
                registry,
                &field.base,
                direct_string_owners,
                lowered,
                visiting,
                scratch_active,
            )?;
        }
    }
    visiting.remove(aggregate);
    Ok(())
}

/// Finds the deepest absorbed member that would require interpreting C
/// scratch bytes back as language layout. Direct fields at the root retain
/// their §28/§30 behavior; once an embedded aggregate, pair element, or
/// pointer member is entered, §32/§33 is script→C only.
fn first_recursive_lowered_member(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    aggregate: &str,
    depth: usize,
    visiting: &mut HashSet<String>,
) -> Result<Option<(String, String)>, ParseError> {
    if !visiting.insert(aggregate.to_string()) {
        return Ok(None);
    }
    let fields = parsed.decls.iter().find_map(|decl| match decl {
        Decl::Struct { name, fields } if name == aggregate => Some(fields.as_slice()),
        _ => None,
    });
    let Some(fields) = fields else {
        visiting.remove(aggregate);
        return Ok(None);
    };
    let pairs = embedded_array_pairs(aggregate, fields, registry)?;
    for (index, field) in fields.iter().enumerate() {
        if pairs.count_idx.contains(&index) {
            continue;
        }
        if pairs.ptr_elem.contains_key(&index) {
            if matches!(registry.get(&field.base), Some(Kind::Boundary)) {
                if let Some(offender) = first_recursive_lowered_member(
                    parsed,
                    registry,
                    &field.base,
                    depth + 1,
                    visiting,
                )? {
                    visiting.remove(aggregate);
                    return Ok(Some(offender));
                }
            }
            if depth > 0 {
                visiting.remove(aggregate);
                return Ok(Some((aggregate.to_string(), field.name.clone())));
            }
            continue;
        }
        if field.pointer {
            if matches!(registry.get(&field.base), Some(Kind::Boundary)) {
                if let Some(offender) = first_recursive_lowered_member(
                    parsed,
                    registry,
                    &field.base,
                    depth + 1,
                    visiting,
                )? {
                    visiting.remove(aggregate);
                    return Ok(Some(offender));
                }
                visiting.remove(aggregate);
                return Ok(Some((aggregate.to_string(), field.name.clone())));
            }
            continue;
        }
        if field.array_len.is_some() {
            continue;
        }
        match registry.get(&field.base) {
            Some(Kind::StringView | Kind::ArrayPair(_)) if depth > 0 => {
                visiting.remove(aggregate);
                return Ok(Some((aggregate.to_string(), field.name.clone())));
            }
            Some(Kind::Boundary) => {
                if let Some(offender) = first_recursive_lowered_member(
                    parsed,
                    registry,
                    &field.base,
                    depth + 1,
                    visiting,
                )? {
                    visiting.remove(aggregate);
                    return Ok(Some(offender));
                }
            }
            _ => {}
        }
    }
    visiting.remove(aggregate);
    Ok(None)
}

/// Emits fixed-shape, tsc-clean provenance comments when the header has a
/// foreign function. A declaration-only header has no C names for either
/// execution tier to recover, so it emits no provenance directives.
fn emit_provenance(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    reachable_callbacks: &HashSet<String>,
    include_spelling: &str,
) -> Result<Option<String>, ParseError> {
    if !parsed
        .decls
        .iter()
        .any(|decl| matches!(decl, Decl::Func { .. }))
    {
        return Ok(None);
    }

    let mut records = vec![format!(
        "// @subscript-c-header include={}",
        quoted(include_spelling)
    )];
    for decl in &parsed.decls {
        match decl {
            Decl::FnPtr { name, .. } if reachable_callbacks.contains(name) => {
                records.push(format!("// @subscript-c-callback typedef={}", quoted(name)));
            }
            Decl::Func { name, params, .. } => {
                emit_parameter_provenance(name, params, parsed, registry, &mut records)?;
            }
            Decl::Enum { .. }
            | Decl::FnPtr { .. }
            | Decl::Struct { .. }
            | Decl::Handle { .. } => {}
        }
    }
    Ok(Some(records.join("\n")))
}

/// Adds provenance for each standalone descriptor or string view absorbed
/// from one foreign-function parameter list.
fn emit_parameter_provenance(
    owner_name: &str,
    params: &[CField],
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    records: &mut Vec<String>,
) -> Result<(), ParseError> {
    let parameter_pairs =
        parameter_array_pairs("foreign function", owner_name, params, registry)?;
    for (index, param) in params.iter().enumerate() {
        if parameter_pairs.count_idx.contains(&index) {
            continue;
        }
        if parameter_pairs.ptr_elem.contains_key(&index) {
            records.push(format!(
                "// @subscript-c-scalar-pair function={} parameter={} element={} const={}",
                quoted(owner_name),
                quoted(&param.name),
                quoted(&param.base),
                param.is_const,
            ));
            continue;
        }
        match registry.get(&param.base) {
            Some(Kind::ArrayPair(_)) => {
                let element = first_struct_field(parsed, &param.base).ok_or_else(|| {
                    ParseError(format!(
                        "descriptor provenance for `{}.{}` lost C struct `{}`",
                        owner_name, param.name, param.base
                    ))
                })?;
                records.push(format!(
                    "// @subscript-c-descriptor function={} parameter={} aggregate={} element={} const={}",
                    quoted(owner_name),
                    quoted(&param.name),
                    quoted(&param.base),
                    quoted(&element.base),
                    element.is_const,
                ));
            }
            Some(Kind::StringView) => {
                records.push(format!(
                    "// @subscript-c-string-view function={} parameter={} aggregate={}",
                    quoted(owner_name),
                    quoted(&param.name),
                    quoted(&param.base),
                ));
            }
            Some(Kind::Enum | Kind::Handle | Kind::FnPtr | Kind::Boundary | Kind::Alias) | None => {
            }
        }
    }
    Ok(())
}

/// Rejects callback typedefs the runtime's sole C-ABI trampoline cannot
/// serve. The first parameter must be a by-value classified string view,
/// followed by exactly two `void*` userdata slots, and the return is void.
fn validate_callback_shapes(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    reachable_callbacks: &HashSet<String>,
) -> Result<(), ParseError> {
    for decl in &parsed.decls {
        let Decl::FnPtr { name, ret, params } = decl else {
            continue;
        };
        if !reachable_callbacks.contains(name) {
            continue;
        }
        let returns_void = ret.base == "void" && !ret.pointer && ret.array_len.is_none();
        let has_string_view = params.first().is_some_and(|param| {
            !param.pointer
                && param.array_len.is_none()
                && matches!(registry.get(&param.base), Some(Kind::StringView))
        });
        let has_two_userdata = params.len() == 3
            && params[1..]
                .iter()
                .all(|param| param.base == "void" && param.pointer && param.array_len.is_none());
        if !(returns_void && has_string_view && has_two_userdata) {
            return Err(ParseError(format!(
                "callback typedef `{name}` has an unsupported signature; \
                 the supported shape is `void Callback(StringView message, \
                 void *userdata1, void *userdata2)`"
            )));
        }
    }
    Ok(())
}

/// Returns the function-pointer typedefs that can cross the mirrored
/// boundary through a field of an emitted struct. Direct foreign-function
/// parameters and returns were rejected before this walk. Unreferenced
/// host-only hooks are not part of the mirror.
fn reachable_callbacks(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
) -> HashSet<String> {
    let callback_names: HashSet<&str> = parsed
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::FnPtr { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let mut reachable = HashSet::new();
    for decl in &parsed.decls {
        match decl {
            Decl::Struct { name, fields }
                if !matches!(
                    registry.get(name),
                    Some(Kind::ArrayPair(_) | Kind::StringView)
                ) =>
            {
                for field in fields {
                    if callback_names.contains(field.base.as_str()) {
                        reachable.insert(field.base.clone());
                    }
                }
            }
            Decl::Func { .. } => {}
            Decl::Enum { .. }
            | Decl::FnPtr { .. }
            | Decl::Handle { .. }
            | Decl::Struct { .. } => {}
        }
    }
    reachable
}

/// Returns the pointer field of a classified two-field descriptor struct.
fn first_struct_field<'a>(parsed: &'a Parsed, name: &str) -> Option<&'a CField> {
    parsed.decls.iter().find_map(|decl| match decl {
        Decl::Struct {
            name: struct_name,
            fields,
        } if struct_name == name => fields.first(),
        _ => None,
    })
}

/// Quotes one directive value with JSON-compatible escaping.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < '\u{20}' => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
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
            if is_plain_char(&fields[0].base) {
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
            && !is_plain_char(&fields[0].base)
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

/// Resolves a scalar typedef alias to its language sized type by
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

/// Descriptor-embedded `(count, pointer)` array pairs within a struct or
/// function parameter list: pointer indices mapped to language element
/// spellings, and count indices elided from the mirror.
#[derive(Default)]
struct EmbeddedPairs {
    /// Pointer field index → element spelling (`u32`, an enum/class name, …).
    ptr_elem: HashMap<usize, String>,
    /// Count field indices to omit from the mirror.
    count_idx: HashSet<usize>,
}

/// Recognizes embedded `(count, pointer)` array pairs (§13.2/§30.2), and
/// ONLY the shape both lowerings reconstruct: a `size_t` count field named
/// `<n>Count` or `<n>_count` **immediately followed** by `[const] T* <n>`
/// (count-first, contiguous). `T` may be a [`lang_scalar`] scalar, a
/// registered enum, registered boundary struct, or (for const input only)
/// a registered opaque handle. The pointer field index maps to `T[]`; the
/// count field index is elided.
///
/// This recognizer is deliberately total for pair-looking struct fields.
/// If matching halves are non-adjacent, pointer-first, or have an
/// unsupported element, it returns a named bind error. That makes a leaked
/// count plus `Enum | null`/`Struct | null` pointer impossible to emit.
fn embedded_array_pairs(
    owner: &str,
    fields: &[CField],
    reg: &HashMap<String, Kind>,
) -> Result<EmbeddedPairs, ParseError> {
    let mut pairs = EmbeddedPairs::default();
    for (count_index, count) in fields.iter().enumerate() {
        if count.pointer || count.array_len.is_some() {
            continue;
        }
        let Some(element_name) = embedded_count_element_name(&count.name) else {
            continue;
        };
        let matching_pointer = fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.pointer && field.name == element_name);
        let immediate_pointer = fields
            .get(count_index + 1)
            .filter(|field| field.pointer);
        let Some((pointer_index, ptr)) = matching_pointer else {
            if let Some(ptr) = immediate_pointer {
                return Err(ParseError(format!(
                    "struct `{owner}` fields `{}` and `{}` form an adjacent count/pointer \
                     shape but their names do not collapse (`{}` requires pointer field \
                     `{element_name}`); refusing to emit a bare count beside a nullable \
                     pointer mirror",
                    count.name, ptr.name, count.name
                )));
            }
            // A count-looking scalar without a matching pointer is still a
            // legitimate ordinary field. The hard rule begins once both
            // named halves exist.
            continue;
        };
        if count.base != "size_t" {
            return Err(ParseError(format!(
                "struct `{owner}` fields `{}` and `{}` form a count/pointer pair but the \
                 count type is `{}` instead of `size_t`; this adjacency has no lowering",
                count.name, ptr.name, count.base
            )));
        }
        if pointer_index != count_index + 1 || ptr.array_len.is_some() {
            return Err(ParseError(format!(
                "struct `{owner}` fields `{}` and `{}` form a count/pointer pair but are not \
                 the supported adjacent count-first shape; refusing to emit a bare count \
                 beside a nullable pointer mirror",
                count.name, ptr.name
            )));
        }
        let elem = if let Some(scalar) = lang_scalar(&ptr.base) {
            scalar.to_string()
        } else {
            match reg.get(&ptr.base) {
                Some(Kind::Enum | Kind::Boundary) => ptr.base.clone(),
                Some(Kind::Handle) if ptr.is_const => ptr.base.clone(),
                _ => {
                    return Err(ParseError(format!(
                        "struct `{owner}` fields `{}` and `{}` form an adjacent count/pointer \
                         pair with unsupported element `{}`; only lang_scalar scalars, \
                         registered enums, registered boundary structs, and registered opaque \
                         handles have a lowering; handles are input-only and require `const H*`",
                        count.name, ptr.name, ptr.base
                    )));
                }
            }
        };
        pairs.count_idx.insert(count_index);
        pairs.ptr_elem.insert(pointer_index, elem);
    }
    Ok(pairs)
}

/// Element field name implied by one supported count spelling.
fn embedded_count_element_name(count: &str) -> Option<&str> {
    count
        .strip_suffix("Count")
        .filter(|name| !name.is_empty())
        .or_else(|| count.strip_suffix("_count").filter(|name| !name.is_empty()))
}

/// Recognizes the §27/§34 array-pair shape in a function parameter list:
/// `size_t <n>Count` immediately followed by `[const] E* <n>`. A
/// [`lang_scalar`] `E` supports both §27 directions; a registered opaque
/// handle supports only §34's `const H*` input direction. Only the exact
/// `<n>Count` spelling is part of the parameter rule.
///
/// Like [`embedded_array_pairs`], this recognizer is total for pair-looking
/// positions. Registered enum/boundary elements are rejected explicitly:
/// struct-level pairs have a lowering, but committing their direct-parameter
/// direction and recursive writeback semantics is outside §34. Unsupported,
/// mismatched, wrong-width, and non-adjacent shapes fail before a bare count
/// plus nullable pointer can reach the mirror.
fn parameter_array_pairs(
    owner_kind: &str,
    owner: &str,
    params: &[CField],
    reg: &HashMap<String, Kind>,
) -> Result<EmbeddedPairs, ParseError> {
    let mut pairs = EmbeddedPairs::default();
    for (count_index, count) in params.iter().enumerate() {
        if count.pointer || count.array_len.is_some() {
            continue;
        }
        let Some(element_name) = count
            .name
            .strip_suffix("Count")
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let matching_pointer = params
            .iter()
            .enumerate()
            .find(|(_, param)| param.pointer && param.name == element_name);
        let immediate_pointer = params
            .get(count_index + 1)
            .filter(|param| param.pointer);
        let Some((pointer_index, ptr)) = matching_pointer else {
            if let Some(ptr) = immediate_pointer {
                return Err(ParseError(format!(
                    "{owner_kind} `{owner}` parameters `{}` and `{}` form an adjacent \
                     count/pointer shape but their names do not collapse (`{}` requires \
                     pointer parameter `{element_name}`); refusing to emit a bare count \
                     beside a nullable pointer mirror",
                    count.name, ptr.name, count.name
                )));
            }
            continue;
        };
        if count.base != "size_t" {
            return Err(ParseError(format!(
                "{owner_kind} `{owner}` parameters `{}` and `{}` form a count/pointer \
                 pair but the count type is `{}` instead of `size_t`; this adjacency has \
                 no lowering",
                count.name, ptr.name, count.base
            )));
        }
        if pointer_index != count_index + 1 || ptr.array_len.is_some() {
            return Err(ParseError(format!(
                "{owner_kind} `{owner}` parameters `{}` and `{}` form a count/pointer \
                 pair but are not the supported adjacent count-first shape; refusing to \
                 emit a bare count beside a nullable pointer mirror",
                count.name, ptr.name
            )));
        }
        let elem = if let Some(scalar) = lang_scalar(&ptr.base) {
            scalar.to_string()
        } else if matches!(reg.get(&ptr.base), Some(Kind::Handle)) && ptr.is_const {
            ptr.base.clone()
        } else {
            let supported = match reg.get(&ptr.base) {
                Some(Kind::Handle) => {
                    "registered opaque handles are input-only and require `const H*`"
                }
                Some(Kind::Enum | Kind::Boundary) => {
                    "registered enum and boundary-struct pairs are supported only at \
                     struct level; direct parameter pairs have no lowering"
                }
                _ => {
                    "only lang_scalar scalars and const registered opaque handles have a \
                     direct parameter-pair lowering"
                }
            };
            return Err(ParseError(format!(
                "{owner_kind} `{owner}` parameters `{}` and `{}` form an adjacent \
                 count/pointer pair with unsupported element `{}`; {supported}; refusing \
                 to emit a bare count beside a nullable pointer mirror",
                count.name, ptr.name, ptr.base
            )));
        };
        pairs.count_idx.insert(count_index);
        pairs.ptr_elem.insert(pointer_index, elem);
    }
    Ok(pairs)
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
        param_sig("callback typedef", name, params, reg)?,
        map_use(ret, reg)?
    ))
}

fn emit_struct(
    name: &str,
    fields: &[CField],
    reg: &HashMap<String, Kind>,
) -> Result<String, ParseError> {
    let pairs = embedded_array_pairs(name, fields, reg)?;
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
        param_sig("foreign function", name, params, reg)?,
        map_use(ret, reg)?
    ))
}

fn param_sig(
    owner_kind: &str,
    owner: &str,
    params: &[CField],
    reg: &HashMap<String, Kind>,
) -> Result<String, ParseError> {
    let pairs = parameter_array_pairs(owner_kind, owner, params, reg)?;
    let mut out = Vec::with_capacity(params.len());
    for (index, p) in params.iter().enumerate() {
        if pairs.count_idx.contains(&index) {
            continue;
        }
        let ty = match pairs.ptr_elem.get(&index) {
            Some(elem) => format!("{elem}[]"),
            None => map_use(p, reg)?,
        };
        out.push(format!("{}: {ty}", p.name));
    }
    Ok(out.join(", "))
}

/// Maps a C field/parameter/return type to its language boundary type
/// (Q13, §31.2, §35).
fn map_use(f: &CField, reg: &HashMap<String, Kind>) -> Result<String, ParseError> {
    if f.nullable
        && !f.pointer
        && f.array_len.is_none()
        && matches!(reg.get(&f.base), Some(Kind::Handle))
    {
        return Ok(format!("{} | null", f.base));
    }
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
    if base == "char" {
        return ParseError(
            "plain `char` has target-dependent signedness; bindgen does not infer it \
             from the host, so use an explicit `signed char` or `unsigned char` spelling"
                .to_string(),
        );
    }
    if base == "__fp16" {
        return ParseError(
            "`__fp16` has a target-dependent half format; use `_Float16` for \
             unambiguous IEEE binary16"
                .to_string(),
        );
    }
    ParseError(format!(
        "unmapped C type `{base}` at a boundary use site: it is neither a mapped \
         scalar/builtin nor a registered named type (struct/enum/handle/alias/\
         array-pair/string-view). Refusing to emit an invalid mirror; add a \
         mapping or a typedef for this type."
    ))
}

/// True when `base` is plain `char`.
///
/// A `const char *` string view is byte-oriented and does not depend on
/// scalar signedness. Scalar mapping deliberately leaves `char` unmapped.
fn is_plain_char(base: &str) -> bool {
    base == "char"
}

/// Language spelling of a C scalar or raw builtin, or `None` for a named
/// type. The stdint typedefs and the width-stable raw builtins map to the
/// sized numerics (`specs/blocks/compiler.md` §13.2). Two deliberate
/// exclusions fail loud rather than mirror a target-dependent width:
///
/// - `long`/`unsigned long` — 64-bit on LP64 (Unix) but 32-bit on LLP64
///   (Windows); an ABI-stable header must spell a 64-bit int
///   `int64_t`/`long long`, so a bare `long` is unmapped.
/// - plain `char` — target-dependent signedness is never inferred from
///   the generator host.
/// - `__fp16` — ARM's alternative half format is not IEEE binary16;
///   `_Float16` is the unambiguous spelling.
///
/// `int`/`unsigned int` (32-bit on every supported target) and `long
/// long`/`unsigned long long` (64-bit everywhere) are width-stable and
/// mapped.
fn lang_scalar(base: &str) -> Option<&'static str> {
    Some(match base {
        "bool" => "boolean",
        "float" => "f32",
        "double" => "f64",
        "_Float16" => "f16",
        "int8_t" | "signed char" => "i8",
        "uint8_t" | "unsigned char" => "u8",
        "int16_t" | "short" | "short int" | "signed short" | "signed short int" => "i16",
        "uint16_t" | "unsigned short" | "unsigned short int" => "u16",
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
            nullable: false,
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
        // Without the libclang target's signedness marker, plain `char`
        // remains ambiguous: fail loud, never guess.
        let decls = vec![Decl::Struct {
            name: "SubHasChar".into(),
            fields: vec![field("char", false, false, "c")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("bare char must fail loud");
        assert!(err.0.contains("char"), "message names the type: {}", err.0);
    }

    #[test]
    fn narrow_scalar_spellings_map_without_guessing() {
        let decls = vec![Decl::Struct {
            name: "SubNarrow".into(),
            fields: vec![
                field("int8_t", false, false, "a"),
                field("unsigned char", false, false, "b"),
                field("short", false, false, "c"),
                field("uint16_t", false, false, "d"),
                field("_Float16", false, false, "e"),
                field("signed char", false, false, "f"),
            ],
        }];
        let m = emit(&parsed_with(decls)).expect("narrow scalars map");
        for expected in [
            "a: i8;",
            "b: u8;",
            "c: i16;",
            "d: u16;",
            "e: f16;",
            "f: i8;",
        ] {
            assert!(m.contains(expected), "{m}");
        }
    }

    #[test]
    fn fp16_without_a_known_format_fails_loud() {
        let decls = vec![Decl::Struct {
            name: "SubHalf".into(),
            fields: vec![field("__fp16", false, false, "value")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("__fp16 must not guess a format");
        assert_eq!(
            err.0,
            "`__fp16` has a target-dependent half format; use `_Float16` for unambiguous IEEE binary16"
        );
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
    fn mutable_struct_scalar_pair_collapses_to_array() {
        let decls = vec![Decl::Struct {
            name: "SubFillList".into(),
            fields: vec![
                field("size_t", false, false, "valuesCount"),
                field("uint16_t", true, false, "values"),
                field("uint32_t", false, false, "tag"),
            ],
        }];
        let m = emit(&parsed_with(decls)).expect("mutable embedded scalar pair maps");
        assert!(m.contains("values: u16[];"), "{m}");
        assert!(!m.contains("valuesCount"), "count field is elided: {m}");
    }

    #[test]
    fn registered_enum_struct_pair_collapses_to_array() {
        let decls = vec![
            Decl::Enum {
                name: "SubFormat".into(),
                members: vec![("SUB_FORMAT_A".into(), 1), ("SUB_FORMAT_B".into(), 2)],
            },
            Decl::Struct {
                name: "SubTexture".into(),
                fields: vec![
                    field("size_t", false, false, "viewFormatsCount"),
                    field("SubFormat", true, true, "viewFormats"),
                    field("uint32_t", false, false, "usage"),
                ],
            },
        ];
        let m = emit(&parsed_with(decls)).expect("registered enum pair maps");
        assert!(m.contains("viewFormats: SubFormat[];"), "{m}");
        assert!(!m.contains("viewFormatsCount"), "count field is elided: {m}");
        assert!(
            m.contains("constructor(viewFormats: SubFormat[], usage: u32);"),
            "{m}"
        );
    }

    #[test]
    fn const_scalar_parameter_pair_collapses_to_array() {
        let decls = vec![Decl::Func {
            name: "subReadBytes".into(),
            ret: field("uint32_t", false, false, ""),
            params: vec![
                field("size_t", false, false, "dataCount"),
                field("uint8_t", true, true, "data"),
            ],
        }];
        let m = emit(&parsed_with(decls)).expect("const scalar parameter pair maps");
        assert!(
            m.contains("declare function subReadBytes(data: u8[]): u32;"),
            "{m}"
        );
        assert!(m.contains(
            "// @subscript-c-scalar-pair function=\"subReadBytes\" parameter=\"data\" element=\"uint8_t\" const=true"
        ), "{m}");
        assert!(!m.contains("dataCount: u64"), "count is elided: {m}");
    }

    #[test]
    fn mutable_scalar_parameter_pair_collapses_to_array() {
        let decls = vec![Decl::Func {
            name: "subFillBytes".into(),
            ret: field("void", false, false, ""),
            params: vec![
                field("size_t", false, false, "dataCount"),
                field("uint8_t", true, false, "data"),
            ],
        }];
        let m = emit(&parsed_with(decls)).expect("mutable scalar parameter pair maps");
        assert!(
            m.contains("declare function subFillBytes(data: u8[]): void;"),
            "{m}"
        );
        assert!(m.contains(
            "// @subscript-c-scalar-pair function=\"subFillBytes\" parameter=\"data\" element=\"uint8_t\" const=false"
        ), "{m}");
    }

    #[test]
    fn u16_scalar_parameter_pair_collapses_to_array() {
        let decls = vec![Decl::Func {
            name: "subFillShorts".into(),
            ret: field("void", false, false, ""),
            params: vec![
                field("size_t", false, false, "valuesCount"),
                field("uint16_t", true, false, "values"),
            ],
        }];
        let m = emit(&parsed_with(decls)).expect("u16 scalar parameter pair maps");
        assert!(
            m.contains("declare function subFillShorts(values: u16[]): void;"),
            "{m}"
        );
    }

    #[test]
    fn lone_scalar_pointer_parameter_fails_loud() {
        let decls = vec![Decl::Func {
            name: "subReadBytes".into(),
            ret: field("void", false, false, ""),
            params: vec![field("uint8_t", true, true, "data")],
        }];
        let err = emit(&parsed_with(decls)).expect_err("lone scalar pointer must fail loud");
        assert!(err.0.contains("uint8_t"), "{}", err.0);
    }

    #[test]
    fn non_adjacent_scalar_parameter_pair_fails_loud() {
        let decls = vec![Decl::Func {
            name: "subReadBytes".into(),
            ret: field("void", false, false, ""),
            params: vec![
                field("size_t", false, false, "dataCount"),
                field("uint32_t", false, false, "tag"),
                field("uint8_t", true, true, "data"),
            ],
        }];
        let err =
            emit(&parsed_with(decls)).expect_err("non-adjacent scalar pair must fail loud");
        assert!(
            err.0
                .contains("not the supported adjacent count-first shape"),
            "{}",
            err.0
        );
        assert!(err.0.contains("bare count"), "{}", err.0);
    }

    #[test]
    fn every_stdint_lang_scalar_maps_at_scalar_parameter_pair_site() {
        let stdint = [
            ("int8_t", "i8"),
            ("uint8_t", "u8"),
            ("int16_t", "i16"),
            ("uint16_t", "u16"),
            ("int32_t", "i32"),
            ("uint32_t", "u32"),
            ("int64_t", "i64"),
            ("uint64_t", "u64"),
        ];
        let decls = stdint
            .iter()
            .enumerate()
            .map(|(index, (c_type, _))| Decl::Func {
                name: format!("subScalarPair{index}"),
                ret: field("void", false, false, ""),
                params: vec![
                    field("size_t", false, false, "itemsCount"),
                    field(c_type, true, index % 2 == 0, "items"),
                ],
            })
            .collect();
        let m = emit(&parsed_with(decls)).expect("every stdint scalar maps at a pair site");
        for (index, (_, lang_type)) in stdint.iter().enumerate() {
            assert!(
                m.contains(&format!(
                    "declare function subScalarPair{index}(items: {lang_type}[]): void;"
                )),
                "missing {lang_type} pair mapping in {m}"
            );
        }
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
        assert!(err.0.contains("SubPtrFirst"), "{}", err.0);
        assert!(err.0.contains("drawsCount"), "{}", err.0);
        assert!(err.0.contains("adjacent count-first"), "{}", err.0);
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
        assert!(err.0.contains("SubGap"), "{}", err.0);
        assert!(err.0.contains("bare count"), "{}", err.0);
    }

    #[test]
    fn adjacent_struct_pair_with_mismatched_names_fails_loud() {
        let decls = vec![
            Decl::Enum {
                name: "SubFormat".into(),
                members: vec![("SUB_FORMAT_A".into(), 1)],
            },
            Decl::Struct {
                name: "SubBadTexture".into(),
                fields: vec![
                    field("size_t", false, false, "viewFormatsCount"),
                    field("SubFormat", true, true, "formats"),
                ],
            },
        ];
        let err = emit(&parsed_with(decls)).expect_err("mismatched adjacency must fail loud");
        assert!(err.0.contains("SubBadTexture"), "{}", err.0);
        assert!(err.0.contains("names do not collapse"), "{}", err.0);
        assert!(err.0.contains("viewFormats"), "{}", err.0);
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
