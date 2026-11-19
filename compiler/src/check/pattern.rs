//! Binding patterns: the accepted forms, and the reason each rejected
//! form carries (`specs/blocks/compiler.md` §107).
//!
//! [`classify`] is the one reader of a pattern's shape. It answers with
//! an accepted form, or with the first reason the form is rejected plus
//! every name in the pattern. A caller declares those
//! names with an error type, so one pattern gives one diagnostic
//! (§107.4).

use crate::check::rejection::RejectionSite;
use oxc_ast::ast;
use oxc_span::{GetSpan, Span};

/// Where one bound name reads its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BindingSource {
    /// Element at this index of a `T[]` or a `FixedArray<T, N>`.
    Element(i32),
    /// Field of this name on a reference or value class.
    Field(String),
}

/// One name that an accepted pattern binds, and where it reads from.
#[derive(Debug, Clone)]
pub(crate) struct PatternBinding<'a> {
    /// The source of the bound value.
    pub source: BindingSource,
    /// The declared name.
    pub binding: &'a ast::BindingIdentifier<'a>,
}

/// A rejected pattern: the offending position, the reason, and every
/// name in the pattern.
#[derive(Debug, Clone)]
pub(crate) struct PatternRejection<'a> {
    /// Position of the part of the pattern that carries the reason.
    pub span: Span,
    /// The reason, which names the work the form needs.
    pub message: &'static str,
    /// The TypeScript form, when stock `tsc` accepts this program.
    pub site: RejectionSite,
    /// Every name in the pattern, in source order.
    pub names: Vec<&'a ast::BindingIdentifier<'a>>,
}

/// An accepted binding pattern, or the reason one is rejected.
#[derive(Debug, Clone)]
pub(crate) enum Pattern<'a> {
    /// A single name. The source binds to it directly.
    Name(&'a ast::BindingIdentifier<'a>),
    /// `[a, , b]` over a `T[]` or a `FixedArray<T, N>` (§107.1 rule 1).
    Array {
        /// Position of the whole pattern.
        span: Span,
        /// The bound names, in source order.
        bindings: Vec<PatternBinding<'a>>,
    },
    /// `{ x, y: renamed }` over a reference or value class (§107.1 rule 2).
    Fields {
        /// Position of the whole pattern.
        span: Span,
        /// The bound names, in source order.
        bindings: Vec<PatternBinding<'a>>,
    },
    /// A form outside §107.1.
    Rejected(PatternRejection<'a>),
}

impl Pattern<'_> {
    /// True for a pattern that reads its names out of one source.
    pub(crate) fn is_destructuring(&self) -> bool {
        matches!(self, Pattern::Array { .. } | Pattern::Fields { .. })
    }

    /// Position of the whole pattern.
    pub(crate) fn span(&self) -> Span {
        match self {
            Pattern::Name(binding) => binding.span,
            Pattern::Array { span, .. } | Pattern::Fields { span, .. } => *span,
            Pattern::Rejected(rejection) => rejection.span,
        }
    }
}

/// The reason a rejected pattern carries, one per §107.3 row.
mod reason {
    /// `const [a = 1] = xs` (§107.3 row 4).
    pub(super) const DEFAULT_VALUE: &str = "a default value in a binding pattern needs a rule for a missing element and for `undefined`; the language has neither";
    /// `const [a, ...rest] = xs` (§107.3 row 5).
    pub(super) const ARRAY_REST: &str = "a rest element in an array binding pattern needs allocation and copy semantics for the rest array";
    /// `const { x, ...rest } = p` (§107.3 row 6).
    pub(super) const OBJECT_REST: &str = "a rest element in a field binding pattern needs a result shape and property-selection rules";
    /// `const [[a, b]] = xss` (§107.3 row 7).
    pub(super) const NESTED: &str =
        "a nested binding pattern needs recursive type checks and ordered effects";
    /// A field name that is neither an identifier nor a string literal.
    pub(super) const FIELD_NAME: &str =
        "a field binding pattern names its fields with an identifier or a string literal";
}

