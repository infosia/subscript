//! Constructor parameter properties become fields and assignments (compiler.md §198 and §341 rule 5).
use oxc_allocator::{Allocator, CloneIn, Vec};
use oxc_ast::ast;
use oxc_ast::builder::AstBuilder;
use oxc_span::Span;

/// Lowers classes at module scope and inside namespaces.
pub(crate) fn lower_module<'a>(allocator: &'a Allocator, module: &mut ast::Program<'a>) {
    items(allocator, &mut module.body);
}

fn items<'a>(allocator: &'a Allocator, statements: &mut [ast::Statement<'a>]) {
    for statement in statements {
        match statement {
            ast::Statement::ClassDeclaration(class) => lower_class(allocator, class),
            ast::Statement::TSNamespaceDeclaration(namespace) => {
                namespace_body(allocator, namespace)
            }
            ast::Statement::ExportDeclaration(export) => {
                decl(allocator, &mut export.declaration);
            }
            ast::Statement::ExportDefaultDeclaration(export) => {
                if let ast::ExportDefaultDeclarationKind::ClassDeclaration(class) =
                    &mut export.declaration
                {
                    lower_class(allocator, class);
                }
            }
            _ => {}
        }
    }
}
fn decl<'a>(allocator: &'a Allocator, declaration: &mut ast::Declaration<'a>) {
    match declaration {
        ast::Declaration::ClassDeclaration(class) => lower_class(allocator, class),
        ast::Declaration::TSNamespaceDeclaration(namespace) => namespace_body(allocator, namespace),
        ast::Declaration::TSExternalModuleDeclaration(module) => {
            if let Some(body) = &mut module.body {
                items(allocator, &mut body.body);
            }
        }
        ast::Declaration::TSGlobalDeclaration(global) => items(allocator, &mut global.body.body),
        _ => {}
    }
}
fn namespace_body<'a>(allocator: &'a Allocator, namespace: &mut ast::TSNamespaceDeclaration<'a>) {
    match &mut namespace.body {
        ast::TSNamespaceDeclarationBody::TSModuleBlock(block) => items(allocator, &mut block.body),
        ast::TSNamespaceDeclarationBody::TSNamespaceDeclaration(nested) => {
            namespace_body(allocator, nested)
        }
    }
}

/// Lowers parameter properties and preserves source spans on every generated node.
pub(crate) fn lower_class<'a>(allocator: &'a Allocator, class: &mut ast::Class<'a>) {
    let builder = AstBuilder::new(allocator);
    let mut fields = Vec::new_in(&allocator);
    for member in &mut class.body.body {
        let ast::ClassElement::MethodDefinition(method) = member else {
            continue;
        };
        if method.kind != ast::MethodDefinitionKind::Constructor {
            continue;
        }
        let mut assignments = Vec::new_in(&allocator);
        for parameter in &mut method.value.params.items {
            if parameter.accessibility.is_none() && !parameter.readonly && !parameter.r#override {
                continue;
            }
            if let ast::BindingPattern::BindingIdentifier(binding) = &parameter.pattern {
                fields.push(ast::ClassElement::PropertyDefinition(
                    ast::PropertyDefinition::boxed(
                        parameter.span,
                        ast::PropertyDefinitionType::PropertyDefinition,
                        Vec::new_in(&allocator),
                        ast::PropertyKey::StaticIdentifier(ast::IdentifierName::boxed(
                            binding.span,
                            binding.name,
                            &builder,
                        )),
                        parameter.type_annotation.clone_in(allocator),
                        None,
                        false,
                        false,
                        false,
                        parameter.r#override,
                        parameter.optional,
                        false,
                        parameter.readonly,
                        parameter.accessibility,
                        &builder,
                    ),
                ));
                assignments.push(assignment(allocator, parameter.span, binding));
            }
            parameter.accessibility = None;
            parameter.readonly = false;
            parameter.r#override = false;
        }
        if let Some(body) = &mut method.value.body {
            assignments.extend(std::mem::replace(
                &mut body.statements,
                Vec::new_in(&allocator),
            ));
            body.statements = assignments;
        }
    }
    if !fields.is_empty() {
        fields.extend(std::mem::replace(
            &mut class.body.body,
            Vec::new_in(&allocator),
        ));
        class.body.body = fields;
    }
}

/// True for a field whose constructor parameter has the same name and span.
pub(crate) fn declares_field(class: &ast::Class<'_>, prop: &ast::PropertyDefinition<'_>) -> bool {
    parameter_of(class, prop).is_some()
}

/// The default expression of the constructor parameter that declares this field.
pub(crate) fn parameter_default<'p, 'a>(
    class: &'p ast::Class<'a>,
    prop: &ast::PropertyDefinition<'a>,
) -> Option<&'p ast::Expression<'a>> {
    parameter_of(class, prop)?.initializer.as_deref()
}

