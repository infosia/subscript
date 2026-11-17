//! Declaration-only checks for ambient sources parsed with normal syntax recovery.
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_parser::error::SyntaxError;

type Failure = (Span, SyntaxError);

pub(super) fn first_error(module: &ast::Module) -> Option<Failure> {
    items(&module.body)
}

fn items(items: &[ast::ModuleItem]) -> Option<Failure> {
    items.iter().find_map(|item| match item {
        ast::ModuleItem::Stmt(ast::Stmt::Decl(value)) => declaration(value),
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(value)) => declaration(&value.decl),
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDefaultDecl(value)) => match &value.decl
        {
            ast::DefaultDecl::Fn(value) => function(&value.function),
            ast::DefaultDecl::Class(value) => class(&value.class),
            _ => None,
        },
        _ => None,
    })
}

fn declaration(value: &ast::Decl) -> Option<Failure> {
    match value {
        ast::Decl::Fn(value) => function(&value.function),
        ast::Decl::Class(value) => class(&value.class),
        ast::Decl::TsModule(value) => value.body.as_ref().and_then(namespace),
        _ => None,
    }
}

fn namespace(value: &ast::TsNamespaceBody) -> Option<Failure> {
    match value {
        ast::TsNamespaceBody::TsModuleBlock(value) => items(&value.body),
        ast::TsNamespaceBody::TsNamespaceDecl(value) => namespace(&value.body),
    }
}

fn parameter(value: &ast::Pat) -> Option<Failure> {
    if matches!(value, ast::Pat::Assign(_)) {
        Some((value.span(), SyntaxError::TS2371))
    } else {
        None
    }
}

fn function(value: &ast::Function) -> Option<Failure> {
    value
        .params
        .iter()
        .find_map(|param| parameter(&param.pat))
        .or_else(|| {
            value
                .body
                .as_ref()
                .map(|body| (body.span, SyntaxError::TS1183))
        })
}

fn class(value: &ast::Class) -> Option<Failure> {
    value.body.iter().find_map(|member| match member {
        ast::ClassMember::Method(value) => function(&value.function),
        ast::ClassMember::PrivateMethod(value) => function(&value.function),
        ast::ClassMember::Constructor(value) => value
            .params
            .iter()
            .find_map(|param| match param {
                ast::ParamOrTsParamProp::Param(value) => parameter(&value.pat),
                ast::ParamOrTsParamProp::TsParamProp(value) => match &value.param {
                    ast::TsParamPropParam::Assign(value) => Some((value.span, SyntaxError::TS2371)),
                    _ => None,
                },
            })
            .or_else(|| {
                value
                    .body
                    .as_ref()
                    .map(|body| (body.span, SyntaxError::TS1183))
            }),
        _ => None,
    })
}
