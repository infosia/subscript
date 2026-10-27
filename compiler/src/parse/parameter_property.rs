//! A constructor parameter property declares a field
//! (`specs/blocks/compiler.md` §198). The front end lowers each one to
//! a plain parameter, an instance field, and an assignment, so every
//! later stage reads the form that a program writes by hand.

use swc_ecma_ast as ast;

/// Lowers every constructor parameter property of each class that the
/// module declares at its top level or inside a namespace body.
///
/// §198.1 rule 2: the parameter-property fields come first, in
/// parameter order, then the declared members in source order. Rule 3:
/// the assignments come first in the constructor body, in parameter
/// order, so they run after the field initializers and before the body.
pub(crate) fn lower_module(module: &mut ast::Module) {
    for item in &mut module.body {
        match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => lower_decl(decl),
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
                lower_decl(&mut export.decl)
            }
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDefaultDecl(export)) => {
                if let ast::DefaultDecl::Class(class) = &mut export.decl {
                    lower_class(&mut class.class);
                }
            }
            _ => {}
        }
    }
}

fn lower_decl(decl: &mut ast::Decl) {
    match decl {
        ast::Decl::Class(class) => lower_class(&mut class.class),
        ast::Decl::TsModule(namespace) => {
            let mut body = namespace.body.as_mut();
            while let Some(current) = body {
                match current {
                    ast::TsNamespaceBody::TsModuleBlock(block) => {
                        for item in &mut block.body {
                            match item {
                                ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => lower_decl(decl),
                                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(
                                    export,
                                )) => lower_decl(&mut export.decl),
                                _ => {}
                            }
                        }
                        body = None;
                    }
                    ast::TsNamespaceBody::TsNamespaceDecl(nested) => {
                        body = Some(nested.body.as_mut());
                    }
                }
            }
        }
        _ => {}
    }
}

/// Lowers the parameter properties of one class.
pub(crate) fn lower_class(class: &mut ast::Class) {
    let mut fields = Vec::new();
    for member in &mut class.body {
        let ast::ClassMember::Constructor(ctor) = member else {
            continue;
        };
        let mut assignments = Vec::new();
        for parameter in &mut ctor.params {
            let ast::ParamOrTsParamProp::TsParamProp(property) = parameter else {
                continue;
            };
            let (pat, binding) = match &property.param {
                ast::TsParamPropParam::Ident(binding) => {
                    (ast::Pat::Ident(binding.clone()), Some(binding.clone()))
                }
                ast::TsParamPropParam::Assign(assign) => (
                    ast::Pat::Assign(assign.clone()),
                    match assign.left.as_ref() {
                        ast::Pat::Ident(binding) => Some(binding.clone()),
                        _ => None,
                    },
                ),
            };
            if let Some(binding) = binding {
                fields.push(field(property, &binding));
                assignments.push(assignment(property.span, &binding.id));
            }
            *parameter = ast::ParamOrTsParamProp::Param(ast::Param {
                span: property.span,
                decorators: std::mem::take(&mut property.decorators),
                pat,
            });
        }
        if let Some(body) = &mut ctor.body {
            body.stmts.splice(0..0, assignments);
        }
    }
    if !fields.is_empty() {
        class.body.splice(0..0, fields);
    }
}

/// True if `prop` is the field that a parameter property of `class`
/// declares: a constructor parameter has its span and its name.
pub(crate) fn declares_field(class: &ast::Class, prop: &ast::ClassProp) -> bool {
    parameter_of(class, prop).is_some()
}

/// The default of the parameter that declares the field `prop`, if
/// `prop` is a parameter-property field and the parameter has one.
pub(crate) fn parameter_default<'a>(
    class: &'a ast::Class,
    prop: &ast::ClassProp,
) -> Option<&'a ast::Expr> {
    match &parameter_of(class, prop)?.pat {
        ast::Pat::Assign(assign) => Some(&assign.right),
        _ => None,
    }
}

fn parameter_of<'a>(class: &'a ast::Class, prop: &ast::ClassProp) -> Option<&'a ast::Param> {
    let ast::PropName::Ident(key) = &prop.key else {
        return None;
    };
    class.body.iter().find_map(|member| {
        let ast::ClassMember::Constructor(ctor) = member else {
            return None;
        };
        ctor.params.iter().find_map(|parameter| {
            let ast::ParamOrTsParamProp::Param(param) = parameter else {
                return None;
            };
            let binding = match &param.pat {
                ast::Pat::Ident(binding) => Some(binding),
                ast::Pat::Assign(assign) => assign.left.as_ident(),
                _ => None,
            };
            (param.span == prop.span && binding.is_some_and(|binding| binding.id.sym == key.sym))
                .then_some(param)
        })
    })
}

/// The instance field that a parameter property declares: its name,
/// type, optionality, and modifiers, with no initializer (§198.1 rule 4).
fn field(property: &ast::TsParamProp, binding: &ast::BindingIdent) -> ast::ClassMember {
    ast::ClassMember::ClassProp(ast::ClassProp {
        span: property.span,
        key: ast::PropName::Ident(ast::IdentName {
            span: binding.id.span,
            sym: binding.id.sym.clone(),
        }),
        value: None,
        type_ann: binding.type_ann.clone(),
        is_static: false,
        decorators: Vec::new(),
        accessibility: property.accessibility,
        is_abstract: false,
        is_optional: binding.id.optional,
        is_override: property.is_override,
        readonly: property.readonly,
        declare: false,
        definite: false,
    })
}

