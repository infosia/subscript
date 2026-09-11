//! Binding patterns: the accepted forms, and the reason each rejected
//! form carries (`specs/blocks/compiler.md` §107).
//!
//! [`classify`] is the one reader of a pattern's shape. It answers with
//! an accepted form, or with the first reason the form is rejected plus
//! every name in the pattern. A caller declares those
//! names with an error type, so one pattern gives one diagnostic
//! (§107.4).

use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;

use crate::divergence::Divergence;

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
    pub binding: &'a ast::BindingIdent,
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
    pub divergence: Divergence,
    /// Every name in the pattern, in source order.
    pub names: Vec<&'a ast::BindingIdent>,
}

/// An accepted binding pattern, or the reason one is rejected.
#[derive(Debug, Clone)]
pub(crate) enum Pattern<'a> {
    /// A single name. The source binds to it directly.
    Name(&'a ast::BindingIdent),
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
            Pattern::Name(binding) => binding.id.span,
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

/// Reads a pattern's shape (§107.1), or the first reason it is rejected
/// (§107.3).
pub(crate) fn classify(pat: &ast::Pat) -> Pattern<'_> {
    match pat {
        ast::Pat::Ident(binding) => Pattern::Name(binding),
        ast::Pat::Array(array) => classify_array(array),
        ast::Pat::Object(object) => classify_object(object),
        other => Pattern::Rejected(PatternRejection {
            span: other.span(),
            message: reason::NESTED,
            divergence: Divergence::NestedPattern,
            names: collect_names(other),
        }),
    }
}

fn classify_array(array: &ast::ArrayPat) -> Pattern<'_> {
    let mut bindings = Vec::new();
    for (index, element) in array.elems.iter().enumerate() {
        // `const [, b] = xs` advances the position and binds no name.
        let Some(element) = element else { continue };
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        let (span, message, divergence) = match element {
            ast::Pat::Ident(binding) => {
                bindings.push(PatternBinding {
                    source: BindingSource::Element(index),
                    binding,
                });
                continue;
            }
            ast::Pat::Rest(rest) => (rest.span, reason::ARRAY_REST, Divergence::ArrayRestPattern),
            ast::Pat::Assign(assign) => (
                assign.span,
                reason::DEFAULT_VALUE,
                Divergence::PatternDefaultValue,
            ),
            other => (other.span(), reason::NESTED, Divergence::NestedPattern),
        };
        return Pattern::Rejected(PatternRejection {
            span,
            message,
            divergence,
            names: array_names(array),
        });
    }
    Pattern::Array {
        span: array.span,
        bindings,
    }
}

fn classify_object(object: &ast::ObjectPat) -> Pattern<'_> {
    let mut bindings = Vec::new();
    for prop in &object.props {
        let (span, message, divergence) = match prop {
            ast::ObjectPatProp::Assign(assign) => match assign.value {
                Some(_) => (
                    assign.span,
                    reason::DEFAULT_VALUE,
                    Divergence::PatternDefaultValue,
                ),
                None => {
                    bindings.push(PatternBinding {
                        source: BindingSource::Field(assign.key.id.sym.to_string()),
                        binding: &assign.key,
                    });
                    continue;
                }
            },
            ast::ObjectPatProp::KeyValue(entry) => {
                let name = match &entry.key {
                    ast::PropName::Ident(key) => Some(key.sym.to_string()),
                    ast::PropName::Str(key) => Some(key.value.to_string()),
                    _ => None,
                };
                match (name, entry.value.as_ref()) {
                    (Some(name), ast::Pat::Ident(binding)) => {
                        bindings.push(PatternBinding {
                            source: BindingSource::Field(name),
                            binding,
                        });
                        continue;
                    }
                    (None, _) => (
                        entry.key.span(),
                        reason::FIELD_NAME,
                        Divergence::PatternFieldName,
                    ),
                    (Some(_), ast::Pat::Assign(assign)) => (
                        assign.span,
                        reason::DEFAULT_VALUE,
                        Divergence::PatternDefaultValue,
                    ),
                    (Some(_), other) => (other.span(), reason::NESTED, Divergence::NestedPattern),
                }
            }
            ast::ObjectPatProp::Rest(rest) => (
                rest.span,
                reason::OBJECT_REST,
                Divergence::ObjectRestPattern,
            ),
        };
        return Pattern::Rejected(PatternRejection {
            span,
            message,
            divergence,
            names: object_names(object),
        });
    }
    Pattern::Fields {
        span: object.span,
        bindings,
    }
}

/// Every name an array pattern binds, in source order.
fn array_names(array: &ast::ArrayPat) -> Vec<&ast::BindingIdent> {
    let mut out = Vec::new();
    for element in array.elems.iter().flatten() {
        collect_into(element, &mut out);
    }
    out
}

/// Every name a field pattern binds, in source order.
fn object_names(object: &ast::ObjectPat) -> Vec<&ast::BindingIdent> {
    let mut out = Vec::new();
    for prop in &object.props {
        match prop {
            ast::ObjectPatProp::Assign(assign) => out.push(&assign.key),
            ast::ObjectPatProp::KeyValue(entry) => collect_into(&entry.value, &mut out),
            ast::ObjectPatProp::Rest(rest) => collect_into(&rest.arg, &mut out),
        }
    }
    out
}

/// Every name a pattern binds, in source order.
pub(crate) fn collect_names(pat: &ast::Pat) -> Vec<&ast::BindingIdent> {
    let mut out = Vec::new();
    collect_into(pat, &mut out);
    out
}

fn collect_into<'a>(pat: &'a ast::Pat, out: &mut Vec<&'a ast::BindingIdent>) {
    match pat {
        ast::Pat::Ident(binding) => out.push(binding),
        ast::Pat::Array(array) => out.extend(array_names(array)),
        ast::Pat::Object(object) => out.extend(object_names(object)),
        ast::Pat::Rest(rest) => collect_into(&rest.arg, out),
        ast::Pat::Assign(assign) => collect_into(&assign.left, out),
        ast::Pat::Expr(_) | ast::Pat::Invalid(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, collect_names, BindingSource, Pattern};
    use swc_ecma_ast as ast;

    /// Parses `const <source> = value;` and answers with its pattern.
    fn with_pattern<R>(source: &str, body: impl FnOnce(&ast::Pat) -> R) -> R {
        let text = format!("const {source} = value;\n");
        swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
            let program =
                crate::parse::parse_program(&[crate::SourceFile::new("pattern.ts", text)])
                    .expect("the pattern parses");
            let ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(declaration))) =
                &program.files[0].module.body[0]
            else {
                panic!("{source}: expected a variable declaration");
            };
            body(&declaration.decls[0].name)
        })
    }

    fn bound(source: &str) -> Vec<(String, BindingSource)> {
        with_pattern(source, |pat| {
            let bindings = match classify(pat) {
                Pattern::Array { bindings, .. } | Pattern::Fields { bindings, .. } => bindings,
                _ => panic!("{source} must be an accepted pattern"),
            };
            bindings
                .iter()
                .map(|binding| (binding.binding.id.sym.to_string(), binding.source.clone()))
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
                    .map(|binding| binding.id.sym.to_string())
                    .collect();
                assert_eq!(names, expected, "{source}");
            });
        }
    }
}
