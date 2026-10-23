//! Context-owned mismatch records for typed JSON validation.

use super::*;

/// One leaf failure and its segments, stored from the leaf toward the root.
#[derive(Debug)]
pub(super) struct Mismatch {
    node: u64,
    label: String,
    target: i32,
    segments: Vec<String>,
}

impl JsonParsers {
    /// Records a failed leaf. Zero denotes a missing field.
    pub(crate) fn record(
        &mut self,
        parser: u64,
        node: u64,
        label: &str,
        target: i32,
    ) -> Option<()> {
        if node != 0 {
            self.value(parser, node)?;
        } else {
            self.documents.get(&parser)?;
        }
        self.mismatch = Some(Mismatch {
            node,
            label: label.to_string(),
            target,
            segments: Vec::new(),
        });
        Some(())
    }

    /// Adds a declared field on the failure return path.
    pub(crate) fn mismatch_field(&mut self, field: &str) -> Option<()> {
        self.mismatch.as_mut()?.segments.push(format!(".{field}"));
        Some(())
    }

    /// Adds an array index on the failure return path.
    pub(crate) fn mismatch_index(&mut self, index: i32) -> Option<()> {
        self.mismatch.as_mut()?.segments.push(format!("[{index}]"));
        Some(())
    }

    /// Uses the nullable label only when the failure belongs to this node.
    pub(crate) fn mismatch_relabel(&mut self, node: u64, label: &str) -> Option<()> {
        let failure = self.mismatch.as_mut()?;
        if failure.node == node {
            failure.label = label.to_string();
        }
        Some(())
    }

    /// Builds the complete message while the document is live.
    pub(crate) fn mismatch_message(&self, parser: u64, target: &str) -> Option<String> {
        let failure = self.mismatch.as_ref()?;
        let found = if failure.node == 0 {
            "no field".to_string()
        } else {
            match self.value(parser, failure.node)? {
                JsonValue::Null => "null".to_string(),
                JsonValue::Bool(_) => "boolean".to_string(),
                JsonValue::Number(number) if failure.target >= 0 => {
                    if failure.target < NUMBER_F32 as i32 && fractional(&number.text) {
                        "a non-integer number".to_string()
                    } else {
                        "a number out of range".to_string()
                    }
                }
                JsonValue::Number(_) => "number".to_string(),
                JsonValue::String(_) => "string".to_string(),
                JsonValue::UnpairedString => "a string with a lone surrogate".to_string(),
                JsonValue::Array(values) if failure.target == -2 => {
                    format!("an array of length {}", values.len())
                }
                JsonValue::Array(_) => "array".to_string(),
                JsonValue::Object(_) => "object".to_string(),
            }
        };
        let path = failure
            .segments
            .iter()
            .rev()
            .fold(String::from("$"), |mut path, segment| {
                path.push_str(segment);
                path
            });
        Some(format!(
            "JSON.parse: document does not match {target} at {path}: expected {}, found {found}",
            failure.label
        ))
    }
}

