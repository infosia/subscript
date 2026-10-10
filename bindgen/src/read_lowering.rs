//! The read direction of the boundary (`specs/blocks/compiler.md` §187).
//!
//! A read position is a place where C writes memory that the script reads
//! after the call. [`view::HeaderStructs`] maps each struct member to the shape
//! that its mirror type gives, from the registry [`Kind`] of its type and
//! the pair recognizer [`crate::emit::embedded_array_pairs`] that the write
//! lowering reads. The pass functions of the boundary crate decide how the
//! call passes each struct and pointer, as both tiers decide it, and the
//! one walk [`subscript_boundary::first_unreadable`] decides each member
//! at every depth, as the checker does. [`validate`] finds every read
//! position with one scan over [`Decl`].

use std::collections::{HashMap, HashSet};

use subscript_boundary::{
    first_cycle, first_unreadable, member_pass, parameter_pass, struct_pass, target_pass,
    value_parameter_pass, written_back_pair_message, FieldShape, PointerPass, Reach, ReadPosition,
    ReadRoot, StructPass, Unreadable, UnreadableKind,
};

use crate::clangfe::Parsed;
use crate::cparse::{CField, Decl, ParseError};
use crate::emit::{classify, embedded_array_pairs, EmbeddedPairs, Kind};

mod view;

pub(crate) use view::HeaderStructs;

/// The pass decision for one boundary struct (§187 rule 3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StructPasses {
    /// The struct name.
    pub name: String,
    /// How a call passes the struct.
    pub pass: StructPass,
    /// Each struct-pointer member, with the pass of its target in a struct
    /// that the call passes as [`StructPasses::pass`].
    pub pointers: Vec<(String, PointerPass)>,
}

/// The pass decision for each boundary struct of `parsed`, in declaration
/// order.
///
/// # Errors
///
/// Returns the pair recognizer's error for a malformed pair.
pub(crate) fn boundary_passes(parsed: &Parsed) -> Result<Vec<StructPasses>, ParseError> {
    let registry = classify(parsed);
    let view = HeaderStructs::new(parsed, &registry);
    let mut passes = Vec::new();
    for decl in &parsed.decls {
        let Decl::Struct { name, fields } = decl else {
            continue;
        };
        if !matches!(registry.get(name), Some(Kind::Boundary)) {
            continue;
        }
        let pass = struct_pass(&view, name.as_str());
        let pairs = embedded_array_pairs(name, fields, &registry)?;
        let pointers = fields
            .iter()
            .enumerate()
            .filter_map(
                |(index, field)| match view.field_shape(&pairs, index, field) {
                    Some(FieldShape::Pointer { target, .. }) => Some((
                        field.name.clone(),
                        member_pass(&view, pass, target, !field.is_const),
                    )),
                    _ => None,
                },
            )
            .collect();
        passes.push(StructPasses {
            name: name.clone(),
            pass,
            pointers,
        });
    }
    Ok(passes)
}

/// Returns the innermost member of `aggregate` that has no read lowering
/// when C reaches `aggregate` at `root`.
///
/// `reach` is [`Reach::Root`] for the read root itself; a position that
/// starts inside a written struct (pair elements, a pointer target) passes
/// [`Reach::Indirect`]. A root that C writes and that is itself a
/// descriptor aggregate or a string view has no read lowering. A type
/// that this header does not declare (an external type, §48) is outside
/// the binder's view; the checker reads it (§187 rule 3). The walk is
/// [`first_unreadable`], which the checker also calls.
fn first_unreadable_in(
    view: &HeaderStructs<'_>,
    registry: &HashMap<String, Kind>,
    aggregate: &str,
    root: ReadRoot,
    reach: Reach,
) -> Option<Unreadable> {
    let root_kind = match registry.get(aggregate) {
        Some(Kind::ArrayPair(_)) => Some(UnreadableKind::DescriptorAggregate),
        Some(Kind::StringView) => Some(UnreadableKind::StringView),
        _ => None,
    };
    if let (Some(kind), ReadRoot::Value | ReadRoot::Fill(_)) = (root_kind, root) {
        let member = view
            .struct_fields(aggregate)
            .and_then(|fields| fields.first())
            .map_or_else(String::new, |field| field.name.clone());
        return Some(Unreadable::new(aggregate.to_string(), member, kind));
    }
    let name = view.struct_name(aggregate)?;
    first_unreadable(view, name, root, reach)
}

