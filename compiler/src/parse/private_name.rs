//! A private name is a private member (`specs/blocks/compiler.md` §206).
//! The front end lowers each `#x` member declaration to a `private`
//! member with the name `#x`, and each `o.#x` member expression to a
//! member expression with that name. Every later stage reads the form
//! that a `private` member gives.
//!
//! No source identifier starts with `#`, so the name `#x` is distinct
//! from every name that a program can write (§206.1 rule 3).
//!
//! These forms stay as the parser gives them, so that the checker
//! rejects them (§206.1 rule 6): a member of an optional chain
//! (`o?.#x`), a private name as an operand (`#x in o`), and every
//! member of a declaration file (`.d.ts`), which is not lowered.

use swc_ecma_ast as ast;

/// The member name that the private name `#x` gives: `#x`.
pub(crate) fn member_name(name: &ast::PrivateName) -> String {
    format!("#{}", name.name)
}

/// True if `name` is a member name that a private name gives: no
/// source identifier starts with `#`.
pub(crate) fn is_private_name(name: &str) -> bool {
    name.starts_with('#')
}

fn ident_name(name: &ast::PrivateName) -> ast::IdentName {
    ast::IdentName::new(member_name(name).into(), name.span)
}

/// Lowers every private name member and member expression of a
/// program module.
pub(crate) fn lower_module(module: &mut ast::Module) {
    for item in &mut module.body {
        match item {
            ast::ModuleItem::Stmt(stmt) => statement(stmt),
            ast::ModuleItem::ModuleDecl(declaration) => module_decl(declaration),
        }
    }
}

fn module_decl(declaration: &mut ast::ModuleDecl) {
    match declaration {
        ast::ModuleDecl::ExportDecl(export) => decl(&mut export.decl),
        ast::ModuleDecl::ExportDefaultDecl(export) => match &mut export.decl {
            ast::DefaultDecl::Class(class_expr) => class(&mut class_expr.class),
            ast::DefaultDecl::Fn(function_expr) => function(&mut function_expr.function),
            ast::DefaultDecl::TsInterfaceDecl(_) => {}
        },
        ast::ModuleDecl::ExportDefaultExpr(export) => expr(&mut export.expr),
        ast::ModuleDecl::TsExportAssignment(export) => expr(&mut export.expr),
        ast::ModuleDecl::Import(_)
        | ast::ModuleDecl::ExportNamed(_)
        | ast::ModuleDecl::ExportAll(_)
        | ast::ModuleDecl::TsImportEquals(_)
        | ast::ModuleDecl::TsNamespaceExport(_) => {}
    }
}

