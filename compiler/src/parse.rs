//! The oxc front end parses TypeScript modules and assigns program-wide positions.

use crate::check::rejection::{diagnostic, RejectionSite};
use crate::diag::{Diagnostic, Pos};
use crate::{provenance, SourceFile};
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast;
use oxc_ast_visit::{walk_mut, VisitMut};
use oxc_span::Span;

mod ambient;
pub(crate) mod local_function;
pub(crate) mod parameter_property;
pub(crate) mod private_name;
mod syntax;
mod value {
    //! Decode the oxc value encoding without source scanning (compiler.md §341 rules 4a and 4b).
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{StringLiteral, TemplateElement};
    use oxc_ast_visit::VisitMut;
    use oxc_span::Span;

    pub(super) struct Values<'a> {
        allocator: &'a Allocator,
        pub error: Option<Span>,
    }
    impl<'a> Values<'a> {
        pub fn new(allocator: &'a Allocator) -> Self {
            Self {
                allocator,
                error: None,
            }
        }
        fn decode(&mut self, value: &str, span: Span) -> Option<&'a str> {
            let mut units = Vec::new();
            let mut chars = value.chars();
            while let Some(ch) = chars.next() {
                if ch == '\u{fffd}' {
                    let mut unit = 0u32;
                    for _ in 0..4 {
                        let Some(digit) = chars.next().and_then(|ch| ch.to_digit(16)) else {
                            self.error.get_or_insert(span);
                            return None;
                        };
                        unit = (unit << 4) | digit;
                    }
                    units.push(unit);
                } else {
                    units.push(ch as u32);
                }
            }
            let mut output = String::new();
            let mut units = units.into_iter();
            while let Some(unit) = units.next() {
                let scalar = if (0xd800..=0xdbff).contains(&unit) {
                    match units.next() {
                        Some(low) if (0xdc00..=0xdfff).contains(&low) => {
                            0x10000 + ((unit - 0xd800) << 10) + low - 0xdc00
                        }
                        _ => {
                            self.error.get_or_insert(span);
                            return None;
                        }
                    }
                } else {
                    unit
                };
                if let Some(ch) = char::from_u32(scalar) {
                    output.push(ch);
                } else {
                    self.error.get_or_insert(span);
                    return None;
                }
            }
            Some(self.allocator.alloc_str(&output))
        }
    }
    impl<'a> VisitMut<'a> for Values<'a> {
        fn visit_string_literal(&mut self, value: &mut StringLiteral<'a>) {
            if value.lone_surrogates {
                if let Some(decoded) = self.decode(value.value.as_str(), value.span) {
                    value.value = decoded.into();
                    value.lone_surrogates = false;
                }
            }
        }
        fn visit_template_element(&mut self, value: &mut TemplateElement<'a>) {
            if value.lone_surrogates {
                if let Some(cooked) = value.value.cooked {
                    if let Some(decoded) = self.decode(cooked.as_str(), value.span) {
                        value.value.cooked = Some(decoded.into());
                        value.lone_surrogates = false;
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // Cost (debug): 0.01 s; seven decoder inputs, without subprocesses.
        #[test]
        fn malformed_code_unit_markers_set_the_literal_error() {
            let allocator = Allocator::default();
            let span = Span::new(7, 18);
            for suffix in ["", "f", "ff", "fff", "fffg", "xyz0"] {
                let mut values = Values::new(&allocator);
                assert!(values.decode(&format!("\u{fffd}{suffix}"), span).is_none());
                assert_eq!(values.error, Some(span));
            }
            let mut values = Values::new(&allocator);
            assert_eq!(values.decode("\u{fffd}fffd", span), Some("\u{fffd}"));
            assert_eq!(values.error, None);
        }
    }
}

/// One parsed source file (compiler.md §341 rules 2 and 3).
pub(crate) struct ParsedFile<'a> {
    pub name: String,
    /// The module key of `name` (compiler.md §354 rule 1).
    pub key: String,
    pub module: ast::Program<'a>,
    pub dts: bool,
    pub entry: bool,
    pub provenance: provenance::Mirror,
}

/// All syntax lives in the allocator of the parse caller (compiler.md §341 rule 5).
pub(crate) struct ParsedProgram<'a> {
    pub allocator: &'a Allocator,
    pub files: Vec<ParsedFile<'a>>,
    pub source_bytes: usize,
    pub local_functions: local_function::LocalFunctions,
    source_map: Vec<SourcePosition>,
    syntax_errors: Vec<(String, Pos)>,
}

