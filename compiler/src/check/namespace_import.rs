//! Resolves namespace qualifiers to imported declaration bindings (compiler.md §341 rule 5).
use super::*;
use oxc_allocator::Box as ArenaBox;
use oxc_ast_visit::{walk_mut, VisitMut};

impl<'p> Checker<'p> {
    fn namespace_member_ident(
        &mut self,
        namespace: &ast::IdentifierReference<'_>,
        member: &ast::IdentifierName<'_>,
    ) -> Option<ast::IdentifierReference<'p>> {
        let ScopeItem::Namespace { module, source } =
            self.type_scope_item(namespace.name.as_str())?
        else {
            return None;
        };
        let item =
            module.and_then(|module| self.exports[module].get(member.name.as_str()).cloned());
        let item = item.unwrap_or_else(|| {
            if module.is_some() {
                self.resolution_error(
                    if member.name == "default" {
                        RejectionSite::NamespaceUnexportedMember
                    } else {
                        RejectionSite::NamespaceMemberMissing
                    },
                    format!("`{}` is not exported by `{source}`", member.name),
                    self.pos(member.span),
                );
            }
            ScopeItem::Poisoned
        });
        let key = format!("{}.{}", namespace.name, member.name);
        self.file_scopes[self.cur_file].insert(
            key.clone(),
            ScopeBinding {
                item,
                imported: true,
                type_only: false,
            },
        );
        Some(ast::IdentifierReference::new(
            member.span,
            self.prog.allocator.alloc_str(&key),
            &oxc_ast::builder::AstBuilder::new(self.prog.allocator),
        ))
    }

    pub(super) fn resolve_namespace_program(&mut self, program: &mut ParsedProgram<'p>) {
        for (file, source) in program.files.iter_mut().enumerate() {
            self.cur_file = file;
            let mut resolver = Resolver {
                checker: self,
                scopes: Vec::new(),
            };
            for statement in &mut source.module.body {
                if let ast::Statement::ExportDeclaration(export) = statement {
                    resolver.visit_declaration(&mut export.declaration);
                } else if !statement.is_module_declaration() {
                    resolver.visit_statement(statement);
                }
            }
        }
        // Templates retain the resolved declarations (compiler.md §341 rule 5).
        for (file, source) in program.files.iter().enumerate() {
            for item in &source.module.body {
                match module_decl(item) {
                    Some(ast::Declaration::FunctionDeclaration(function)) => {
                        let symbol = self.declaration_symbol(file, function.declaration_name());
                        if let Some(template) = self.generic_fns.get_mut(&symbol) {
                            template.function =
                                Rc::new(function.as_ref().clone_in(program.allocator));
                        }
                    }
                    Some(ast::Declaration::ClassDeclaration(class)) => {
                        let symbol = self.declaration_symbol(file, class.declaration_name());
                        if let Some(template) = self.generic_classes.get_mut(&symbol) {
                            template.class = Rc::new(class.as_ref().clone_in(program.allocator));
                            for ((name, is_static), method) in
                                &mut template.rejected_generic_methods
                            {
                                if let Some(resolved) = class.body.body.iter().find_map(|member| match member {
                                    ast::ClassElement::MethodDefinition(method) if method.kind != ast::MethodDefinitionKind::Constructor && method.r#static == *is_static && matches!(&method.key, ast::PropertyKey::StaticIdentifier(key) if Some(key.name.to_string()) == *name) => Some(method),
                                    _ => None,
                                }) { method.function = Rc::new(resolved.value.as_ref().clone_in(program.allocator)); }
                            }
                        }
                    }
                    Some(ast::Declaration::TSInterfaceDeclaration(interface)) => {
                        self.replace_interface_declaration(file, interface)
                    }
                    _ => {}
                }
            }
        }
    }
}