fn decl(declaration: &mut ast::Decl) {
    match declaration {
        ast::Decl::Class(class_decl) => class(&mut class_decl.class),
        ast::Decl::Fn(function_decl) => function(&mut function_decl.function),
        ast::Decl::Var(var) => vars(&mut var.decls),
        ast::Decl::Using(using) => vars(&mut using.decls),
        ast::Decl::TsEnum(enumeration) => {
            for member in &mut enumeration.members {
                if let Some(init) = &mut member.init {
                    expr(init);
                }
            }
        }
        ast::Decl::TsModule(namespace) => {
            let mut body = namespace.body.as_mut();
            while let Some(current) = body {
                match current {
                    ast::TsNamespaceBody::TsModuleBlock(block) => {
                        for item in &mut block.body {
                            match item {
                                ast::ModuleItem::Stmt(stmt) => statement(stmt),
                                ast::ModuleItem::ModuleDecl(declaration) => {
                                    module_decl(declaration)
                                }
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
        ast::Decl::TsInterface(_) | ast::Decl::TsTypeAlias(_) => {}
    }
}

fn vars(declarators: &mut [ast::VarDeclarator]) {
    for declarator in declarators {
        pat(&mut declarator.name);
        if let Some(init) = &mut declarator.init {
            expr(init);
        }
    }
}

/// §206.1 rule 1: a `#x` member is the `private` member `#x`.
fn class(class: &mut ast::Class) {
    if let Some(super_class) = &mut class.super_class {
        expr(super_class);
    }
    decorators(&mut class.decorators);
    for member in &mut class.body {
        let lowered = match member {
            ast::ClassMember::PrivateProp(prop) => {
                Some(ast::ClassMember::ClassProp(ast::ClassProp {
                    span: prop.span,
                    key: ast::PropName::Ident(ident_name(&prop.key)),
                    value: prop.value.take(),
                    type_ann: prop.type_ann.take(),
                    is_static: prop.is_static,
                    decorators: std::mem::take(&mut prop.decorators),
                    accessibility: Some(ast::Accessibility::Private),
                    is_abstract: false,
                    is_optional: prop.is_optional,
                    is_override: prop.is_override,
                    readonly: prop.readonly,
                    declare: false,
                    definite: prop.definite,
                }))
            }
            ast::ClassMember::PrivateMethod(method) => {
                Some(ast::ClassMember::Method(ast::ClassMethod {
                    span: method.span,
                    key: ast::PropName::Ident(ident_name(&method.key)),
                    function: method.function.clone(),
                    kind: method.kind,
                    is_static: method.is_static,
                    accessibility: Some(ast::Accessibility::Private),
                    is_abstract: method.is_abstract,
                    is_optional: method.is_optional,
                    is_override: method.is_override,
                }))
            }
            _ => None,
        };
        if let Some(lowered) = lowered {
            *member = lowered;
        }
        match member {
            ast::ClassMember::Method(method) => {
                prop_name(&mut method.key);
                function(&mut method.function);
            }
            ast::ClassMember::ClassProp(prop) => {
                prop_name(&mut prop.key);
                decorators(&mut prop.decorators);
                if let Some(value) = &mut prop.value {
                    expr(value);
                }
            }
            ast::ClassMember::Constructor(constructor) => {
                for param in &mut constructor.params {
                    match param {
                        ast::ParamOrTsParamProp::Param(param) => {
                            decorators(&mut param.decorators);
                            pat(&mut param.pat);
                        }
                        ast::ParamOrTsParamProp::TsParamProp(prop) => {
                            decorators(&mut prop.decorators);
                            if let ast::TsParamPropParam::Assign(assign) = &mut prop.param {
                                pat(&mut assign.left);
                                expr(&mut assign.right);
                            }
                        }
                    }
                }
                if let Some(body) = &mut constructor.body {
                    block(body);
                }
            }
            ast::ClassMember::StaticBlock(static_block) => block(&mut static_block.body),
            ast::ClassMember::AutoAccessor(accessor) => {
                if let Some(value) = &mut accessor.value {
                    expr(value);
                }
            }
            ast::ClassMember::PrivateMethod(_)
            | ast::ClassMember::PrivateProp(_)
            | ast::ClassMember::TsIndexSignature(_)
            | ast::ClassMember::Empty(_) => {}
        }
    }
}

fn decorators(decorators: &mut [ast::Decorator]) {
    for decorator in decorators {
        expr(&mut decorator.expr);
    }
}

fn prop_name(name: &mut ast::PropName) {
    if let ast::PropName::Computed(computed) = name {
        expr(&mut computed.expr);
    }
}

fn function(function: &mut ast::Function) {
    decorators(&mut function.decorators);
    for param in &mut function.params {
        decorators(&mut param.decorators);
        pat(&mut param.pat);
    }
    if let Some(body) = &mut function.body {
        block(body);
    }
}

fn block(block: &mut ast::BlockStmt) {
    for stmt in &mut block.stmts {
        statement(stmt);
    }
}

fn statement(stmt: &mut ast::Stmt) {
    match stmt {
        ast::Stmt::Block(body) => block(body),
        ast::Stmt::Decl(declaration) => decl(declaration),
        ast::Stmt::Expr(node) => expr(&mut node.expr),
        ast::Stmt::Return(node) => {
            if let Some(arg) = &mut node.arg {
                expr(arg);
            }
        }
        ast::Stmt::Throw(node) => expr(&mut node.arg),
        ast::Stmt::If(node) => {
            expr(&mut node.test);
            statement(&mut node.cons);
            if let Some(alt) = &mut node.alt {
                statement(alt);
            }
        }
        ast::Stmt::While(node) => {
            expr(&mut node.test);
            statement(&mut node.body);
        }
        ast::Stmt::DoWhile(node) => {
            statement(&mut node.body);
            expr(&mut node.test);
        }
        ast::Stmt::For(node) => {
            match &mut node.init {
                Some(ast::VarDeclOrExpr::VarDecl(var)) => vars(&mut var.decls),
                Some(ast::VarDeclOrExpr::Expr(init)) => expr(init),
                None => {}
            }
            if let Some(test) = &mut node.test {
                expr(test);
            }
            if let Some(update) = &mut node.update {
                expr(update);
            }
            statement(&mut node.body);
        }
        ast::Stmt::ForOf(node) => {
            for_head(&mut node.left);
            expr(&mut node.right);
            statement(&mut node.body);
        }
        ast::Stmt::ForIn(node) => {
            for_head(&mut node.left);
            expr(&mut node.right);
            statement(&mut node.body);
        }
        ast::Stmt::Switch(node) => {
            expr(&mut node.discriminant);
            for case in &mut node.cases {
                if let Some(test) = &mut case.test {
                    expr(test);
                }
                for stmt in &mut case.cons {
                    statement(stmt);
                }
            }
        }
        ast::Stmt::Try(node) => {
            block(&mut node.block);
            if let Some(handler) = &mut node.handler {
                if let Some(param) = &mut handler.param {
                    pat(param);
                }
                block(&mut handler.body);
            }
            if let Some(finalizer) = &mut node.finalizer {
                block(finalizer);
            }
        }
        ast::Stmt::Labeled(node) => statement(&mut node.body),
        ast::Stmt::With(node) => {
            expr(&mut node.obj);
            statement(&mut node.body);
        }
        ast::Stmt::Empty(_)
        | ast::Stmt::Debugger(_)
        | ast::Stmt::Break(_)
        | ast::Stmt::Continue(_) => {}
    }
}

fn for_head(head: &mut ast::ForHead) {
    match head {
        ast::ForHead::VarDecl(var) => vars(&mut var.decls),
        ast::ForHead::UsingDecl(using) => vars(&mut using.decls),
        ast::ForHead::Pat(target) => pat(target),
    }
}

fn pat(target: &mut ast::Pat) {
    match target {
        ast::Pat::Array(array) => array_pat(array),
        ast::Pat::Object(object) => object_pat(object),
        ast::Pat::Assign(assign) => {
            pat(&mut assign.left);
            expr(&mut assign.right);
        }
        ast::Pat::Rest(rest) => pat(&mut rest.arg),
        ast::Pat::Expr(target) => expr(target),
        ast::Pat::Ident(_) | ast::Pat::Invalid(_) => {}
    }
}

fn array_pat(array: &mut ast::ArrayPat) {
    for element in array.elems.iter_mut().flatten() {
        pat(element);
    }
}

fn object_pat(object: &mut ast::ObjectPat) {
    for prop in &mut object.props {
        match prop {
            ast::ObjectPatProp::KeyValue(prop) => {
                prop_name(&mut prop.key);
                pat(&mut prop.value);
            }
            ast::ObjectPatProp::Assign(prop) => {
                if let Some(value) = &mut prop.value {
                    expr(value);
                }
            }
            ast::ObjectPatProp::Rest(rest) => pat(&mut rest.arg),
        }
    }
}

/// §206.1 rule 2: `o.#x` reads the member `#x`.
fn member(member: &mut ast::MemberExpr) {
    expr(&mut member.obj);
    let lowered = match &mut member.prop {
        ast::MemberProp::PrivateName(name) => Some(ast::MemberProp::Ident(ident_name(name))),
        ast::MemberProp::Computed(computed) => {
            expr(&mut computed.expr);
            None
        }
        ast::MemberProp::Ident(_) => None,
    };
    if let Some(lowered) = lowered {
        member.prop = lowered;
    }
}

/// A member of an optional chain keeps its private name: `o?.#x` stays
/// rejected (§206.1 rule 6).
fn chain_member(member: &mut ast::MemberExpr) {
    expr(&mut member.obj);
    if let ast::MemberProp::Computed(computed) = &mut member.prop {
        expr(&mut computed.expr);
    }
}

fn args(arguments: &mut [ast::ExprOrSpread]) {
    for argument in arguments {
        expr(&mut argument.expr);
    }
}

fn expr(e: &mut ast::Expr) {
    match e {
        ast::Expr::Member(target) => member(target),
        ast::Expr::OptChain(chain) => match &mut *chain.base {
            ast::OptChainBase::Member(target) => chain_member(target),
            ast::OptChainBase::Call(call) => {
                expr(&mut call.callee);
                args(&mut call.args);
            }
        },
        ast::Expr::Paren(paren) => expr(&mut paren.expr),
        ast::Expr::Call(call) => {
            if let ast::Callee::Expr(callee) = &mut call.callee {
                expr(callee);
            }
            args(&mut call.args);
        }
        ast::Expr::New(new) => {
            expr(&mut new.callee);
            if let Some(arguments) = &mut new.args {
                args(arguments);
            }
        }
        ast::Expr::Await(target) => expr(&mut target.arg),
        ast::Expr::Yield(target) => {
            if let Some(arg) = &mut target.arg {
                expr(arg);
            }
        }
        ast::Expr::Unary(target) => expr(&mut target.arg),
        ast::Expr::Update(target) => expr(&mut target.arg),
        ast::Expr::Bin(binary) => {
            expr(&mut binary.left);
            expr(&mut binary.right);
        }
        ast::Expr::Cond(conditional) => {
            expr(&mut conditional.test);
            expr(&mut conditional.cons);
            expr(&mut conditional.alt);
        }
        ast::Expr::Assign(assign) => {
            match &mut assign.left {
                ast::AssignTarget::Simple(target) => simple_target(target),
                ast::AssignTarget::Pat(ast::AssignTargetPat::Array(array)) => array_pat(array),
                ast::AssignTarget::Pat(ast::AssignTargetPat::Object(object)) => object_pat(object),
                ast::AssignTarget::Pat(ast::AssignTargetPat::Invalid(_)) => {}
            }
            expr(&mut assign.right);
        }
        ast::Expr::Array(array) => {
            for element in array.elems.iter_mut().flatten() {
                expr(&mut element.expr);
            }
        }
        ast::Expr::Object(object) => {
            for prop in &mut object.props {
                match prop {
                    ast::PropOrSpread::Spread(spread) => expr(&mut spread.expr),
                    ast::PropOrSpread::Prop(prop) => match &mut **prop {
                        ast::Prop::KeyValue(prop) => {
                            prop_name(&mut prop.key);
                            expr(&mut prop.value);
                        }
                        ast::Prop::Assign(prop) => expr(&mut prop.value),
                        ast::Prop::Method(prop) => {
                            prop_name(&mut prop.key);
                            function(&mut prop.function);
                        }
                        ast::Prop::Getter(prop) => {
                            prop_name(&mut prop.key);
                            if let Some(body) = &mut prop.body {
                                block(body);
                            }
                        }
                        ast::Prop::Setter(prop) => {
                            prop_name(&mut prop.key);
                            pat(&mut prop.param);
                            if let Some(body) = &mut prop.body {
                                block(body);
                            }
                        }
                        ast::Prop::Shorthand(_) => {}
                    },
                }
            }
        }
        ast::Expr::Fn(function_expr) => function(&mut function_expr.function),
        ast::Expr::Arrow(arrow) => {
            for param in &mut arrow.params {
                pat(param);
            }
            match &mut *arrow.body {
                ast::BlockStmtOrExpr::BlockStmt(body) => block(body),
                ast::BlockStmtOrExpr::Expr(body) => expr(body),
            }
        }
        ast::Expr::Class(class_expr) => class(&mut class_expr.class),
        ast::Expr::Seq(sequence) => {
            for e in &mut sequence.exprs {
                expr(e);
            }
        }
        ast::Expr::Tpl(template) => {
            for e in &mut template.exprs {
                expr(e);
            }
        }
        ast::Expr::TaggedTpl(tagged) => {
            expr(&mut tagged.tag);
            for e in &mut tagged.tpl.exprs {
                expr(e);
            }
        }
        ast::Expr::TsAs(target) => expr(&mut target.expr),
        ast::Expr::TsTypeAssertion(target) => expr(&mut target.expr),
        ast::Expr::TsSatisfies(target) => expr(&mut target.expr),
        ast::Expr::TsInstantiation(target) => expr(&mut target.expr),
        ast::Expr::TsNonNull(target) => expr(&mut target.expr),
        ast::Expr::TsConstAssertion(target) => expr(&mut target.expr),
        ast::Expr::SuperProp(target) => {
            if let ast::SuperProp::Computed(computed) = &mut target.prop {
                expr(&mut computed.expr);
            }
        }
        ast::Expr::JSXMember(_)
        | ast::Expr::JSXNamespacedName(_)
        | ast::Expr::JSXEmpty(_)
        | ast::Expr::JSXElement(_)
        | ast::Expr::JSXFragment(_)
        | ast::Expr::PrivateName(_)
        | ast::Expr::This(_)
        | ast::Expr::Ident(_)
        | ast::Expr::Lit(_)
        | ast::Expr::MetaProp(_)
        | ast::Expr::Invalid(_) => {}
    }
}

fn simple_target(target: &mut ast::SimpleAssignTarget) {
    match target {
        ast::SimpleAssignTarget::Member(target) => member(target),
        ast::SimpleAssignTarget::SuperProp(target) => {
            if let ast::SuperProp::Computed(computed) = &mut target.prop {
                expr(&mut computed.expr);
            }
        }
        ast::SimpleAssignTarget::Paren(paren) => expr(&mut paren.expr),
        ast::SimpleAssignTarget::OptChain(chain) => match &mut *chain.base {
            ast::OptChainBase::Member(target) => chain_member(target),
            ast::OptChainBase::Call(call) => {
                expr(&mut call.callee);
                args(&mut call.args);
            }
        },
        ast::SimpleAssignTarget::TsAs(target) => expr(&mut target.expr),
        ast::SimpleAssignTarget::TsSatisfies(target) => expr(&mut target.expr),
        ast::SimpleAssignTarget::TsNonNull(target) => expr(&mut target.expr),
        ast::SimpleAssignTarget::TsTypeAssertion(target) => expr(&mut target.expr),
        ast::SimpleAssignTarget::TsInstantiation(target) => expr(&mut target.expr),
        ast::SimpleAssignTarget::Ident(_) | ast::SimpleAssignTarget::Invalid(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use swc_common::{FileName, SourceMap};
    use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

    use super::*;

    /// A private name in each position that the walk visits, then the two
    /// positions that keep it: an optional chain member and an `in` operand.
    const SOURCE: &str = "
class C {
  #a: i32 = 1;
  static #s: i32 = 2;
  b: i32 = this.#a;
  #m(): i32 { return this.#a; }
  get #g(): i32 { return C.#s; }
  set #g(v: i32) { this.#a = v; }
  constructor(p: i32 = C.#s) { this.#a = p; }
  f(o: C): void {
    const k = (): i32 => o.#a + [o.#a][0] + { v: o.#a }.v;
    for (let i = o.#a; i < o.#a; i++) { o.#a += `${o.#m()}`.length; }
    if (o.#a > 0) { o.#a++; } else { switch (o.#a) { case 1: break; } }
    try { throw new Error(`${o.#g}`); } catch (e) { o.#g = o.#a ? 1 : 2; }
    k();
  }
  h(o: C | null): boolean { return o?.#a === 1 && #a in o; }
}
export default class { #d: i32 = 3; e(): i32 { return this.#d; } }
";

    const MARK: &str = "PrivateName {";

    fn parse() -> ast::Module {
        let source_map = SourceMap::default();
        let file = source_map.new_source_file(FileName::Anon.into(), SOURCE.to_string());
        let syntax = Syntax::Typescript(TsSyntax {
            decorators: true,
            ..TsSyntax::default()
        });
        let lexer = Lexer::new(
            syntax,
            ast::EsVersion::Es2022,
            StringInput::from(&*file),
            None,
        );
        Parser::new_from(lexer).parse_module().expect("parses")
    }

    #[test]
    fn the_walk_lowers_each_private_name_outside_a_chain_and_an_in_operand() {
        let mut module = parse();
        // Control: the parser gives a private name at every position.
        assert!(format!("{module:?}").matches(MARK).count() > 20);
        lower_module(&mut module);
        let lowered = format!("{module:?}");
        assert_eq!(lowered.matches(MARK).count(), 2, "{lowered}");
        assert!(lowered.contains("sym: \"#a\""));
        assert!(lowered.contains("sym: \"#d\""));
    }
}