#[derive(Clone)]
struct SourcePosition {
    name: String,
    text: String,
    base: u32,
    line_starts: Vec<usize>,
    non_ascii: Vec<NonAsciiPosition>,
    extra_bytes: usize,
}

#[derive(Clone)]
struct NonAsciiPosition {
    offset: usize,
    extra_bytes_before: usize,
}

impl SourcePosition {
    fn new(name: String, text: String, base: u32) -> Self {
        let mut line_starts = vec![0];
        let mut non_ascii = Vec::new();
        let mut extra_bytes = 0;
        for (offset, ch) in text.char_indices() {
            if !ch.is_ascii() {
                non_ascii.push(NonAsciiPosition {
                    offset,
                    extra_bytes_before: extra_bytes,
                });
                // Columns count UTF-16 units (compiler.md §341 rule 3).
                extra_bytes += ch.len_utf8() - ch.len_utf16();
            }
            match ch {
                '\r' => line_starts.push(
                    offset + 1 + usize::from(text.as_bytes().get(offset + 1) == Some(&b'\n')),
                ),
                '\n' if offset == 0 || text.as_bytes()[offset - 1] != b'\r' => {
                    line_starts.push(offset + 1);
                }
                '\u{2028}' | '\u{2029}' => line_starts.push(offset + ch.len_utf8()),
                _ => {}
            }
        }
        Self {
            name,
            text,
            base,
            line_starts,
            non_ascii,
            extra_bytes,
        }
    }

    fn extra_bytes_before(&self, offset: usize) -> usize {
        let index = self.non_ascii.partition_point(|ch| ch.offset < offset);
        self.non_ascii
            .get(index)
            .map_or(self.extra_bytes, |ch| ch.extra_bytes_before)
    }
}

// Each copied syntax node uses the same caller-owned arena (compiler.md §341 rule 5).
impl Clone for ParsedProgram<'_> {
    fn clone(&self) -> Self {
        Self {
            allocator: self.allocator,
            files: self
                .files
                .iter()
                .map(|file| ParsedFile {
                    name: file.name.clone(),
                    key: file.key.clone(),
                    module: file.module.clone_in(self.allocator),
                    dts: file.dts,
                    entry: file.entry,
                    provenance: file.provenance.clone(),
                })
                .collect(),
            source_bytes: self.source_bytes,
            local_functions: self.local_functions.clone(),
            source_map: self.source_map.clone(),
            syntax_errors: self.syntax_errors.clone(),
        }
    }
}

impl ParsedProgram<'_> {
    /// The program module that `specifier` in file `importer` names
    /// (compiler.md §354 rules 2 and 3). An ambient file has no key.
    pub fn import_target(&self, importer: usize, specifier: &str) -> Option<usize> {
        let key = crate::resolve_module_specifier(&self.files[importer].name, specifier)?;
        self.files
            .iter()
            .position(|file| !file.dts && file.key == key)
    }

    // Only a successful checker result exposes these errors (compiler.md §341 rule 4c).
    pub fn syntax_diagnostics(&self) -> Vec<Diagnostic> {
        self.syntax_errors
            .iter()
            .map(|(message, pos)| {
                diagnostic(
                    RejectionSite::ParserSyntaxError,
                    format!("parse error: {message}"),
                    pos.clone(),
                )
            })
            .collect()
    }

    pub fn pos(&self, span: Span) -> Pos {
        self.pos_at(span.start)
    }

    pub fn pos_at(&self, at: u32) -> Pos {
        let index = self.source_map.partition_point(|file| file.base <= at);
        if let Some(file) = index
            .checked_sub(1)
            .and_then(|index| self.source_map.get(index))
            .filter(|file| at - file.base <= file.text.len() as u32)
        {
            position(file, at)
        } else {
            Pos::new(
                self.files.first().map_or("", |file| file.name.as_str()),
                1,
                1,
            )
        }
    }

    pub fn snippet(&self, span: Span) -> Option<String> {
        let file = self.source_map.iter().find(|file| {
            span.start >= file.base
                && span.end >= span.start
                && span.end - file.base <= file.text.len() as u32
        })?;
        file.text
            .get((span.start - file.base) as usize..(span.end - file.base) as usize)
            .map(str::to_owned)
    }
}

