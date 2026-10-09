//! Completion selections and result validation (§178 rules 4–6).

use std::collections::HashMap;

use crate::clangfe::Parsed;
use crate::cparse::{Decl, ParseError};
use crate::emit::{lang_scalar, Kind};

/// The runtime ABI endpoint spelling, independent of the host header name.
pub(crate) const ENDPOINT: &str = "subscript_rt_completion";

/// The C spelling and mirror spelling of a validated completion result.
pub(crate) struct ResultType {
    /// The C result spelling carried by provenance.
    pub(crate) c: String,
    /// The Promise element type.
    pub(crate) mirror: String,
}

/// Validates every endpoint parameter and explicit completion selection.
pub(crate) fn select(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    requested: &[(String, String)],
) -> Result<HashMap<String, ResultType>, ParseError> {
    for decl in &parsed.decls {
        if let Decl::Func { name, params, .. } = decl {
            if let Some(param) = params
                .iter()
                .take(params.len().saturating_sub(1))
                .find(|param| param.base == ENDPOINT)
            {
                return Err(ParseError(format!(
                    "completion function `{name}` parameter `{}` of type `{ENDPOINT}` must be last",
                    param.name
                )));
            }
        }
    }
    let mut selected = HashMap::new();
    for (function, result) in requested {
        if selected.contains_key(function) {
            return Err(ParseError(format!(
                "duplicate `--completion` selection for function `{function}`"
            )));
        }
        let Some((ret, params)) = parsed.decls.iter().find_map(|decl| match decl {
            Decl::Func { name, ret, params } if name == function => Some((ret, params)),
            _ => None,
        }) else {
            return Err(ParseError(format!(
                "`--completion` names function `{function}`, but this header declares no such function"
            )));
        };
        if params.last().is_none_or(|param| {
            param.base != ENDPOINT || param.pointer || param.array_len.is_some() || param.nullable
        }) {
            return Err(ParseError(format!(
                "completion function `{function}` requires a trailing by-value `{ENDPOINT}` parameter"
            )));
        }
        if ret.base != "void" || ret.pointer || ret.array_len.is_some() {
            return Err(ParseError(format!(
                "completion function `{function}` must return `void`"
            )));
        }
        let mirror = if matches!(result.as_str(), "void" | "string" | "u8[]") {
            result.clone()
        } else if let Some(scalar) = lang_scalar(result) {
            scalar.to_string()
        } else if result != ENDPOINT
            && matches!(
                registry.get(result),
                Some(Kind::Boundary | Kind::Alias | Kind::Enum)
            )
        {
            result.clone()
        } else {
            return Err(ParseError(format!(
                "completion function `{function}` result `{result}` is outside §178 rule 6: \
                 expected a mapped C scalar, a boundary class struct, `void`, `string`, or `u8[]`"
            )));
        };
        if matches!(registry.get(result), Some(Kind::Boundary)) {
            validate_struct(parsed, registry, function, result, &mut Vec::new())?;
        }
        selected.insert(
            function.clone(),
            ResultType {
                c: result.clone(),
                mirror,
            },
        );
    }
    for decl in &parsed.decls {
        if let Decl::Func { name, params, .. } = decl {
            if params.iter().any(|param| param.base == ENDPOINT) && !selected.contains_key(name) {
                return Err(ParseError(format!(
                    "function `{name}` has a `{ENDPOINT}` parameter but no `--completion` selection"
                )));
            }
        }
    }
    Ok(selected)
}

/// Requires a byte-identical recursive scalar layout before result emission.
fn validate_struct(
    parsed: &Parsed,
    registry: &HashMap<String, Kind>,
    function: &str,
    name: &str,
    visiting: &mut Vec<String>,
) -> Result<(), ParseError> {
    if visiting.iter().any(|item| item == name) {
        return Err(ParseError(format!(
            "completion function `{function}` struct `{name}` has a recursive value layout"
        )));
    }
    visiting.push(name.to_string());
    let fields = parsed
        .decls
        .iter()
        .find_map(|decl| match decl {
            Decl::Struct {
                name: found,
                fields,
            } if found == name => Some(fields),
            _ => None,
        })
        .ok_or_else(|| {
            ParseError(format!(
                "completion function `{function}` struct `{name}` has no fields"
            ))
        })?;
    for field in fields {
        let scalar = lang_scalar(&field.base).is_some()
            || matches!(registry.get(&field.base), Some(Kind::Alias | Kind::Enum));
        if field.pointer
            || field.array_len.is_some()
            || field.nullable
            || (!scalar && !matches!(registry.get(&field.base), Some(Kind::Boundary)))
        {
            return Err(ParseError(format!("completion function `{function}` struct `{name}` field `{}` is outside §178 rule 6", field.name)));
        }
        if !scalar {
            validate_struct(parsed, registry, function, &field.base, visiting)?;
        }
    }
    visiting.pop();
    Ok(())
}