// Exact decimal integrality does not depend on an f64 rounding or a u128 range.
fn fractional(text: &str) -> bool {
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    if digits.iter().all(|digit| *digit == b'0') {
        return false;
    }
    let fraction = mantissa.split_once('.').map_or(0, |(_, tail)| tail.len());
    let exponent = decimal_exponent(exponent.as_bytes()).unwrap_or_else(|| {
        if exponent.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    });
    let fractional_digits = (fraction as i64).saturating_sub(exponent);
    fractional_digits > 0
        && (fractional_digits as u64 > digits.len() as u64
            || digits
                .iter()
                .rev()
                .take(fractional_digits as usize)
                .any(|digit| *digit != b'0'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::ffi::*;

    // Cost: one parse per mismatch class, plus two path segments; no external process.
    #[test]
    fn mismatch_classes_and_segments_use_the_runtime_record() {
        let mut context = Context::new();
        let ctx: *mut Context = &mut *context;
        let label = context.alloc_str(b"Target", 0);
        let field = context.alloc_str(b"child", 0);
        let nullable = context.alloc_str(b"Target | null", 0);
        for (text, target, found) in [
            ("null", -1, "null"),
            ("true", -1, "boolean"),
            ("42", -1, "number"),
            ("\"x\"", -1, "string"),
            ("[]", -1, "array"),
            ("{}", -1, "object"),
            ("256", NUMBER_U8 as i32, "a number out of range"),
            ("1.5", NUMBER_I32 as i32, "a non-integer number"),
            ("1e400", NUMBER_F64 as i32, "a number out of range"),
            ("1e39", NUMBER_F32 as i32, "a number out of range"),
            ("1e-400", NUMBER_I32 as i32, "a non-integer number"),
            (
                "10000000000000000000000000000000000000000",
                NUMBER_I64 as i32,
                "a number out of range",
            ),
            ("\"\\ud800\"", -1, "a string with a lone surrogate"),
            ("[1,2]", -2, "an array of length 2"),
        ] {
            let input = context.alloc_str(text.as_bytes(), 0);
            // SAFETY: all handles belong to this live Context.
            unsafe {
                let parser = subscript_rt_json_parse_begin(ctx, input, 0);
                let node = subscript_rt_json_parse_root(ctx, parser, 0);
                subscript_rt_json_parse_record(ctx, parser, node, label, target, 0);
                subscript_rt_json_parse_relabel(ctx, node + 1, nullable, 0);
                subscript_rt_json_parse_field(ctx, field, 0);
                subscript_rt_json_parse_index(ctx, 12, 0);
                let message = subscript_rt_json_parse_mismatch(ctx, parser, label, 0);
                assert_eq!(context.str_view(message), format!(
                    "JSON.parse: document does not match Target at $[12].child: expected Target, found {found}"
                ).as_bytes());
                subscript_rt_json_parse_end(ctx, parser, 0);
            }
        }
        let input = context.alloc_str(b"{}", 0);
        // SAFETY: all handles belong to this live Context; zero denotes the absent field.
        unsafe {
            let parser = subscript_rt_json_parse_begin(ctx, input, 0);
            subscript_rt_json_parse_record(ctx, parser, 0, nullable, -1, 0);
            subscript_rt_json_parse_field(ctx, field, 0);
            let message = subscript_rt_json_parse_mismatch(ctx, parser, label, 0);
            assert_eq!(context.str_view(message), b"JSON.parse: document does not match Target at $.child: expected Target | null, found no field");
            subscript_rt_json_parse_end(ctx, parser, 0);
        }
        assert!(!context.trapped());
    }

    // Cost: one scalar parse; no external process or loop.
    #[test]
    fn nullable_relabels_only_its_own_leaf() {
        let mut parsers = JsonParsers::default();
        let parser = parsers.begin(b"5");
        let node = parsers.root(parser).expect("root");
        parsers.record(parser, node, "Person", -1).expect("record");
        parsers
            .mismatch_relabel(node, "Person | null")
            .expect("relabel");
        assert_eq!(parsers.mismatch_message(parser, "Person"), Some(
            "JSON.parse: document does not match Person at $: expected Person | null, found number".to_string()
        ));
        assert!(parsers.finish(parser));
    }

    // Cost: two scalar parses; no external process or loop.
    #[test]
    fn begin_clears_the_previous_mismatch() {
        let mut parsers = JsonParsers::default();
        let previous = parsers.begin(b"false");
        let node = parsers.root(previous).expect("root");
        parsers
            .record(previous, node, "i32", NUMBER_I32 as i32)
            .expect("record");
        assert!(parsers.mismatch.is_some());
        let parser = parsers.begin(b"42");
        assert!(parsers.mismatch.is_none());
        assert!(parsers.finish(previous));
        assert!(parsers.finish(parser));
        assert!(parsers.documents.is_empty());
    }
}