fn position(file: &SourcePosition, at: u32) -> Pos {
    let mut offset = at.saturating_sub(file.base) as usize;
    if !file.text.is_char_boundary(offset) {
        return Pos::new(&file.name, 1, 1);
    }
    // A CR already ends its line before the LF of a CRLF pair.
    if offset > 0
        && file.text.as_bytes().get(offset) == Some(&b'\n')
        && file.text.as_bytes()[offset - 1] == b'\r'
    {
        offset += 1;
    }
    let line = file.line_starts.partition_point(|&start| start <= offset) - 1;
    let start = file.line_starts[line];
    let extra_bytes = file.extra_bytes_before(offset) - file.extra_bytes_before(start);
    Pos::new(
        &file.name,
        line as u32 + 1,
        (offset - start - extra_bytes) as u32 + 1,
    )
}

/// Parses static module dependencies in source order.
///
/// # Errors
/// Returns syntax and ambient declaration diagnostics.
pub fn parse_import_specifiers(source: &SourceFile) -> Result<Vec<String>, Vec<Diagnostic>> {
    let allocator = Allocator::default();
    let program = parse_program(&allocator, std::slice::from_ref(source))?;
    let mut specifiers = Vec::new();
    for file in program.files {
        for statement in file.module.body {
            let source = match statement {
                ast::Statement::ImportDeclaration(import) => Some(import.source.value),
                ast::Statement::ExportFromDeclaration(export) => Some(export.source.value),
                ast::Statement::ExportAllDeclaration(export) => Some(export.source.value),
                _ => None,
            };
            if let Some(source) = source {
                specifiers.push(source.to_string());
            }
        }
    }
    Ok(specifiers)
}

/// Discovers modules breadth-first, with dependencies in source order (§128 rule 8).
/// The loader supplies stable file identities and returns `None` for absent or unsupported modules.
///
/// # Errors
/// Returns loader errors or parser diagnostics converted by `parse_error`.
pub fn discover_module_sources<K, E>(
    entry: (K, SourceFile),
    mut load: impl FnMut(&K, &str) -> Result<Option<(K, SourceFile)>, E>,
    mut parse_error: impl FnMut(&[SourceFile], Vec<Diagnostic>) -> E,
) -> Result<Vec<SourceFile>, E>
where
    K: Clone + Eq + std::hash::Hash,
{
    let (key, mut source) = entry;
    source.entry = true;
    let mut seen = std::collections::HashSet::from([key.clone()]);
    let mut keys = vec![key];
    let mut sources = vec![source];
    let mut index = 0;
    while index < sources.len() {
        let specifiers = parse_import_specifiers(&sources[index])
            .map_err(|diagnostics| parse_error(&sources, diagnostics))?;
        for specifier in specifiers {
            if let Some((key, source)) = load(&keys[index], &specifier)? {
                if seen.insert(key.clone()) {
                    keys.push(key);
                    sources.push(source);
                }
            }
        }
        index += 1;
    }
    Ok(sources)
}