/// True when a position of type `base` holds a struct that the predicate
/// reads.
fn is_struct(registry: &HashMap<String, Kind>, base: &str) -> bool {
    matches!(
        registry.get(base),
        Some(Kind::Boundary | Kind::ArrayPair(_) | Kind::StringView)
    )
}

/// Rejects `aggregate` at `position` when C reaches it at `root` and it
/// holds a member with no read lowering.
///
/// # Errors
///
/// Returns the §187 diagnostic.
pub(crate) fn reject_unreadable(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    position: ReadPosition<'_>,
    aggregate: &str,
    root: ReadRoot,
    reach: Reach,
) -> Result<(), ParseError> {
    reject_unreadable_in(
        &HeaderStructs::new(parsed, registry),
        registry,
        position,
        aggregate,
        root,
        reach,
    )
}

/// [`reject_unreadable`] over a view that the caller builds once.
fn reject_unreadable_in(
    view: &HeaderStructs<'_>,
    registry: &HashMap<String, Kind>,
    position: ReadPosition<'_>,
    aggregate: &str,
    root: ReadRoot,
    reach: Reach,
) -> Result<(), ParseError> {
    match first_unreadable_in(view, registry, aggregate, root, reach) {
        Some(found) => Err(ParseError(found.message(position))),
        None => Ok(()),
    }
}

/// Rejects every read position whose value holds a member with no read
/// lowering.
///
/// The scan matches every [`Decl`] variant with no wildcard arm, so a new
/// declaration kind does not compile until this function classifies it.
/// The read positions are:
///
/// - a foreign-function result, by value or through a pointer;
/// - a non-`const` struct-pointer parameter, which the call fills;
/// - a parameter of a reachable callback, which C passes to the script;
/// - a non-`const` pointer target that a parameter reaches, from a pointer
///   root or a by-value root: the elements of a mutable pair, and the
///   target of a non-`const` struct-pointer member.
///
/// A completion result is the fifth position; [`crate::completion`] calls
/// [`reject_unreadable`] when it validates the selection.
///
/// The binder sees only the structs that its parse defines. The checker
/// is the total scan (§187 rule 3); this scan is its early diagnostic.
///
/// # Errors
///
/// Returns the diagnostic for the first position that reaches a member
/// with no read lowering.
pub(crate) fn validate(
    parsed: &Parsed,
    reachable_callbacks: &HashSet<String>,
) -> Result<(), ParseError> {
    let view = read_view(parsed)?;
    let mut registry = classify(&view);
    registry.remove(crate::completion::ENDPOINT);
    scan(&view, &registry, reachable_callbacks)
}

/// The parse with the definitions of its external structs (§48, §187
/// rule 3). An included `typedef` joins the main declarations unless the
/// main file declares the same name. The view declares no external type,
/// so the registry classifies an external struct by its definition. A
/// name with no visible definition stays unregistered: it is opaque.
///
/// # Errors
///
/// Returns an error when an external type reaches an included `typedef`
/// that the frontend does not model.
fn read_view(parsed: &Parsed) -> Result<Parsed, ParseError> {
    let mut view = parsed.clone();
    view.externals.clear();
    let local: HashSet<&str> = parsed.decls.iter().filter_map(decl_name).collect();
    let included: Vec<Decl> = parsed
        .included_decls
        .iter()
        .filter(|decl| decl_name(decl).is_some_and(|name| !local.contains(name)))
        .cloned()
        .collect();
    view.decls.splice(0..0, included);
    view.aliases.extend(
        parsed
            .included_aliases
            .iter()
            .filter(|alias| !local.contains(alias.name.as_str()))
            .cloned(),
    );
    let mut pending: Vec<&str> = parsed.externals.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    while let Some(name) = pending.pop() {
        if !seen.insert(name) {
            continue;
        }
        if parsed
            .included_unmodeled
            .iter()
            .any(|unmodeled| unmodeled == name)
        {
            return Err(ParseError(format!(
                "external type `{name}` reaches an included definition that the binder does \
                 not model, so its read positions cannot be classified (compiler.md §187)"
            )));
        }
        if let Some(fields) = parsed.included_decls.iter().find_map(|decl| match decl {
            Decl::Struct {
                name: owner,
                fields,
            } if owner == name => Some(fields),
            _ => None,
        }) {
            pending.extend(fields.iter().map(|field| field.base.as_str()));
        }
    }
    Ok(view)
}

fn decl_name(decl: &Decl) -> Option<&str> {
    match decl {
        Decl::Enum { name, .. }
        | Decl::Struct { name, .. }
        | Decl::Handle { name }
        | Decl::FnPtr { name, .. } => Some(name),
        Decl::Func { .. } => None,
    }
}

