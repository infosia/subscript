//! A local function is a module function (`specs/blocks/compiler.md`
//! §207).
//!
//! A `function` declaration in a block is a local function. The front
//! end resolves every name in each function body against the blocks
//! that enclose it. A local function that reads no local, parameter,
//! type parameter, or `this` of an enclosing function moves to module
//! scope with a hidden name, and each reference to it takes that name
//! (§207.1 rule 2). Every later stage reads a module function.
//!
//! The hidden name is `[[identity:local:K]]f`. No source identifier
//! contains `[`, so the name differs from every name that a program can
//! write, and the checker gives it the module identity of its file
//! (§207.1 rule 3). `K` encodes the chain of declarations that encloses
//! the function, its name, and its ordinal among the local functions
//! with that chain and name. The ordinal counts in source order, so
//! two local functions with one name never share `K`, and an edit in
//! another declaration leaves `K` unchanged.
//!
//! A local function that reads a binding of an enclosing function stays
//! in its block. The result records the first such read, and the
//! checker rejects the declaration with it (§207.1 rule 4). A call to a
//! local function that stays in its block is such a read, so the rule
//! applies until no function changes.
//!
//! The walk does not enter a form that the checker rejects as a whole:
//! a function expression, a class expression, an object literal method
//! or accessor, and a local class, enum, or namespace. A local function
//! inside a function that stays in its block also stays there, because
//! the checker does not check that body.

use std::collections::HashMap;

use swc_common::BytePos;
use swc_ecma_ast as ast;

/// The first read that keeps a local function in its block (§207.1
/// rule 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalFunctionRead {
    /// The source name of the local function.
    pub function: String,
    /// The binding that the function reads, as the diagnostic names it:
    /// "the local `k`", "the parameter `p`", or "`this`".
    pub read: String,
    /// The start of the read.
    pub at: BytePos,
}

/// Moves each local function that reads no binding of an enclosing
/// function to module scope. Returns the first read of each local
/// function that stays in its block, keyed by the start of its name.
pub(crate) fn lower_module(module: &mut ast::Module) -> HashMap<BytePos, LocalFunctionRead> {
    let mut walker = Walker::default();
    for item in &mut module.body {
        walker.module_item(item);
    }
    let status = walker.decide();
    let mut reads = HashMap::new();
    for (record, decision) in walker.records.iter().zip(&status) {
        if let Decision::Stays(Some((at, read))) = decision {
            reads.insert(
                record.declared_at,
                LocalFunctionRead {
                    function: record.name.clone(),
                    read: read.clone(),
                    at: *at,
                },
            );
        }
    }
    walker.lifted = status
        .iter()
        .map(|decision| matches!(decision, Decision::Lifted))
        .collect();
    walker.apply = true;
    let items = std::mem::take(&mut module.body);
    for mut item in items {
        walker.path.clear();
        walker.module_item(&mut item);
        for function in walker.hoisted.drain(..) {
            module
                .body
                .push(ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Fn(
                    function,
                ))));
        }
        module.body.push(item);
    }
    reads
}

/// One local function.
struct Record {
    name: String,
    hidden: String,
    declared_at: BytePos,
    /// The enclosing local function.
    parent: Option<usize>,
    /// The function level of the parameters and the body.
    body_level: usize,
    /// Reads of a binding of an enclosing function.
    reads: Vec<(BytePos, String)>,
    /// References to a local function that an enclosing block declares.
    calls: Vec<(BytePos, usize)>,
}

enum Decision {
    Lifted,
    /// The function stays in its block. `None` if an enclosing local
    /// function stays in its block, so the checker does not reach it.
    Stays(Option<(BytePos, String)>),
}

#[derive(Clone, Copy)]
enum Value {
    Local,
    Parameter,
    Function(usize),
}

#[derive(Clone, Copy)]
enum TypeName {
    Local,
    Parameter,
}

#[derive(Default)]
struct Scope {
    level: usize,
    values: HashMap<String, Value>,
    types: HashMap<String, TypeName>,
}

#[derive(Default)]
struct Walker {
    /// False for the resolution walk; true for the walk that moves
    /// functions and renames references.
    apply: bool,
    scopes: Vec<Scope>,
    level: usize,
    /// The local functions that enclose the current position.
    open: Vec<usize>,
    /// The local function whose `this` the current position reads.
    this_owner: Option<usize>,
    path: Vec<String>,
    ordinals: HashMap<String, usize>,
    records: Vec<Record>,
    by_declaration: HashMap<BytePos, usize>,
    references: HashMap<BytePos, usize>,
    lifted: Vec<bool>,
    hoisted: Vec<ast::FnDecl>,
}