/// Parses files in one arena and gives every source span a distinct base.
pub(crate) fn parse_program<'a>(
    allocator: &'a Allocator,
    sources: &[SourceFile],
) -> Result<ParsedProgram<'a>, Vec<Diagnostic>> {
    let mut program = ParsedProgram {
        allocator,
        files: Vec::new(),
        source_bytes: 0,
        local_functions: local_function::LocalFunctions::default(),
        source_map: Vec::new(),
        syntax_errors: Vec::new(),
    };
    let mut diagnostics = Vec::new();
    let mut base = 1u32;
    for source in sources {
        let Some(next_base) = u32::try_from(source.source.len())
            .ok()
            .and_then(|length| base.checked_add(length)?.checked_add(1))
        else {
            diagnostics.push(diagnostic(
                RejectionSite::ParserSyntaxError,
                "parse error: source positions exceed the parser range",
                Pos::new(&source.name, 1, 1),
            ));
            break;
        };
        let map = SourcePosition::new(source.name.clone(), source.source.clone(), base);
        let provenance = if source.dts {
            match provenance::parse(&source.name, &source.source) {
                Ok(provenance) => provenance,
                Err(error) => {
                    diagnostics.push(error);
                    base = next_base;
                    continue;
                }
            }
        } else {
            provenance::Mirror::default()
        };
        let text = allocator.alloc_str(&source.source);
        let parsed = oxc_parser::Parser::new(
            allocator,
            text,
            oxc_span::SourceType::ts().with_module(true),
        )
        .parse();
        let mut module = parsed.program;
        let error = parsed
            .diagnostics
            .iter()
            .find(|error| !syntax::ignored(error.message.as_ref(), source.dts));
        if let Some(error) = error {
            let offset = error.labels.first().map_or(0, |label| label.offset());
            diagnostics.push(diagnostic(
                RejectionSite::ParserSyntaxError,
                format!("parse error: {}", error.message),
                position(&map, base + offset),
            ));
        } else {
            program
                .syntax_errors
                .extend(syntax::check(&module, &map, source.dts));
            let mut values = value::Values::new(allocator);
            values.visit_program(&mut module);
            Directives(allocator).visit_program(&mut module);
            if let Some(span) = values.error {
                diagnostics.push(diagnostic(RejectionSite::ParserLoneSurrogateEscape, "a lone surrogate escape has no UTF-8 encoding; write the paired escape or the character", position(&map, base + span.start)));
            } else if let Some((span, message)) =
                source.dts.then(|| ambient::first_error(&module)).flatten()
            {
                diagnostics.push(diagnostic(
                    RejectionSite::ParserSyntaxError,
                    format!("parse error: {message}"),
                    position(&map, base + span.start),
                ));
            } else {
                // Shift before rewrites: each copied source span already has its base (compiler.md §341 rule 3).
                SpanBase(base).visit_program(&mut module);
                if !source.dts {
                    parameter_property::lower_module(allocator, &mut module);
                    private_name::lower_module(allocator, &mut module);
                    let facts = local_function::lower_module(allocator, &mut module);
                    program.local_functions.outcomes.extend(facts.outcomes);
                    program.local_functions.classes.extend(facts.classes);
                }
                program.files.push(ParsedFile {
                    name: source.name.clone(),
                    key: crate::module_key(&source.name),
                    module,
                    dts: source.dts,
                    entry: source.entry,
                    provenance,
                });
            }
        }
        program.source_bytes += source.source.len();
        program.source_map.push(map);
        base = next_base;
    }
    if diagnostics.is_empty() {
        Ok(program)
    } else {
        Err(diagnostics)
    }
}

struct SpanBase(u32);
impl<'a> VisitMut<'a> for SpanBase {
    fn visit_program(&mut self, program: &mut ast::Program<'a>) {
        // Comments are outside the generated AST walk (compiler.md §341 rule 3).
        for comment in &mut program.comments {
            self.visit_span(&mut comment.span);
            if comment.attached_to != 0 {
                comment.attached_to += self.0;
            }
        }
        walk_mut::walk_program(self, program);
    }
    fn visit_span(&mut self, span: &mut Span) {
        span.start += self.0;
        span.end += self.0;
    }
}

