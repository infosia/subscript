//! The boundary intermediate representation ([`CField`], [`Decl`]) shared
//! by the frontend and the emitter, plus the original narrow fixture
//! parser.
//!
//! The narrow parser (`specs/blocks/compiler.md` §12.1: comments,
//! preprocessor lines, `typedef enum`/`struct`, opaque-handle typedefs,
//! function-pointer typedefs, function declarations — no unions, no
//! bitfields) was the P5 frontend. At P6.1 it is superseded by the
//! libclang frontend ([`crate::clangfe`], §13.1); it is retained
//! **test-only** — under `#[cfg(test)]` — as a documented record of the
//! fixture grammar and is exercised by its own unit tests. The
//! production path never uses it; only the [`CField`]/[`Decl`] types below
//! remain in the shipped library.

use std::fmt;

/// A parse failure with a human-readable reason. Carries no source
/// position (the fixture is tiny); the message names the offending text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bindgen: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// One field of a C struct, or one parameter of a function or function
/// pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CField {
    /// Base type spelling with `struct`/`const` stripped, e.g.
    /// `uint32_t`, `char`, `void`, `SubChainHeader`.
    pub base: String,
    /// True when the declaration had a `const` qualifier.
    pub is_const: bool,
    /// True when the declarator is a pointer (`*`).
    pub pointer: bool,
    /// Fixed C array length `[N]`, when present.
    pub array_len: Option<u32>,
    /// Declared name (empty for an anonymous return type).
    pub name: String,
}

/// One top-level declaration recognized in the header, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    /// `typedef enum Name { M = v, ... } Name;`
    Enum {
        /// Type name.
        name: String,
        /// Members with resolved constant values, in order.
        members: Vec<(String, i64)>,
    },
    /// `typedef struct Name { fields } Name;`
    Struct {
        /// Type name.
        name: String,
        /// Fields in declaration order.
        fields: Vec<CField>,
    },
    /// `typedef struct Name_T *Name;` — an opaque handle.
    Handle {
        /// Handle type name.
        name: String,
    },
    /// `typedef Ret (*Name)(params);` — a function pointer.
    FnPtr {
        /// Alias name.
        name: String,
        /// Return type.
        ret: CField,
        /// Parameters in order.
        params: Vec<CField>,
    },
    /// `Ret name(params);` — a function declaration.
    Func {
        /// Function/symbol name.
        name: String,
        /// Return type.
        ret: CField,
        /// Parameters in order.
        params: Vec<CField>,
    },
}

#[cfg(test)]
fn err<T>(msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(msg.into()))
}

/// Parses the header into ordered declarations. Retained test-only (see
/// the module docs); superseded in production by [`crate::clangfe`].
///
/// # Errors
///
/// Returns [`ParseError`] for any construct outside the constrained
/// fixture grammar.
#[cfg(test)]
pub fn parse_header(src: &str) -> Result<Vec<Decl>, ParseError> {
    let cleaned = strip_comments_and_directives(src);
    let mut decls = Vec::new();
    for stmt in split_statements(&cleaned) {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        decls.push(parse_statement(stmt)?);
    }
    Ok(decls)
}

/// Removes `/* ... */` and `// ...` comments and `#...` preprocessor
/// lines, leaving the declaration text.
#[cfg(test)]
fn strip_comments_and_directives(src: &str) -> String {
    // First strip comments.
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            out.push(' ');
        } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    // Then drop preprocessor lines.
    out.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Splits into `;`-terminated top-level statements, respecting brace
