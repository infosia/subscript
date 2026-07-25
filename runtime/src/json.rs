//! Shared JSON output builder for P13 `JSON.stringify`.
//!
//! The checker emits the traversal for one exact static type. This module
//! owns only representation-independent behavior: byte assembly, ECMA
//! string escaping, Q14 number formatting with JSON's negative-zero rule,
//! and the active-reference set used by statically cycle-capable graphs.

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
}