fn hex(text: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    text.bytes()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

fn prop_label(key: &ast::PropName) -> String {
    match key {
        ast::PropName::Ident(name) => name.sym.to_string(),
        ast::PropName::Str(text) => text.value.to_string(),
        ast::PropName::Num(number) => number.value.to_string(),
        ast::PropName::Computed(_) | ast::PropName::BigInt(_) => "[computed]".to_owned(),
    }
}

impl Walker {
    /// Decides which local functions move (§207.1 rules 2 and 4).
    fn decide(&self) -> Vec<Decision> {
        let count = self.records.len();
        let mut stays: Vec<bool> = self.records.iter().map(|r| !r.reads.is_empty()).collect();
        loop {
            let mut changed = false;
            for index in 0..count {
                if !stays[index] && self.records[index].calls.iter().any(|(_, g)| stays[*g]) {
                    stays[index] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        (0..count)
            .map(|index| {
                let mut parent = self.records[index].parent;
                while let Some(enclosing) = parent {
                    if stays[enclosing] {
                        return Decision::Stays(None);
                    }
                    parent = self.records[enclosing].parent;
                }
                if !stays[index] {
                    return Decision::Lifted;
                }
                let record = &self.records[index];
                let calls = record
                    .calls
                    .iter()
                    .filter(|(_, g)| stays[*g])
                    .map(|(at, g)| {
                        (
                            *at,
                            format!("the local function `{}`", self.records[*g].name),
                        )
                    });
                Decision::Stays(
                    record
                        .reads
                        .iter()
                        .cloned()
                        .chain(calls)
                        .min_by_key(|(at, _)| *at),
                )
            })
            .collect()
    }

    fn push_scope(&mut self) {
        if !self.apply {
            self.scopes.push(Scope {
                level: self.level,
                ..Scope::default()
            });
        }
    }

    fn pop_scope(&mut self) {
        if !self.apply {
            self.scopes.pop();
        }
    }

    fn declare_value(&mut self, name: &str, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.values.insert(name.to_owned(), value);
        }
    }

    fn declare_type(&mut self, name: &str, kind: TypeName) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.types.insert(name.to_owned(), kind);
        }
    }

    fn declare_type_params(&mut self, params: Option<&mut ast::TsTypeParamDecl>) {
        let Some(params) = params else {
            return;
        };
        if !self.apply {
            for param in &params.params {
                self.declare_type(param.name.sym.as_ref(), TypeName::Parameter);
            }
        }
        for param in &mut params.params {
            if let Some(constraint) = &mut param.constraint {
                self.ty(constraint);
            }
        }
    }

    /// Records a read of a binding at `level` by every open local
    /// function that the binding encloses.
    fn read(&mut self, level: usize, at: BytePos, read: String) {
        for &open in &self.open {
            if self.records[open].body_level > level {
                self.records[open].reads.push((at, read.clone()));
            }
        }
    }

    /// Resolves a value reference, or gives it its hidden name.
    fn value(&mut self, id: &mut ast::Ident) {
        if self.apply {
            if let Some(&record) = self.references.get(&id.span.lo) {
                if self.lifted[record] {
                    id.sym = self.records[record].hidden.as_str().into();
                }
            }
            return;
        }
        let name = id.sym.as_ref();
        let Some((level, value)) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.values.get(name).map(|value| (scope.level, *value)))
        else {
            return;
        };
        match value {
            Value::Local => self.read(level, id.span.lo, format!("the local `{name}`")),
            Value::Parameter => self.read(level, id.span.lo, format!("the parameter `{name}`")),
            Value::Function(record) => {
                self.references.insert(id.span.lo, record);
                for &open in &self.open {
                    if open != record && self.records[open].body_level > level {
                        self.records[open].calls.push((id.span.lo, record));
                    }
                }
            }
        }
    }

    fn type_name(&mut self, id: &ast::Ident) {
        if self.apply {
            return;
        }
        let name = id.sym.as_ref();
        let Some((level, kind)) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.types.get(name).map(|kind| (scope.level, *kind)))
        else {
            return;
        };
        let read = match kind {
            TypeName::Local => format!("the local `{name}`"),
            TypeName::Parameter => format!("the type parameter `{name}`"),
        };
        self.read(level, id.span.lo, read);
    }

    fn module_item(&mut self, item: &mut ast::ModuleItem) {
        match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(declaration)) => self.module_decl(declaration),
            ast::ModuleItem::Stmt(stmt) => {
                self.path = vec!["top".to_owned()];
                self.statement(stmt);
            }
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
                self.module_decl(&mut export.decl);
            }
            ast::ModuleItem::ModuleDecl(_) => {}
        }
    }

    /// A module declaration. A function or a class member is a function
    /// level; a module variable is not a local.
    fn module_decl(&mut self, declaration: &mut ast::Decl) {
        match declaration {
            ast::Decl::Fn(function) => {
                self.path = vec![function.ident.sym.to_string()];
                self.function(&mut function.function, None);
            }
            ast::Decl::Class(class) => {
                self.path = vec![class.ident.sym.to_string()];
                self.class(&mut class.class);
            }
            ast::Decl::Var(var) => {
                self.path = vec![var
                    .decls
                    .first()
                    .and_then(|d| d.name.as_ident())
                    .map_or_else(|| "top".to_owned(), |id| id.sym.to_string())];
                for declarator in &mut var.decls {
                    self.binding_pat(&mut declarator.name, None);
                    if let Some(init) = &mut declarator.init {
                        self.expr(init);
                    }
                }
            }
            ast::Decl::Using(using) => {
                self.path = vec!["top".to_owned()];
                for declarator in &mut using.decls {
                    self.binding_pat(&mut declarator.name, None);
                    if let Some(init) = &mut declarator.init {
                        self.expr(init);
                    }
                }
            }
            ast::Decl::TsEnum(_)
            | ast::Decl::TsModule(_)
            | ast::Decl::TsInterface(_)
            | ast::Decl::TsTypeAlias(_) => {}
        }
    }

    fn class(&mut self, class: &mut ast::Class) {
        let class_path = self.path.clone();
        self.push_scope();
        self.declare_type_params(class.type_params.as_deref_mut());
        for member in &mut class.body {
            match member {
                ast::ClassMember::Method(method) => {
                    self.path = class_path.clone();
                    self.path.push(prop_label(&method.key));
                    self.function(&mut method.function, None);
                }
                ast::ClassMember::Constructor(constructor) => {
                    self.path = class_path.clone();
                    self.path.push("constructor".to_owned());
                    self.constructor(constructor);
                }
                ast::ClassMember::ClassProp(prop) => {
                    self.path = class_path.clone();
                    self.path.push(prop_label(&prop.key));
                    if let Some(type_ann) = &mut prop.type_ann {
                        self.ty(&mut type_ann.type_ann);
                    }
                    if let Some(value) = &mut prop.value {
                        let saved = self.enter_level(None);
                        self.expr(value);
                        self.leave_level(saved);
                    }
                }
                ast::ClassMember::PrivateMethod(_)
                | ast::ClassMember::PrivateProp(_)
                | ast::ClassMember::TsIndexSignature(_)
                | ast::ClassMember::Empty(_)
                | ast::ClassMember::StaticBlock(_)
                | ast::ClassMember::AutoAccessor(_) => {}
            }
        }
        self.pop_scope();
        self.path = class_path;
    }

    /// Enters a function level. `owner` is the local function whose
    /// `this` the level reads, or `None` for a function that has its
    /// own receiver.
    fn enter_level(&mut self, owner: Option<usize>) -> Option<usize> {
        self.level += 1;
        std::mem::replace(&mut self.this_owner, owner)
    }

    fn leave_level(&mut self, saved: Option<usize>) {
        self.level -= 1;
        self.this_owner = saved;
    }

    fn constructor(&mut self, constructor: &mut ast::Constructor) {
        let saved = self.enter_level(None);
        self.push_scope();
        for param in &mut constructor.params {
            match param {
                ast::ParamOrTsParamProp::Param(param) => {
                    self.binding_pat(&mut param.pat, Some(Value::Parameter));
                }
                ast::ParamOrTsParamProp::TsParamProp(prop) => match &mut prop.param {
                    ast::TsParamPropParam::Ident(binding) => {
                        self.binding_ident(binding, Some(Value::Parameter));
                    }
                    ast::TsParamPropParam::Assign(assign) => {
                        self.binding_pat(&mut assign.left, Some(Value::Parameter));
                        self.expr(&mut assign.right);
                    }
                },
            }
        }
        if let Some(body) = &mut constructor.body {
            self.block(&mut body.stmts);
        }
        self.pop_scope();
        self.leave_level(saved);
    }

    /// A function body. `local` is the record of a local function.
    fn function(&mut self, function: &mut ast::Function, local: Option<usize>) {
        let saved = self.enter_level(local);
        self.push_scope();
        self.declare_type_params(function.type_params.as_deref_mut());
        for param in &mut function.params {
            self.binding_pat(&mut param.pat, Some(Value::Parameter));
        }
        if let Some(return_type) = &mut function.return_type {
            self.ty(&mut return_type.type_ann);
        }
        if let Some(body) = &mut function.body {
            self.block(&mut body.stmts);
        }
        self.pop_scope();
        self.leave_level(saved);
    }

    fn local_function(&mut self, function: &mut ast::FnDecl) {
        let Some(&record) = self.by_declaration.get(&function.ident.span.lo) else {
            return;
        };
        if self.apply && !self.lifted[record] {
            return;
        }
        self.open.push(record);
        self.path.push(function.ident.sym.to_string());
        self.function(&mut function.function, Some(record));
        self.path.pop();
        self.open.pop();
    }

    /// Declares the bindings of a block: every declaration is visible in
    /// the whole block.
    fn declare_block<'a>(&mut self, stmts: impl Iterator<Item = &'a ast::Stmt>) {
        if self.apply {
            return;
        }
        for stmt in stmts {
            let ast::Stmt::Decl(declaration) = stmt else {
                continue;
            };
            match declaration {
                ast::Decl::Var(var) => {
                    for declarator in &var.decls {
                        self.declare_pat(&declarator.name, Value::Local);
                    }
                }
                ast::Decl::Using(using) => {
                    for declarator in &using.decls {
                        self.declare_pat(&declarator.name, Value::Local);
                    }
                }
                ast::Decl::Fn(function) => {
                    let name = function.ident.sym.to_string();
                    let key = format!("{}/{name}", self.path.join("/"));
                    let ordinal = self.ordinals.entry(key.clone()).or_insert(0);
                    let hidden = format!(
                        "[[identity:local:{}]]{name}",
                        hex(&format!("{key}#{ordinal}"))
                    );
                    *ordinal += 1;
                    let record = self.records.len();
                    self.records.push(Record {
                        name: name.clone(),
                        hidden,
                        declared_at: function.ident.span.lo,
                        parent: self.open.last().copied(),
                        body_level: self.level + 1,
                        reads: Vec::new(),
                        calls: Vec::new(),
                    });
                    self.by_declaration.insert(function.ident.span.lo, record);
                    self.declare_value(&name, Value::Function(record));
                }
                ast::Decl::Class(class) => {
                    self.declare_value(class.ident.sym.as_ref(), Value::Local);
                    self.declare_type(class.ident.sym.as_ref(), TypeName::Local);
                }
                ast::Decl::TsEnum(enumeration) => {
                    self.declare_value(enumeration.id.sym.as_ref(), Value::Local);
                    self.declare_type(enumeration.id.sym.as_ref(), TypeName::Local);
                }
                ast::Decl::TsInterface(interface) => {
                    self.declare_type(interface.id.sym.as_ref(), TypeName::Local);
                }
                ast::Decl::TsTypeAlias(alias) => {
                    self.declare_type(alias.id.sym.as_ref(), TypeName::Local);
                }
                ast::Decl::TsModule(namespace) => {
                    if let ast::TsModuleName::Ident(id) = &namespace.id {
                        self.declare_value(id.sym.as_ref(), Value::Local);
                    }
                }
            }
        }
    }

    fn declare_pat(&mut self, target: &ast::Pat, value: Value) {
        match target {
            ast::Pat::Ident(binding) => self.declare_value(binding.id.sym.as_ref(), value),
            ast::Pat::Array(array) => {
                for element in array.elems.iter().flatten() {
                    self.declare_pat(element, value);
                }
            }
            ast::Pat::Object(object) => {
                for prop in &object.props {
                    match prop {
                        ast::ObjectPatProp::KeyValue(prop) => self.declare_pat(&prop.value, value),
                        ast::ObjectPatProp::Assign(prop) => {
                            self.declare_value(prop.key.id.sym.as_ref(), value);
                        }
                        ast::ObjectPatProp::Rest(rest) => self.declare_pat(&rest.arg, value),
                    }
                }
            }
            ast::Pat::Assign(assign) => self.declare_pat(&assign.left, value),
            ast::Pat::Rest(rest) => self.declare_pat(&rest.arg, value),
            ast::Pat::Expr(_) | ast::Pat::Invalid(_) => {}
        }
    }

    /// A statement list that is one block scope. A moved local function
    /// leaves the list.
    fn block(&mut self, stmts: &mut Vec<ast::Stmt>) {
        self.push_scope();
        self.declare_block(stmts.iter());
        self.block_items(stmts);
        self.pop_scope();
    }

    fn block_items(&mut self, stmts: &mut Vec<ast::Stmt>) {
        let mut index = 0;
        while index < stmts.len() {
            if self.apply {
                if let ast::Stmt::Decl(ast::Decl::Fn(function)) = &stmts[index] {
                    if let Some(&record) = self.by_declaration.get(&function.ident.span.lo) {
                        if self.lifted[record] {
                            if let ast::Stmt::Decl(ast::Decl::Fn(mut function)) =
                                stmts.remove(index)
                            {
                                self.local_function(&mut function);
                                function.ident.sym = self.records[record].hidden.as_str().into();
                                self.hoisted.push(function);
                            }
                            continue;
                        }
                    }
                }
            }
            self.statement(&mut stmts[index]);
            index += 1;
        }
    }

    fn statement(&mut self, stmt: &mut ast::Stmt) {
        match stmt {
            ast::Stmt::Block(body) => self.block(&mut body.stmts),
            ast::Stmt::Decl(declaration) => self.local_decl(declaration),
            ast::Stmt::Expr(node) => self.expr(&mut node.expr),
            ast::Stmt::Return(node) => {
                if let Some(arg) = &mut node.arg {
                    self.expr(arg);
                }
            }
            ast::Stmt::Throw(node) => self.expr(&mut node.arg),
            ast::Stmt::If(node) => {
                self.expr(&mut node.test);
                self.statement(&mut node.cons);
                if let Some(alt) = &mut node.alt {
                    self.statement(alt);
                }
            }
            ast::Stmt::While(node) => {
                self.expr(&mut node.test);
                self.statement(&mut node.body);
            }
            ast::Stmt::DoWhile(node) => {
                self.statement(&mut node.body);
                self.expr(&mut node.test);
            }
            ast::Stmt::For(node) => {
                self.push_scope();
                match &mut node.init {
                    Some(ast::VarDeclOrExpr::VarDecl(var)) => {
                        for declarator in &mut var.decls {
                            self.binding_pat(&mut declarator.name, Some(Value::Local));
                            if let Some(init) = &mut declarator.init {
                                self.expr(init);
                            }
                        }
                    }
                    Some(ast::VarDeclOrExpr::Expr(init)) => self.expr(init),
                    None => {}
                }
                if let Some(test) = &mut node.test {
                    self.expr(test);
                }
                if let Some(update) = &mut node.update {
                    self.expr(update);
                }
                self.statement(&mut node.body);
                self.pop_scope();
            }
            ast::Stmt::ForOf(node) => {
                self.expr(&mut node.right);
                self.push_scope();
                self.for_head(&mut node.left);
                self.statement(&mut node.body);
                self.pop_scope();
            }
            ast::Stmt::ForIn(node) => {
                self.expr(&mut node.right);
                self.push_scope();
                self.for_head(&mut node.left);
                self.statement(&mut node.body);
                self.pop_scope();
            }
            ast::Stmt::Switch(node) => {
                self.expr(&mut node.discriminant);
                self.push_scope();
                self.declare_block(node.cases.iter().flat_map(|case| case.cons.iter()));
                for case in &mut node.cases {
                    if let Some(test) = &mut case.test {
                        self.expr(test);
                    }
                    self.block_items(&mut case.cons);
                }
                self.pop_scope();
            }
            ast::Stmt::Try(node) => {
                self.block(&mut node.block.stmts);
                if let Some(handler) = &mut node.handler {
                    self.push_scope();
                    if let Some(param) = &mut handler.param {
                        self.binding_pat(param, Some(Value::Local));
                    }
                    self.block(&mut handler.body.stmts);
                    self.pop_scope();
                }
                if let Some(finalizer) = &mut node.finalizer {
                    self.block(&mut finalizer.stmts);
                }
            }
            ast::Stmt::Labeled(node) => self.statement(&mut node.body),
            ast::Stmt::With(node) => {
                self.expr(&mut node.obj);
                self.statement(&mut node.body);
            }
            ast::Stmt::Empty(_)
            | ast::Stmt::Debugger(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_) => {}
        }
    }

    /// A declaration in a block. Its names are declared with the block.
    fn local_decl(&mut self, declaration: &mut ast::Decl) {
        match declaration {
            ast::Decl::Var(var) => {
                for declarator in &mut var.decls {
                    self.binding_pat(&mut declarator.name, None);
                    if let Some(init) = &mut declarator.init {
                        self.expr(init);
                    }
                }
            }
            ast::Decl::Using(using) => {
                for declarator in &mut using.decls {
                    self.binding_pat(&mut declarator.name, None);
                    if let Some(init) = &mut declarator.init {
                        self.expr(init);
                    }
                }
            }
            ast::Decl::Fn(function) => self.local_function(function),
            ast::Decl::Class(_)
            | ast::Decl::TsEnum(_)
            | ast::Decl::TsModule(_)
            | ast::Decl::TsInterface(_)
            | ast::Decl::TsTypeAlias(_) => {}
        }
    }

    fn for_head(&mut self, head: &mut ast::ForHead) {
        match head {
            ast::ForHead::VarDecl(var) => {
                for declarator in &mut var.decls {
                    self.binding_pat(&mut declarator.name, Some(Value::Local));
                }
            }
            ast::ForHead::UsingDecl(using) => {
                for declarator in &mut using.decls {
                    self.binding_pat(&mut declarator.name, Some(Value::Local));
                }
            }
            ast::ForHead::Pat(target) => self.assign_pat(target),
        }
    }

    fn binding_ident(&mut self, binding: &mut ast::BindingIdent, value: Option<Value>) {
        if let (Some(value), false) = (value, self.apply) {
            self.declare_value(binding.id.sym.as_ref(), value);
        }
        if let Some(type_ann) = &mut binding.type_ann {
            self.ty(&mut type_ann.type_ann);
        }
    }

    /// A binding pattern. `value` declares its names in the current
    /// scope; `None` leaves them to the enclosing block.
    fn binding_pat(&mut self, target: &mut ast::Pat, value: Option<Value>) {
        match target {
            ast::Pat::Ident(binding) => self.binding_ident(binding, value),
            ast::Pat::Array(array) => {
                for element in array.elems.iter_mut().flatten() {
                    self.binding_pat(element, value);
                }
                if let Some(type_ann) = &mut array.type_ann {
                    self.ty(&mut type_ann.type_ann);
                }
            }
            ast::Pat::Object(object) => {
                for prop in &mut object.props {
                    match prop {
                        ast::ObjectPatProp::KeyValue(prop) => {
                            self.prop_name(&mut prop.key);
                            self.binding_pat(&mut prop.value, value);
                        }
                        ast::ObjectPatProp::Assign(prop) => {
                            self.binding_ident(&mut prop.key, value);
                            if let Some(default) = &mut prop.value {
                                self.expr(default);
                            }
                        }
                        ast::ObjectPatProp::Rest(rest) => self.binding_pat(&mut rest.arg, value),
                    }
                }
                if let Some(type_ann) = &mut object.type_ann {
                    self.ty(&mut type_ann.type_ann);
                }
            }
            ast::Pat::Assign(assign) => {
                self.binding_pat(&mut assign.left, value);
                self.expr(&mut assign.right);
            }
            ast::Pat::Rest(rest) => {
                self.binding_pat(&mut rest.arg, value);
                if let Some(type_ann) = &mut rest.type_ann {
                    self.ty(&mut type_ann.type_ann);
                }
            }
            ast::Pat::Expr(target) => self.expr(target),
            ast::Pat::Invalid(_) => {}
        }
    }

    /// An assignment pattern: each name is a reference.
    fn assign_pat(&mut self, target: &mut ast::Pat) {
        match target {
            ast::Pat::Ident(binding) => self.value(&mut binding.id),
            ast::Pat::Array(array) => {
                for element in array.elems.iter_mut().flatten() {
                    self.assign_pat(element);
                }
            }
            ast::Pat::Object(object) => self.assign_object_pat(object),
            ast::Pat::Assign(assign) => {
                self.assign_pat(&mut assign.left);
                self.expr(&mut assign.right);
            }
            ast::Pat::Rest(rest) => self.assign_pat(&mut rest.arg),
            ast::Pat::Expr(target) => self.expr(target),
            ast::Pat::Invalid(_) => {}
        }
    }

    fn assign_object_pat(&mut self, object: &mut ast::ObjectPat) {
        for prop in &mut object.props {
            match prop {
                ast::ObjectPatProp::KeyValue(prop) => {
                    self.prop_name(&mut prop.key);
                    self.assign_pat(&mut prop.value);
                }
                ast::ObjectPatProp::Assign(prop) => {
                    self.value(&mut prop.key.id);
                    if let Some(default) = &mut prop.value {
                        self.expr(default);
                    }
                }
                ast::ObjectPatProp::Rest(rest) => self.assign_pat(&mut rest.arg),
            }
        }
    }

    fn prop_name(&mut self, name: &mut ast::PropName) {
        if let ast::PropName::Computed(computed) = name {
            self.expr(&mut computed.expr);
        }
    }

    fn type_args(&mut self, args: Option<&mut ast::TsTypeParamInstantiation>) {
        if let Some(args) = args {
            for arg in &mut args.params {
                self.ty(arg);
            }
        }
    }

    fn fn_params(&mut self, params: &mut [ast::TsFnParam]) {
        for param in params {
            match param {
                ast::TsFnParam::Ident(binding) => self.binding_ident(binding, None),
                ast::TsFnParam::Array(array) => {
                    if let Some(type_ann) = &mut array.type_ann {
                        self.ty(&mut type_ann.type_ann);
                    }
                }
                ast::TsFnParam::Rest(rest) => {
                    if let Some(type_ann) = &mut rest.type_ann {
                        self.ty(&mut type_ann.type_ann);
                    }
                }
                ast::TsFnParam::Object(object) => {
                    if let Some(type_ann) = &mut object.type_ann {
                        self.ty(&mut type_ann.type_ann);
                    }
                }
            }
        }
    }

    fn ty(&mut self, ty: &mut ast::TsType) {
        match ty {
            ast::TsType::TsTypeRef(reference) => {
                let mut name = &reference.type_name;
                while let ast::TsEntityName::TsQualifiedName(qualified) = name {
                    name = &qualified.left;
                }
                if let ast::TsEntityName::Ident(id) = name {
                    let id = id.clone();
                    self.type_name(&id);
                }
                self.type_args(reference.type_params.as_deref_mut());
            }
            ast::TsType::TsTypeQuery(query) => {
                if let ast::TsTypeQueryExpr::TsEntityName(ast::TsEntityName::Ident(id)) =
                    &mut query.expr_name
                {
                    self.value(id);
                }
                self.type_args(query.type_args.as_deref_mut());
            }
            ast::TsType::TsFnOrConstructorType(function) => match function {
                ast::TsFnOrConstructorType::TsFnType(function) => {
                    self.fn_params(&mut function.params);
                    self.ty(&mut function.type_ann.type_ann);
                }
                ast::TsFnOrConstructorType::TsConstructorType(function) => {
                    self.fn_params(&mut function.params);
                    self.ty(&mut function.type_ann.type_ann);
                }
            },
            ast::TsType::TsTypeLit(literal) => {
                for member in &mut literal.members {
                    match member {
                        ast::TsTypeElement::TsPropertySignature(prop) => {
                            if let Some(type_ann) = &mut prop.type_ann {
                                self.ty(&mut type_ann.type_ann);
                            }
                        }
                        ast::TsTypeElement::TsMethodSignature(method) => {
                            self.fn_params(&mut method.params);
                            if let Some(type_ann) = &mut method.type_ann {
                                self.ty(&mut type_ann.type_ann);
                            }
                        }
                        ast::TsTypeElement::TsIndexSignature(index) => {
                            self.fn_params(&mut index.params);
                            if let Some(type_ann) = &mut index.type_ann {
                                self.ty(&mut type_ann.type_ann);
                            }
                        }
                        ast::TsTypeElement::TsCallSignatureDecl(_)
                        | ast::TsTypeElement::TsConstructSignatureDecl(_)
                        | ast::TsTypeElement::TsGetterSignature(_)
                        | ast::TsTypeElement::TsSetterSignature(_) => {}
                    }
                }
            }
            ast::TsType::TsArrayType(array) => self.ty(&mut array.elem_type),
            ast::TsType::TsTupleType(tuple) => {
                for element in &mut tuple.elem_types {
                    self.ty(&mut element.ty);
                }
            }
            ast::TsType::TsOptionalType(inner) => self.ty(&mut inner.type_ann),
            ast::TsType::TsRestType(inner) => self.ty(&mut inner.type_ann),
            ast::TsType::TsParenthesizedType(inner) => self.ty(&mut inner.type_ann),
            ast::TsType::TsTypeOperator(inner) => self.ty(&mut inner.type_ann),
            ast::TsType::TsUnionOrIntersectionType(types) => {
                let types = match types {
                    ast::TsUnionOrIntersectionType::TsUnionType(union) => &mut union.types,
                    ast::TsUnionOrIntersectionType::TsIntersectionType(both) => &mut both.types,
                };
                for ty in types {
                    self.ty(ty);
                }
            }
            ast::TsType::TsIndexedAccessType(access) => {
                self.ty(&mut access.obj_type);
                self.ty(&mut access.index_type);
            }
            ast::TsType::TsConditionalType(conditional) => {
                self.ty(&mut conditional.check_type);
                self.ty(&mut conditional.extends_type);
                self.ty(&mut conditional.true_type);
                self.ty(&mut conditional.false_type);
            }
            ast::TsType::TsKeywordType(_)
            | ast::TsType::TsThisType(_)
            | ast::TsType::TsInferType(_)
            | ast::TsType::TsMappedType(_)
            | ast::TsType::TsLitType(_)
            | ast::TsType::TsTypePredicate(_)
            | ast::TsType::TsImportType(_) => {}
        }
    }

    fn args(&mut self, arguments: &mut [ast::ExprOrSpread]) {
        for argument in arguments {
            self.expr(&mut argument.expr);
        }
    }

    fn member(&mut self, member: &mut ast::MemberExpr) {
        self.expr(&mut member.obj);
        if let ast::MemberProp::Computed(computed) = &mut member.prop {
            self.expr(&mut computed.expr);
        }
    }

    fn arrow(&mut self, arrow: &mut ast::ArrowExpr) {
        let owner = self.this_owner;
        let saved = self.enter_level(owner);
        self.push_scope();
        self.declare_type_params(arrow.type_params.as_deref_mut());
        for param in &mut arrow.params {
            self.binding_pat(param, Some(Value::Parameter));
        }
        if let Some(return_type) = &mut arrow.return_type {
            self.ty(&mut return_type.type_ann);
        }
        match &mut *arrow.body {
            ast::BlockStmtOrExpr::BlockStmt(body) => self.block(&mut body.stmts),
            ast::BlockStmtOrExpr::Expr(body) => self.expr(body),
        }
        self.pop_scope();
        self.leave_level(saved);
    }

    fn expr(&mut self, e: &mut ast::Expr) {
        match e {
            ast::Expr::Ident(id) => self.value(id),
            ast::Expr::This(this) => {
                if let (Some(owner), false) = (self.this_owner, self.apply) {
                    self.records[owner]
                        .reads
                        .push((this.span.lo, "`this`".to_owned()));
                }
            }
            ast::Expr::Member(target) => self.member(target),
            ast::Expr::OptChain(chain) => match &mut *chain.base {
                ast::OptChainBase::Member(target) => self.member(target),
                ast::OptChainBase::Call(call) => {
                    self.expr(&mut call.callee);
                    self.args(&mut call.args);
                    self.type_args(call.type_args.as_deref_mut());
                }
            },
            ast::Expr::Paren(paren) => self.expr(&mut paren.expr),
            ast::Expr::Call(call) => {
                if let ast::Callee::Expr(callee) = &mut call.callee {
                    self.expr(callee);
                }
                self.args(&mut call.args);
                self.type_args(call.type_args.as_deref_mut());
            }
            ast::Expr::New(new) => {
                self.expr(&mut new.callee);
                if let Some(arguments) = &mut new.args {
                    self.args(arguments);
                }
                self.type_args(new.type_args.as_deref_mut());
            }
            ast::Expr::Await(target) => self.expr(&mut target.arg),
            ast::Expr::Yield(target) => {
                if let Some(arg) = &mut target.arg {
                    self.expr(arg);
                }
            }
            ast::Expr::Unary(target) => self.expr(&mut target.arg),
            ast::Expr::Update(target) => self.expr(&mut target.arg),
            ast::Expr::Bin(binary) => {
                self.expr(&mut binary.left);
                self.expr(&mut binary.right);
            }
            ast::Expr::Cond(conditional) => {
                self.expr(&mut conditional.test);
                self.expr(&mut conditional.cons);
                self.expr(&mut conditional.alt);
            }
            ast::Expr::Assign(assign) => {
                match &mut assign.left {
                    ast::AssignTarget::Simple(target) => self.simple_target(target),
                    ast::AssignTarget::Pat(ast::AssignTargetPat::Array(array)) => {
                        for element in array.elems.iter_mut().flatten() {
                            self.assign_pat(element);
                        }
                    }
                    ast::AssignTarget::Pat(ast::AssignTargetPat::Object(object)) => {
                        self.assign_object_pat(object);
                    }
                    ast::AssignTarget::Pat(ast::AssignTargetPat::Invalid(_)) => {}
                }
                self.expr(&mut assign.right);
            }
            ast::Expr::Array(array) => {
                for element in array.elems.iter_mut().flatten() {
                    self.expr(&mut element.expr);
                }
            }
            ast::Expr::Object(object) => self.object(object),
            ast::Expr::Arrow(arrow) => self.arrow(arrow),
            ast::Expr::Seq(sequence) => {
                for e in &mut sequence.exprs {
                    self.expr(e);
                }
            }
            ast::Expr::Tpl(template) => {
                for e in &mut template.exprs {
                    self.expr(e);
                }
            }
            ast::Expr::TaggedTpl(tagged) => {
                self.expr(&mut tagged.tag);
                for e in &mut tagged.tpl.exprs {
                    self.expr(e);
                }
            }
            ast::Expr::TsAs(target) => {
                self.expr(&mut target.expr);
                self.ty(&mut target.type_ann);
            }
            ast::Expr::TsTypeAssertion(target) => {
                self.expr(&mut target.expr);
                self.ty(&mut target.type_ann);
            }
            ast::Expr::TsSatisfies(target) => {
                self.expr(&mut target.expr);
                self.ty(&mut target.type_ann);
            }
            ast::Expr::TsInstantiation(target) => {
                self.expr(&mut target.expr);
                self.type_args(Some(&mut target.type_args));
            }
            ast::Expr::TsNonNull(target) => self.expr(&mut target.expr),
            ast::Expr::TsConstAssertion(target) => self.expr(&mut target.expr),
            ast::Expr::SuperProp(target) => {
                if let ast::SuperProp::Computed(computed) = &mut target.prop {
                    self.expr(&mut computed.expr);
                }
            }
            // The checker rejects these forms as a whole and does not
            // check their bodies.
            ast::Expr::Fn(_) | ast::Expr::Class(_) => {}
            ast::Expr::JSXMember(_)
            | ast::Expr::JSXNamespacedName(_)
            | ast::Expr::JSXEmpty(_)
            | ast::Expr::JSXElement(_)
            | ast::Expr::JSXFragment(_)
            | ast::Expr::PrivateName(_)
            | ast::Expr::Lit(_)
            | ast::Expr::MetaProp(_)
            | ast::Expr::Invalid(_) => {}
        }
    }

    fn object(&mut self, object: &mut ast::ObjectLit) {
        for prop in &mut object.props {
            match prop {
                ast::PropOrSpread::Spread(spread) => self.expr(&mut spread.expr),
                ast::PropOrSpread::Prop(prop) => {
                    let renamed = match &mut **prop {
                        ast::Prop::KeyValue(prop) => {
                            self.prop_name(&mut prop.key);
                            self.expr(&mut prop.value);
                            None
                        }
                        ast::Prop::Assign(prop) => {
                            self.expr(&mut prop.value);
                            None
                        }
                        ast::Prop::Shorthand(id) => {
                            let key = ast::IdentName::new(id.sym.clone(), id.span);
                            let mut value = id.clone();
                            self.value(&mut value);
                            (value.sym != id.sym).then_some((key, value))
                        }
                        // The checker rejects a method or an accessor in
                        // an object literal and does not check its body.
                        ast::Prop::Method(_) | ast::Prop::Getter(_) | ast::Prop::Setter(_) => None,
                    };
                    // `{ f }` keeps the key `f` and reads the hidden name.
                    if let Some((key, value)) = renamed {
                        **prop = ast::Prop::KeyValue(ast::KeyValueProp {
                            key: ast::PropName::Ident(key),
                            value: Box::new(ast::Expr::Ident(value)),
                        });
                    }
                }
            }
        }
    }

    fn simple_target(&mut self, target: &mut ast::SimpleAssignTarget) {
        match target {
            ast::SimpleAssignTarget::Ident(binding) => self.value(&mut binding.id),
            ast::SimpleAssignTarget::Member(target) => self.member(target),
            ast::SimpleAssignTarget::SuperProp(target) => {
                if let ast::SuperProp::Computed(computed) = &mut target.prop {
                    self.expr(&mut computed.expr);
                }
            }
            ast::SimpleAssignTarget::Paren(paren) => self.expr(&mut paren.expr),
            ast::SimpleAssignTarget::OptChain(chain) => match &mut *chain.base {
                ast::OptChainBase::Member(target) => self.member(target),
                ast::OptChainBase::Call(call) => {
                    self.expr(&mut call.callee);
                    self.args(&mut call.args);
                }
            },
            ast::SimpleAssignTarget::TsAs(target) => self.expr(&mut target.expr),
            ast::SimpleAssignTarget::TsSatisfies(target) => self.expr(&mut target.expr),
            ast::SimpleAssignTarget::TsNonNull(target) => self.expr(&mut target.expr),
            ast::SimpleAssignTarget::TsTypeAssertion(target) => self.expr(&mut target.expr),
            ast::SimpleAssignTarget::TsInstantiation(target) => self.expr(&mut target.expr),
            ast::SimpleAssignTarget::Invalid(_) => {}
        }
    }
}

