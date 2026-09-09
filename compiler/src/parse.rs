//! SWC front end: parses TypeScript sources (TC39 standard decorators
//! enabled) and maps byte positions back to file/line/column.

use swc_common::{BytePos, FileName, SourceMap, Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::divergence::Divergence;
use crate::provenance;
use crate::SourceFile;

/// One parsed source file.
pub(crate) struct ParsedFile {
    /// File name as supplied by the caller.
    pub name: String,
    /// Module stem used for import resolution (`math` for `math.ts`).
    pub stem: String,
    /// The SWC module AST.
    pub module: ast::Module,
    /// True for an ambient declaration source (`.d.ts`): the checker
    /// ingests its declarations into the global ambient surface (the
    /// generated mirror, compiler.md §12.2) and does not check it as a
    /// program module.
    pub dts: bool,
    /// Fixed-shape C provenance parsed from a generated ambient mirror.
    pub provenance: provenance::Mirror,
}

/// A parsed program: all files plus the shared source map.
pub(crate) struct ParsedProgram {
    pub files: Vec<ParsedFile>,
    source_map: SourceMap,
}

impl ParsedProgram {
    /// Converts an SWC span start to a TS position (1-based line/col).
    pub fn pos(&self, span: Span) -> Pos {
        self.pos_at(span.lo)
    }

    /// Converts a byte position to a TS position (1-based line/col).
    pub fn pos_at(&self, at: BytePos) -> Pos {
        // BytePos(0) is SWC's dummy position; map it to the first file.
        if at == BytePos(0) {
            let file = self
                .files
                .first()
                .map(|f| f.name.clone())
                .unwrap_or_default();
            return Pos::new(file, 1, 1);
        }
        let loc = self.source_map.lookup_char_pos(at);
        let file = match &*loc.file.name {
            FileName::Custom(name) => name.clone(),
            other => other.to_string(),
        };
        Pos::new(file, loc.line as u32, loc.col.0 as u32 + 1)
    }
}

/// Derives the import stem from a file name: base name without a
/// trailing `.ts`.
fn stem_of(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.strip_suffix(".ts").unwrap_or(base).to_string()
}

/// Parses one source and returns its static import module specifiers in
/// source order.
///
/// Only TypeScript `import` declarations in the parsed module are
/// returned. Import-like text in comments and string literals is not an
/// import.
///
/// # Errors
///
/// Returns parser or ambient-provenance diagnostics for an invalid source.
pub fn parse_import_specifiers(source: &SourceFile) -> Result<Vec<String>, Vec<Diagnostic>> {
    swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
        let program = parse_program(std::slice::from_ref(source))?;
        let mut specifiers = Vec::new();
        for file in program.files {
            for item in file.module.body {
                if let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) = item {
                    specifiers.push(import.src.value.to_string());
                }
            }
        }
        Ok(specifiers)
    })
}

/// Parses every source file. Parse failures become `S100` diagnostics;
/// the parser never panics on malformed input.
pub(crate) fn parse_program(sources: &[SourceFile]) -> Result<ParsedProgram, Vec<Diagnostic>> {
    let source_map = SourceMap::default();
    let mut files = Vec::new();
    let mut diags = Vec::new();

    for source in sources {
        let provenance = if source.dts {
            match provenance::parse(&source.name, &source.source) {
                Ok(provenance) => provenance,
                Err(diag) => {
                    diags.push(diag);
                    continue;
                }
            }
        } else {
            provenance::Mirror::default()
        };
        let fm = source_map.new_source_file(
            FileName::Custom(source.name.clone()).into(),
            source.source.clone(),
        );
        let syntax = Syntax::Typescript(TsSyntax {
            tsx: false,
            decorators: true,
            dts: source.dts,
            no_early_errors: false,
            disallow_ambiguous_jsx_like: false,
        });
        let lexer = Lexer::new(
            syntax,
            ast::EsVersion::Es2022,
            StringInput::from(&*fm),
            None,
        );
        let mut parser = Parser::new_from(lexer);
        let parsed = parser.parse_module();
        let mut errors = parser.take_errors();
        match parsed {
            Ok(module) => {
                if let Some(err) = errors.drain(..).next() {
                    let pos = lookup(&source_map, &source.name, err.span());
                    diags.push(parser_diagnostic(&err, pos));
                } else {
                    files.push(ParsedFile {
                        name: source.name.clone(),
                        stem: stem_of(&source.name),
                        module,
                        dts: source.dts,
                        provenance,
                    });
                }
            }
            Err(err) => {
                let pos = lookup(&source_map, &source.name, err.span());
                diags.push(parser_diagnostic(&err, pos));
            }
        }
    }

    if diags.is_empty() {
        Ok(ParsedProgram { files, source_map })
    } else {
        Err(diags)
    }
}

