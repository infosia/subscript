//! Shared JSON support for P13 `JSON.stringify` and `JSON.parse`.
//!
//! The checker emits traversal for one exact static type. This module owns
//! the representation-independent behavior: output assembly and escaping,
//! plus a transient syntax tree for parsing. Typed parse helpers first
//! validate that tree without allocating language values, then construct
//! the exact monomorphized result.

use std::collections::{HashMap, HashSet};

/// Context-owned output builders. A successful `finish` removes its
/// entry. A trapped dev run drops unfinished transient entries when the
/// host clears the trap before its next call.
#[derive(Debug, Default)]
pub(crate) struct JsonBuilders {
    next: u64,
    output: HashMap<u64, Vec<u8>>,
    active: HashMap<u64, HashSet<usize>>,
}

/// Result of inserting one reference in a tracked builder's active path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Visit {
    /// The reference was newly inserted.
    Inserted,
    /// The reference was already on the active path.
    Cycle,
    /// The builder id was absent or was not created as tracked.
    InvalidBuilder,
}

/// Stable kind tags used by checker-generated parse validators.
pub(crate) const KIND_NULL: u32 = 0;
pub(crate) const KIND_BOOL: u32 = 1;
pub(crate) const KIND_NUMBER: u32 = 2;
pub(crate) const KIND_STRING: u32 = 3;
pub(crate) const KIND_ARRAY: u32 = 4;
pub(crate) const KIND_OBJECT: u32 = 5;

/// Stable numeric-target tags used by checker-generated parse validators.
pub(crate) const NUMBER_I8: u32 = 0;
pub(crate) const NUMBER_U8: u32 = 1;
pub(crate) const NUMBER_I16: u32 = 2;
pub(crate) const NUMBER_U16: u32 = 3;
pub(crate) const NUMBER_I32: u32 = 4;
pub(crate) const NUMBER_U32: u32 = 5;
pub(crate) const NUMBER_I64: u32 = 6;
pub(crate) const NUMBER_U64: u32 = 7;
pub(crate) const NUMBER_F32: u32 = 8;
pub(crate) const NUMBER_F64: u32 = 9;

/// Maximum number of nested JSON arrays/objects accepted from input.
///
/// Parsing uses recursive descent, so this bounds stack use for
/// host-provided documents. A scalar at the root has depth zero.
pub(crate) const MAX_JSON_DEPTH: usize = 128;

#[derive(Debug, Clone, PartialEq)]
struct JsonNumber {
    text: String,
    value: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<u64>),
    Object(Vec<(String, u64)>),
}

impl JsonValue {
    fn kind(&self) -> u32 {
        match self {
            JsonValue::Null => KIND_NULL,
            JsonValue::Bool(_) => KIND_BOOL,
            JsonValue::Number(_) => KIND_NUMBER,
            JsonValue::String(_) => KIND_STRING,
            JsonValue::Array(_) => KIND_ARRAY,
            JsonValue::Object(_) => KIND_OBJECT,
        }
    }
}

#[derive(Debug)]
struct JsonDocument {
    root: u64,
    values: Vec<JsonValue>,
}

/// Context-owned transient parsed documents. Node handles are 1-based;
/// zero is reserved for a missing object field.
#[derive(Debug, Default)]
pub(crate) struct JsonParsers {
    next: u64,
    documents: HashMap<u64, JsonDocument>,
}

impl JsonParsers {
    /// Parses one complete JSON text. Malformed input returns zero and
    /// creates no transient document.
    pub(crate) fn begin(&mut self, bytes: &[u8]) -> u64 {
        let Some(document) = Parser::new(bytes).parse() else {
            return 0;
        };
        let Some(next) = self.next.checked_add(1) else {
            return 0;
        };
        self.next = next;
        self.documents.insert(next, document);
        next
    }

    /// Drops one completed transient document.
    pub(crate) fn finish(&mut self, parser: u64) -> bool {
        self.documents.remove(&parser).is_some()
    }