/// Cost: one parse and one walk of a nine-line source, with no checker
/// call.
#[cfg(test)]
mod tests {
    use swc_common::{FileName, SourceMap};
    use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

    use super::*;

    /// A block that declares `sq` shadows the module function `sq`; a
    /// block that declares a local `sq` shadows the outer local function.
    const SOURCE: &str = "
function sq(x: i32): i32 { return x; }
function f(): void {
  { sq(1); function sq(x: i32): i32 { return x * x; } const o = { sq }; }
  sq(2);
  function g(): i32 { return 1; }
  { const g: i32 = 2; g; }
  g();
}
";

    fn parse() -> ast::Module {
        let source_map = SourceMap::default();
        let file = source_map.new_source_file(FileName::Anon.into(), SOURCE.to_string());
        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax::default()),
            ast::EsVersion::Es2022,
            StringInput::from(&*file),
            None,
        );
        Parser::new_from(lexer).parse_module().expect("parses")
    }

    fn function_names(module: &ast::Module) -> Vec<String> {
        module
            .body
            .iter()
            .filter_map(|item| match item {
                ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Fn(f))) => {
                    Some(f.ident.sym.to_string())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn each_reference_takes_the_name_of_the_function_its_block_declares() {
        let mut module = parse();
        let reads = lower_module(&mut module);
        assert!(reads.is_empty(), "{reads:?}");
        let names = function_names(&module);
        assert_eq!(names.len(), 4, "{names:?}");
        let (local_sq, local_g) = (&names[1], &names[2]);
        assert!(local_sq.starts_with("[[identity:local:") && local_sq.ends_with("]]sq"));
        assert!(local_g.starts_with("[[identity:local:") && local_g.ends_with("]]g"));
        let text = format!("{module:?}");
        // Two references to the local `sq`: the call before the
        // declaration and the shorthand value. `sq(2)` reads the module
        // function, and the shorthand key keeps the name `sq`.
        assert_eq!(text.matches(&format!("{local_sq:?}")).count(), 3, "{text}");
        assert!(text.contains("KeyValue"), "{text}");
        // The local `const g` shadows the local function in its block.
        assert_eq!(text.matches(&format!("{local_g:?}")).count(), 2, "{text}");
        // Control: the module function keeps its name.
        assert_eq!(names[0], "sq");
    }
}
