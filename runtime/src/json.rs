//! Shared JSON support for `JSON.stringify` and `JSON.parse`
//! (stdlib.md §13).
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
/// The kind of a string whose escapes spell a lone surrogate. No
/// validator asks for it, so it matches no target (`compiler.md` §115.7
/// rule 5).
pub(crate) const KIND_UNPAIRED_STRING: u32 = 6;

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
    UnpairedString,
    Array(Vec<u64>),
    Object(Vec<(Option<String>, u64)>),
}

impl JsonValue {
    fn kind(&self) -> u32 {
        match self {
            JsonValue::Null => KIND_NULL,
            JsonValue::Bool(_) => KIND_BOOL,
            JsonValue::Number(_) => KIND_NUMBER,
            JsonValue::String(_) => KIND_STRING,
            JsonValue::UnpairedString => KIND_UNPAIRED_STRING,
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
    /// The failure of the last `begin` that returned zero, until
    /// `take_failure` reads it.
    failure: Option<ParseFailure>,
}

impl JsonParsers {
    /// Parses one complete JSON text. Malformed input returns zero,
    /// creates no transient document, and records the failure for
    /// `take_failure`. An exhausted handle space returns zero and
    /// records nothing, which `take_failure` reports as an internal fault.
    pub(crate) fn begin(&mut self, bytes: &[u8]) -> u64 {
        self.failure = None;
        let document = match Parser::new(bytes).parse() {
            Ok(document) => document,
            Err(failure) => {
                self.failure = Some(failure);
                return 0;
            }
        };
        let Some(next) = self.next.checked_add(1) else {
            return 0;
        };
        self.next = next;
        self.documents.insert(next, document);
        next
    }

    /// Returns and clears the failure of the last `begin` that returned
    /// zero.
    pub(crate) fn take_failure(&mut self) -> Option<ParseFailure> {
        self.failure.take()
    }

    /// Drops one completed transient document.
    pub(crate) fn finish(&mut self, parser: u64) -> bool {
        self.documents.remove(&parser).is_some()
    }

    /// Drops transient documents left by a trapping construction pass.
    pub(crate) fn clear(&mut self) {
        self.documents.clear();
        self.failure = None;
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
                    .find_map(|(candidate, value)| {
                        (candidate.as_deref() == Some(key)).then_some(*value)
                    })
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

/// Why a parse stopped, with the UTF-8 byte offset of the first byte the
/// parser cannot accept (`compiler.md` §115.7 rules 3 and 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseFailure {
    /// Malformed text or trailing bytes.
    Syntax(usize),
    /// An array or object that opens past `MAX_JSON_DEPTH`.
    Depth(usize),
}

impl ParseFailure {
    /// The `SyntaxError` message of the failure.
    pub(crate) fn message(self) -> String {
        match self {
            ParseFailure::Syntax(at) => format!("JSON.parse: invalid syntax at byte {at}"),
            ParseFailure::Depth(at) => {
                format!("JSON.parse: nesting deeper than {MAX_JSON_DEPTH} at byte {at}")
            }
        }
    }
}

/// One decoded JSON string. `unpaired` is true when an escape spells a
/// lone surrogate, which no language string can hold (Q5).
struct JsonText {
    text: String,
    unpaired: bool,
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    values: Vec<JsonValue>,
    failure: Option<ParseFailure>,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            values: Vec::new(),
            failure: None,
        }
    }

    fn parse(mut self) -> Result<JsonDocument, ParseFailure> {
        self.ws();
        let Some(root) = self.value(0) else {
            return Err(self.failure.unwrap_or(ParseFailure::Syntax(self.at)));
        };
        self.ws();
        if self.at != self.bytes.len() {
            return Err(ParseFailure::Syntax(self.at));
        }
        Ok(JsonDocument {
            root,
            values: self.values,
        })
    }

    /// Records the first failure. Every caller returns `None` after it.
    fn fail<T>(&mut self, failure: ParseFailure) -> Option<T> {
        self.failure.get_or_insert(failure);
        None
    }

    fn syntax<T>(&mut self, at: usize) -> Option<T> {
        self.fail(ParseFailure::Syntax(at))
    }

    fn value(&mut self, depth: usize) -> Option<u64> {
        self.ws();
        let Some(byte) = self.peek() else {
            return self.syntax(self.at);
        };
        match byte {
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
                self.push(if value.unpaired {
                    JsonValue::UnpairedString
                } else {
                    JsonValue::String(value.text)
                })
            }
            b'[' | b'{' if depth >= MAX_JSON_DEPTH => self.fail(ParseFailure::Depth(self.at)),
            b'[' => self.array(depth + 1),
            b'{' => self.object(depth + 1),
            b'-' | b'0'..=b'9' => {
                let value = self.number_value()?;
                self.push(JsonValue::Number(value))
            }
            _ => self.syntax(self.at),
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
            // A key that spells a lone surrogate matches no field name.
            fields.push(((!key.unpaired).then_some(key.text), value));
            self.ws();
            if self.consume(b'}') {
                break;
            }
            self.take(b',')?;
        }
        self.push(JsonValue::Object(fields))
    }