/// The scan over every declaration of `parsed`.
fn scan(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    reachable_callbacks: &HashSet<String>,
) -> Result<(), ParseError> {
    // One view for every position (core principle 15).
    let view = HeaderStructs::new(parsed, registry);
    for decl in &parsed.decls {
        match decl {
            Decl::Func { name, ret, params } => {
                if ret.array_len.is_none() && is_struct(registry, &ret.base) {
                    reject_unreadable_in(
                        &view,
                        registry,
                        ReadPosition::Result { function: name },
                        &ret.base,
                        ReadRoot::Value,
                        Reach::Root,
                    )?;
                }
                for param in params {
                    validate_parameter(&view, registry, name, param)?;
                }
            }
            Decl::FnPtr { name, params, .. } => {
                if !reachable_callbacks.contains(name) {
                    continue;
                }
                // The callback shape rule admits a by-value string view,
                // which the trampoline materializes. Every other struct
                // argument is a read root.
                for param in params {
                    let by_value_view = !param.pointer
                        && matches!(registry.get(&param.base), Some(Kind::StringView));
                    if param.array_len.is_none()
                        && is_struct(registry, &param.base)
                        && !by_value_view
                    {
                        reject_unreadable_in(
                            &view,
                            registry,
                            ReadPosition::CallbackParameter {
                                callback: name,
                                parameter: &param.name,
                            },
                            &param.base,
                            ReadRoot::Value,
                            Reach::Root,
                        )?;
                    }
                }
            }
            // A struct, an enum, and a handle declare no position. A
            // struct is reached from the positions above.
            Decl::Struct { .. } | Decl::Enum { .. } | Decl::Handle { .. } => {}
        }
    }
    Ok(())
}

fn validate_parameter(
    view: &HeaderStructs<'_>,
    registry: &HashMap<String, Kind>,
    function: &str,
    param: &CField,
) -> Result<(), ParseError> {
    if param.array_len.is_some() || !is_struct(registry, &param.base) {
        return Ok(());
    }
    let boundary = matches!(registry.get(&param.base), Some(Kind::Boundary));
    // A non-`const` pointer is a fill: C writes the whole struct, at the
    // pass that the call gives it. A `const` pointer, a by-value struct,
    // and a by-value descriptor aggregate are inputs: C writes only
    // through their non-`const` pointers. A descriptor aggregate is a
    // struct whose pointer field is its pair.
    let writable = param.pointer && !param.is_const;
    let pointer =
        (boundary && param.pointer).then(|| parameter_pass(view, param.base.as_str(), writable));
    let pass = if !boundary {
        StructPass::Scratch
    } else if let Some(pointer) = pointer {
        target_pass(pointer)
    } else {
        value_parameter_pass(view, param.base.as_str())
    };
    let position = ReadPosition::Parameter {
        function,
        parameter: &param.name,
    };
    let Some(name) = view.struct_name(&param.base) else {
        return Ok(());
    };
    // §187 rule 9: the call builds every struct that does not copy its
    // bytes, and no call can build a struct cycle.
    if pass != StructPass::Bytes {
        if let Some(cycle) = first_cycle(view, name) {
            return Err(ParseError(cycle.message(position)));
        }
    }
    let root = if writable {
        ReadRoot::Fill(pass)
    } else {
        ReadRoot::Input(pass)
    };
    let descriptor =
        !param.pointer && matches!(registry.get(&param.base), Some(Kind::ArrayPair(_)));
    match first_unreadable_in(view, registry, name, root, Reach::Root) {
        // §187 rule 11: the pair of a by-value descriptor is the
        // parameter, so the diagnostic names the parameter, as the
        // checker's does.
        Some(Unreadable {
            kind: UnreadableKind::WrittenBackPair,
            owner,
            through: None,
            written_back: Some(cause),
            ..
        }) if descriptor && owner == name => {
            Err(ParseError(written_back_pair_message(position, &cause)))
        }
        Some(found) => Err(ParseError(found.message(position))),
        None => Ok(()),
    }
}

/// The pair record of a descriptor aggregate: its first field is the
/// element pointer and its second field is the count.
pub(crate) fn descriptor_pair(fields: &[CField], element: &str) -> EmbeddedPairs {
    let mut pairs = EmbeddedPairs::default();
    if fields.len() == 2 {
        pairs.ptr_elem.insert(0, element.to_string());
        pairs.count_idx.insert(1);
    }
    pairs
}

#[cfg(test)]
mod tests;