fn parser_diagnostic(err: &swc_ecma_parser::error::Error, pos: Pos) -> Diagnostic {
    if matches!(
        err.kind(),
        swc_ecma_parser::error::SyntaxError::LoneSurrogateEscape
    ) {
        let mut diagnostic = Diagnostic::new(
            RuleCode::S100,
            "a lone surrogate escape has no UTF-8 encoding; write the paired escape or the character",
            pos,
        );
        diagnostic.divergence = Some(Divergence::LoneSurrogateEscape);
        diagnostic
    } else {
        Diagnostic::new(
            RuleCode::S100,
            format!("parse error: {}", err.kind().msg()),
            pos,
        )
    }
}

fn lookup(source_map: &SourceMap, fallback_file: &str, span: Span) -> Pos {
    if span.lo == BytePos(0) {
        return Pos::new(fallback_file, 1, 1);
    }
    let loc = source_map.lookup_char_pos(span.lo);
    let file = match &*loc.file.name {
        FileName::Custom(name) => name.clone(),
        other => other.to_string(),
    };
    Pos::new(file, loc.line as u32, loc.col.0 as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(name: &str, text: &str) -> SourceFile {
        SourceFile {
            name: name.to_string(),
            source: text.to_string(),
            dts: false,
        }
    }

    #[test]
    fn parses_a_decorated_class() {
        let program = parse_program(&[src(
            "t.ts",
            "@CStruct\nclass V { x: f32;\n constructor(x: f32) { this.x = x; } }\n",
        )])
        .expect("parse");
        assert_eq!(program.files.len(), 1);
    }

    #[test]
    fn reports_parse_errors_as_s100() {
        let Err(err) = parse_program(&[src("bad.ts", "function ( {")]) else {
            panic!("expected a parse error");
        };
        assert_eq!(err[0].code, RuleCode::S100);
        assert_eq!(err[0].pos.file, "bad.ts");
    }

    #[test]
    fn positions_are_one_based() {
        let program = parse_program(&[src("p.ts", "const x: i32 = 1;\n")]).expect("parse");
        let item = &program.files[0].module.body[0];
        use swc_common::Spanned;
        let pos = program.pos(item.span());
        assert_eq!((pos.line, pos.col), (1, 1));
    }

    #[test]
    fn stems_strip_directories_and_extension() {
        assert_eq!(stem_of("corpus/accept/a19-modules/math.ts"), "math");
        assert_eq!(stem_of("math.ts"), "math");
    }

    #[test]
    fn public_import_parser_returns_only_ast_import_declarations() {
        let imports = parse_import_specifiers(&src(
            "main.ts",
            concat!(
                "// import { fake } from \"./comment\";\n",
                "const text: string = 'import from \"./string\"';\n",
                "import { first } from \"./first\";\n",
                "import \"./side-effect\";\n",
                "export function main(): void { print(text); }\n",
            ),
        ))
        .expect("valid source parses");
        assert_eq!(imports, ["./first", "./side-effect"]);
    }

    #[test]
    fn public_import_parser_reports_parse_diagnostics() {
        let diagnostics = parse_import_specifiers(&src("bad.ts", "import {"))
            .expect_err("invalid source must be rejected");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(diagnostics[0].pos.file, "bad.ts");
    }

    fn string_parts(source: &str) -> Vec<String> {
        let program = parse_program(&[src("value.ts", source)]).expect("valid string expression");
        let ast::ModuleItem::Stmt(ast::Stmt::Expr(statement)) = &program.files[0].module.body[0]
        else {
            panic!("expected an expression");
        };
        match &*statement.expr {
            ast::Expr::Lit(ast::Lit::Str(value)) => vec![value.value.to_string()],
            ast::Expr::Tpl(template) => template
                .quasis
                .iter()
                .map(|part| {
                    part.cooked
                        .as_ref()
                        .expect("cooked template part")
                        .to_string()
                })
                .collect(),
            _ => panic!("expected a string or template"),
        }
    }

    #[test]
    fn surrogate_pairs_equal_literal_bytes() {
        for source in [r#""\ud83d\udc4dZ""#, r#"'\uD83D\uDC4DZ'"#] {
            assert_eq!(
                string_parts(source)[0].as_bytes(),
                string_parts("\"👍Z\"")[0].as_bytes()
            );
        }
        assert_eq!(string_parts(r#""\u00e9\ud83d\udc4d\u{1F600}""#), ["é👍😀"]);
        assert_eq!(string_parts(r#"`t\ud83d\udc4du`"#), ["t👍u"]);
        assert_eq!(
            string_parts(r#"`\ud83d\udc4d${"x"}\ud83d\udc4d${"y"}\ud83d\udc4d`"#),
            ["👍", "👍", "👍"]
        );
    }

    #[test]
    fn surrogate_pair_spellings_and_continuations_decode_by_value() {
        for high in [r"\ud83d", r"\u{d83d}"] {
            for low in [r"\udc4d", r"\u{dc4d}"] {
                for separator in ["", "\\\n", "\\\r\n", "\\\n\\\r\n"] {
                    let pair = format!("{high}{separator}{low}");
                    for quote in ['"', '\''] {
                        let source = format!("{quote}a{pair}b{quote}");
                        assert_eq!(string_parts(&source), ["a👍b"], "{source:?}");
                    }
                    let source = format!("`{pair}${{1}}{pair}${{2}}{pair}`");
                    assert_eq!(string_parts(&source), ["👍", "👍", "👍"], "{source:?}");
                }
            }
        }
    }

    #[test]
    fn lone_surrogate_positions_have_positive_controls() {
        for (bad, good, parts, column) in [
            (r#""\ud83d\u{41}""#, r#""\ud83d\u{dc4d}""#, vec!["👍"], 2),
            (r#"`\ud83d\u{41}`"#, r#"`\ud83d\u{dc4d}`"#, vec!["👍"], 2),
            (r#""\ud83d\ud83d""#, r#""\u{d83d}\udc4d""#, vec!["👍"], 2),
            (r#"`\ud83d\ud83d`"#, r#"`\u{d83d}\udc4d`"#, vec!["👍"], 2),
            (r#""\u{dc4d}""#, r#""\u{d83d}\u{dc4d}""#, vec!["👍"], 2),
            (r#"`\u{dc4d}`"#, r#"`\u{d83d}\u{dc4d}`"#, vec!["👍"], 2),
            (
                "\"\\ud83d\\\n\\u{41}\"",
                "\"\\ud83d\\\n\\udc4d\"",
                vec!["👍"],
                2,
            ),
            (
                "\"\\ud83d\\\r\n\\u{41}\"",
                "\"\\ud83d\\\r\n\\udc4d\"",
                vec!["👍"],
                2,
            ),
            (r#""\ud83dab""#, r#""\ud83d\udc4dab""#, vec!["👍ab"], 2),
            (r#""a\ud83db""#, r#""a\ud83d\udc4db""#, vec!["a👍b"], 3),
            (r#""ab\ud83d""#, r#""ab\ud83d\udc4d""#, vec!["ab👍"], 4),
            (r#"'a\ud83db'"#, r#"'a\ud83d\udc4db'"#, vec!["a👍b"], 3),
            (r#""\udc4d""#, r#""\ud83d\udc4d""#, vec!["👍"], 2),
            (r#""\u{D83D}""#, r#""\u{1F600}""#, vec!["😀"], 2),
            (r#""é👍\ud83d""#, r#""é👍\ud83d\udc4d""#, vec!["é👍👍"], 5),
            (r#"`a\ud83db`"#, r#"`a\ud83d\udc4db`"#, vec!["a👍b"], 3),
            (
                r#"`\ud83d${"x"}ok`"#,
                r#"`\ud83d\udc4d${"x"}ok`"#,
                vec!["👍", "ok"],
                2,
            ),
            (
                r#"`ok${"x"}\ud83d${"y"}ok`"#,
                r#"`ok${"x"}\ud83d\udc4d${"y"}ok`"#,
                vec!["ok", "👍", "ok"],
                10,
            ),
            (
                r#"`ok${"x"}\ud83d`"#,
                r#"`ok${"x"}\ud83d\udc4d`"#,
                vec!["ok", "👍"],
                10,
            ),
        ] {
            let Err(diagnostics) = parse_program(&[src("lone.ts", &format!("\n{bad}"))]) else {
                panic!("accepted {bad}");
            };
            assert_eq!(diagnostics.len(), 1, "{bad}");
            let diagnostic = &diagnostics[0];
            assert_eq!(diagnostic.code, RuleCode::S100, "{bad}");
            assert_eq!(diagnostic.pos, Pos::new("lone.ts", 2, column), "{bad}");
            assert_eq!(diagnostic.message, "a lone surrogate escape has no UTF-8 encoding; write the paired escape or the character", "{bad}");
            assert_eq!(
                diagnostic.divergence,
                Some(Divergence::LoneSurrogateEscape),
                "{bad}"
            );
            assert_eq!(string_parts(good), parts, "{good}");
        }
    }

    #[test]
    fn escaped_backslash_keeps_six_bytes() {
        for source in [r#""\\ud83d""#, r#"`\\ud83d`"#] {
            assert_eq!(string_parts(source)[0].as_bytes(), b"\\ud83d");
            assert_eq!(string_parts(source)[0].len(), 6);
        }
    }

    #[test]
    fn identifier_surrogate_pair_stays_rejected() {
        let Err(diagnostics) = parse_program(&[src("identifier.ts", r"const \ud801\udc00 = 1;")])
        else {
            panic!("accepted identifier surrogate escapes");
        };
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(diagnostics[0].pos, Pos::new("identifier.ts", 1, 7));
        assert_eq!(
            diagnostics[0].message,
            "parse error: Invalid character in identifier"
        );
        assert_eq!(diagnostics[0].divergence, None);
        for good in [r"const \u{10400} = 1;", "const 𐐀 = 1;"] {
            assert!(
                parse_program(&[src("identifier.ts", good)]).is_ok(),
                "{good}"
            );
        }
    }
}