/// nesting (a struct/enum body's inner `;`/`,` do not split).
#[cfg(test)]
fn split_statements(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in src.chars() {
        match ch {
            '{' => {
                depth += 1;
                cur.push(ch);
            }
            '}' => {
                depth -= 1;
                cur.push(ch);
            }
            ';' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    out
}

#[cfg(test)]
fn parse_statement(stmt: &str) -> Result<Decl, ParseError> {
    if let Some(rest) = stmt.strip_prefix("typedef") {
        let rest = rest.trim_start();
        if rest.starts_with("enum") {
            parse_enum(rest)
        } else if rest.contains("(*") {
            parse_fn_ptr(rest)
        } else if rest.starts_with("struct") && rest.contains('{') {
            parse_struct(rest)
        } else if rest.starts_with("struct") {
            parse_handle(rest)
        } else {
            err(format!("unsupported typedef form: `{stmt}`"))
        }
    } else {
        parse_func(stmt)
    }
}

/// `enum Name { members } Name`
#[cfg(test)]
fn parse_enum(rest: &str) -> Result<Decl, ParseError> {
    let open = rest
        .find('{')
        .ok_or_else(|| ParseError("enum without body".into()))?;
    let close = rest
        .rfind('}')
        .ok_or_else(|| ParseError("enum without closing brace".into()))?;
    let name = rest[close + 1..].trim().to_string();
    if name.is_empty() {
        return err("enum without a typedef name");
    }
    let body = &rest[open + 1..close];
    let mut members = Vec::new();
    let mut next: i64 = 0;
    for raw in body.split(',') {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        let (member, value) = if let Some((n, v)) = item.split_once('=') {
            let value: i64 = v
                .trim()
                .parse()
                .map_err(|_| ParseError(format!("non-integer enum value in `{item}`")))?;
            (n.trim().to_string(), value)
        } else {
            (item.to_string(), next)
        };
        next = value
            .checked_add(1)
            .ok_or_else(|| ParseError("enum value overflow".into()))?;
        members.push((member, value));
    }
    Ok(Decl::Enum { name, members })
}

/// `struct Name { fields } Name`
#[cfg(test)]
fn parse_struct(rest: &str) -> Result<Decl, ParseError> {
    let open = rest
        .find('{')
        .ok_or_else(|| ParseError("struct without body".into()))?;
    let close = rest
        .rfind('}')
        .ok_or_else(|| ParseError("struct without closing brace".into()))?;
    let name = rest[close + 1..].trim().to_string();
    if name.is_empty() {
        return err("struct without a typedef name");
    }
    let body = &rest[open + 1..close];
    let mut fields = Vec::new();
    for raw in body.split(';') {
        let decl = raw.trim();
        if decl.is_empty() {
            continue;
        }
        fields.push(parse_field(decl)?);
    }
    Ok(Decl::Struct { name, fields })
}

/// `struct Name_T *Name`
#[cfg(test)]
fn parse_handle(rest: &str) -> Result<Decl, ParseError> {
    // rest looks like `struct SubDevice_T *SubDevice`.
    let star = rest
        .find('*')
        .ok_or_else(|| ParseError("opaque handle without a pointer declarator".into()))?;
    let name = rest[star + 1..].trim().to_string();
    if name.is_empty() {
        return err("opaque handle without a typedef name");
    }
    Ok(Decl::Handle { name })
}

/// `Ret (*Name)(params)`
#[cfg(test)]
fn parse_fn_ptr(rest: &str) -> Result<Decl, ParseError> {
    let star_paren = rest
        .find("(*")
        .ok_or_else(|| ParseError("function pointer without `(*`".into()))?;
    let ret_text = rest[..star_paren].trim();
    let ret = parse_field(&format!("{ret_text} __ret"))?;
    let after = &rest[star_paren + 2..];
    let name_end = after
        .find(')')
        .ok_or_else(|| ParseError("function pointer name without `)`".into()))?;
    let name = after[..name_end].trim().to_string();
    let params_text = param_list(&after[name_end + 1..])?;
    let params = parse_params(params_text)?;
    Ok(Decl::FnPtr {
        name,
        ret: strip_name(ret),
        params,
    })
}

/// `Ret name(params)`
#[cfg(test)]
fn parse_func(stmt: &str) -> Result<Decl, ParseError> {
    let open = stmt
        .find('(')
        .ok_or_else(|| ParseError(format!("not a function declaration: `{stmt}`")))?;
    let head = stmt[..open].trim();
    // The declared name is the last identifier in the head; the rest is
    // the return type.
    let name_start = head
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let name = head[name_start..].to_string();
    if name.is_empty() {
        return err(format!("function without a name: `{stmt}`"));
    }
    let ret_text = head[..name_start].trim();
    let ret = strip_name(parse_field(&format!("{ret_text} __ret"))?);
    let params_text = param_list(&stmt[open..])?;
    let params = parse_params(params_text)?;
    Ok(Decl::Func { name, ret, params })
}

/// Returns the text inside the first balanced `(...)`.
#[cfg(test)]
fn param_list(s: &str) -> Result<&str, ParseError> {
    let open = s
        .find('(')
        .ok_or_else(|| ParseError("expected `(` for a parameter list".into()))?;
    let close = s
        .rfind(')')
        .ok_or_else(|| ParseError("expected `)` closing a parameter list".into()))?;
    if close < open {
        return err("malformed parameter list");
    }
    Ok(&s[open + 1..close])
}

#[cfg(test)]
fn parse_params(text: &str) -> Result<Vec<CField>, ParseError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "void" {
        return Ok(Vec::new());
    }
    let mut params = Vec::new();
    for raw in trimmed.split(',') {
        let decl = raw.trim();
        if decl.is_empty() {
            continue;
        }
        params.push(parse_field(decl)?);
    }
    Ok(params)
}

#[cfg(test)]
fn strip_name(mut f: CField) -> CField {
    f.name.clear();
    f
}

/// Parses one declaration `TYPE name`, e.g. `const uint32_t *items`,
/// `struct SubChainHeader *next`, `float basis[16]`, `SubDevice device`.
#[cfg(test)]
fn parse_field(decl: &str) -> Result<CField, ParseError> {
    let spaced = decl
        .replace('*', " * ")
        .replace('[', " [ ")
        .replace(']', " ] ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    if tokens.is_empty() {
        return err("empty declaration");
    }
    let mut idx = 0;
    let mut is_const = false;
    if tokens[idx] == "const" {
        is_const = true;
        idx += 1;
    }
    if tokens.get(idx) == Some(&"struct") {
        idx += 1;
    }
    let base = tokens
        .get(idx)
        .ok_or_else(|| ParseError(format!("declaration without a base type: `{decl}`")))?
        .to_string();
    idx += 1;
    let mut pointer = false;
    while tokens.get(idx) == Some(&"*") {
        pointer = true;
        idx += 1;
    }
    let name = tokens.get(idx).map(|s| s.to_string()).unwrap_or_default();
    if !name.is_empty() {
        idx += 1;
    }
    let mut array_len = None;
    if tokens.get(idx) == Some(&"[") {
        let n = tokens
            .get(idx + 1)
            .ok_or_else(|| ParseError(format!("array without a length: `{decl}`")))?;
        let n: u32 = n
            .parse()
            .map_err(|_| ParseError(format!("non-integer array length in `{decl}`")))?;
        array_len = Some(n);
    }
    Ok(CField {
        base,
        is_const,
        pointer,
        array_len,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_forms_parse() {
        assert_eq!(
            parse_field("const uint32_t *items").unwrap(),
            CField {
                base: "uint32_t".into(),
                is_const: true,
                pointer: true,
                array_len: None,
                name: "items".into()
            }
        );
        assert_eq!(
            parse_field("struct SubChainHeader *next").unwrap(),
            CField {
                base: "SubChainHeader".into(),
                is_const: false,
                pointer: true,
                array_len: None,
                name: "next".into()
            }
        );
        assert_eq!(
            parse_field("float basis[16]").unwrap(),
            CField {
                base: "float".into(),
                is_const: false,
                pointer: false,
                array_len: Some(16),
                name: "basis".into()
            }
        );
    }

    #[test]
    fn enum_values_are_running() {
        let d = parse_statement("typedef enum E { A = 5, B, C = 9 } E").unwrap();
        let Decl::Enum { members, .. } = d else {
            panic!("expected enum");
        };
        assert_eq!(
            members,
            vec![("A".into(), 5), ("B".into(), 6), ("C".into(), 9)]
        );
    }

    #[test]
    fn handle_and_fnptr_and_func() {
        assert_eq!(
            parse_statement("typedef struct SubDevice_T *SubDevice").unwrap(),
            Decl::Handle {
                name: "SubDevice".into()
            }
        );
        let Decl::FnPtr { name, params, ret } =
            parse_statement("typedef void (*Cb)(SubStringView m, void *ud)").unwrap()
        else {
            panic!("expected fnptr");
        };
        assert_eq!(name, "Cb");
        assert_eq!(ret.base, "void");
        assert_eq!(params.len(), 2);
        assert!(params[1].pointer && params[1].base == "void");

        let Decl::Func { name, ret, params } =
            parse_statement("SubDevice subDeviceCreate(SubChainHeader *chain)").unwrap()
        else {
            panic!("expected func");
        };
        assert_eq!(name, "subDeviceCreate");
        assert_eq!(ret.base, "SubDevice");
        assert_eq!(params.len(), 1);
        assert!(params[0].pointer);
    }

    #[test]
    fn unions_and_bitfields_are_out_of_grammar() {
        // A union typedef is not one of the recognized forms.
        assert!(parse_statement("typedef union U { int a; float b; } U").is_err());
    }

    #[test]
    fn parse_header_strips_comments_and_directives() {
        // Exercises the full narrow-parser entry point: block/line
        // comments and `#` preprocessor lines are removed, and the
        // remaining `;`-terminated statements parse in order.
        let src = "\
/* leading block comment */
#include <stdint.h>
typedef enum E { A = 0, B = 1 } E; // trailing line comment
typedef struct S { uint32_t x; } S;
";
        let decls = parse_header(src).unwrap();
        assert_eq!(decls.len(), 2);
        assert!(matches!(&decls[0], Decl::Enum { name, .. } if name == "E"));
        assert!(matches!(&decls[1], Decl::Struct { name, .. } if name == "S"));
    }
}
