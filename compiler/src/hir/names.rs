//! Source names and module labels for user-facing text (compiler.md §125).

use crate::Pos;

/// Source spelling retained beside a checker-assigned symbol.
pub fn source_name(symbol: &str) -> String {
    let mut result = String::new();
    let mut rest = symbol;
    while let Some(start) = rest.find("[[identity:") {
        result.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("]]") else {
            result.push_str(&rest[start..]);
            return result;
        };
        rest = &rest[start + end + 2..];
    }
    result.push_str(rest);
    result
}

/// Adds the module when declarations share a source name. An empty origin denotes the prelude.
pub fn declaration_label(name: &str, pos: &Pos, shared: bool) -> String {
    if shared {
        let file = if pos.file.is_empty() {
            "prelude/lang.d.ts"
        } else {
            &pos.file
        };
        format!("{name} ({file})")
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_qualify_shared_names_and_preserve_source_spelling() {
        assert_eq!(
            declaration_label("C", &Pos::new("lib.ts", 1, 1), false),
            "C"
        );
        assert_eq!(
            declaration_label("C", &Pos::new("lib.ts", 1, 1), true),
            "C (lib.ts)"
        );
        assert_eq!(
            declaration_label("Error", &Pos::new("", 0, 0), true),
            "Error (prelude/lang.d.ts)"
        );
        assert_eq!(
            source_name("[[identity:module:00]]pick<[[identity:module:01]]E>"),
            "pick<E>"
        );
        assert_eq!(source_name("plain"), "plain");
    }
}
