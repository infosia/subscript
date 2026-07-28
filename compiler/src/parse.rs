//! SWC front end: parses TypeScript sources (TC39 standard decorators
//! enabled) and maps byte positions back to file/line/column.

use swc_common::{BytePos, FileName, SourceMap, Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

use crate::diag::{Diagnostic, Pos, RuleCode};
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
    /// True for an ambient declaration source (`.d.ts`): its
    /// declarations are ingested into the global ambient surface (the
    /// P5.2 mirror), not checked as a program module.
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
                    diags.push(Diagnostic::new(
                        RuleCode::S100,
                        format!("parse error: {}", err.kind().msg()),
                        pos,
                    ));
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
                diags.push(Diagnostic::new(
                    RuleCode::S100,
                    format!("parse error: {}", err.kind().msg()),
                    pos,
                ));
            }
        }
    }

    if diags.is_empty() {
        Ok(ParsedProgram { files, source_map })
    } else {
        Err(diags)
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
}