    /// Drops transient documents left by a trapping construction pass.
    pub(crate) fn clear(&mut self) {
        self.documents.clear();
    }

    pub(crate) fn root(&self, parser: u64) -> Option<u64> {
        self.documents.get(&parser).map(|document| document.root)
    }

    pub(crate) fn is_kind(&self, parser: u64, node: u64, kind: u32) -> Option<bool> {
        Some(self.value(parser, node)?.kind() == kind)
    }

    pub(crate) fn number_fits(&self, parser: u64, node: u64, target: u32) -> Option<bool> {
        let JsonValue::Number(value) = self.value(parser, node)? else {
            return Some(false);
        };
        Some(number_fits(value, target))
    }

    pub(crate) fn number(&self, parser: u64, node: u64) -> Option<f64> {
        match self.value(parser, node)? {
            JsonValue::Number(value) => Some(value.value),
            _ => None,
        }
    }

    pub(crate) fn integer(&self, parser: u64, node: u64, target: u32) -> Option<u64> {
        let JsonValue::Number(value) = self.value(parser, node)? else {
            return None;
        };
        integer_value(&value.text, target)
    }

    pub(crate) fn boolean(&self, parser: u64, node: u64) -> Option<bool> {
        match self.value(parser, node)? {
            JsonValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn string(&self, parser: u64, node: u64) -> Option<&str> {
        match self.value(parser, node)? {
            JsonValue::String(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn array_len(&self, parser: u64, node: u64) -> Option<usize> {
        match self.value(parser, node)? {
            JsonValue::Array(values) => Some(values.len()),
            _ => None,
        }
    }

    pub(crate) fn array_get(&self, parser: u64, node: u64, index: usize) -> Option<u64> {
        match self.value(parser, node)? {
            JsonValue::Array(values) => values.get(index).copied(),
            _ => None,
        }
    }

    /// Looks up an object field from the end so duplicate keys take their
    /// last occurrence, matching ECMA `JSON.parse`.
    pub(crate) fn object_get(&self, parser: u64, node: u64, key: &str) -> Option<u64> {
        match self.value(parser, node)? {
            JsonValue::Object(fields) => Some(
                fields
                    .iter()
                    .rev()
                    .find_map(|(candidate, value)| (candidate == key).then_some(*value))
                    .unwrap_or(0),
            ),
            _ => None,
        }
    }

    fn value(&self, parser: u64, node: u64) -> Option<&JsonValue> {
        let index = usize::try_from(node.checked_sub(1)?).ok()?;
        self.documents.get(&parser)?.values.get(index)
    }
}

fn number_fits(number: &JsonNumber, target: u32) -> bool {
    match target {
        NUMBER_F64 => number.value.is_finite(),
        NUMBER_F32 => number.value.is_finite() && (number.value as f32).is_finite(),
        NUMBER_I8 | NUMBER_U8 | NUMBER_I16 | NUMBER_U16 | NUMBER_I32 | NUMBER_U32 | NUMBER_I64
        | NUMBER_U64 => integer_value(&number.text, target).is_some(),
        _ => false,
    }
}

/// Converts a syntactically valid JSON decimal to one exact integer
/// target. The conversion never passes through the cached `f64`: a
/// fractional or exponential spelling is accepted only when its exact
/// mathematical value is integral and in range.
///
/// Signed results use their two's-complement bits in the returned
/// `u64`; checker-generated construction casts those bits to the exact
/// target width.
fn integer_value(text: &str, target: u32) -> Option<u64> {
    let (negative, magnitude) = exact_integer(text)?;
    let (signed, positive_max, negative_max) = match target {
        NUMBER_I8 => (true, u128::from(i8::MAX as u8), 1u128 << 7),
        NUMBER_U8 => (false, u128::from(u8::MAX), 0),
        NUMBER_I16 => (true, i16::MAX as u128, 1u128 << 15),
        NUMBER_U16 => (false, u128::from(u16::MAX), 0),
        NUMBER_I32 => (true, i32::MAX as u128, 1u128 << 31),
        NUMBER_U32 => (false, u128::from(u32::MAX), 0),
        NUMBER_I64 => (true, i64::MAX as u128, 1u128 << 63),
        NUMBER_U64 => (false, u128::from(u64::MAX), 0),
        _ => return None,
    };

    if negative && magnitude != 0 {
        if !signed || magnitude > negative_max {
            return None;
        }
        let value = -(magnitude as i128);
        Some(value as i64 as u64)
    } else if magnitude <= positive_max {
        Some(magnitude as u64)
    } else {
        None
    }
}

/// Returns the sign and magnitude of an exact integral JSON decimal.
fn exact_integer(text: &str) -> Option<(bool, u128)> {
    let bytes = text.as_bytes();
    let negative = bytes.first() == Some(&b'-');
    let start = usize::from(negative);
    let exponent_at = bytes[start..]
        .iter()
        .position(|byte| matches!(byte, b'e' | b'E'))
        .map_or(bytes.len(), |index| start + index);
    let mantissa = &bytes[start..exponent_at];
    let decimal_at = mantissa.iter().position(|byte| *byte == b'.');
    let fraction_len = decimal_at.map_or(0, |index| mantissa.len() - index - 1);
    let digits: Vec<u8> = mantissa
        .iter()
        .copied()
        .filter(|byte| *byte != b'.')
        .collect();

    // Zero remains exactly zero regardless of decimal point, exponent,
    // or sign (including JSON's `-0`).
    if digits.iter().all(|byte| *byte == b'0') {
        return Some((negative, 0));
    }

    let exponent = if exponent_at == bytes.len() {
        0
    } else {
        decimal_exponent(&bytes[exponent_at + 1..])?
    };
    let fraction_len = i64::try_from(fraction_len).unwrap_or(i64::MAX);
    let shift = exponent.saturating_sub(fraction_len);

    let retained = if shift < 0 {
        let remove = usize::try_from(shift.unsigned_abs()).ok()?;
        if remove >= digits.len()
            || digits[digits.len() - remove..]
                .iter()
                .any(|byte| *byte != b'0')
        {
            return None;
        }
        &digits[..digits.len() - remove]
    } else {
        &digits[..]
    };
    let retained = retained
        .iter()
        .position(|byte| *byte != b'0')
        .map_or(&retained[retained.len()..], |index| &retained[index..]);
    let append = if shift > 0 {
        usize::try_from(shift).ok()?
    } else {
        0
    };

    // Every accepted target is at most u64, so more than 20 decimal
    // digits cannot fit. This also prevents work proportional to a huge
    // positive exponent.
    if retained.len().checked_add(append)? > 20 {
        return None;
    }
    let mut magnitude = 0u128;
    for &digit in retained {
        magnitude = magnitude
            .checked_mul(10)?
            .checked_add(u128::from(digit - b'0'))?;
    }
    for _ in 0..append {
        magnitude = magnitude.checked_mul(10)?;
    }
    Some((negative, magnitude))
}

fn decimal_exponent(bytes: &[u8]) -> Option<i64> {
    let (negative, digits) = match bytes.first()? {
        b'+' => (false, &bytes[1..]),
        b'-' => (true, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() {
        return None;
    }
    let mut magnitude = 0i64;
    for &digit in digits {
        if !digit.is_ascii_digit() {
            return None;
        }
        magnitude = magnitude
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'));
    }
    Some(if negative { -magnitude } else { magnitude })
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    values: Vec<JsonValue>,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            values: Vec::new(),
        }
    }

    fn parse(mut self) -> Option<JsonDocument> {
        self.ws();
        let root = self.value(0)?;
        self.ws();
        (self.at == self.bytes.len()).then_some(JsonDocument {
            root,
            values: self.values,
        })
    }

    fn value(&mut self, depth: usize) -> Option<u64> {
        self.ws();
        match self.peek()? {
            b'n' => {
                self.word(b"null")?;
                self.push(JsonValue::Null)
            }
            b't' => {
                self.word(b"true")?;
                self.push(JsonValue::Bool(true))
            }
            b'f' => {
                self.word(b"false")?;
                self.push(JsonValue::Bool(false))
            }
            b'"' => {
                let value = self.string_value()?;
                self.push(JsonValue::String(value))
            }
            b'[' if depth < MAX_JSON_DEPTH => self.array(depth + 1),
            b'{' if depth < MAX_JSON_DEPTH => self.object(depth + 1),
            b'-' | b'0'..=b'9' => {
                let value = self.number_value()?;
                self.push(JsonValue::Number(value))
            }
            _ => None,
        }
    }

    fn array(&mut self, depth: usize) -> Option<u64> {
        self.take(b'[')?;
        self.ws();
        let mut values = Vec::new();
        if self.consume(b']') {
            return self.push(JsonValue::Array(values));
        }
        loop {
            values.push(self.value(depth)?);
            self.ws();
            if self.consume(b']') {
                break;
            }
            self.take(b',')?;
        }
        self.push(JsonValue::Array(values))
    }

    fn object(&mut self, depth: usize) -> Option<u64> {
        self.take(b'{')?;
        self.ws();
        let mut fields = Vec::new();
        if self.consume(b'}') {
            return self.push(JsonValue::Object(fields));
        }
        loop {
            self.ws();
            let key = self.string_value()?;
            self.ws();
            self.take(b':')?;
            let value = self.value(depth)?;
            fields.push((key, value));
            self.ws();
            if self.consume(b'}') {
                break;
            }
            self.take(b',')?;
        }
        self.push(JsonValue::Object(fields))
    }

    fn string_value(&mut self) -> Option<String> {
        self.take(b'"')?;
        let mut output = String::new();
        let mut raw_start = self.at;
        loop {
            let byte = self.peek()?;
            match byte {
                b'"' => {
                    output.push_str(std::str::from_utf8(&self.bytes[raw_start..self.at]).ok()?);
                    self.at += 1;
                    return Some(output);
                }
                b'\\' => {
                    output.push_str(std::str::from_utf8(&self.bytes[raw_start..self.at]).ok()?);
                    self.at += 1;
                    let escaped = self.next()?;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let scalar = if (0xd800..=0xdbff).contains(&first) {
                                self.take(b'\\')?;
                                self.take(b'u')?;
                                let second = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return None;
                                }
                                0x1_0000
                                    + ((u32::from(first) - 0xd800) << 10)
                                    + (u32::from(second) - 0xdc00)
                            } else if (0xdc00..=0xdfff).contains(&first) {
                                return None;
                            } else {
                                u32::from(first)
                            };
                            output.push(char::from_u32(scalar)?);
                        }
                        _ => return None,
                    }
                    raw_start = self.at;
                }
                0x00..=0x1f => return None,
                _ => self.at += 1,
            }
        }
    }

    fn number_value(&mut self) -> Option<JsonNumber> {
        let start = self.at;
        self.consume(b'-');
        match self.peek()? {
            b'0' => {
                self.at += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return None;
                }
            }
            b'1'..=b'9' => {
                self.at += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return None,
        }
        if self.consume(b'.') {
            let fraction = self.at;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            if self.at == fraction {
                return None;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            let exponent = self.at;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            if self.at == exponent {
                return None;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
        Some(JsonNumber {
            text: text.to_string(),
            value: text.parse().ok()?,
        })
    }

    fn hex4(&mut self) -> Option<u16> {
        let mut value = 0u16;
        for _ in 0..4 {
            value = value.checked_mul(16)?;
            value = value.checked_add(match self.next()? {
                b'0'..=b'9' => u16::from(self.bytes[self.at - 1] - b'0'),
                b'a'..=b'f' => u16::from(self.bytes[self.at - 1] - b'a' + 10),
                b'A'..=b'F' => u16::from(self.bytes[self.at - 1] - b'A' + 10),
                _ => return None,
            })?;
        }
        Some(value)
    }

    fn push(&mut self, value: JsonValue) -> Option<u64> {
        self.values.push(value);
        u64::try_from(self.values.len()).ok()
    }

    fn word(&mut self, word: &[u8]) -> Option<()> {
        (self.bytes.get(self.at..self.at.checked_add(word.len())?)? == word).then(|| {
            self.at += word.len();
        })
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.at += 1;
        Some(value)
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn take(&mut self, expected: u8) -> Option<()> {
        self.consume(expected).then_some(())
    }
}

impl JsonBuilders {
    /// Starts one builder. Only the tracked spelling allocates an active
    /// reference set.
    pub(crate) fn begin(&mut self, tracked: bool) -> Option<u64> {
        self.next = self.next.checked_add(1)?;
        let id = self.next;
        self.output.insert(id, Vec::new());
        if tracked {
            self.active.insert(id, HashSet::new());
        }
        Some(id)
    }

    /// Removes a completed builder and returns its exact JSON bytes.
    pub(crate) fn finish(&mut self, id: u64) -> Option<Vec<u8>> {
        self.active.remove(&id);
        self.output.remove(&id)
    }

    /// Drops every transient builder after a trapped run unwound.
    pub(crate) fn clear(&mut self) {
        self.output.clear();
        self.active.clear();
    }

    /// Appends bytes that the generated serializer already shaped as JSON
    /// punctuation.
    pub(crate) fn raw(&mut self, id: u64, bytes: &[u8]) -> bool {
        let Some(output) = self.output.get_mut(&id) else {
            return false;
        };
        output.extend_from_slice(bytes);
        true
    }

    /// Appends one quoted JSON string. Language strings are valid UTF-8,
    /// so all non-control bytes can pass through unchanged: unlike a JS
    /// UTF-16 string, there is no lone-surrogate case.
    pub(crate) fn string(&mut self, id: u64, bytes: &[u8]) -> bool {
        let Some(output) = self.output.get_mut(&id) else {
            return false;
        };
        append_quoted(output, bytes);
        true
    }

    /// Appends a signed 32-bit integer through the shared Q14 formatter.
    pub(crate) fn i32(&mut self, id: u64, value: i32) -> bool {
        self.raw(id, crate::fmt::fmt_i32(value).as_bytes())
    }

    /// Appends an unsigned 32-bit integer through the shared Q14
    /// formatter.
    pub(crate) fn u32(&mut self, id: u64, value: u32) -> bool {
        self.raw(id, crate::fmt::fmt_u32(value).as_bytes())
    }

    /// Appends a signed 64-bit integer through the shared Q14 formatter.
    pub(crate) fn i64(&mut self, id: u64, value: i64) -> bool {
        self.raw(id, crate::fmt::fmt_i64(value).as_bytes())
    }

    /// Appends an unsigned 64-bit integer through the shared Q14
    /// formatter.
    pub(crate) fn u64(&mut self, id: u64, value: u64) -> bool {
        self.raw(id, crate::fmt::fmt_u64(value).as_bytes())
    }

    /// Appends a finite `f32`, normalizing either zero sign to JSON `0`.
    pub(crate) fn f32(&mut self, id: u64, value: f32) -> bool {
        if value == 0.0 {
            self.raw(id, b"0")
        } else {
            self.raw(id, crate::fmt::fmt_f32(value).as_bytes())
        }
    }

    /// Appends a finite `f64`, normalizing either zero sign to JSON `0`.
    pub(crate) fn f64(&mut self, id: u64, value: f64) -> bool {
        if value == 0.0 {
            self.raw(id, b"0")
        } else {
            self.raw(id, crate::fmt::fmt_f64(value).as_bytes())
        }
    }

    /// Inserts `reference` into a tracked builder's active path.
    pub(crate) fn visit(&mut self, id: u64, reference: usize) -> Visit {
        if !self.output.contains_key(&id) {
            return Visit::InvalidBuilder;
        }
        let Some(active) = self.active.get_mut(&id) else {
            return Visit::InvalidBuilder;
        };
        if active.insert(reference) {
            Visit::Inserted
        } else {
            Visit::Cycle
        }
    }

    /// Removes `reference` after its object body has been serialized.
    pub(crate) fn leave(&mut self, id: u64, reference: usize) -> bool {
        self.active
            .get_mut(&id)
            .is_some_and(|active| active.remove(&reference))
    }
}

fn append_quoted(output: &mut Vec<u8>, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    output.push(b'"');
    for &byte in bytes {
        match byte {
            b'"' => output.extend_from_slice(br#"\""#),
            b'\\' => output.extend_from_slice(br#"\\"#),
            0x08 => output.extend_from_slice(br"\b"),
            0x09 => output.extend_from_slice(br"\t"),
            0x0a => output.extend_from_slice(br"\n"),
            0x0c => output.extend_from_slice(br"\f"),
            0x0d => output.extend_from_slice(br"\r"),
            0x00..=0x1f => {
                output.extend_from_slice(br"\u00");
                output.push(HEX[usize::from(byte >> 4)]);
                output.push(HEX[usize::from(byte & 0x0f)]);
            }
            _ => output.push(byte),
        }
    }
    output.push(b'"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_matches_node_24_control_boundary() {
        let mut builders = JsonBuilders::default();
        let id = builders.begin(false).expect("builder");
        let mut input: Vec<u8> = (0..=0x20).collect();
        input.extend_from_slice(&[b'"', b'/', b'\\', 0x7f]);
        assert!(builders.string(id, &input));
        assert_eq!(
            builders.finish(id).expect("output"),
            br#""\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\b\t\n\u000b\f\r\u000e\u000f\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f \"/\\""#
        );
    }

    #[test]
    fn floats_reuse_q14_but_json_normalizes_negative_zero() {
        let mut builders = JsonBuilders::default();
        let id = builders.begin(false).expect("builder");
        assert!(builders.f64(id, -0.0));
        assert!(builders.raw(id, b"|"));
        assert!(builders.f64(id, 1e21));
        assert!(builders.raw(id, b"|"));
        assert!(builders.f32(id, 0.1));
        assert_eq!(
            builders.finish(id).expect("output"),
            b"0|1e+21|0.1"
        );
    }

    #[test]
    fn tracked_builder_uses_an_active_path_not_a_global_seen_set() {
        let mut builders = JsonBuilders::default();
        let id = builders.begin(true).expect("builder");
        assert_eq!(builders.visit(id, 7), Visit::Inserted);
        assert_eq!(builders.visit(id, 7), Visit::Cycle);
        assert!(builders.leave(id, 7));
        assert_eq!(builders.visit(id, 7), Visit::Inserted);
    }

    #[test]
    fn parser_matches_node_number_and_duplicate_key_edges() {
        let mut parsers = JsonParsers::default();
        let id = parsers.begin(
            br#"{"duplicate":1,"duplicate":2,"negative":-0,"beyond":9007199254740993,"overflow":1e400}"#,
        );
        assert_ne!(id, 0);
        let root = parsers.root(id).expect("root");
        let duplicate = parsers.object_get(id, root, "duplicate").expect("object");
        assert_eq!(parsers.number(id, duplicate), Some(2.0));
        let negative = parsers.object_get(id, root, "negative").expect("object");
        assert!(parsers
            .number(id, negative)
            .expect("number")
            .is_sign_negative());
        let beyond = parsers.object_get(id, root, "beyond").expect("object");
        assert_eq!(parsers.number(id, beyond), Some(9_007_199_254_740_992.0));
        assert_eq!(
            parsers.integer(id, beyond, NUMBER_I64),
            Some(9_007_199_254_740_993)
        );
        let overflow = parsers.object_get(id, root, "overflow").expect("object");
        assert_eq!(parsers.number(id, overflow), Some(f64::INFINITY));
        assert_eq!(parsers.number_fits(id, overflow, NUMBER_F64), Some(false));
    }

    #[test]
    fn integer_targets_parse_decimal_text_exactly() {
        fn parse(text: &str, target: u32) -> Option<u64> {
            let mut parsers = JsonParsers::default();
            let id = parsers.begin(text.as_bytes());
            assert_ne!(id, 0, "{text}");
            let root = parsers.root(id).expect("root");
            assert_eq!(
                parsers.number_fits(id, root, target),
                Some(parsers.integer(id, root, target).is_some()),
                "{text}"
            );
            parsers.integer(id, root, target)
        }

        assert_eq!(
            parse("9007199254740993", NUMBER_I64),
            Some(9_007_199_254_740_993)
        );
        assert_eq!(
            parse("9223372036854775807", NUMBER_I64),
            Some(i64::MAX as u64)
        );
        assert_eq!(
            parse("-9223372036854775808", NUMBER_I64),
            Some(i64::MIN as u64)
        );
        assert_eq!(parse("9223372036854775808", NUMBER_I64), None);
        assert_eq!(parse("-9223372036854775809", NUMBER_I64), None);
        assert_eq!(parse("18446744073709551615", NUMBER_U64), Some(u64::MAX));
        assert_eq!(parse("18446744073709551616", NUMBER_U64), None);
        assert_eq!(parse("-1", NUMBER_U64), None);
        assert_eq!(parse("-0", NUMBER_U64), Some(0));

        assert_eq!(parse("1.0", NUMBER_I8), Some(1));
        assert_eq!(parse("10e-1", NUMBER_I8), Some(1));
        assert_eq!(parse("1.20e1", NUMBER_I8), Some(12));
        assert_eq!(parse("0e-999999999999999999999", NUMBER_U8), Some(0));
        assert_eq!(parse("1.2", NUMBER_I8), None);
        assert_eq!(parse("1e-1", NUMBER_I8), None);
        assert_eq!(parse("128", NUMBER_I8), None);
        assert_eq!(parse("256", NUMBER_U8), None);
    }

    #[test]
    fn parser_rejects_malformed_text_without_creating_a_document() {
        let mut parsers = JsonParsers::default();
        for malformed in [
            br#"{"x":"#.as_slice(),
            br#"[1,]"#,
            br#"01"#,
            br#""\ud800""#,
            br#"true false"#,
        ] {
            assert_eq!(parsers.begin(malformed), 0, "{malformed:?}");
        }
    }

    #[test]
    fn parser_rejects_input_past_the_depth_limit_without_overflowing() {
        let accepted = format!(
            "{}0{}",
            "[".repeat(MAX_JSON_DEPTH),
            "]".repeat(MAX_JSON_DEPTH)
        );
        let rejected = format!(
            "{}0{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        let mut parsers = JsonParsers::default();
        assert_ne!(parsers.begin(accepted.as_bytes()), 0);
        assert_eq!(parsers.begin(rejected.as_bytes()), 0);
    }

    #[test]
    fn parser_decodes_unicode_escapes_and_array_nodes() {
        let mut parsers = JsonParsers::default();
        let id = parsers.begin(br#"["A\u00e9\uD83D\uDE00",null,true]"#);
        let root = parsers.root(id).expect("root");
        assert_eq!(parsers.array_len(id, root), Some(3));
        let text = parsers.array_get(id, root, 0).expect("text node");
        assert_eq!(parsers.string(id, text), Some("Aé😀"));
        let null = parsers.array_get(id, root, 1).expect("null node");
        assert_eq!(parsers.is_kind(id, null, KIND_NULL), Some(true));
        let boolean = parsers.array_get(id, root, 2).expect("bool node");
        assert_eq!(parsers.boolean(id, boolean), Some(true));
    }
}