// Literal statements carry directive values through the checker (compiler.md §341 rule 5).
struct Directives<'a>(&'a Allocator);
impl<'a> Directives<'a> {
    fn prepend(
        &self,
        directives: &mut oxc_allocator::Vec<'a, ast::Directive<'a>>,
        statements: &mut oxc_allocator::Vec<'a, ast::Statement<'a>>,
    ) {
        if directives.is_empty() {
            return;
        }
        let builder = oxc_ast::builder::AstBuilder::new(self.0);
        let mut prefix = oxc_allocator::Vec::new_in(&self.0);
        for directive in std::mem::replace(directives, oxc_allocator::Vec::new_in(&self.0)) {
            prefix.push(ast::Statement::ExpressionStatement(
                ast::ExpressionStatement::boxed(
                    directive.span,
                    ast::Expression::StringLiteral(oxc_allocator::Box::new_in(
                        directive.expression,
                        &self.0,
                    )),
                    &builder,
                ),
            ));
        }
        prefix.extend(std::mem::replace(
            statements,
            oxc_allocator::Vec::new_in(&self.0),
        ));
        *statements = prefix;
    }
}
impl<'a> VisitMut<'a> for Directives<'a> {
    fn visit_program(&mut self, program: &mut ast::Program<'a>) {
        self.prepend(&mut program.directives, &mut program.body);
        walk_mut::walk_program(self, program);
    }
    fn visit_function_body(&mut self, body: &mut ast::FunctionBody<'a>) {
        self.prepend(&mut body.directives, &mut body.statements);
        walk_mut::walk_function_body(self, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::RuleCode;
    use crate::divergence::Divergence;
    use oxc_span::GetSpan;

    fn src(name: &str, text: &str) -> SourceFile {
        SourceFile {
            name: name.to_string(),
            source: text.to_string(),
            dts: false,
            entry: false,
        }
    }

    #[test]
    fn ambient_sources_reject_implementations_and_default_arguments() {
        let allocator = Allocator::default();
        for text in [
            "function host(): void {}",
            "class Point { constructor() {} }",
            "class Point { method(): void {} }",
            "function host(value: i32 = 1): void;",
            "class Point { constructor(value: i32 = 1); }",
            "namespace Host { function run(): void {} }",
        ] {
            let errors = match parse_program(&allocator, &[SourceFile::ambient("bad.d.ts", text)]) {
                Err(errors) => errors,
                Ok(_) => panic!("ambient implementation must fail: {text}"),
            };
            assert_eq!(errors[0].code, RuleCode::S100);
            assert!(errors[0].message.contains("implementation"), "{errors:?}");
        }
        // The checker, rather than the parser, owns the mirror variable restriction.
        assert!(parse_program(
            &allocator,
            &[SourceFile::ambient("const.d.ts", "const value: i32;")]
        )
        .is_ok());
    }

    #[test]
    fn malformed_ambient_assignment_returns_a_syntax_diagnostic() {
        let allocator = Allocator::default();
        let sources = [SourceFile::ambient(
            "bad.d.ts",
            "declar= function subWireModeRecordEchoMode(value: object | null): i32;",
        )];
        let errors = match parse_program(&allocator, &sources) {
            Err(errors) => errors,
            Ok(_) => panic!("malformed declaration must fail"),
        };
        assert_eq!(errors[0].code, RuleCode::S100);
        let parsed = parse_program(&allocator, &[SourceFile::ambient("good.d.ts",
            "declare class Point { x: i32; constructor(x: i32); } declare function usePoint(p: Point | null): void;")]).unwrap_or_else(|errors| panic!("{errors:?}"));
        assert!(parsed.files[0].dts);
    }

    #[test]
    fn discovery_keeps_breadth_first_source_order_and_visits_cycles_once() {
        let files = [
            src(
                "main.ts",
                "import './b'; export { a } from './a'; import './b';",
            ),
            src("a.ts", "export const a: i32 = 1;"),
            src(
                "b.ts",
                "export { deep } from './deep'; import './main'; import './absent';",
            ),
            src("deep.ts", "export const deep: i32 = 2;"),
        ];
        let discover = |entry: SourceFile| {
            discover_module_sources(
                (entry.name.clone(), entry),
                |importer: &String, specifier| {
                    let Some(key) = crate::resolve_module_specifier(importer, specifier) else {
                        return Ok(None);
                    };
                    let name = format!("{key}.ts");
                    Ok(files
                        .iter()
                        .find(|file| file.name == name)
                        .map(|file| (name, file.clone())))
                },
                |_, diagnostics| diagnostics,
            )
        };
        let ordered = discover(files[0].clone()).expect("discovery");
        assert_eq!(
            ordered.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["main.ts", "b.ts", "a.ts", "deep.ts"]
        );
        let control = discover(src("main.ts", "import './a'; import './b';")).expect("control");
        assert_eq!(
            control.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["main.ts", "a.ts", "b.ts", "deep.ts"]
        );
        assert_eq!(
            discover(src("bad.ts", "import {")).expect_err("invalid source")[0].code,
            RuleCode::S100
        );
    }

    #[test]
    fn parses_a_decorated_class() {
        let allocator = Allocator::default();
        let program = parse_program(
            &allocator,
            &[src(
                "t.ts",
                "@ValueType\nclass V { x: f32;\n constructor(x: f32) { this.x = x; } }\n",
            )],
        )
        .expect("parse");
        assert_eq!(program.files.len(), 1);
    }

    #[test]
    fn reports_parse_errors_as_s100() {
        let allocator = Allocator::default();
        let Err(err) = parse_program(&allocator, &[src("bad.ts", "function ( {")]) else {
            panic!("expected a parse error");
        };
        assert_eq!(err[0].code, RuleCode::S100);
        assert_eq!(err[0].pos.file, "bad.ts");
    }

    #[test]
    fn positions_are_one_based() {
        let allocator = Allocator::default();
        let program =
            parse_program(&allocator, &[src("p.ts", "const x: i32 = 1;\n")]).expect("parse");
        let item = &program.files[0].module.body[0];
        use oxc_span::GetSpan;
        let pos = program.pos(item.span());
        assert_eq!((pos.line, pos.col), (1, 1));
    }

    #[test]
    fn public_import_parser_returns_only_ast_module_dependencies() {
        let imports = parse_import_specifiers(&src(
            "main.ts",
            concat!(
                "// import { fake } from \"./comment\";\n",
                "const text: string = 'import from \"./string\"';\n",
                "import { first } from \"./first\";\n",
                "import \"./side-effect\";\n",
                "export { value as alias } from \"./named\";\n",
                "export * from \"./star\";\n",
                "export * as ns from \"./namespace\";\n",
                "export { text };\n",
                "export function main(): void { print(text); }\n",
            ),
        ))
        .expect("valid source parses");
        assert_eq!(
            imports,
            [
                "./first",
                "./side-effect",
                "./named",
                "./star",
                "./namespace"
            ]
        );
    }

    #[test]
    fn public_import_parser_reports_parse_diagnostics() {
        let diagnostics = parse_import_specifiers(&src("bad.ts", "import {"))
            .expect_err("invalid source must be rejected");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(diagnostics[0].pos.file, "bad.ts");
    }

    fn string_parts(source: &str) -> Vec<String> {
        let allocator = Allocator::default();
        let program =
            parse_program(&allocator, &[src("value.ts", source)]).expect("valid string expression");
        if let Some(directive) = program.files[0].module.directives.first() {
            return vec![directive.expression.value.to_string()];
        }
        let ast::Statement::ExpressionStatement(statement) = &program.files[0].module.body[0]
        else {
            panic!("expected expression");
        };
        match &statement.expression {
            ast::Expression::StringLiteral(value) => vec![value.value.to_string()],
            ast::Expression::TemplateLiteral(template) => template
                .quasis
                .iter()
                .map(|part| part.value.cooked.as_ref().expect("cooked part").to_string())
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
        let allocator = Allocator::default();
        for (bad, good, parts, _column) in [
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
            let Err(diagnostics) =
                parse_program(&allocator, &[src("lone.ts", &format!("\n{bad}"))])
            else {
                panic!("accepted {bad}");
            };
            assert_eq!(diagnostics.len(), 1, "{bad}");
            let diagnostic = &diagnostics[0];
            assert_eq!(diagnostic.code, RuleCode::S100, "{bad}");
            assert_eq!(
                diagnostic.pos,
                Pos::new(
                    "lone.ts",
                    2,
                    if bad.starts_with('`') {
                        if bad.contains("ok${") {
                            10
                        } else {
                            2
                        }
                    } else {
                        1
                    }
                ),
                "{bad}"
            );
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
        let allocator = Allocator::default();
        let Err(diagnostics) = parse_program(
            &allocator,
            &[src("identifier.ts", r"const \ud801\udc00 = 1;")],
        ) else {
            panic!("accepted identifier surrogate escapes");
        };
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(diagnostics[0].pos, Pos::new("identifier.ts", 1, 8));
        assert!(diagnostics[0].message.starts_with("parse error:"));
        assert_eq!(diagnostics[0].divergence, None);
        for good in [r"const \u{10400} = 1;", "const 𐐀 = 1;"] {
            assert!(
                parse_program(&allocator, &[src("identifier.ts", good)]).is_ok(),
                "{good}"
            );
        }
    }
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn file_bases_keep_equal_local_offsets_distinct_in_a_span_map() {
        let allocator = Allocator::default();
        let sources = [
            src(
                "a.ts",
                "/* leading */ class C { constructor(public x: i32) {} }",
            ),
            src(
                "b.ts",
                "/* leading */ class C { constructor(public x: i32) {} }",
            ),
        ];
        let program = parse_program(&allocator, &sources).expect("parse");
        let mut spans = std::collections::HashMap::new();
        for file in &program.files {
            let ast::Statement::ClassDeclaration(class) = &file.module.body[0] else {
                panic!("class");
            };
            let ast::ClassElement::PropertyDefinition(field) = &class.body.body[0] else {
                panic!("generated field");
            };
            spans.insert(field.span, file.name.as_str());
            assert_eq!(program.pos(file.module.comments[0].span).file, file.name);
            assert_eq!(
                program.snippet(file.module.comments[0].span).as_deref(),
                Some("/* leading */")
            );
            assert_eq!(program.pos(field.span).file, file.name);
            assert_eq!(
                program.snippet(field.span).as_deref(),
                Some("public x: i32")
            );
        }
        assert_eq!(spans.len(), 2);
        assert_eq!(program.files[0].module.span.start, 1);
        assert_eq!(
            program.files[1].module.span.start,
            sources[0].source.len() as u32 + 2
        );
        assert_eq!(program.pos(Span::new(0, 0)), Pos::new("a.ts", 1, 1));
    }

    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn bare_case_using_keeps_the_ast_and_another_error_in_the_same_file() {
        let allocator = Allocator::default();
        let source = "switch (x) { case 1: using r = acquire(); break; }";
        let parsed = parse_program(&allocator, &[src("using.ts", source)])
            .expect("recoverable using diagnostic");
        let ast::Statement::SwitchStatement(switch) = &parsed.files[0].module.body[0] else {
            panic!("switch");
        };
        assert!(
            matches!(switch.cases[0].consequent[0], ast::Statement::VariableDeclaration(ref declaration) if declaration.kind == ast::VariableDeclarationKind::Using)
        );
        let errors = parse_program(
            &allocator,
            &[src("using.ts", &format!("{source} const bad = ;"))],
        )
        .err()
        .expect("control parse error");
        assert_eq!(errors[0].code, RuleCode::S100);
        assert!(errors[0].message.starts_with("parse error:"));
        assert!(!errors[0].message.contains("bare case"));
    }

    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn encoded_replacement_character_is_not_a_surrogate_marker() {
        assert_eq!(
            string_parts(r#""�abcd\u{d83d}\udc4d�fffd""#),
            ["�abcd👍�fffd"]
        );
        assert_eq!(string_parts(r#"`�\ud83d\u{dc4d}�fffd`"#), ["�👍�fffd"]);
        let allocator = Allocator::default();
        for bad in [r#""�\ud83dX""#, r#"`�\udc4d`"#] {
            let errors = parse_program(&allocator, &[src("bad.ts", bad)])
                .err()
                .expect("lone surrogate");
            assert_eq!(errors[0].divergence, Some(Divergence::LoneSurrogateEscape));
        }
    }

    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn positions_count_utf16_units_and_each_typescript_line_terminator() {
        let allocator = Allocator::default();
        for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
            let text = format!("/*é👍*/ const n = 1;{newline}const m = 2;");
            let program = parse_program(&allocator, &[src("unicode.ts", &text)]).expect("parse");
            let span = program.files[0].module.body[0].span();
            assert_eq!(program.pos(span), Pos::new("unicode.ts", 1, 9));
            assert_eq!(
                program.pos(program.files[0].module.body[1].span()),
                Pos::new("unicode.ts", 2, 1)
            );
            assert_eq!(program.snippet(span).as_deref(), Some("const n = 1;"));
        }
    }

    // Cost (debug): about 0.03 s; every byte offset, without subprocesses.
    #[test]
    fn position_tables_match_the_scan_at_every_byte_offset() {
        fn scan(file: &SourcePosition, at: u32) -> Pos {
            let prefix = file
                .text
                .get(..at.saturating_sub(file.base) as usize)
                .unwrap_or("");
            let (mut line, mut col) = (1u32, 1u32);
            let mut chars = prefix.chars().peekable();
            while let Some(ch) = chars.next() {
                match ch {
                    '\r' => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        line += 1;
                        col = 1;
                    }
                    '\n' | '\u{2028}' | '\u{2029}' => {
                        line += 1;
                        col = 1;
                    }
                    _ => col += ch.len_utf16() as u32,
                }
            }
            Pos::new(&file.name, line, col)
        }

        let started = std::time::Instant::now();
        let allocator = Allocator::default();
        let sources = [
            src("empty.ts", ""),
            src("lines.ts", "//é中😀\r\n//\r//\u{2028}//\u{2029}//é中😀\n"),
            src("long.ts", &format!("//{}é中😀\r\n", "a".repeat(2048))),
            src("end.ts", "/*é中😀*/\r\n\r\n"),
        ];
        let program = parse_program(&allocator, &sources).expect("parse position inputs");
        for file in &program.source_map {
            for offset in 0..=file.text.len() {
                let at = file.base + offset as u32;
                let expected = scan(file, at);
                assert_eq!(position(file, at), expected, "{} byte {offset}", file.name);
                assert_eq!(program.pos_at(at), expected, "{} byte {offset}", file.name);
            }
        }
        assert_eq!(program.pos_at(0), Pos::new("empty.ts", 1, 1));
        assert_eq!(program.pos_at(u32::MAX), Pos::new("empty.ts", 1, 1));
        eprintln!("position table reference test: {:?}", started.elapsed());
    }

    // Cost (debug): 0.03 s; one checker input, without subprocesses.
    #[test]
    fn diagnostic_columns_count_utf16_code_units() {
        let diagnostics = crate::check_program(&[SourceFile::entry(
            "utf16.ts",
            "export function main(): void { const s = \"👍\"; const x: i32 = 1.5; }",
        )])
        .expect_err("an i32 initializer cannot hold a fraction");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].pos, Pos::new("utf16.ts", 1, 63));
    }

    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn directives_keep_literal_statements_and_values_at_each_function_level() {
        let allocator = Allocator::default();
        let program = parse_program(
            &allocator,
            &[src(
                "directives.ts",
                "\"hello\"; function f() { \"world\"; }",
            )],
        )
        .expect("parse");
        assert!(program.files[0].module.directives.is_empty());
        assert!(matches!(
            program.files[0].module.body[0],
            ast::Statement::ExpressionStatement(_)
        ));
        let ast::Statement::FunctionDeclaration(function) = &program.files[0].module.body[1] else {
            panic!("function");
        };
        let body = function.body.as_ref().expect("body");
        assert!(body.directives.is_empty());
        assert!(matches!(
            body.statements[0],
            ast::Statement::ExpressionStatement(_)
        ));
    }
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn cloned_programs_use_the_parse_arena_and_keep_independent_syntax() {
        let allocator = Allocator::default();
        let program =
            parse_program(&allocator, &[src("clone.ts", "function f(): void {}")]).expect("parse");
        let mut cloned = program.clone();
        assert!(std::ptr::eq(cloned.allocator, &allocator));
        let ast::Statement::FunctionDeclaration(function) = &mut cloned.files[0].module.body[0]
        else {
            panic!("function");
        };
        function.id.as_mut().expect("name").name = allocator.alloc_str("renamed").into();
        let ast::Statement::FunctionDeclaration(original) = &program.files[0].module.body[0] else {
            panic!("function");
        };
        assert_eq!(original.id.as_ref().expect("name").name, "f");
        assert_eq!(cloned.pos(original.span), program.pos(original.span));
        assert_eq!(
            cloned.snippet(original.span),
            program.snippet(original.span)
        );
    }
}