fn parameter_of<'p, 'a>(
    class: &'p ast::Class<'a>,
    prop: &ast::PropertyDefinition<'a>,
) -> Option<&'p ast::FormalParameter<'a>> {
    let ast::PropertyKey::StaticIdentifier(key) = &prop.key else {
        return None;
    };
    class.body.body.iter().find_map(|member| {
        let ast::ClassElement::MethodDefinition(method) = member else { return None; };
        if method.kind != ast::MethodDefinitionKind::Constructor { return None; }
        method.value.params.items.iter().find(|parameter| parameter.span == prop.span && matches!(&parameter.pattern, ast::BindingPattern::BindingIdentifier(binding) if binding.name == key.name))
    })
}

fn assignment<'a>(
    allocator: &'a Allocator,
    span: Span,
    name: &ast::BindingIdentifier<'a>,
) -> ast::Statement<'a> {
    let builder = AstBuilder::new(allocator);
    let member = ast::StaticMemberExpression::boxed(
        span,
        ast::Expression::ThisExpression(ast::ThisExpression::boxed(span, &builder)),
        ast::IdentifierName::new(name.span, name.name, &builder),
        false,
        &builder,
    );
    ast::Statement::ExpressionStatement(ast::ExpressionStatement::boxed(
        span,
        ast::Expression::AssignmentExpression(ast::AssignmentExpression::boxed(
            span,
            ast::AssignmentOperator::Assign,
            ast::AssignmentTarget::StaticMemberExpression(member),
            ast::Expression::Identifier(ast::IdentifierReference::boxed(
                name.span, name.name, &builder,
            )),
            &builder,
        )),
        &builder,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceFile;
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn fields_defaults_and_assignments_keep_parameter_order() {
        let allocator = Allocator::default();
        let program = crate::parse::parse_program(&allocator, &[SourceFile::entry("main.ts", "class C { tag: i32 = 1; constructor(public a: i32, private readonly c: f64 = 2.0) { print(a); } m(): void {} }")]).expect("parse");
        let ast::Statement::ClassDeclaration(class) = &program.files[0].module.body[0] else {
            panic!("class");
        };
        let fields: std::vec::Vec<_> = class
            .body
            .body
            .iter()
            .filter_map(|member| {
                if let ast::ClassElement::PropertyDefinition(field) = member {
                    Some(field)
                } else {
                    None
                }
            })
            .collect();
        let names: std::vec::Vec<_> = fields
            .iter()
            .map(|field| {
                if let ast::PropertyKey::StaticIdentifier(key) = &field.key {
                    key.name.as_str()
                } else {
                    "?"
                }
            })
            .collect();
        assert_eq!(names, ["a", "c", "tag"]);
        assert!(fields[1].readonly);
        assert_eq!(fields[1].accessibility, Some(ast::TSAccessibility::Private));
        assert!(fields[1].value.is_none());
        assert!(fields[1].type_annotation.is_some());
        assert!(declares_field(class, fields[0]));
        assert!(!declares_field(class, fields[2]));
        assert!(parameter_default(class, fields[0]).is_none());
        assert!(parameter_default(class, fields[1]).is_some());
        let ctor = class
            .body
            .body
            .iter()
            .find_map(|member| match member {
                ast::ClassElement::MethodDefinition(method)
                    if method.kind == ast::MethodDefinitionKind::Constructor =>
                {
                    Some(method)
                }
                _ => None,
            })
            .expect("constructor");
        assert!(ctor
            .value
            .params
            .items
            .iter()
            .all(|param| param.accessibility.is_none() && !param.readonly));
        let body = &ctor.value.body.as_ref().expect("body").statements;
        assert_eq!(body.len(), 3);
        for (statement, name) in body.iter().zip(["a", "c"]) {
            let ast::Statement::ExpressionStatement(statement) = statement else {
                panic!("assignment statement");
            };
            let ast::Expression::AssignmentExpression(assign) = &statement.expression else {
                panic!("assignment");
            };
            let ast::AssignmentTarget::StaticMemberExpression(target) = &assign.left else {
                panic!("member");
            };
            let ast::Expression::Identifier(value) = &assign.right else {
                panic!("identifier");
            };
            assert_eq!(target.property.name, name);
            assert_eq!(value.name, name);
        }
    }
    /// §198.1 rule 5: a field and a parameter property with one name are
    /// rejected with S017. The control renames the field and is accepted.
    #[test]
    fn a_field_and_a_parameter_property_with_one_name_are_rejected() {
        let check = |field: &str| {
            crate::check_program(&[SourceFile::entry(
                "main.ts",
                &format!(
                    "class C {{ {field}: i32 = 1; constructor(public x: i32) {{}} }}\nexport function main(): void {{}}\n"
                ),
            )])
        };
        let diagnostics = check("x").expect_err("the name clash must be rejected");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, crate::RuleCode::S017);
        assert!(
            diagnostics[0].message.contains("`x`"),
            "{}",
            diagnostics[0].message
        );
        check("y").unwrap_or_else(|diagnostics| panic!("control: {diagnostics:?}"));
    }

    fn rejected(class: &str) -> std::vec::Vec<crate::Diagnostic> {
        crate::check_program(&[SourceFile::entry(
            "main.ts",
            &format!("{class}\nexport function main(): void {{}}\n"),
        )])
        .expect_err("the program must be rejected")
    }

    /// §198.1 rule 6: an unannotated parameter property with a literal
    /// default takes the C4 type of the literal. The control is a
    /// non-literal default, which keeps its own text.
    #[test]
    fn a_literal_default_gives_an_unannotated_parameter_property_its_type() {
        let source = "class C { constructor(public a = 1, public b = 2.5, public c = \"s\", public d = true, public e = -3, public f = false) {} }\n\
            export function main(): void { const x = new C(); const a: i32 = x.a; const b: f64 = x.b; const c: string = x.c; const d: boolean = x.d; const e: i32 = x.e; const f: boolean = x.f; }\n";
        crate::check_program(&[SourceFile::entry("main.ts", source)])
            .unwrap_or_else(|diagnostics| panic!("{diagnostics:?}"));
        let mismatch = "class C { constructor(public b = 2.5) {} }\n\
            export function main(): void { const b: i32 = new C().b; }\n";
        let diagnostics = crate::check_program(&[SourceFile::entry("main.ts", mismatch)])
            .expect_err("a fractional default gives f64");
        assert_eq!(
            diagnostics[0].code,
            crate::RuleCode::S007,
            "{diagnostics:?}"
        );
        let diagnostics = rejected("class C { constructor(public count = 1 + 1) {} }");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, crate::RuleCode::S100);
        assert_eq!(
            diagnostics[0].message,
            "a parameter property requires a type annotation"
        );
        assert_eq!(
            diagnostics[0].divergence,
            Some(crate::divergence::Divergence::ParameterPropertyTypeAnnotation)
        );
    }

    /// §198.1 rule 5a: a diagnostic that the field and the parameter give
    /// for the shared annotation reports once. The control is a declared
    /// field and a parameter that each spell the unknown type.
    #[test]
    fn a_shared_annotation_reports_an_identical_diagnostic_once() {
        let diagnostics = rejected("class C { constructor(public x: Foo) {} }");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, crate::RuleCode::S016);
        let control = rejected("class C { x: Foo; constructor(x: Foo) { this.x = x; } }");
        assert_eq!(control.len(), 2, "{control:?}");
        assert!(control.iter().all(|d| d.code == crate::RuleCode::S016));
    }

    /// §198.1 rule 5a: a field diagnostic that the parameter does not
    /// give stays. Each shape gives the diagnostics of its hand-written
    /// form.
    #[test]
    fn a_parameter_property_field_keeps_each_field_diagnostic() {
        let slot = "class Enemy { hp: i32 = 1; }\n\
            @ValueType class Slot<T> { item: T; constructor(item: T) { this.item = item; } }\n";
        for (property, by_hand) in [
            (
                "class World { constructor(public slot: Slot<Enemy>) {} }",
                "class World { slot: Slot<Enemy>; constructor(slot: Slot<Enemy>) { this.slot = slot; } }",
            ),
            (
                "class A { constructor(private group: TaskGroup) {} }",
                "class A { private group: TaskGroup; constructor(group: TaskGroup) { this.group = group; } }",
            ),
        ] {
            let messages = |class: &str| -> std::vec::Vec<(crate::RuleCode, String)> {
                rejected(&format!("{slot}{class}"))
                    .into_iter()
                    .map(|d| (d.code, d.message))
                    .collect()
            };
            let property_messages = messages(property);
            assert!(!property_messages.is_empty(), "{property}");
            assert_eq!(property_messages, messages(by_hand), "{property}");
        }
        let first = rejected(&format!(
            "{slot}class World {{ constructor(public slot: Slot<Enemy>) {{}} }}"
        ));
        assert!(
            first[0]
                .message
                .starts_with("field type `Enemy` is outside the value-class whitelist"),
            "{first:?}"
        );
    }
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn namespace_classes_lower_and_plain_classes_keep_their_form() {
        let allocator = Allocator::default();
        let mut parsed = oxc_parser::Parser::new(&allocator, "namespace A.B { export class C { constructor(public x: i32) {} } } class D { x: i32 = 1; constructor(x: i32) { this.x = x; } }", oxc_span::SourceType::ts().with_module(true)).parse();
        assert!(parsed.diagnostics.is_empty());
        let before = format!("{:?}", parsed.program.body[1]);
        lower_module(&allocator, &mut parsed.program);
        assert_eq!(format!("{:?}", parsed.program.body[1]), before);
        let ast::Statement::TSNamespaceDeclaration(a) = &parsed.program.body[0] else {
            panic!("namespace");
        };
        let ast::TSNamespaceDeclarationBody::TSNamespaceDeclaration(b) = &a.body else {
            panic!("nested namespace");
        };
        let ast::TSNamespaceDeclarationBody::TSModuleBlock(block) = &b.body else {
            panic!("namespace block");
        };
        let ast::Statement::ExportDeclaration(export) = &block.body[0] else {
            panic!("export");
        };
        let ast::Declaration::ClassDeclaration(class) = &export.declaration else {
            panic!("class");
        };
        assert!(matches!(
            class.body.body[0],
            ast::ClassElement::PropertyDefinition(_)
        ));
    }
}
