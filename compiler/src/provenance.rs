//! Fixed-shape C provenance directives carried by generated ambient mirrors.

use std::collections::{HashMap, HashSet};

use crate::diag::{Diagnostic, Pos, RuleCode};

/// One parsed directive plus the source text used for loud diagnostics.
#[derive(Debug, Clone)]
pub(crate) struct Record<T> {
    pub value: T,
    pub line: u32,
    pub raw: String,
}

/// C provenance for one absorbed foreign-function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Parameter {
    Descriptor {
        aggregate: String,
        element: String,
        element_const: bool,
    },
    ScalarPair {
        element: String,
        element_const: bool,
    },
    StringView {
        aggregate: String,
    },
}

/// All fixed-shape directives parsed from one ambient mirror.
#[derive(Debug, Clone, Default)]
pub(crate) struct Mirror {
    pub header: Option<Record<String>>,
    pub parameters: HashMap<(String, String), Record<Parameter>>,
    pub callbacks: HashMap<String, Record<String>>,
    /// C typedef spelling → ambient CEnum alias name. The checker does not
    /// resolve this mapping itself; retaining it makes the generated mirror's
    /// provenance complete while ordinary ambient type resolution verifies
    /// the referenced alias (compiler.md §51).
    pub cenums: HashMap<String, Record<String>>,
}

/// Parses every `@subscript-c-*` record in one mirror.
pub(crate) fn parse(name: &str, source: &str) -> Result<Mirror, Diagnostic> {
    let mut mirror = Mirror::default();
    let mut externals = HashSet::new();
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let Some(body) = trimmed.strip_prefix("// @subscript-c-") else {
            continue;
        };
        let line_number = index as u32 + 1;
        let parsed =
            parse_line(body).map_err(|reason| malformed(name, line_number, trimmed, reason))?;
        match parsed {
            Parsed::Header(include) => {
                if include.is_empty()
                    || include.contains(['/', '\\'])
                    || include.chars().any(char::is_control)
                {
                    return Err(malformed(
                        name,
                        line_number,
                        trimmed,
                        "header include must be a non-empty basename",
                    ));
                }
                if mirror.header.is_some() {
                    return Err(duplicate(name, line_number, trimmed, "header"));
                }
                mirror.header = Some(Record {
                    value: include,
                    line: line_number,
                    raw: trimmed.to_string(),
                });
            }
            Parsed::Descriptor {
                function,
                parameter,
                aggregate,
                element,
                element_const,
            } => {
                if [
                    function.as_str(),
                    parameter.as_str(),
                    aggregate.as_str(),
                    element.as_str(),
                ]
                .contains(&"")
                {
                    return Err(malformed(
                        name,
                        line_number,
                        trimmed,
                        "descriptor fields must be non-empty",
                    ));
                }
                let key = (function, parameter);
                if mirror.parameters.contains_key(&key) {
                    return Err(duplicate(name, line_number, trimmed, "parameter"));
                }
                mirror.parameters.insert(
                    key,
                    Record {
                        value: Parameter::Descriptor {
                            aggregate,
                            element,
                            element_const,
                        },
                        line: line_number,
                        raw: trimmed.to_string(),
                    },
                );
            }
            Parsed::StringView {
                function,
                parameter,
                aggregate,
            } => {
                if [function.as_str(), parameter.as_str(), aggregate.as_str()].contains(&"") {
                    return Err(malformed(
                        name,
                        line_number,
                        trimmed,
                        "string-view fields must be non-empty",
                    ));
                }
                let key = (function, parameter);
                if mirror.parameters.contains_key(&key) {
                    return Err(duplicate(name, line_number, trimmed, "parameter"));
                }
                mirror.parameters.insert(
                    key,
                    Record {
                        value: Parameter::StringView { aggregate },
                        line: line_number,
                        raw: trimmed.to_string(),
                    },
                );
            }
            Parsed::ScalarPair {
                function,
                parameter,
                element,
                element_const,
            } => {
                if [function.as_str(), parameter.as_str(), element.as_str()].contains(&"") {
                    return Err(malformed(
                        name,
                        line_number,
                        trimmed,
                        "scalar-pair fields must be non-empty",
                    ));
                }
                let key = (function, parameter);
                if mirror.parameters.contains_key(&key) {
                    return Err(duplicate(name, line_number, trimmed, "parameter"));
                }
                mirror.parameters.insert(
                    key,
                    Record {
                        value: Parameter::ScalarPair {
                            element,
                            element_const,
                        },
                        line: line_number,
                        raw: trimmed.to_string(),
                    },
                );
            }
            Parsed::Callback(typedef_name) => {
                if typedef_name.is_empty() {
                    return Err(malformed(
                        name,
                        line_number,
                        trimmed,
                        "callback typedef must be non-empty",
                    ));
                }
                if mirror.callbacks.contains_key(&typedef_name) {
                    return Err(duplicate(name, line_number, trimmed, "callback typedef"));
                }
                mirror.callbacks.insert(
                    typedef_name.clone(),
                    Record {
                        value: typedef_name,
                        line: line_number,
                        raw: trimmed.to_string(),
                    },
                );
            }
            Parsed::External(type_name) => {
                if type_name.is_empty() {
                    return Err(malformed(
                        name,
                        line_number,
                        trimmed,
                        "external type must be non-empty",
                    ));
                }
                if !externals.insert(type_name) {
                    return Err(duplicate(name, line_number, trimmed, "external type"));
                }
            }
            Parsed::CEnum {
                typedef_name,
                alias,
            } => {
                if typedef_name.is_empty() || alias.is_empty() {
                    return Err(malformed(
                        name,
                        line_number,
                        trimmed,
                        "cenum typedef and alias fields must be non-empty",
                    ));
                }
                if mirror.cenums.contains_key(&typedef_name) {
                    return Err(duplicate(name, line_number, trimmed, "cenum typedef"));
                }
                mirror.cenums.insert(
                    typedef_name,
                    Record {
                        value: alias,
                        line: line_number,
                        raw: trimmed.to_string(),
                    },
                );
            }
        }
    }
    Ok(mirror)
}

