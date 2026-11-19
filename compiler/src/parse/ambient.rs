//! Declaration-only checks for ambient sources (compiler.md §341 rule 2).
use oxc_ast::ast;
use oxc_span::Span;

type Failure = (Span, &'static str);

pub(super) fn first_error(module: &ast::Program<'_>) -> Option<Failure> {
    items(&module.body)
}

fn items(statements: &[ast::Statement<'_>]) -> Option<Failure> {
    statements.iter().find_map(|statement| match statement {
        ast::Statement::FunctionDeclaration(value) => function(value),
        ast::Statement::ClassDeclaration(value) => class(value),
        ast::Statement::TSNamespaceDeclaration(value) => namespace(value),
        ast::Statement::TSExternalModuleDeclaration(value) => {
            value.body.as_ref().and_then(|body| items(&body.body))
        }
        ast::Statement::TSGlobalDeclaration(value) => items(&value.body.body),
        ast::Statement::ExportDeclaration(value) => declaration(&value.declaration),
        ast::Statement::ExportDefaultDeclaration(value) => match &value.declaration {
            ast::ExportDefaultDeclarationKind::FunctionDeclaration(value) => function(value),
            ast::ExportDefaultDeclarationKind::ClassDeclaration(value) => class(value),
            _ => None,
        },
        _ => None,
    })
}
fn declaration(value: &ast::Declaration<'_>) -> Option<Failure> {
    match value {
        ast::Declaration::FunctionDeclaration(value) => function(value),
        ast::Declaration::ClassDeclaration(value) => class(value),
        ast::Declaration::TSNamespaceDeclaration(value) => namespace(value),
        ast::Declaration::TSExternalModuleDeclaration(value) => {
            value.body.as_ref().and_then(|body| items(&body.body))
        }
        ast::Declaration::TSGlobalDeclaration(value) => items(&value.body.body),
        _ => None,
    }
}
fn namespace(value: &ast::TSNamespaceDeclaration<'_>) -> Option<Failure> {
    match &value.body {
        ast::TSNamespaceDeclarationBody::TSModuleBlock(body) => items(&body.body),
        ast::TSNamespaceDeclarationBody::TSNamespaceDeclaration(value) => namespace(value),
    }
}
fn function(value: &ast::Function<'_>) -> Option<Failure> {
    value.params.items.iter().find_map(|parameter| parameter.initializer.as_ref().map(|_| (parameter.span, "A parameter initializer is only allowed in a function or constructor implementation.")))
        .or_else(|| value.body.as_ref().map(|body| (body.span, "An implementation cannot be declared in ambient contexts.")))
}
fn class(value: &ast::Class<'_>) -> Option<Failure> {
    value.body.body.iter().find_map(|member| match member {
        ast::ClassElement::MethodDefinition(value) => function(&value.value),
        _ => None,
    })
}
