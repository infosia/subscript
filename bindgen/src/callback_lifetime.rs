//! Selection of the explicit callback lifetime for one boundary aggregate
//! (`specs/blocks/compiler.md` §111 rule 1).
//!
//! The binder input names the aggregates. This module validates each name
//! against the parsed header and returns the validated set. The mirror
//! text carries one directive line for each member of that set; the
//! declarations themselves do not change.

use std::collections::HashSet;

use crate::clangfe::Parsed;
use crate::cparse::{Decl, ParseError};

/// Validates the selected aggregate names against the parsed header.
///
/// `reachable_callbacks` holds the callback typedefs that an emitted
/// boundary struct carries as a field. `absorbed` holds the aggregate
/// names that the mirror absorbs into `T[]` or `string`, so the mirror
/// declares no class for them.
///
/// # Errors
///
/// Returns [`ParseError`] when the header declares no struct with the
/// name, when the mirror absorbs that struct, when the struct carries no
/// callback field, or when one name is selected two times.
pub(crate) fn select(
    parsed: &Parsed,
    reachable_callbacks: &HashSet<String>,
    absorbed: &HashSet<String>,
    selected: &[String],
) -> Result<HashSet<String>, ParseError> {
    let mut validated = HashSet::new();
    for aggregate in selected {
        // The binder rejects a repeated directive for one name (§48
        // `@subscript-external`, §51 `@subscript-cenum`); a repeated
        // selection follows that rule.
        if !validated.insert(aggregate.clone()) {
            return Err(ParseError(format!(
                "duplicate `--explicit-callback-lifetime` selection for aggregate `{aggregate}`"
            )));
        }
        let Some(fields) = struct_fields(parsed, aggregate) else {
            return Err(ParseError(format!(
                "`--explicit-callback-lifetime` names aggregate `{aggregate}`, but this header \
                 declares no such struct"
            )));
        };
        if absorbed.contains(aggregate) {
            return Err(ParseError(format!(
                "`--explicit-callback-lifetime` names aggregate `{aggregate}`, which the mirror \
                 absorbs into a boundary type and declares as no class"
            )));
        }
        if !fields
            .iter()
            .any(|field| reachable_callbacks.contains(&field.base))
        {
            return Err(ParseError(format!(
                "`--explicit-callback-lifetime` names aggregate `{aggregate}`, which carries no \
                 callback field"
            )));
        }
    }
    Ok(validated)
}

/// Returns the fields of the named struct declaration of this header.
fn struct_fields<'a>(parsed: &'a Parsed, name: &str) -> Option<&'a [crate::cparse::CField]> {
    parsed.decls.iter().find_map(|decl| match decl {
        Decl::Struct {
            name: declared,
            fields,
        } if declared == name => Some(fields.as_slice()),
        _ => None,
    })
}