/// Reads an accepted binding shape or its first rejection (compiler.md §107).
pub(crate) fn classify<'a>(pat: &'a ast::BindingPattern<'a>) -> Pattern<'a> {
    match pat {
        ast::BindingPattern::BindingIdentifier(binding) => Pattern::Name(binding),
        ast::BindingPattern::ArrayPattern(array) => classify_array(array),
        ast::BindingPattern::ObjectPattern(object) => classify_object(object),
        other => Pattern::Rejected(PatternRejection {
            span: other.span(),
            message: reason::NESTED,
            site: RejectionSite::BindingPatternUnsupportedRoot,
            names: collect_names(other),
        }),
    }
}
fn classify_array<'a>(array: &'a ast::ArrayPattern<'a>) -> Pattern<'a> {
    let mut bindings = Vec::new();
    for (index, element) in array.elements.iter().enumerate() {
        let Some(element) = element else {
            continue;
        };
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        if let ast::BindingPattern::BindingIdentifier(binding) = element {
            bindings.push(PatternBinding {
                source: BindingSource::Element(index),
                binding,
            });
            continue;
        }
        let (span, message, site) = match element {
            ast::BindingPattern::AssignmentPattern(assign) => (
                assign.span,
                reason::DEFAULT_VALUE,
                RejectionSite::ArrayBindingDefaultValue,
            ),
            other => (
                other.span(),
                reason::NESTED,
                RejectionSite::ArrayBindingNestedPattern,
            ),
        };
        return Pattern::Rejected(PatternRejection {
            span,
            message,
            site,
            names: array_names(array),
        });
    }
    if let Some(rest) = &array.rest {
        return Pattern::Rejected(PatternRejection {
            span: rest.span,
            message: reason::ARRAY_REST,
            site: RejectionSite::ArrayBindingRestElement,
            names: array_names(array),
        });
    }
    Pattern::Array {
        span: array.span,
        bindings,
    }
}
fn classify_object<'a>(object: &'a ast::ObjectPattern<'a>) -> Pattern<'a> {
    let mut bindings = Vec::new();
    for property in &object.properties {
        let name = if property.computed {
            None
        } else {
            match &property.key {
                ast::PropertyKey::StaticIdentifier(key) => Some(key.name.to_string()),
                ast::PropertyKey::StringLiteral(key) => Some(key.value.to_string()),
                _ => None,
            }
        };
        if let (Some(name), ast::BindingPattern::BindingIdentifier(binding)) =
            (&name, &property.value)
        {
            bindings.push(PatternBinding {
                source: BindingSource::Field(name.clone()),
                binding,
            });
            continue;
        }
        let (span, message, site) = if name.is_none() {
            (
                Span::new(property.span.start, property.key.span().end),
                reason::FIELD_NAME,
                RejectionSite::ObjectBindingNonLiteralFieldName,
            )
        } else if let ast::BindingPattern::AssignmentPattern(assign) = &property.value {
            (
                if property.shorthand {
                    property.span
                } else {
                    assign.span
                },
                reason::DEFAULT_VALUE,
                if property.shorthand {
                    RejectionSite::ObjectBindingShorthandDefault
                } else {
                    RejectionSite::ObjectBindingFieldDefault
                },
            )
        } else {
            (
                property.value.span(),
                reason::NESTED,
                RejectionSite::ObjectBindingNestedPattern,
            )
        };
        return Pattern::Rejected(PatternRejection {
            span,
            message,
            site,
            names: object_names(object),
        });
    }
    if let Some(rest) = &object.rest {
        return Pattern::Rejected(PatternRejection {
            span: rest.span,
            message: reason::OBJECT_REST,
            site: RejectionSite::ObjectBindingRestProperty,
            names: object_names(object),
        });
    }
    Pattern::Fields {
        span: object.span,
        bindings,
    }
}
fn array_names<'a, 'p>(array: &'a ast::ArrayPattern<'p>) -> Vec<&'a ast::BindingIdentifier<'p>> {
    let mut out = Vec::new();
    for element in array.elements.iter().flatten() {
        collect_into(element, &mut out);
    }
    if let Some(rest) = &array.rest {
        collect_into(&rest.argument, &mut out);
    }
    out
}
fn object_names<'a, 'p>(object: &'a ast::ObjectPattern<'p>) -> Vec<&'a ast::BindingIdentifier<'p>> {
    let mut out = Vec::new();
    for property in &object.properties {
        collect_into(&property.value, &mut out);
    }
    if let Some(rest) = &object.rest {
        collect_into(&rest.argument, &mut out);
    }
    out
}
/// Every bound name, in source order.
pub(crate) fn collect_names<'a, 'p>(
    pat: &'a ast::BindingPattern<'p>,
) -> Vec<&'a ast::BindingIdentifier<'p>> {
    let mut out = Vec::new();
    collect_into(pat, &mut out);
    out
}
fn collect_into<'a, 'p>(
    pat: &'a ast::BindingPattern<'p>,
    out: &mut Vec<&'a ast::BindingIdentifier<'p>>,
) {
    match pat {
        ast::BindingPattern::BindingIdentifier(binding) => out.push(binding),
        ast::BindingPattern::ArrayPattern(array) => out.extend(array_names(array)),
        ast::BindingPattern::ObjectPattern(object) => out.extend(object_names(object)),
        ast::BindingPattern::AssignmentPattern(assign) => collect_into(&assign.left, out),
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, collect_names, BindingSource, Pattern};
    use oxc_ast::ast;

    /// Parses `const <source> = value;` and answers with its pattern.
    fn with_pattern<R>(source: &str, body: impl FnOnce(&ast::BindingPattern) -> R) -> R {
        let text = format!("const {source} = value;\n");
        let allocator = oxc_allocator::Allocator::default();
        let program =
            crate::parse::parse_program(&allocator, &[crate::SourceFile::new("pattern.ts", text)])
                .expect("the pattern parses");
        let ast::Statement::VariableDeclaration(declaration) = &program.files[0].module.body[0]
        else {
            panic!("expected variable declaration");
        };
        body(&declaration.declarations[0].id)
    }

    fn bound(source: &str) -> Vec<(String, BindingSource)> {
        with_pattern(source, |pat| {
            let bindings = match classify(pat) {
                Pattern::Array { bindings, .. } | Pattern::Fields { bindings, .. } => bindings,
                _ => panic!("{source} must be an accepted pattern"),
            };
            bindings
                .iter()
                .map(|binding| (binding.binding.name.to_string(), binding.source.clone()))
                .collect()
        })
    }

    #[test]
    fn a_plain_name_is_not_a_destructuring_pattern() {
        with_pattern("value", |pat| {
            let pattern = classify(pat);
            assert!(matches!(pattern, Pattern::Name(_)));
            assert!(!pattern.is_destructuring());
        });
    }

    #[test]
    fn an_array_pattern_binds_each_element_by_position() {
        assert_eq!(
            bound("[a, , b]"),
            vec![
                ("a".to_string(), BindingSource::Element(0)),
                ("b".to_string(), BindingSource::Element(2)),
            ]
        );
    }

    #[test]
    fn a_field_pattern_binds_the_shorthand_and_the_renamed_form() {
        assert_eq!(
            bound("{ x, y: renamed, \"z\": third }"),
            vec![
                ("x".to_string(), BindingSource::Field("x".to_string())),
                ("renamed".to_string(), BindingSource::Field("y".to_string())),
                ("third".to_string(), BindingSource::Field("z".to_string())),
            ]
        );
    }

    #[test]
    fn an_empty_pattern_binds_no_name() {
        for source in ["[]", "{}"] {
            assert!(bound(source).is_empty(), "{source} binds a name");
        }
    }

    #[test]
    fn each_rejected_form_carries_its_own_reason() {
        for (source, fragment) in [
            ("[a = 1]", "a default value"),
            ("[a, ...rest]", "a rest element in an array"),
            ("{ x, ...rest }", "a rest element in a field"),
            ("[[a, b]]", "a nested binding pattern"),
            ("{ x: { y } }", "a nested binding pattern"),
            ("{ x = 1 }", "a default value"),
            ("{ [key]: value }", "names its fields with an identifier"),
        ] {
            with_pattern(source, |pat| {
                let Pattern::Rejected(rejection) = classify(pat) else {
                    panic!("{source} must be rejected");
                };
                assert!(
                    rejection.message.contains(fragment),
                    "{source}: `{}` does not name `{fragment}`",
                    rejection.message
                );
            });
        }
    }

    #[test]
    fn a_rejected_pattern_reports_every_name_in_the_pattern() {
        for (source, expected) in [
            ("[a, ...rest]", vec!["a", "rest"]),
            ("{ x, ...rest }", vec!["x", "rest"]),
            ("[[a, b], c]", vec!["a", "b", "c"]),
            ("[a = 1]", vec!["a"]),
            ("{ x = 1 }", vec!["x"]),
        ] {
            with_pattern(source, |pat| {
                let names: Vec<String> = collect_names(pat)
                    .iter()
                    .map(|binding| binding.name.to_string())
                    .collect();
                assert_eq!(names, expected, "{source}");
            });
        }
    }
}
