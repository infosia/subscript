//! Feature-on regular-expression validation used by the checker.

use regress::Regex;

/// Validates the language's supported ECMAScript flags and compiles a
/// literal pattern so syntax errors are checker diagnostics.
pub(crate) fn validate_literal(pattern: &str, flags: &str) -> Result<(), String> {
    let engine_flags = validate_flags(flags)?;
    Regex::with_flags(pattern, engine_flags.as_str())
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn validate_flags(flags: &str) -> Result<String, String> {
    let mut seen = [false; 128];
    for flag in flags.bytes() {
        if !matches!(flag, b'd' | b'g' | b'i' | b'm' | b's' | b'u' | b'v') {
            return Err(format!(
                "unsupported regular-expression flag `{}`; supported flags are d, g, i, m, s, u, v",
                char::from(flag)
            ));
        }
        let slot = &mut seen[usize::from(flag)];
        if *slot {
            return Err(format!(
                "duplicate regular-expression flag `{}`",
                char::from(flag)
            ));
        }
        *slot = true;
    }
    if seen[usize::from(b'u')] && seen[usize::from(b'v')] {
        return Err("regular-expression flags `u` and `v` are mutually exclusive".to_string());
    }
    Ok(flags
        .chars()
        .filter(|flag| !matches!(flag, 'd' | 'g'))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_flags_and_pattern_syntax() {
        assert!(validate_literal(r"(?<word>\w+)", "gim").is_ok());
        assert!(validate_literal("(", "").is_err());
        assert!(validate_literal("x", "gg").is_err());
        assert!(validate_literal("x", "y").is_err());
        assert!(validate_literal("x", "uv").is_err());
    }
}
