//! Resolves namespace qualifiers to ordinary imported declaration bindings.

use super::*;

impl<'p> Checker<'p> {
    /// Creates a private lookup key for the exported declaration identity.
    /// The key is checker-only; HIR retains the declaration's existing symbol.
    fn namespace_member_ident(
        &mut self,
        namespace: &ast::Ident,
        member: &ast::IdentName,
    ) -> Option<ast::Ident> {
        let ScopeItem::Namespace { module, source } =
            self.type_scope_item(namespace.sym.as_ref())?
        else {
            return None;
        };
        let item = module.and_then(|module| self.exports[module].get(member.sym.as_ref()).cloned());
        let item = item.unwrap_or_else(|| {
            if module.is_some() {
                self.resolution_error(
                    RuleCode::S016,
                    format!("`{}` is not exported by `{source}`", member.sym),
                    self.pos(member.span),
                );
            }
            ScopeItem::Poisoned
        });
        let key = format!("{}.{}", namespace.sym, member.sym);
        self.file_scopes[self.cur_file].insert(
            key.clone(),
            ScopeBinding {
                item,
                imported: true,
                type_only: false,
            },
        );
        Some(ast::Ident::new_no_ctxt(key.into(), member.span))
    }

    /// Resolves the program before signatures and bodies consume its declarations.
    pub(super) fn resolve_namespace_program(&mut self, program: &mut ParsedProgram) {
        for (file, source) in program.files.iter_mut().enumerate() {
            self.cur_file = file;
            let mut resolver = Resolver {
                checker: self,
                scopes: Vec::new(),
            };
            for item in &mut source.module.body {
                match item {
                    ast::ModuleItem::Stmt(stmt) => resolver.stmt(stmt),
                    ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
                        resolver.decl(&mut export.decl)
                    }
                    _ => {}
                }
            }
        }
        // Pass A stores generic templates. Replace those copies with resolved declarations.
        for (file, source) in program.files.iter().enumerate() {
            for item in &source.module.body {
                match module_decl(item) {
                    Some(ast::Decl::Fn(decl)) => {
                        let symbol = self.declaration_symbol(file, decl.ident.sym.as_ref());
                        if let Some(template) = self.generic_fns.get_mut(&symbol) {
                            template.function = (*decl.function).clone();
                        }
                    }
                    Some(ast::Decl::Class(decl)) => {
                        let symbol = self.declaration_symbol(file, decl.ident.sym.as_ref());
                        if let Some(template) = self.generic_classes.get_mut(&symbol) {
                            template.class = (*decl.class).clone();
                            for ((name, is_static), method) in
                                &mut template.rejected_generic_methods
                            {
                                if let Some(ast::ClassMember::Method(resolved)) = decl.class.body.iter().find(|member| {
                                    matches!(member, ast::ClassMember::Method(m) if m.is_static == *is_static &&
                                        matches!(&m.key, ast::PropName::Ident(key) if Some(key.sym.to_string()) == *name))
                                }) { method.function = (*resolved.function).clone(); }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

struct Resolver<'a, 'p> {
    checker: &'a mut Checker<'p>,
    scopes: Vec<HashSet<String>>,
}

impl Resolver<'_, '_> {
    fn shadowed(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn member(&mut self, member: &mut ast::MemberExpr) -> Option<ast::Ident> {
        if let (ast::Expr::Ident(namespace), ast::MemberProp::Ident(name)) =
            (super::expr::unparen_expr(&member.obj), &member.prop)
        {
            if !self.shadowed(namespace.sym.as_ref()) {
                if let Some(ident) = self.checker.namespace_member_ident(namespace, name) {
                    return Some(ident);
                }
            }
        }
        self.expr(&mut member.obj);
        if let ast::MemberProp::Computed(prop) = &mut member.prop {
            self.expr(&mut prop.expr);
        }
        None
    }

    fn ann(&mut self, ann: &mut Option<Box<ast::TsTypeAnn>>) {
        if let Some(ann) = ann {
            self.ty(&mut ann.type_ann);
        }
    }

    fn args(&mut self, args: &mut Option<Box<ast::TsTypeParamInstantiation>>) {
        if let Some(args) = args {
            for ty in &mut args.params {
                self.ty(ty);
            }
        }
    }

    fn params(&mut self, params: &mut Option<Box<ast::TsTypeParamDecl>>) {
        if let Some(params) = params {
            for param in &mut params.params {
                if let Some(ty) = &mut param.constraint {
                    self.ty(ty);
                }
                if let Some(ty) = &mut param.default {
                    self.ty(ty);
                }
            }
        }
    }

    fn pat(&mut self, pat: &mut ast::Pat) {
        match pat {
            ast::Pat::Ident(p) => self.ann(&mut p.type_ann),
            ast::Pat::Array(p) => {
                self.ann(&mut p.type_ann);
                for p in p.elems.iter_mut().flatten() {
                    self.pat(p);
                }
            }
            ast::Pat::Object(p) => {
                self.ann(&mut p.type_ann);
                for prop in &mut p.props {
                    match prop {
                        ast::ObjectPatProp::KeyValue(p) => {
                            self.key(&mut p.key);
                            self.pat(&mut p.value);
                        }
                        ast::ObjectPatProp::Assign(p) => {
                            if let Some(e) = &mut p.value {
                                self.expr(e);
                            }
                        }
                        ast::ObjectPatProp::Rest(p) => self.pat(&mut p.arg),
                    }
                }
            }
            ast::Pat::Assign(p) => {
                self.pat(&mut p.left);
                self.expr(&mut p.right);
            }
            ast::Pat::Rest(p) => {
                self.ann(&mut p.type_ann);
                self.pat(&mut p.arg);
            }
            ast::Pat::Expr(e) => self.expr(e),
            ast::Pat::Invalid(_) => {}
        }
    }

    fn key(&mut self, key: &mut ast::PropName) {
        if let ast::PropName::Computed(key) = key {
            self.expr(&mut key.expr);
        }
    }

    fn block(&mut self, block: &mut ast::BlockStmt) {
        let mut names = HashSet::new();
        for stmt in &block.stmts {
            if let ast::Stmt::Decl(decl) = stmt {
                decl_names(decl, &mut names);
            }
        }
        self.scopes.push(names);
        for stmt in &mut block.stmts {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn function(&mut self, function: &mut ast::Function) {
        let mut names = HashSet::new();
        for param in &function.params {
            pat_names(&param.pat, &mut names);
        }
        self.scopes.push(names);
        self.params(&mut function.type_params);
        self.ann(&mut function.return_type);
        for decorator in &mut function.decorators {
            self.expr(&mut decorator.expr);
        }
        for param in &mut function.params {
            self.pat(&mut param.pat);
        }
        if let Some(body) = &mut function.body {
            self.block(body);
        }
        self.scopes.pop();
    }

    fn decl(&mut self, decl: &mut ast::Decl) {
        match decl {
            ast::Decl::Fn(f) => self.function(&mut f.function),
            ast::Decl::Class(c) => self.class(&mut c.class),
            ast::Decl::Var(v) => self.vars(&mut v.decls),
            ast::Decl::Using(v) => self.vars(&mut v.decls),
            ast::Decl::TsTypeAlias(t) => {
                self.params(&mut t.type_params);
                self.ty(&mut t.type_ann);
            }
            ast::Decl::TsEnum(e) => {
                for m in &mut e.members {
                    if let Some(e) = &mut m.init {
                        self.expr(e);
                    }
                }
            }
            // Interfaces and TS namespaces have no accepted bodies.
            ast::Decl::TsInterface(_) | ast::Decl::TsModule(_) => {}
        }
    }

    fn vars(&mut self, vars: &mut [ast::VarDeclarator]) {
        for var in vars {
            self.pat(&mut var.name);
            if let Some(e) = &mut var.init {
                self.expr(e);
            }
        }
    }

    fn class(&mut self, class: &mut ast::Class) {
        self.params(&mut class.type_params);
        self.args(&mut class.super_type_params);
        if let Some(e) = &mut class.super_class {
            self.expr(e);
        }
        for d in &mut class.decorators {
            self.expr(&mut d.expr);
        }
        for member in &mut class.body {
            match member {
                ast::ClassMember::Method(m) => {
                    self.key(&mut m.key);
                    self.function(&mut m.function);
                }
                ast::ClassMember::PrivateMethod(m) => self.function(&mut m.function),
                ast::ClassMember::ClassProp(p) => {
                    self.key(&mut p.key);
                    self.ann(&mut p.type_ann);
                    if let Some(e) = &mut p.value {
                        self.expr(e);
                    }
                    for d in &mut p.decorators {
                        self.expr(&mut d.expr);
                    }
                }
                ast::ClassMember::PrivateProp(p) => {
                    self.ann(&mut p.type_ann);
                    if let Some(e) = &mut p.value {
                        self.expr(e);
                    }
                }
                ast::ClassMember::Constructor(c) => {
                    let mut names = HashSet::new();
                    for p in &c.params {
                        match p {
                            ast::ParamOrTsParamProp::Param(p) => pat_names(&p.pat, &mut names),
                            ast::ParamOrTsParamProp::TsParamProp(p) => match &p.param {
                                ast::TsParamPropParam::Ident(i) => {
                                    names.insert(i.id.sym.to_string());
                                }
                                ast::TsParamPropParam::Assign(p) => pat_names(&p.left, &mut names),
                            },
                        }
                    }
                    self.scopes.push(names);
                    for p in &mut c.params {
                        match p {
                            ast::ParamOrTsParamProp::Param(p) => self.pat(&mut p.pat),
                            ast::ParamOrTsParamProp::TsParamProp(p) => match &mut p.param {
                                ast::TsParamPropParam::Ident(i) => self.ann(&mut i.type_ann),
                                ast::TsParamPropParam::Assign(p) => {
                                    self.pat(&mut p.left);
                                    self.expr(&mut p.right);
                                }
                            },
                        }
                    }
                    if let Some(body) = &mut c.body {
                        self.block(body);
                    }
                    self.scopes.pop();
                }
                ast::ClassMember::TsIndexSignature(i) => {
                    for param in &mut i.params {
                        self.fn_param(param);
                    }
                    self.ann(&mut i.type_ann);
                }
                ast::ClassMember::StaticBlock(b) => self.block(&mut b.body),
                ast::ClassMember::AutoAccessor(_) | ast::ClassMember::Empty(_) => {}
            }
        }
    }

    fn head(&mut self, head: &mut ast::ForHead) {
        match head {
            ast::ForHead::VarDecl(v) => self.vars(&mut v.decls),
            ast::ForHead::UsingDecl(v) => self.vars(&mut v.decls),
            ast::ForHead::Pat(p) => self.pat(p),
        }
    }

    fn stmt(&mut self, stmt: &mut ast::Stmt) {
        match stmt {
            ast::Stmt::Block(b) => self.block(b),
            ast::Stmt::Decl(d) => self.decl(d),
            ast::Stmt::Expr(e) => self.expr(&mut e.expr),
            ast::Stmt::Return(r) => {
                if let Some(e) = &mut r.arg {
                    self.expr(e);
                }
            }
            ast::Stmt::Throw(t) => self.expr(&mut t.arg),
            ast::Stmt::If(i) => {
                self.expr(&mut i.test);
                self.stmt(&mut i.cons);
                if let Some(s) = &mut i.alt {
                    self.stmt(s);
                }
            }
            ast::Stmt::While(w) => {
                self.expr(&mut w.test);
                self.stmt(&mut w.body);
            }
            ast::Stmt::DoWhile(w) => {
                self.stmt(&mut w.body);
                self.expr(&mut w.test);
            }
            ast::Stmt::For(f) => {
                let mut names = HashSet::new();
                if let Some(ast::VarDeclOrExpr::VarDecl(v)) = &f.init {
                    for d in &v.decls {
                        pat_names(&d.name, &mut names);
                    }
                }
                self.scopes.push(names);
                if let Some(init) = &mut f.init {
                    match init {
                        ast::VarDeclOrExpr::VarDecl(v) => self.vars(&mut v.decls),
                        ast::VarDeclOrExpr::Expr(e) => self.expr(e),
                    }
                }
                if let Some(e) = &mut f.test {
                    self.expr(e);
                }
                if let Some(e) = &mut f.update {
                    self.expr(e);
                }
                self.stmt(&mut f.body);
                self.scopes.pop();
            }
            ast::Stmt::ForOf(f) => {
                self.expr(&mut f.right);
                self.scopes.push(head_names(&f.left));
                self.head(&mut f.left);
                self.stmt(&mut f.body);
                self.scopes.pop();
            }
            ast::Stmt::ForIn(f) => {
                self.expr(&mut f.right);
                self.scopes.push(head_names(&f.left));
                self.head(&mut f.left);
                self.stmt(&mut f.body);
                self.scopes.pop();
            }
            ast::Stmt::Switch(s) => {
                self.expr(&mut s.discriminant);
                let mut names = HashSet::new();
                for c in &s.cases {
                    for s in &c.cons {
                        if let ast::Stmt::Decl(d) = s {
                            decl_names(d, &mut names);
                        }
                    }
                }
                self.scopes.push(names);
                for c in &mut s.cases {
                    if let Some(e) = &mut c.test {
                        self.expr(e);
                    }
                    for s in &mut c.cons {
                        self.stmt(s);
                    }
                }
                self.scopes.pop();
            }
            ast::Stmt::Try(t) => {
                self.block(&mut t.block);
                if let Some(c) = &mut t.handler {
                    let mut names = HashSet::new();
                    if let Some(p) = &c.param {
                        pat_names(p, &mut names);
                    }
                    self.scopes.push(names);
                    if let Some(p) = &mut c.param {
                        self.pat(p);
                    }
                    self.block(&mut c.body);
                    self.scopes.pop();
                }
                if let Some(b) = &mut t.finalizer {
                    self.block(b);
                }
            }
            ast::Stmt::Labeled(s) => self.stmt(&mut s.body),
            ast::Stmt::With(s) => {
                self.expr(&mut s.obj);
                self.stmt(&mut s.body);
            }
            ast::Stmt::Empty(_)
            | ast::Stmt::Debugger(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_) => {}
        }
    }

    fn expr(&mut self, expr: &mut ast::Expr) {
        match expr {
            ast::Expr::Member(m) => {
                if let Some(i) = self.member(m) {
                    *expr = ast::Expr::Ident(i);
                }
            }
            ast::Expr::Paren(p) => self.expr(&mut p.expr),
            ast::Expr::Call(c) => {
                if let ast::Callee::Expr(e) = &mut c.callee {
                    self.expr(e);
                }
                self.args(&mut c.type_args);
                for a in &mut c.args {
                    self.expr(&mut a.expr);
                }
            }
            ast::Expr::New(n) => {
                self.expr(&mut n.callee);
                self.args(&mut n.type_args);
                if let Some(args) = &mut n.args {
                    for a in args {
                        self.expr(&mut a.expr);
                    }
                }
            }
            ast::Expr::Await(a) => self.expr(&mut a.arg),
            ast::Expr::Yield(y) => {
                if let Some(e) = &mut y.arg {
                    self.expr(e);
                }
            }
            ast::Expr::Unary(u) => self.expr(&mut u.arg),
            ast::Expr::Update(u) => self.expr(&mut u.arg),
            ast::Expr::Bin(b) => {
                self.expr(&mut b.left);
                self.expr(&mut b.right);
            }
            ast::Expr::Cond(c) => {
                self.expr(&mut c.test);
                self.expr(&mut c.cons);
                self.expr(&mut c.alt);
            }
            ast::Expr::Assign(a) => {
                match &mut a.left {
                    ast::AssignTarget::Simple(target) => {
                        let mut e: Box<ast::Expr> = target.clone().into();
                        self.expr(&mut e);
                        if let Ok(resolved) = ast::SimpleAssignTarget::try_from(e) {
                            *target = resolved;
                        }
                    }
                    ast::AssignTarget::Pat(p) => {
                        let mut pat: ast::Pat = p.clone().into();
                        self.pat(&mut pat);
                        if let Ok(resolved) = ast::AssignTargetPat::try_from(pat) {
                            *p = resolved;
                        }
                    }
                }
                self.expr(&mut a.right);
            }
            ast::Expr::Array(a) => {
                for e in a.elems.iter_mut().flatten() {
                    self.expr(&mut e.expr);
                }
            }
            ast::Expr::Object(o) => {
                for p in &mut o.props {
                    match p {
                        ast::PropOrSpread::Spread(s) => self.expr(&mut s.expr),
                        ast::PropOrSpread::Prop(p) => match &mut **p {
                            ast::Prop::KeyValue(p) => {
                                self.key(&mut p.key);
                                self.expr(&mut p.value);
                            }
                            ast::Prop::Assign(p) => self.expr(&mut p.value),
                            ast::Prop::Method(p) => {
                                self.key(&mut p.key);
                                self.function(&mut p.function);
                            }
                            ast::Prop::Getter(p) => {
                                self.key(&mut p.key);
                                self.ann(&mut p.type_ann);
                                if let Some(b) = &mut p.body {
                                    self.block(b);
                                }
                            }
                            ast::Prop::Setter(p) => {
                                self.key(&mut p.key);
                                let mut names = HashSet::new();
                                pat_names(&p.param, &mut names);
                                self.scopes.push(names);
                                self.pat(&mut p.param);
                                if let Some(b) = &mut p.body {
                                    self.block(b);
                                }
                                self.scopes.pop();
                            }
                            ast::Prop::Shorthand(_) => {}
                        },
                    }
                }
            }
            ast::Expr::Fn(f) => {
                let mut names = HashSet::new();
                if let Some(i) = &f.ident {
                    names.insert(i.sym.to_string());
                }
                self.scopes.push(names);
                self.function(&mut f.function);
                self.scopes.pop();
            }
            ast::Expr::Arrow(a) => {
                let mut names = HashSet::new();
                for p in &a.params {
                    pat_names(p, &mut names);
                }
                self.scopes.push(names);
                self.params(&mut a.type_params);
                self.ann(&mut a.return_type);
                for p in &mut a.params {
                    self.pat(p);
                }
                match &mut *a.body {
                    ast::BlockStmtOrExpr::BlockStmt(b) => self.block(b),
                    ast::BlockStmtOrExpr::Expr(e) => self.expr(e),
                }
                self.scopes.pop();
            }
            ast::Expr::Class(c) => self.class(&mut c.class),
            ast::Expr::Seq(s) => {
                for e in &mut s.exprs {
                    self.expr(e);
                }
            }
            ast::Expr::Tpl(t) => {
                for e in &mut t.exprs {
                    self.expr(e);
                }
            }
            ast::Expr::TaggedTpl(t) => {
                self.expr(&mut t.tag);
                self.args(&mut t.type_params);
                for e in &mut t.tpl.exprs {
                    self.expr(e);
                }
            }
            ast::Expr::TsAs(t) => {
                self.expr(&mut t.expr);
                self.ty(&mut t.type_ann);
            }
            ast::Expr::TsTypeAssertion(t) => {
                self.expr(&mut t.expr);
                self.ty(&mut t.type_ann);
            }
            ast::Expr::TsSatisfies(t) => {
                self.expr(&mut t.expr);
                self.ty(&mut t.type_ann);
            }
            ast::Expr::TsInstantiation(t) => {
                self.expr(&mut t.expr);
                for ty in &mut t.type_args.params {
                    self.ty(ty);
                }
            }
            ast::Expr::TsNonNull(t) => self.expr(&mut t.expr),
            ast::Expr::TsConstAssertion(t) => self.expr(&mut t.expr),
            ast::Expr::OptChain(c) => match &mut *c.base {
                ast::OptChainBase::Member(m) => {
                    if let Some(ident) = self.member(m) {
                        *expr = ast::Expr::Ident(ident);
                    }
                }
                ast::OptChainBase::Call(c) => {
                    self.expr(&mut c.callee);
                    self.args(&mut c.type_args);
                    for a in &mut c.args {
                        self.expr(&mut a.expr);
                    }
                }
            },
            ast::Expr::SuperProp(s) => {
                if let ast::SuperProp::Computed(p) = &mut s.prop {
                    self.expr(&mut p.expr);
                }
            }
            // JSX has no accepted expression form.
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

    fn ty(&mut self, ty: &mut ast::TsType) {
        match ty {
            ast::TsType::TsTypeRef(r) => {
                if let ast::TsEntityName::TsQualifiedName(q) = &r.type_name {
                    if let ast::TsEntityName::Ident(ns) = &q.left {
                        if !self.shadowed(ns.sym.as_ref()) {
                            if let Some(i) = self.checker.namespace_member_ident(ns, &q.right) {
                                r.type_name = ast::TsEntityName::Ident(i);
                            }
                        }
                    }
                }
                self.args(&mut r.type_params);
            }
            ast::TsType::TsArrayType(t) => self.ty(&mut t.elem_type),
            ast::TsType::TsTupleType(t) => {
                for e in &mut t.elem_types {
                    self.ty(&mut e.ty);
                }
            }
            ast::TsType::TsOptionalType(t) => self.ty(&mut t.type_ann),
            ast::TsType::TsRestType(t) => self.ty(&mut t.type_ann),
            ast::TsType::TsParenthesizedType(t) => self.ty(&mut t.type_ann),
            ast::TsType::TsUnionOrIntersectionType(t) => match t {
                ast::TsUnionOrIntersectionType::TsUnionType(t) => {
                    for t in &mut t.types {
                        self.ty(t);
                    }
                }
                ast::TsUnionOrIntersectionType::TsIntersectionType(t) => {
                    for t in &mut t.types {
                        self.ty(t);
                    }
                }
            },
            ast::TsType::TsFnOrConstructorType(t) => match t {
                ast::TsFnOrConstructorType::TsFnType(t) => {
                    self.params(&mut t.type_params);
                    for p in &mut t.params {
                        self.fn_param(p);
                    }
                    self.ty(&mut t.type_ann.type_ann);
                }
                ast::TsFnOrConstructorType::TsConstructorType(t) => {
                    self.params(&mut t.type_params);
                    for p in &mut t.params {
                        self.fn_param(p);
                    }
                    self.ty(&mut t.type_ann.type_ann);
                }
            },
            ast::TsType::TsTypeLit(t) => {
                for member in &mut t.members {
                    self.type_element(member);
                }
            }
            ast::TsType::TsTypeOperator(t) => self.ty(&mut t.type_ann),
            ast::TsType::TsIndexedAccessType(t) => {
                self.ty(&mut t.obj_type);
                self.ty(&mut t.index_type);
            }
            ast::TsType::TsConditionalType(t) => {
                self.ty(&mut t.check_type);
                self.ty(&mut t.extends_type);
                self.ty(&mut t.true_type);
                self.ty(&mut t.false_type);
            }
            ast::TsType::TsTypePredicate(t) => self.ann(&mut t.type_ann),
            ast::TsType::TsImportType(t) => self.args(&mut t.type_args),
            ast::TsType::TsMappedType(t) => {
                if let Some(t) = &mut t.type_ann {
                    self.ty(t);
                }
                if let Some(t) = &mut t.name_type {
                    self.ty(t);
                }
                if let Some(t) = &mut t.type_param.constraint {
                    self.ty(t);
                }
            }
            ast::TsType::TsInferType(t) => {
                if let Some(t) = &mut t.type_param.constraint {
                    self.ty(t);
                }
            }
            ast::TsType::TsTypeQuery(t) => self.args(&mut t.type_args),
            ast::TsType::TsLitType(_)
            | ast::TsType::TsKeywordType(_)
            | ast::TsType::TsThisType(_) => {}
        }
    }

    fn fn_param(&mut self, p: &mut ast::TsFnParam) {
        match p {
            ast::TsFnParam::Ident(p) => self.ann(&mut p.type_ann),
            ast::TsFnParam::Array(p) => self.ann(&mut p.type_ann),
            ast::TsFnParam::Object(p) => self.ann(&mut p.type_ann),
            ast::TsFnParam::Rest(p) => {
                self.ann(&mut p.type_ann);
                self.pat(&mut p.arg);
            }
        }
    }

    fn type_element(&mut self, element: &mut ast::TsTypeElement) {
        match element {
            ast::TsTypeElement::TsPropertySignature(p) => {
                self.expr(&mut p.key);
                self.ann(&mut p.type_ann);
            }
            ast::TsTypeElement::TsMethodSignature(p) => {
                self.expr(&mut p.key);
                self.params(&mut p.type_params);
                for p in &mut p.params {
                    self.fn_param(p);
                }
                self.ann(&mut p.type_ann);
            }
            ast::TsTypeElement::TsCallSignatureDecl(p) => {
                self.params(&mut p.type_params);
                for p in &mut p.params {
                    self.fn_param(p);
                }
                self.ann(&mut p.type_ann);
            }
            ast::TsTypeElement::TsConstructSignatureDecl(p) => {
                self.params(&mut p.type_params);
                for p in &mut p.params {
                    self.fn_param(p);
                }
                self.ann(&mut p.type_ann);
            }
            ast::TsTypeElement::TsIndexSignature(p) => {
                for p in &mut p.params {
                    self.fn_param(p);
                }
                self.ann(&mut p.type_ann);
            }
            ast::TsTypeElement::TsGetterSignature(p) => {
                self.expr(&mut p.key);
                self.ann(&mut p.type_ann);
            }
            ast::TsTypeElement::TsSetterSignature(p) => {
                self.expr(&mut p.key);
                self.fn_param(&mut p.param);
            }
        }
    }
}

fn pat_names(pat: &ast::Pat, names: &mut HashSet<String>) {
    match pat {
        ast::Pat::Ident(p) => {
            names.insert(p.id.sym.to_string());
        }
        ast::Pat::Array(p) => {
            for p in p.elems.iter().flatten() {
                pat_names(p, names);
            }
        }
        ast::Pat::Object(p) => {
            for p in &p.props {
                match p {
                    ast::ObjectPatProp::KeyValue(p) => pat_names(&p.value, names),
                    ast::ObjectPatProp::Assign(p) => {
                        names.insert(p.key.id.sym.to_string());
                    }
                    ast::ObjectPatProp::Rest(p) => pat_names(&p.arg, names),
                }
            }
        }
        ast::Pat::Rest(p) => pat_names(&p.arg, names),
        ast::Pat::Assign(p) => pat_names(&p.left, names),
        ast::Pat::Expr(_) | ast::Pat::Invalid(_) => {}
    }
}

fn decl_names(decl: &ast::Decl, names: &mut HashSet<String>) {
    match decl {
        ast::Decl::Var(v) => {
            for d in &v.decls {
                pat_names(&d.name, names);
            }
        }
        ast::Decl::Using(v) => {
            for d in &v.decls {
                pat_names(&d.name, names);
            }
        }
        ast::Decl::Class(c) => {
            names.insert(c.ident.sym.to_string());
        }
        ast::Decl::Fn(f) => {
            names.insert(f.ident.sym.to_string());
        }
        ast::Decl::TsTypeAlias(t) => {
            names.insert(t.id.sym.to_string());
        }
        ast::Decl::TsEnum(e) => {
            names.insert(e.id.sym.to_string());
        }
        ast::Decl::TsInterface(i) => {
            names.insert(i.id.sym.to_string());
        }
        ast::Decl::TsModule(_) => {}
    }
}

fn head_names(head: &ast::ForHead) -> HashSet<String> {
    let mut names = HashSet::new();
    match head {
        ast::ForHead::VarDecl(v) => {
            for d in &v.decls {
                pat_names(&d.name, &mut names);
            }
        }
        ast::ForHead::UsingDecl(v) => {
            for d in &v.decls {
                pat_names(&d.name, &mut names);
            }
        }
        ast::ForHead::Pat(p) => pat_names(p, &mut names),
    }
    names
}
