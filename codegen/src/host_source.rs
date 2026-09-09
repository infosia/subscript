//! Shared lexical recognition of a C main definition.

struct Token {
    text: Vec<u8>,
    end: usize,
}

// Apply C translation phases in order and retain source offsets for insertion.
fn tokens(source: &str) -> Vec<Token> {
    let input = source.as_bytes();
    let mut translated = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let trigraph = if input[i..].starts_with(b"??") {
            input.get(i + 2).and_then(|byte| match byte {
                b'=' => Some(b'#'),
                b'/' => Some(b'\\'),
                b'\'' => Some(b'^'),
                b'(' => Some(b'['),
                b')' => Some(b']'),
                b'!' => Some(b'|'),
                b'<' => Some(b'{'),
                b'>' => Some(b'}'),
                b'-' => Some(b'~'),
                _ => None,
            })
        } else {
            None
        };
        if let Some(byte) = trigraph {
            i += 3;
            translated.push((byte, i));
        } else {
            translated.push((input[i], i + 1));
            i += 1;
        }
    }
    let mut bytes = Vec::new();
    let mut offsets = Vec::new();
    i = 0;
    while i < translated.len() {
        if translated[i].0 == b'\\' && translated.get(i + 1).is_some_and(|b| b.0 == b'\n') {
            i += 2;
        } else if translated[i].0 == b'\\'
            && translated.get(i + 1).is_some_and(|b| b.0 == b'\r')
            && translated.get(i + 2).is_some_and(|b| b.0 == b'\n')
        {
            i += 3;
        } else {
            bytes.push(translated[i].0);
            offsets.push(translated[i].1);
            i += 1;
        }
    }
    let mut result = Vec::new();
    i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if matches!(bytes[i], b'\'' | b'"') {
            let quote = bytes[i];
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            result.push(Token {
                text: bytes[start..i].to_vec(),
                end: offsets[i - 1],
            });
        } else {
            let start = i;
            let digraph = match bytes.get(i..i + 2) {
                Some(b"<%") => Some(b'{'),
                Some(b"%>") => Some(b'}'),
                Some(b"<:") => Some(b'['),
                Some(b":>") => Some(b']'),
                Some(b"%:") => Some(b'#'),
                _ => None,
            };
            if let Some(byte) = digraph {
                i += 2;
                result.push(Token {
                    text: vec![byte],
                    end: offsets[i - 1],
                });
                continue;
            }
            i += 1;
            if bytes[start].is_ascii_alphanumeric() || bytes[start] == b'_' {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
            }
            result.push(Token {
                text: bytes[start..i].to_vec(),
                end: offsets[i - 1],
            });
        }
    }
    result
}

fn text(tokens: &[Token], index: usize) -> &[u8] {
    tokens.get(index).map_or(b"", |token| token.text.as_slice())
}

// Return the first token after a balanced C parameter, attribute, or type group.
fn group_end(tokens: &[Token], start: usize) -> Option<usize> {
    let close: &[u8] = match text(tokens, start) {
        b"(" => b")",
        b"[" => b"]",
        b"{" => b"}",
        _ => return None,
    };
    let mut i = start + 1;
    while i < tokens.len() {
        match text(tokens, i) {
            token if token == close => return Some(i + 1),
            b"(" | b"[" | b"{" => i = group_end(tokens, i)?,
            b")" | b"]" | b"}" => return None,
            _ => i += 1,
        }
    }
    None
}

/// Returns the source byte after the opening brace of a C main definition.
/// Recognizes the whole direct declarator, independent of its return-type spelling.
pub(crate) fn main_body_start(source: &str) -> Option<usize> {
    let tokens = tokens(source);
    for name in 0..tokens.len() {
        if text(&tokens, name) != b"main" {
            continue;
        }
        let mut left = name;
        let mut right = name + 1;
        let mut function = false;
        loop {
            if text(&tokens, right) == b"(" {
                let Some(end) = group_end(&tokens, right) else {
                    break;
                };
                right = end;
                function = true;
            } else if left > 0 && text(&tokens, left - 1) == b"(" && text(&tokens, right) == b")" {
                left -= 1;
                right += 1;
            } else {
                break;
            }
        }
        // Control conditions and return expressions are not declarations.
        if !function
            || left.checked_sub(1).is_some_and(|before| {
                matches!(
                    text(&tokens, before),
                    b"if" | b"while" | b"switch" | b"for" | b"return" | b"sizeof" | b"_Alignof"
                )
            })
        {
            continue;
        }
        // Host compilers also accept attributes after a function declarator.
        while matches!(text(&tokens, right), b"__attribute__" | b"__declspec") {
            let Some(end) = group_end(&tokens, right + 1) else {
                break;
            };
            right = end;
        }
        if text(&tokens, right) == b"{" {
            return Some(tokens[right].end);
        }
        // C11 also permits an old-style parameter declaration list. A prototype
        // ends immediately in a semicolon; it cannot enter this list.
        while text(&tokens, right)
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        {
            let mut end = right;
            while end < tokens.len() && !matches!(text(&tokens, end), b";" | b"=" | b"}") {
                if matches!(text(&tokens, end), b"(" | b"[" | b"{") {
                    let Some(next) = group_end(&tokens, end) else {
                        break;
                    };
                    end = next;
                } else {
                    end += 1;
                }
            }
            if text(&tokens, end) != b";" {
                break;
            }
            right = end + 1;
            if text(&tokens, right) == b"{" {
                return Some(tokens[right].end);
            }
        }
    }
    None
}