fn malformed(name: &str, line: u32, raw: &str, reason: impl AsRef<str>) -> Diagnostic {
    Diagnostic::new(
        RuleCode::S100,
        format!(
            "mirror `{name}` has malformed provenance record `{raw}`: {}",
            reason.as_ref()
        ),
        Pos::new(name, line, 1),
    )
}

fn duplicate(name: &str, line: u32, raw: &str, kind: &str) -> Diagnostic {
    Diagnostic::new(
        RuleCode::S100,
        format!("mirror `{name}` has duplicate provenance for one {kind}: `{raw}`"),
        Pos::new(name, line, 1),
    )
}

enum Parsed {
    Header(String),
    Descriptor {
        function: String,
        parameter: String,
        aggregate: String,
        element: String,
        element_const: bool,
    },
    StringView {
        function: String,
        parameter: String,
        aggregate: String,
    },
    ScalarPair {
        function: String,
        parameter: String,
        element: String,
        element_const: bool,
    },
    Callback(String),
    External(String),
    CEnum {
        typedef_name: String,
        alias: String,
    },
}

fn parse_line(body: &str) -> Result<Parsed, String> {
    let mut cursor = Cursor::new(body);
    let kind = cursor.token()?;
    let parsed = match kind {
        "header" => Parsed::Header(cursor.string("include")?),
        "descriptor" => Parsed::Descriptor {
            function: cursor.string("function")?,
            parameter: cursor.string("parameter")?,
            aggregate: cursor.string("aggregate")?,
            element: cursor.string("element")?,
            element_const: cursor.boolean("const")?,
        },
        "string-view" => Parsed::StringView {
            function: cursor.string("function")?,
            parameter: cursor.string("parameter")?,
            aggregate: cursor.string("aggregate")?,
        },
        "scalar-pair" => Parsed::ScalarPair {
            function: cursor.string("function")?,
            parameter: cursor.string("parameter")?,
            element: cursor.string("element")?,
            element_const: cursor.boolean("const")?,
        },
        "callback" => Parsed::Callback(cursor.string("typedef")?),
        "external" => Parsed::External(cursor.string("type")?),
        "cenum" => Parsed::CEnum {
            typedef_name: cursor.string("typedef")?,
            alias: cursor.string("alias")?,
        },
        other => return Err(format!("unknown record kind `{other}`")),
    };
    cursor.finish()?;
    Ok(parsed)
}

