//! Diagnostics: stable rule codes, source positions, and messages.

use std::fmt;

/// Stable rule code carried by every diagnostic.
///
/// Codes are the tested contract (`specs/blocks/compiler.md` §6); they are
/// never renumbered. `S100` is the catch-all for constructs outside the
/// decided surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RuleCode {
    /// `any` is not part of the language.
    S001,
    /// No dynamic code evaluation (`eval`, `new Function`).
    S002,
    /// No prototype mutation.
    S003,
    /// Nominal types are closed (no undeclared properties).
    S004,
    /// No structural substitution between nominal types.
    S005,
    /// Value classes do not inherit.
    S006,
    /// Bare `number` rejected; sized numeric types are mandatory (C3).
    S007,
    /// Numeric literal invalid for its context (out of range, or a
    /// fractional literal in an integer context) (C4).
    S008,
    /// A capturing lambda may not escape its defining function (C5).
    S009,
    /// Exceptions are not in the language (C6).
    S010,
    /// Unions are limited to `Ref | null`; nullable values must be
    /// narrowed before member access (C7).
    S011,
    /// `undefined` is banned; the single null story is `null` (C7).
    S012,
    /// Promise object construction/combinators and un-awaited async calls
    /// are rejected; Q34 exposes no Promise object surface (C8).
    S013,
    /// Out-of-subset standard-library use, or arithmetic on storage-only
    /// `f16` (Q23).
    S014,
    /// Catch-all: construct outside the decided language surface.
    S100,
}

impl RuleCode {
    /// The stable textual form of the code, e.g. `"S007"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RuleCode::S001 => "S001",
            RuleCode::S002 => "S002",
            RuleCode::S003 => "S003",
            RuleCode::S004 => "S004",
            RuleCode::S005 => "S005",
            RuleCode::S006 => "S006",
            RuleCode::S007 => "S007",
            RuleCode::S008 => "S008",
            RuleCode::S009 => "S009",
            RuleCode::S010 => "S010",
            RuleCode::S011 => "S011",
            RuleCode::S012 => "S012",
            RuleCode::S013 => "S013",
            RuleCode::S014 => "S014",
            RuleCode::S100 => "S100",
        }
    }

    /// A one-line explanation of the rule enforced by this code.
    #[must_use]
    pub fn explanation(self) -> &'static str {
        match self {
            RuleCode::S001 => "`any` is not part of the language.",
            RuleCode::S002 => "No dynamic code evaluation (`eval`, `new Function`).",
            RuleCode::S003 => "No prototype mutation.",
            RuleCode::S004 => "Nominal types are closed (no undeclared properties).",
            RuleCode::S005 => "No structural substitution between nominal types.",
            RuleCode::S006 => "Value classes do not inherit.",
            RuleCode::S007 => "Bare `number` is rejected; sized numeric types are mandatory.",
            RuleCode::S008 => {
                "Numeric literals must fit their context and be integral in an integer context."
            }
            RuleCode::S009 => "A capturing lambda may not escape its defining function.",
            RuleCode::S010 => "Exceptions are not in the language.",
            RuleCode::S011 => {
                "Unions are limited to `Ref | null`; nullable values must be narrowed before member access."
            }
            RuleCode::S012 => "`undefined` is banned; the single null story is `null`.",
            RuleCode::S013 => {
                "Promise objects and combinators are not in the language; async calls must be directly awaited."
            }
            RuleCode::S014 => {
                "Out-of-subset standard-library use and arithmetic on storage-only `f16` are rejected."
            }
            RuleCode::S100 => "Constructs outside the decided language surface are rejected.",
        }
    }
}

impl fmt::Display for RuleCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A TypeScript source position: file name, 1-based line, 1-based column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pos {
    /// Name of the source file as supplied to the checker.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub col: u32,
}

impl Pos {
    /// Builds a position.
    #[must_use]
    pub fn new(file: impl Into<String>, line: u32, col: u32) -> Self {
        Pos {
            file: file.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

/// One rejection produced by the checker.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// The stable rule code.
    pub code: RuleCode,
    /// Free-form human-readable message.
    pub message: String,
    /// Position of the offending construct.
    pub pos: Pos,
}

impl Diagnostic {
    /// Builds a diagnostic.
    #[must_use]
    pub fn new(code: RuleCode, message: impl Into<String>, pos: Pos) -> Self {
        Diagnostic {
            code,
            message: message.into(),
            pos,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} [{}]", self.pos, self.message, self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_code_display_matches_as_str() {
        assert_eq!(RuleCode::S001.as_str(), "S001");
        assert_eq!(RuleCode::S100.to_string(), "S100");
    }

    #[test]
    fn every_rule_code_has_an_explanation() {
        let codes = [
            RuleCode::S001,
            RuleCode::S002,
            RuleCode::S003,
            RuleCode::S004,
            RuleCode::S005,
            RuleCode::S006,
            RuleCode::S007,
            RuleCode::S008,
            RuleCode::S009,
            RuleCode::S010,
            RuleCode::S011,
            RuleCode::S012,
            RuleCode::S013,
            RuleCode::S014,
            RuleCode::S100,
        ];
        assert_eq!(codes.len(), 15);
        for code in codes {
            assert!(!code.explanation().is_empty(), "{code}");
            assert!(!code.explanation().contains('\n'), "{code}");
        }
    }

    #[test]
    fn pos_display_is_file_line_col() {
        let p = Pos::new("a.ts", 3, 7);
        assert_eq!(p.to_string(), "a.ts:3:7");
    }

    #[test]
    fn diagnostic_display_contains_code_and_pos() {
        let d = Diagnostic::new(RuleCode::S007, "bare number", Pos::new("a.ts", 1, 1));
        let s = d.to_string();
        assert!(s.contains("S007"));
        assert!(s.contains("a.ts:1:1"));
    }
}