struct Resolver<'s, 'p> {
    checker: &'s mut Checker<'p>,
    scopes: Vec<HashSet<String>>,
}
impl<'p> Resolver<'_, 'p> {
    fn shadowed(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
    fn member(
        &mut self,
        member: &ast::StaticMemberExpression<'p>,
    ) -> Option<ast::IdentifierReference<'p>> {
        if let ast::Expression::Identifier(namespace) = super::expr::unparen_expr(&member.object) {
            if !self.shadowed(namespace.name.as_str()) {
                return self
                    .checker
                    .namespace_member_ident(namespace, &member.property);
            }
        }
        None
    }
    fn type_name(&mut self, name: &mut ast::TSTypeName<'p>) {
        if let ast::TSTypeName::QualifiedName(qualified) = name {
            if let ast::TSTypeName::IdentifierReference(namespace) = &qualified.left {
                if let Some(ident) = self
                    .checker
                    .namespace_member_ident(namespace, &qualified.right)
                {
                    *name = ast::TSTypeName::IdentifierReference(ArenaBox::new_in(
                        ident,
                        &self.checker.prog.allocator,
                    ));
                }
            }
        }
    }
    fn block(&mut self, statements: &mut [ast::Statement<'p>]) {
        let mut names = HashSet::new();
        for statement in statements.iter() {
            if let Some(declaration) = statement.as_declaration() {
                decl_names(declaration, &mut names);
            }
        }
        self.scopes.push(names);
        for statement in statements {
            self.visit_statement(statement);
        }
        self.scopes.pop();
    }
    fn function(&mut self, function: &mut ast::Function<'p>) {
        let mut names = HashSet::new();
        for parameter in &function.params.items {
            pat_names(&parameter.pattern, &mut names);
        }
        if function.this_param.is_some() {
            names.insert("this".to_owned());
        }
        if let Some(rest) = &function.params.rest {
            pat_names(&rest.rest.argument, &mut names);
        }
        self.scopes.push(names);
        if let Some(params) = &mut function.type_parameters {
            self.visit_ts_type_parameter_declaration(params);
        }
        if let Some(this) = &mut function.this_param {
            self.visit_ts_this_parameter(this);
        }
        if let Some(ty) = &mut function.return_type {
            self.visit_ts_type_annotation(ty);
        }
        self.visit_formal_parameters(&mut function.params);
        if let Some(body) = &mut function.body {
            self.block(&mut body.statements);
        }
        self.scopes.pop();
    }
}
impl<'p> VisitMut<'p> for Resolver<'_, 'p> {
    fn visit_statement(&mut self, statement: &mut ast::Statement<'p>) {
        match statement {
            ast::Statement::FunctionDeclaration(function) => self.function(function),
            ast::Statement::BlockStatement(block) => self.block(&mut block.body),
            ast::Statement::ForStatement(node) => {
                let mut names = HashSet::new();
                if let Some(ast::ForStatementInit::VariableDeclaration(var)) = &node.init {
                    for declaration in &var.declarations {
                        pat_names(&declaration.id, &mut names);
                    }
                }
                self.scopes.push(names);
                walk_mut::walk_for_statement(self, node);
                self.scopes.pop();
            }
            ast::Statement::ForInStatement(node) => {
                self.visit_expression(&mut node.right);
                self.scopes.push(head_names(&node.left));
                self.visit_for_statement_left(&mut node.left);
                self.visit_statement(&mut node.body);
                self.scopes.pop();
            }
            ast::Statement::ForOfStatement(node) => {
                self.visit_expression(&mut node.right);
                self.scopes.push(head_names(&node.left));
                self.visit_for_statement_left(&mut node.left);
                self.visit_statement(&mut node.body);
                self.scopes.pop();
            }
            ast::Statement::SwitchStatement(node) => {
                self.visit_expression(&mut node.discriminant);
                let mut names = HashSet::new();
                for case in &node.cases {
                    for statement in &case.consequent {
                        if let Some(decl) = statement.as_declaration() {
                            decl_names(decl, &mut names);
                        }
                    }
                }
                self.scopes.push(names);
                for case in &mut node.cases {
                    if let Some(test) = &mut case.test {
                        self.visit_expression(test);
                    }
                    for statement in &mut case.consequent {
                        self.visit_statement(statement);
                    }
                }
                self.scopes.pop();
            }
            declaration if declaration.as_declaration().is_some() => {
                if let Some(declaration) = declaration.as_declaration_mut() {
                    self.visit_declaration(declaration);
                }
            }
            _ => walk_mut::walk_statement(self, statement),
        }
    }
    fn visit_declaration(&mut self, declaration: &mut ast::Declaration<'p>) {
        match declaration {
            ast::Declaration::FunctionDeclaration(function) => self.function(function),
            ast::Declaration::TSNamespaceDeclaration(_)
            | ast::Declaration::TSExternalModuleDeclaration(_)
            | ast::Declaration::TSGlobalDeclaration(_) => {}
            ast::Declaration::TSInterfaceDeclaration(interface) => {
                for element in &mut interface.body.body {
                    if let ast::TSSignature::TSMethodSignature(method) = element {
                        if method.kind == ast::TSMethodSignatureKind::Method {
                            if let Some(this) = &mut method.this_param {
                                self.visit_ts_this_parameter(this);
                            }
                            for parameter in &mut method.params.items {
                                if matches!(
                                    parameter.pattern,
                                    ast::BindingPattern::BindingIdentifier(_)
                                ) {
                                    if let Some(ty) = &mut parameter.type_annotation {
                                        self.visit_ts_type_annotation(ty);
                                    }
                                }
                            }
                            if let Some(ty) = &mut method.return_type {
                                self.visit_ts_type_annotation(ty);
                            }
                        } else if method.kind == ast::TSMethodSignatureKind::Get {
                            if let Some(ty) = &mut method.return_type {
                                self.visit_ts_type_annotation(ty);
                            }
                        }
                    }
                }
            }
            _ => walk_mut::walk_declaration(self, declaration),
        }
    }
    fn visit_catch_clause(&mut self, catch: &mut ast::CatchClause<'p>) {
        let mut names = HashSet::new();
        if let Some(param) = &catch.param {
            pat_names(&param.pattern, &mut names);
        }
        self.scopes.push(names);
        walk_mut::walk_catch_clause(self, catch);
        self.scopes.pop();
    }
    fn visit_expression(&mut self, expression: &mut ast::Expression<'p>) {
        let alias = match expression {
            ast::Expression::StaticMemberExpression(member) => self.member(member),
            ast::Expression::ChainExpression(chain) => match &chain.expression {
                ast::ChainElement::StaticMemberExpression(member) => self.member(member),
                _ => None,
            },
            _ => None,
        };
        if let Some(alias) = alias {
            *expression =
                ast::Expression::Identifier(ArenaBox::new_in(alias, &self.checker.prog.allocator));
            return;
        }
        match expression {
            ast::Expression::FunctionExpression(function) => {
                let names = function.id.iter().map(|id| id.name.to_string()).collect();
                self.scopes.push(names);
                self.function(function);
                self.scopes.pop();
            }
            ast::Expression::ArrowFunctionExpression(arrow) => {
                let mut names = HashSet::new();
                for parameter in &arrow.params.items {
                    pat_names(&parameter.pattern, &mut names);
                }
                if let Some(rest) = &arrow.params.rest {
                    pat_names(&rest.rest.argument, &mut names);
                }
                self.scopes.push(names);
                if let Some(params) = &mut arrow.type_parameters {
                    self.visit_ts_type_parameter_declaration(params);
                }
                if let Some(ty) = &mut arrow.return_type {
                    self.visit_ts_type_annotation(ty);
                }
                self.visit_formal_parameters(&mut arrow.params);
                match &mut arrow.body {
                    ast::ArrowFunctionBody::FunctionBody(body) => self.block(&mut body.statements),
                    body => {
                        if let Some(expr) = body.as_expression_mut() {
                            self.visit_expression(expr);
                        }
                    }
                }
                self.scopes.pop();
            }
            _ => walk_mut::walk_expression(self, expression),
        }
    }
    fn visit_assignment_target(&mut self, target: &mut ast::AssignmentTarget<'p>) {
        if let ast::AssignmentTarget::StaticMemberExpression(member) = target {
            if let Some(alias) = self.member(member) {
                *target = ast::AssignmentTarget::AssignmentTargetIdentifier(ArenaBox::new_in(
                    alias,
                    &self.checker.prog.allocator,
                ));
                return;
            }
        }
        walk_mut::walk_assignment_target(self, target);
    }
    fn visit_simple_assignment_target(&mut self, target: &mut ast::SimpleAssignmentTarget<'p>) {
        if let ast::SimpleAssignmentTarget::StaticMemberExpression(member) = target {
            if let Some(alias) = self.member(member) {
                *target = ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(
                    ArenaBox::new_in(alias, &self.checker.prog.allocator),
                );
                return;
            }
        }
        walk_mut::walk_simple_assignment_target(self, target);
    }
    fn visit_class(&mut self, class: &mut ast::Class<'p>) {
        if let Some(params) = &mut class.type_parameters {
            self.visit_ts_type_parameter_declaration(params);
        }
        if let Some(heritage) = &mut class.heritage {
            self.visit_class_heritage(heritage);
        }
        for clause in &mut class.implements {
            self.type_name(&mut clause.expression);
            if let Some(args) = &mut clause.type_arguments {
                self.visit_ts_type_parameter_instantiation(args);
            }
        }
        self.visit_decorators(&mut class.decorators);
        for member in &mut class.body.body {
            match member {
                ast::ClassElement::MethodDefinition(method) => {
                    if method.computed {
                        self.visit_property_key(&mut method.key);
                    }
                    self.visit_decorators(&mut method.decorators);
                    self.function(&mut method.value);
                }
                ast::ClassElement::PropertyDefinition(property) => {
                    if property.computed {
                        self.visit_property_key(&mut property.key);
                    }
                    if let Some(ty) = &mut property.type_annotation {
                        self.visit_ts_type_annotation(ty);
                    }
                    if let Some(value) = &mut property.value {
                        self.visit_expression(value);
                    }
                    if !matches!(property.key, ast::PropertyKey::PrivateIdentifier(_)) {
                        self.visit_decorators(&mut property.decorators);
                    }
                }
                ast::ClassElement::StaticBlock(block) => self.block(&mut block.body),
                ast::ClassElement::TSIndexSignature(signature) => {
                    self.visit_ts_index_signature(signature)
                }
                _ => {}
            }
        }
    }
    fn visit_ts_type(&mut self, ty: &mut ast::TSType<'p>) {
        match ty {
            ast::TSType::TSTypeReference(reference) => {
                self.type_name(&mut reference.type_name);
                if let Some(args) = &mut reference.type_arguments {
                    self.visit_ts_type_parameter_instantiation(args);
                }
            }
            ast::TSType::TSTypeQuery(query) => {
                if let Some(args) = &mut query.type_arguments {
                    self.visit_ts_type_parameter_instantiation(args);
                }
            }
            _ => walk_mut::walk_ts_type(self, ty),
        }
    }
}
fn pat_names(pattern: &ast::BindingPattern<'_>, names: &mut HashSet<String>) {
    for binding in pattern::collect_names(pattern) {
        names.insert(binding.name.to_string());
    }
}
fn decl_names(declaration: &ast::Declaration<'_>, names: &mut HashSet<String>) {
    if !matches!(
        declaration,
        ast::Declaration::TSNamespaceDeclaration(_)
            | ast::Declaration::TSExternalModuleDeclaration(_)
            | ast::Declaration::TSGlobalDeclaration(_)
    ) {
        for binding in exports::declaration_names(declaration) {
            names.insert(binding.name.to_string());
        }
    }
}
fn head_names(head: &ast::ForStatementLeft<'_>) -> HashSet<String> {
    let mut names = HashSet::new();
    if let ast::ForStatementLeft::VariableDeclaration(var) = head {
        for declaration in &var.declarations {
            pat_names(&declaration.id, &mut names);
        }
    } else if let Some(target) = head.as_assignment_target() {
        struct Names<'s>(&'s mut HashSet<String>);
        impl<'a> oxc_ast_visit::Visit<'a> for Names<'_> {
            fn visit_identifier_reference(&mut self, ident: &ast::IdentifierReference<'a>) {
                self.0.insert(ident.name.to_string());
            }
            fn visit_expression(&mut self, _: &ast::Expression<'a>) {}
            fn visit_member_expression(&mut self, _: &ast::MemberExpression<'a>) {}
        }
        oxc_ast_visit::Visit::visit_assignment_target(&mut Names(&mut names), target);
    }
    names
}