struct Cursor<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a str) -> Self {
        Cursor { input, offset: 0 }
    }

    fn token(&mut self) -> Result<&'a str, String> {
        let start = self.offset;
        while self
            .input
            .as_bytes()
            .get(self.offset)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            self.offset += 1;
        }
        if start == self.offset {
            Err("record kind is missing".to_string())
        } else {
            Ok(&self.input[start..self.offset])
        }
    }

    fn separator(&mut self) -> Result<(), String> {
        let start = self.offset;
        while self
            .input
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.offset += 1;
        }
        if start == self.offset {
            Err("fields must be separated by whitespace".to_string())
        } else {
            Ok(())
        }
    }

    fn key(&mut self, expected: &str) -> Result<(), String> {
        self.separator()?;
        let key = format!("{expected}=");
        if self.input[self.offset..].starts_with(&key) {
            self.offset += key.len();
            Ok(())
        } else {
            Err(format!("expected `{expected}=`"))
        }
    }

    fn string(&mut self, key: &str) -> Result<String, String> {
        self.key(key)?;
        if self.input.as_bytes().get(self.offset) != Some(&b'"') {
            return Err(format!("`{key}` must be a quoted string"));
        }
        self.offset += 1;
        let mut out = String::new();
        loop {
            let Some(&byte) = self.input.as_bytes().get(self.offset) else {
                return Err(format!("unterminated quoted value for `{key}`"));
            };
            match byte {
                b'"' => {
                    self.offset += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.offset += 1;
                    let Some(&escaped) = self.input.as_bytes().get(self.offset) else {
                        return Err(format!("unterminated escape in `{key}`"));
                    };
                    self.offset += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape(key)?),
                        other => {
                            return Err(format!(
                                "unsupported escape `\\{}` in `{key}`",
                                char::from(other)
                            ));
                        }
                    }
                }
                byte if byte < 0x20 => {
                    return Err(format!("unescaped control character in `{key}`"));
                }
                _ => {
                    let ch = self.input[self.offset..]
                        .chars()
                        .next()
                        .ok_or_else(|| format!("invalid UTF-8 in `{key}`"))?;
                    out.push(ch);
                    self.offset += ch.len_utf8();
                }
            }
        }
    }

    fn unicode_escape(&mut self, key: &str) -> Result<char, String> {
        let end = self.offset.saturating_add(4);
        let digits = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| format!("short Unicode escape in `{key}`"))?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("invalid Unicode escape in `{key}`"));
        }
        self.offset = end;
        let value = u32::from_str_radix(digits, 16)
            .map_err(|_| format!("invalid Unicode escape in `{key}`"))?;
        char::from_u32(value).ok_or_else(|| format!("invalid Unicode scalar in `{key}`"))
    }

    fn boolean(&mut self, key: &str) -> Result<bool, String> {
        self.key(key)?;
        let rest = &self.input[self.offset..];
        if rest.starts_with("true") {
            self.offset += 4;
            Ok(true)
        } else if rest.starts_with("false") {
            self.offset += 5;
            Ok(false)
        } else {
            Err(format!("`{key}` must be `true` or `false`"))
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        while self
            .input
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.offset += 1;
        }
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err("record has trailing data".to_string())
        }
    }
}