    /// Appends the raw bytes `start..self.at` to `output`.
    fn raw_text(&mut self, output: &mut String, start: usize) -> Option<()> {
        match std::str::from_utf8(&self.bytes[start..self.at]) {
            Ok(text) => {
                output.push_str(text);
                Some(())
            }
            Err(error) => self.syntax(start + error.valid_up_to()),
        }
    }

    fn string_value(&mut self) -> Option<JsonText> {
        self.take(b'"')?;
        let mut output = String::new();
        let mut unpaired = false;
        let mut raw_start = self.at;
        loop {
            let Some(byte) = self.peek() else {
                return self.syntax(self.at);
            };
            match byte {
                b'"' => {
                    self.raw_text(&mut output, raw_start)?;
                    self.at += 1;
                    return Some(JsonText {
                        text: output,
                        unpaired,
                    });
                }
                b'\\' => {
                    self.raw_text(&mut output, raw_start)?;
                    self.at += 1;
                    let Some(escaped) = self.peek() else {
                        return self.syntax(self.at);
                    };
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => {}
                        _ => return self.syntax(self.at),
                    }
                    self.at += 1;
                    if escaped == b'u' {
                        let first = self.hex4()?;
                        match first {
                            0xd800..=0xdbff => match self.low_surrogate() {
                                Some(second) => {
                                    let scalar = 0x1_0000
                                        + ((u32::from(first) - 0xd800) << 10)
                                        + (u32::from(second) - 0xdc00);
                                    match char::from_u32(scalar) {
                                        Some(scalar) => output.push(scalar),
                                        None => unpaired = true,
                                    }
                                }
                                None => unpaired = true,
                            },
                            0xdc00..=0xdfff => unpaired = true,
                            _ => match char::from_u32(u32::from(first)) {
                                Some(scalar) => output.push(scalar),
                                None => unpaired = true,
                            },
                        }
                    }
                    raw_start = self.at;
                }
                0x00..=0x1f => return self.syntax(self.at),
                _ => self.at += 1,
            }
        }
    }

    /// Consumes a `\uDC00`–`\uDFFF` escape that follows a high surrogate,
    /// and returns its code unit. Any other text stays unconsumed, so the
    /// high surrogate is lone.
    fn low_surrogate(&mut self) -> Option<u16> {
        let escape = self.bytes.get(self.at..self.at.checked_add(6)?)?;
        if escape[0] != b'\\' || escape[1] != b'u' {
            return None;
        }
        let mut unit = 0u16;
        for &digit in &escape[2..] {
            unit = unit * 16 + u16::from(hex_digit(digit)?);
        }
        if !(0xdc00..=0xdfff).contains(&unit) {
            return None;
        }
        self.at += 6;
        Some(unit)
    }

    fn number_value(&mut self) -> Option<JsonNumber> {
        let start = self.at;
        self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.at += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return self.syntax(self.at);
                }
            }
            Some(b'1'..=b'9') => {
                self.at += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return self.syntax(self.at),
        }
        if self.consume(b'.') {
            let fraction = self.at;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            if self.at == fraction {
                return self.syntax(self.at);
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
                return self.syntax(self.at);
            }
        }
        let Ok(text) = std::str::from_utf8(&self.bytes[start..self.at]) else {
            return self.syntax(start);
        };
        let Ok(value) = text.parse() else {
            return self.syntax(start);
        };
        Some(JsonNumber {
            text: text.to_string(),
            value,
        })
    }

    fn hex4(&mut self) -> Option<u16> {
        let mut value = 0u16;
        for _ in 0..4 {
            let Some(digit) = self.peek().and_then(hex_digit) else {
                return self.syntax(self.at);
            };
            value = value * 16 + u16::from(digit);
            self.at += 1;
        }
        Some(value)
    }

    /// Records one node.
    fn push(&mut self, value: JsonValue) -> Option<u64> {
        self.values.push(value);
        match u64::try_from(self.values.len()) {
            Ok(node) => Some(node),
            Err(_) => self.syntax(self.at),
        }
    }

    fn word(&mut self, word: &[u8]) -> Option<()> {
        for (offset, expected) in word.iter().enumerate() {
            if self.bytes.get(self.at + offset) != Some(expected) {
                return self.syntax(self.at + offset);
            }
        }
        self.at += word.len();
        Some(())
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
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
        if self.consume(expected) {
            Some(())
        } else {
            self.syntax(self.at)
        }
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
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
    fn a_builder_reports_an_unknown_id_after_it_finishes() {
        let mut builders = JsonBuilders::default();
        let id = builders.begin(false).expect("builder");
        assert!(builders.raw(id, b"[1,2]"));
        assert!(builders.string(id, b"abcdefgh"));
        assert_eq!(builders.finish(id).expect("output"), br#"[1,2]"abcdefgh""#);
        assert!(!builders.raw(id, b"x"));
    }

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
        assert_eq!(builders.finish(id).expect("output"), b"0|1e+21|0.1");
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
        for (malformed, at) in [
            (br#"{"x":"#.as_slice(), 5),
            (br#"[1,]"#, 3),
            (br#"01"#, 1),
            (br#"true false"#, 5),
            (br#"nope"#, 1),
            (br#""ab"#, 3),
            (br#""\q""#, 2),
            (br#""\u12G4""#, 5),
            (b"\"a\x01\"", 2),
            (br#"{,}"#, 1),
            (br#"1."#, 2),
            (br#"-"#, 1),
            (b"", 0),
            (b"  ", 2),
            ("\"\u{e9}\" x".as_bytes(), 5),
        ] {
            assert_eq!(parsers.begin(malformed), 0, "{malformed:?}");
            assert_eq!(
                parsers.take_failure(),
                Some(ParseFailure::Syntax(at)),
                "{malformed:?}"
            );
            assert_eq!(parsers.take_failure(), None);
            assert!(parsers.documents.is_empty());
        }
    }

    #[test]
    fn failure_messages_name_the_byte_offset() {
        assert_eq!(
            ParseFailure::Syntax(7).message(),
            "JSON.parse: invalid syntax at byte 7"
        );
        assert_eq!(
            ParseFailure::Depth(128).message(),
            "JSON.parse: nesting deeper than 128 at byte 128"
        );
    }

    #[test]
    fn a_lone_surrogate_is_well_formed_and_matches_no_string_target() {
        let mut parsers = JsonParsers::default();
        for text in [
            br#""\ud800""#.as_slice(),
            br#""\udc00""#,
            br#""\ud800x""#,
            br#""\ud800A""#,
        ] {
            let id = parsers.begin(text);
            assert_ne!(id, 0, "{text:?}");
            let root = parsers.root(id).expect("root");
            assert_eq!(parsers.is_kind(id, root, KIND_STRING), Some(false));
            assert_eq!(parsers.is_kind(id, root, KIND_UNPAIRED_STRING), Some(true));
            assert!(parsers.finish(id));
        }
        let id = parsers.begin(br#"{"\ud800":1,"a":2}"#);
        let root = parsers.root(id).expect("root");
        assert_ne!(parsers.object_get(id, root, "a"), Some(0));
        assert!(parsers.finish(id));
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
        assert_eq!(parsers.take_failure(), None);
        assert_eq!(parsers.begin(rejected.as_bytes()), 0);
        assert_eq!(
            parsers.take_failure(),
            Some(ParseFailure::Depth(MAX_JSON_DEPTH))
        );
    }

    /// `compiler.md` §115.10: 10,000 caught parse failures leave the
    /// parser and builder tables empty. The loop makes the runtime calls
    /// of the generated root helper on each failure path.
    #[test]
    fn caught_parse_failures_leave_the_json_tables_empty() {
        use crate::context::Context;
        use crate::ffi::{
            subscript_rt_json_parse_begin, subscript_rt_json_parse_end,
            subscript_rt_json_parse_failure, subscript_rt_json_parse_is_kind,
            subscript_rt_json_parse_root,
        };

        let mut context = Context::new();
        let deep = format!(
            "{}0{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        let malformed = context.alloc_str(br#"{"count":"#, 0);
        let too_deep = context.alloc_str(deep.as_bytes(), 0);
        let mismatch = context.alloc_str(br#"{"count":"three"}"#, 0);
        let ctx: *mut Context = &mut *context;
        for _ in 0..10_000 {
            for text in [malformed, too_deep] {
                // SAFETY: `ctx` is live and `text` is a live string handle.
                unsafe {
                    assert_eq!(subscript_rt_json_parse_begin(ctx, text, 0), 0);
                    assert!(!subscript_rt_json_parse_failure(ctx, 0).is_null());
                }
            }
            // SAFETY: `ctx` is live and `mismatch` is a live string handle.
            unsafe {
                let parser = subscript_rt_json_parse_begin(ctx, mismatch, 0);
                assert_ne!(parser, 0);
                let root = subscript_rt_json_parse_root(ctx, parser, 0);
                assert_eq!(
                    subscript_rt_json_parse_is_kind(ctx, parser, root, KIND_ARRAY, 0),
                    0
                );
                subscript_rt_json_parse_end(ctx, parser, 0);
            }
        }
        assert!(!context.trapped());
        let parsers = context.json_parsers();
        assert!(parsers.documents.is_empty());
        assert_eq!(parsers.failure, None);
        let builders = context.json_builders();
        assert!(builders.output.is_empty());
        assert!(builders.active.is_empty());
    }

    /// A failure read with no recorded failure is an internal fault.
    #[test]
    fn a_failure_read_without_a_failed_begin_traps() {
        use crate::context::Context;
        use crate::trap::TrapKind;

        let mut context = Context::new();
        let ctx: *mut Context = &mut *context;
        // SAFETY: `ctx` is live.
        let text = unsafe { crate::ffi::subscript_rt_json_parse_failure(ctx, 0) };
        assert!(text.is_null());
        assert_eq!(
            context.trap_record().map(|record| record.kind),
            Some(TrapKind::Internal)
        );
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