/// `this.name = name;`, the assignment that a program writes by hand.
fn assignment(span: swc_common::Span, name: &ast::Ident) -> ast::Stmt {
    let target = ast::MemberExpr {
        span,
        obj: Box::new(ast::Expr::This(ast::ThisExpr { span })),
        prop: ast::MemberProp::Ident(ast::IdentName {
            span: name.span,
            sym: name.sym.clone(),
        }),
    };
    let mut value = name.clone();
    value.optional = false;
    ast::Stmt::Expr(ast::ExprStmt {
        span,
        expr: Box::new(ast::Expr::Assign(ast::AssignExpr {
            span,
            op: ast::AssignOp::Assign,
            left: ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(target)),
            right: Box::new(ast::Expr::Ident(value)),
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_program;
    use crate::SourceFile;

    fn class_of(source: &str) -> ast::Class {
        let program = parse_program(&[SourceFile::entry("main.ts", source)]).expect("parse");
        let ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Class(class))) =
            &program.files[0].module.body[0]
        else {
            panic!("the first item must be a class");
        };
        *class.class.clone()
    }

    fn member_names(class: &ast::Class) -> Vec<String> {
        class
            .body
            .iter()
            .map(|member| match member {
                ast::ClassMember::ClassProp(prop) => match &prop.key {
                    ast::PropName::Ident(key) => format!("field {}", key.sym),
                    _ => "field ?".to_string(),
                },
                ast::ClassMember::Constructor(_) => "constructor".to_string(),
                ast::ClassMember::Method(_) => "method".to_string(),
                _ => "other".to_string(),
            })
            .collect()
    }

    #[test]
    fn lower_module_puts_parameter_property_fields_first_in_parameter_order() {
        let class = class_of(
            "class C { tag: i32 = 1; constructor(public a: i32, b: i32, private readonly c: f64 = 2.0) {} m(): void {} }",
        );
        assert_eq!(
            member_names(&class),
            ["field a", "field c", "field tag", "constructor", "method"]
        );
        let ast::ClassMember::ClassProp(c) = &class.body[1] else {
            panic!("the second member must be a field");
        };
        assert!(c.readonly);
        assert_eq!(c.accessibility, Some(ast::Accessibility::Private));
        assert!(
            c.value.is_none(),
            "a parameter-property field has no initializer"
        );
        assert!(c.type_ann.is_some());
    }

    #[test]
    fn lower_module_keeps_each_parameter_and_assigns_it_before_the_body() {
        let class = class_of(
            "class C { constructor(public a: i32, protected b: i32 = 3) { print(`${a}`); } }",
        );
        let Some(ast::ClassMember::Constructor(ctor)) = class
            .body
            .iter()
            .find(|member| matches!(member, ast::ClassMember::Constructor(_)))
        else {
            panic!("the class must keep its constructor");
        };
        assert!(ctor
            .params
            .iter()
            .all(|p| matches!(p, ast::ParamOrTsParamProp::Param(_))));
        let ast::ParamOrTsParamProp::Param(second) = &ctor.params[1] else {
            panic!("the second parameter must be a plain parameter");
        };
        assert!(
            matches!(second.pat, ast::Pat::Assign(_)),
            "the default stays"
        );
        let body = &ctor.body.as_ref().expect("body").stmts;
        let assigned: Vec<String> = body
            .iter()
            .filter_map(|statement| match statement {
                ast::Stmt::Expr(ast::ExprStmt { expr, .. }) => match expr.as_ref() {
                    ast::Expr::Assign(ast::AssignExpr {
                        left: ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)),
                        right,
                        ..
                    }) => match (&member.prop, right.as_ref()) {
                        (ast::MemberProp::Ident(field), ast::Expr::Ident(value)) => {
                            Some(format!("{}={}", field.sym, value.sym))
                        }
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(assigned, ["a=a", "b=b"]);
        assert_eq!(body.len(), 3, "the two assignments, then the body");
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

    fn rejected(class: &str) -> Vec<crate::Diagnostic> {
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
            let messages = |class: &str| -> Vec<(crate::RuleCode, String)> {
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

    #[test]
    fn parameter_default_gives_the_default_of_a_parameter_property_field() {
        let class = class_of(
            "class C { y: i32 = 1; constructor(public a: i32 = 4, public b: i32, y: i32 = 5) {} }",
        );
        let defaults: Vec<bool> = class
            .body
            .iter()
            .filter_map(|member| match member {
                ast::ClassMember::ClassProp(prop) => {
                    Some(parameter_default(&class, prop).is_some())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            defaults,
            [true, false, false],
            "`a`, `b`, then the declared `y`"
        );
    }

    #[test]
    fn declares_field_holds_only_for_a_parameter_property_field() {
        let class =
            class_of("class C { y: i32; constructor(public x: i32, y: i32) { this.y = y; } }");
        let fields: Vec<bool> = class
            .body
            .iter()
            .filter_map(|member| match member {
                ast::ClassMember::ClassProp(prop) => Some(declares_field(&class, prop)),
                _ => None,
            })
            .collect();
        assert_eq!(
            fields,
            [true, false],
            "the field `x`, then the declared field `y`"
        );
    }

    #[test]
    fn lower_class_leaves_a_class_without_parameter_properties_unchanged() {
        let before = class_of("class C { x: i32 = 1; constructor(y: i32) { this.x = y; } }");
        let mut after = before.clone();
        lower_class(&mut after);
        assert_eq!(member_names(&before), member_names(&after));
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
    }
}
