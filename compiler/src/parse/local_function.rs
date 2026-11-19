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
//! A moved function keeps each fact of its declaring place that a check
//! of its body reads (§207.1 rule 2a). The result names the class body
//! that encloses it, for `private` and `#x` member access. A local
//! function whose name a declaration of its block or a parameter also
//! binds stays in its block, so the checker reports the duplicate
//! declaration in that scope.
//!
//! A local function that reads or writes a binding of an enclosing
//! function stays in its block. The result records the first such read,
//! and the checker rejects the declaration with it (§207.1 rule 4). A
//! call to a local function that stays in its block is such a read, so
//! the rule applies until no function changes. A name that a rejected
//! local declaration binds is not a read: the function stays in its
//! block with no diagnostic of its own, because the declaration has one.
//!
//! The walk does not enter a form that the checker rejects as a whole:
//! a function expression, a class expression, an object literal method
//! or accessor, and a local class, enum, or namespace. A local function
//! inside a function that stays in its block also stays there, because
//! the checker does not check that body.

use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_ast::ast;
use oxc_ast_visit::{walk_mut, VisitMut};
use std::collections::HashMap;

/// The first read that keeps a local function in its block (§207.1
/// rule 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalFunctionRead {
    /// The source name of the local function.
    pub function: String,
    /// The access as the diagnostic states it, for example "reads the
    /// local `k` of its enclosing function".
    pub read: String,
    /// The start of the read.
    pub at: u32,
}

/// Why a local function that the checker reaches stays in its block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalFunctionOutcome {
    /// It reads a binding of an enclosing function (§207.1 rule 4).
    Reads(LocalFunctionRead),
    /// Another declaration of its scope binds its name (§207.1 rule 2a).
    Duplicate,
    /// It names a binding of a rejected local declaration, which has a
    /// diagnostic of its own (§207.1 rule 4).
    Poisoned,
}

/// The facts that the front end gives the checker about local functions.
#[derive(Debug, Clone, Default)]
pub(crate) struct LocalFunctions {
    /// The outcome of each local function that stays in its block, keyed
    /// by the start of its name.
    pub outcomes: HashMap<u32, LocalFunctionOutcome>,
    /// The class body that encloses each moved local function, keyed by
    /// the start of its name (§207.1 rule 2a).
    pub classes: HashMap<u32, String>,
}

/// One local function.
struct Record {
    name: String,
    hidden: String,
    declared_at: u32,
    /// The enclosing local function.
    parent: Option<usize>,
    /// The function level of the parameters and the body.
    body_level: usize,
    /// The class body that encloses the function.
    class: Option<String>,
    /// Another declaration of its scope binds its name.
    duplicate: bool,
    /// It names a binding of a rejected local declaration.
    poisoned: bool,
    /// Reads and writes of a binding of an enclosing function, as the
    /// diagnostic states them.
    reads: Vec<(u32, String)>,
    /// References to a local function that an enclosing block declares,
    /// with the text that states the reference as a read.
    calls: Vec<(u32, usize, String)>,
}

enum Decision {
    Lifted,
    /// An enclosing local function stays in its block, so the checker
    /// does not reach this one.
    Buried,
    Reads(u32, String),
    Duplicate,
    Poisoned,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Lifted,
    Reads,
    Silent,
    Duplicate,
}

#[derive(Clone, Copy)]
enum Value {
    Local,
    Parameter,
    Function(usize),
    /// A name that a rejected local declaration binds.
    Rejected,
}

#[derive(Clone, Copy)]
enum TypeName {
    Parameter,
    Rejected,
}

#[derive(Default)]
struct Scope {
    level: usize,
    /// The declaration that owns the scope, as the diagnostic names it.
    owner: &'static str,
    values: HashMap<String, Value>,
    types: HashMap<String, TypeName>,
}

struct Walker<'a> {
    /// False for the resolution walk; true for the walk that moves
    /// functions and renames references.
    allocator: &'a Allocator,
    apply: bool,
    scopes: Vec<Scope>,
    level: usize,
    /// The local functions that enclose the current position.
    open: Vec<usize>,
    /// The local function whose `this` the current position reads.
    this_owner: Option<usize>,
    /// The class body that encloses the current position.
    class: Option<String>,
    path: Vec<String>,
    ordinals: HashMap<String, usize>,
    records: Vec<Record>,
    by_declaration: HashMap<u32, usize>,
    references: HashMap<u32, usize>,
    lifted: Vec<bool>,
    hoisted: Vec<oxc_allocator::Box<'a, ast::Function<'a>>>,
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

impl<'a> Walker<'a> {
    /// Decides which local functions move (§207.1 rules 2, 2a, and 4).
    fn decide(&self) -> Vec<Decision> {
        let mut state: Vec<State> = self
            .records
            .iter()
            .map(|record| {
                if record.duplicate {
                    State::Duplicate
                } else if !record.reads.is_empty() {
                    State::Reads
                } else if record.poisoned {
                    State::Silent
                } else {
                    State::Lifted
                }
            })
            .collect();
        loop {
            let mut changed = false;
            for index in 0..self.records.len() {
                let calls = &self.records[index].calls;
                let next = match state[index] {
                    State::Lifted | State::Silent
                        if calls.iter().any(|(_, g, _)| state[*g] == State::Reads) =>
                    {
                        State::Reads
                    }
                    State::Lifted
                        if calls.iter().any(|(_, g, _)| {
                            matches!(state[*g], State::Silent | State::Duplicate)
                        }) =>
                    {
                        State::Silent
                    }
                    current => current,
                };
                if next != state[index] {
                    state[index] = next;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        (0..self.records.len())
            .map(|index| {
                let mut parent = self.records[index].parent;
                while let Some(enclosing) = parent {
                    if state[enclosing] != State::Lifted {
                        return Decision::Buried;
                    }
                    parent = self.records[enclosing].parent;
                }
                let record = &self.records[index];
                match state[index] {
                    State::Lifted => Decision::Lifted,
                    State::Silent => Decision::Poisoned,
                    State::Duplicate => Decision::Duplicate,
                    State::Reads => {
                        let calls = record
                            .calls
                            .iter()
                            .filter(|(_, g, _)| state[*g] == State::Reads)
                            .map(|(at, _, read)| (*at, read.clone()));
                        record
                            .reads
                            .iter()
                            .cloned()
                            .chain(calls)
                            .min_by_key(|(at, _)| *at)
                            .map_or(Decision::Poisoned, |(at, read)| Decision::Reads(at, read))
                    }
                }
            })
            .collect()
    }

    fn push_scope(&mut self) {
        // A module-level block belongs to no function (§207.1 rule 4).
        let owner = if self.level == 0 { "block" } else { "function" };
        self.push_owned_scope(owner);
    }

    fn push_owned_scope(&mut self, owner: &'static str) {
        if !self.apply {
            self.scopes.push(Scope {
                level: self.level,
                owner,
                ..Scope::default()
            });
        }
    }

    fn pop_scope(&mut self) {
        if !self.apply {
            self.scopes.pop();
        }
    }

    /// Declares a value name. A local function and another declaration
    /// of one name in one scope are a duplicate declaration (§207.1 rule
    /// 2a), so each local function with the name stays in its block.
    fn declare_value(&mut self, name: &str, value: Value) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if let Some(previous) = scope.values.insert(name.to_owned(), value) {
            for declared in [previous, value] {
                if let Value::Function(record) = declared {
                    self.records[record].duplicate = true;
                }
            }
        }
    }

    fn declare_type(&mut self, name: &str, kind: TypeName) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.types.insert(name.to_owned(), kind);
        }
    }

    /// Records a read of a binding at `level` by every open local
    /// function that the binding encloses.
    fn read(&mut self, level: usize, at: u32, read: &str) {
        for &open in &self.open {
            if self.records[open].body_level > level {
                self.records[open].reads.push((at, read.to_owned()));
            }
        }
    }

    /// Records a name of a rejected local declaration at `level`.
    fn poison(&mut self, level: usize) {
        for &open in &self.open {
            if self.records[open].body_level > level {
                self.records[open].poisoned = true;
            }
        }
    }

    /// Resolves a value reference, or gives it its hidden name. `write`
    /// marks an assignment target.
    fn value(&mut self, id: &mut ast::IdentifierReference<'a>, write: bool) {
        if self.apply {
            if let Some(&record) = self.references.get(&id.span.start) {
                if self.lifted[record] {
                    id.name = self
                        .allocator
                        .alloc_str(&self.records[record].hidden)
                        .into();
                }
            }
            return;
        }
        let name = id.name.as_ref();
        let Some((level, owner, value)) = self.scopes.iter().rev().find_map(|scope| {
            scope
                .values
                .get(name)
                .map(|value| (scope.level, scope.owner, *value))
        }) else {
            return;
        };
        let verb = if write { "writes" } else { "reads" };
        match value {
            Value::Local => self.read(
                level,
                id.span.start,
                &format!("{verb} the local `{name}` of its enclosing {owner}"),
            ),
            Value::Parameter => self.read(
                level,
                id.span.start,
                &format!("{verb} the parameter `{name}` of its enclosing {owner}"),
            ),
            Value::Rejected => self.poison(level),
            Value::Function(record) => {
                self.references.insert(id.span.start, record);
                let read = format!("reads the local function `{name}` of its enclosing {owner}");
                for &open in &self.open {
                    if open != record && self.records[open].body_level > level {
                        self.records[open]
                            .calls
                            .push((id.span.start, record, read.clone()));
                    }
                }
            }
        }
    }

    fn type_name(&mut self, name: &str, at: u32) {
        if self.apply {
            return;
        }
        let Some((level, owner, kind)) = self.scopes.iter().rev().find_map(|scope| {
            scope
                .types
                .get(name)
                .map(|kind| (scope.level, scope.owner, *kind))
        }) else {
            return;
        };
        match kind {
            TypeName::Rejected => self.poison(level),
            TypeName::Parameter => self.read(
                level,
                at,
                &format!("reads the type parameter `{name}` of its enclosing {owner}"),
            ),
        }
    }

    fn declare_type_params(&mut self, params: Option<&mut ast::TSTypeParameterDeclaration<'a>>) {
        let Some(params) = params else {
            return;
        };
        if !self.apply {
            for param in &params.params {
                self.declare_type(param.name.name.as_str(), TypeName::Parameter);
            }
        }
        for param in &mut params.params {
            if let Some(constraint) = &mut param.constraint {
                self.visit_ts_type(constraint);
            }
        }
    }

    fn signature_parameters(&mut self, params: &mut ast::FormalParameters<'a>) {
        for parameter in &mut params.items {
            if let Some(ty) = &mut parameter.type_annotation {
                self.visit_ts_type(&mut ty.type_annotation);
            }
        }
        if let Some(rest) = &mut params.rest {
            if let Some(ty) = &mut rest.type_annotation {
                self.visit_ts_type(&mut ty.type_annotation);
            }
        }
    }
    fn this_parameter(&mut self, parameter: Option<&mut ast::TSThisParameter<'a>>) {
        if let Some(parameter) = parameter {
            if let Some(ty) = &mut parameter.type_annotation {
                self.visit_ts_type(&mut ty.type_annotation);
            }
        }
    }
    fn enter_level(&mut self, owner: Option<usize>) -> Option<usize> {
        self.level += 1;
        std::mem::replace(&mut self.this_owner, owner)
    }
    fn leave_level(&mut self, saved: Option<usize>) {
        self.level -= 1;
        self.this_owner = saved;
    }

    fn module_item(&mut self, item: &mut ast::Statement<'a>) {
        match item {
            ast::Statement::FunctionDeclaration(function) => {
                self.path = vec![function
                    .id
                    .as_ref()
                    .map_or("", |id| id.name.as_str())
                    .to_owned()];
                self.function(function, None);
            }
            ast::Statement::ClassDeclaration(class) => {
                let name = class
                    .id
                    .as_ref()
                    .map_or("", |id| id.name.as_str())
                    .to_owned();
                self.path = vec![name.clone()];
                self.class = Some(name);
                self.module_class(class);
                self.class = None;
            }
            ast::Statement::VariableDeclaration(var) => {
                self.path = vec![variable_path(var)];
                self.variables(var, None);
            }
            ast::Statement::ExportDeclaration(export) => {
                {
                    let declaration = &mut export.declaration;
                    // Declaration and statement share their inherited variants in oxc.
                    match declaration {
                        ast::Declaration::FunctionDeclaration(function) => {
                            self.path = vec![function
                                .id
                                .as_ref()
                                .map_or("", |id| id.name.as_str())
                                .to_owned()];
                            self.function(function, None);
                        }
                        ast::Declaration::ClassDeclaration(class) => {
                            let name = class
                                .id
                                .as_ref()
                                .map_or("", |id| id.name.as_str())
                                .to_owned();
                            self.path = vec![name.clone()];
                            self.class = Some(name);
                            self.module_class(class);
                            self.class = None;
                        }
                        ast::Declaration::VariableDeclaration(var) => {
                            self.path = vec![variable_path(var)];
                            self.variables(var, None);
                        }
                        _ => {}
                    }
                }
            }
            ast::Statement::TSNamespaceDeclaration(_)
            | ast::Statement::TSExternalModuleDeclaration(_)
            | ast::Statement::TSGlobalDeclaration(_)
            | ast::Statement::TSEnumDeclaration(_)
            | ast::Statement::TSInterfaceDeclaration(_)
            | ast::Statement::TSTypeAliasDeclaration(_) => {}
            item if item.is_module_declaration() => {}
            item => {
                self.path = vec!["top".to_owned()];
                self.visit_statement(item);
            }
        }
    }

    fn module_class(&mut self, class: &mut ast::Class<'a>) {
        let path = self.path.clone();
        self.push_owned_scope("class");
        self.declare_type_params(class.type_parameters.as_deref_mut());
        for member in &mut class.body.body {
            match member {
                ast::ClassElement::MethodDefinition(method)
                    if !matches!(method.key, ast::PropertyKey::PrivateIdentifier(_)) =>
                {
                    self.path = path.clone();
                    self.path
                        .push(if method.kind == ast::MethodDefinitionKind::Constructor {
                            "constructor".to_owned()
                        } else {
                            prop_label(&method.key, method.computed)
                        });
                    self.function(&mut method.value, None);
                }
                ast::ClassElement::PropertyDefinition(prop)
                    if !matches!(prop.key, ast::PropertyKey::PrivateIdentifier(_)) =>
                {
                    self.path = path.clone();
                    self.path.push(prop_label(&prop.key, prop.computed));
                    if let Some(ty) = &mut prop.type_annotation {
                        self.visit_ts_type(&mut ty.type_annotation);
                    }
                    if let Some(value) = &mut prop.value {
                        let saved = self.enter_level(None);
                        self.visit_expression(value);
                        self.leave_level(saved);
                    }
                }
                _ => {}
            }
        }
        self.pop_scope();
        self.path = path;
    }

    fn parameters(&mut self, params: &mut ast::FormalParameters<'a>, value: Option<Value>) {
        for param in &mut params.items {
            self.binding_pat(&mut param.pattern, value);
            if let Some(ty) = &mut param.type_annotation {
                self.visit_ts_type(&mut ty.type_annotation);
            }
            if let Some(init) = &mut param.initializer {
                self.visit_expression(init);
            }
        }
        if let Some(rest) = &mut params.rest {
            self.binding_pat(&mut rest.rest.argument, value);
            if let Some(ty) = &mut rest.type_annotation {
                self.visit_ts_type(&mut ty.type_annotation);
            }
        }
    }

    fn function(&mut self, function: &mut ast::Function<'a>, local: Option<usize>) {
        let saved = self.enter_level(local);
        self.push_scope();
        self.declare_type_params(function.type_parameters.as_deref_mut());
        self.this_parameter(function.this_param.as_deref_mut());
        self.parameters(&mut function.params, Some(Value::Parameter));
        if let Some(ty) = &mut function.return_type {
            self.visit_ts_type(&mut ty.type_annotation);
        }
        if let Some(body) = &mut function.body {
            self.body(&mut body.statements);
        }
        self.pop_scope();
        self.leave_level(saved);
    }

    fn local_function(&mut self, function: &mut ast::Function<'a>) {
        let Some(id) = &function.id else {
            return;
        };
        let Some(&record) = self.by_declaration.get(&id.span.start) else {
            return;
        };
        if self.apply && !self.lifted[record] {
            return;
        }
        self.open.push(record);
        self.path.push(id.name.to_string());
        self.function(function, Some(record));
        self.path.pop();
        self.open.pop();
    }

    fn declare_block<'p>(&mut self, stmts: impl Iterator<Item = &'p ast::Statement<'a>>)
    where
        'a: 'p,
    {
        if self.apply {
            return;
        }
        for statement in stmts {
            match statement {
                ast::Statement::VariableDeclaration(var) => {
                    for declaration in &var.declarations {
                        self.declare_pat(&declaration.id, Value::Local);
                    }
                }
                ast::Statement::FunctionDeclaration(function) => {
                    let Some(id) = &function.id else {
                        continue;
                    };
                    let name = id.name.to_string();
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
                        declared_at: id.span.start,
                        parent: self.open.last().copied(),
                        body_level: self.level + 1,
                        class: self.class.clone(),
                        duplicate: false,
                        poisoned: false,
                        reads: Vec::new(),
                        calls: Vec::new(),
                    });
                    self.by_declaration.insert(id.span.start, record);
                    self.declare_value(&name, Value::Function(record));
                }
                ast::Statement::ClassDeclaration(class) => {
                    if let Some(id) = &class.id {
                        self.declare_value(id.name.as_str(), Value::Rejected);
                        self.declare_type(id.name.as_str(), TypeName::Rejected);
                    }
                }
                ast::Statement::TSEnumDeclaration(enumeration) => {
                    self.declare_value(enumeration.id.name.as_str(), Value::Rejected);
                    self.declare_type(enumeration.id.name.as_str(), TypeName::Rejected);
                }
                ast::Statement::TSInterfaceDeclaration(interface) => {
                    self.declare_type(interface.id.name.as_str(), TypeName::Rejected)
                }
                ast::Statement::TSTypeAliasDeclaration(alias) => {
                    self.declare_type(alias.id.name.as_str(), TypeName::Rejected)
                }
                ast::Statement::TSNamespaceDeclaration(namespace) => {
                    self.declare_value(namespace.id.name.as_str(), Value::Rejected);
                }
                _ => {}
            }
        }
    }

    fn declare_pat(&mut self, pattern: &ast::BindingPattern<'a>, value: Value) {
        match pattern {
            ast::BindingPattern::BindingIdentifier(id) => {
                self.declare_value(id.name.as_str(), value)
            }
            ast::BindingPattern::AssignmentPattern(assign) => self.declare_pat(&assign.left, value),
            ast::BindingPattern::ArrayPattern(array) => {
                for element in array.elements.iter().flatten() {
                    self.declare_pat(element, value);
                }
                if let Some(rest) = &array.rest {
                    self.declare_pat(&rest.argument, value);
                }
            }
            ast::BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    self.declare_pat(&property.value, value);
                }
                if let Some(rest) = &object.rest {
                    self.declare_pat(&rest.argument, value);
                }
            }
        }
    }

    fn binding_pat(&mut self, pattern: &mut ast::BindingPattern<'a>, value: Option<Value>) {
        if !self.apply {
            if let Some(value) = value {
                self.declare_pat(pattern, value);
            }
        }
        // The visitor reads computed keys and defaults, but never reads a binding identifier.
        walk_mut::walk_binding_pattern(self, pattern);
    }

    fn variables(&mut self, variables: &mut ast::VariableDeclaration<'a>, value: Option<Value>) {
        for declaration in &mut variables.declarations {
            self.binding_pat(&mut declaration.id, value);
            if let Some(ty) = &mut declaration.type_annotation {
                self.visit_ts_type(&mut ty.type_annotation);
            }
            if let Some(init) = &mut declaration.init {
                self.visit_expression(init);
            }
        }
    }

    fn body(&mut self, statements: &mut ArenaVec<'a, ast::Statement<'a>>) {
        self.declare_block(statements.iter());
        self.block_items(statements);
    }

    fn block_items(&mut self, statements: &mut ArenaVec<'a, ast::Statement<'a>>) {
        let mut index = 0;
        while index < statements.len() {
            if self.apply {
                if let ast::Statement::FunctionDeclaration(function) = &statements[index] {
                    if let Some(record) = function
                        .id
                        .as_ref()
                        .and_then(|id| self.by_declaration.get(&id.span.start))
                        .copied()
                    {
                        if self.lifted[record] {
                            if let ast::Statement::FunctionDeclaration(mut function) =
                                statements.remove(index)
                            {
                                self.local_function(&mut function);
                                if let Some(id) = &mut function.id {
                                    id.name = self
                                        .allocator
                                        .alloc_str(&self.records[record].hidden)
                                        .into();
                                }
                                self.hoisted.push(function);
                            }
                            continue;
                        }
                    }
                }
            }
            self.visit_statement(&mut statements[index]);
            index += 1;
        }
    }

    fn for_left(&mut self, left: &mut ast::ForStatementLeft<'a>) {
        if let ast::ForStatementLeft::VariableDeclaration(var) = left {
            for declaration in &mut var.declarations {
                self.binding_pat(&mut declaration.id, Some(Value::Local));
            }
        } else if let Some(target) = left.as_assignment_target_mut() {
            self.visit_assignment_target(target);
        }
    }
}

fn variable_path(var: &ast::VariableDeclaration<'_>) -> String {
    if matches!(
        var.kind,
        ast::VariableDeclarationKind::Using | ast::VariableDeclarationKind::AwaitUsing
    ) {
        return "top".to_owned();
    }
    var.declarations
        .first()
        .and_then(|declaration| match &declaration.id {
            ast::BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "top".to_owned())
}

fn prop_label(key: &ast::PropertyKey<'_>, computed: bool) -> String {
    if computed {
        return "[computed]".to_owned();
    }
    match key {
        ast::PropertyKey::StaticIdentifier(name) => name.name.to_string(),
        ast::PropertyKey::StringLiteral(value) => value.value.to_string(),
        ast::PropertyKey::NumericLiteral(value) => value.value.to_string(),
        _ => "[computed]".to_owned(),
    }
}

impl<'a> VisitMut<'a> for Walker<'a> {
    fn visit_statement(&mut self, statement: &mut ast::Statement<'a>) {
        match statement {
            ast::Statement::BlockStatement(block) => {
                self.push_scope();
                self.body(&mut block.body);
                self.pop_scope();
            }
            ast::Statement::FunctionDeclaration(function) => self.local_function(function),
            ast::Statement::VariableDeclaration(var) => self.variables(var, None),
            ast::Statement::ClassDeclaration(_)
            | ast::Statement::TSNamespaceDeclaration(_)
            | ast::Statement::TSExternalModuleDeclaration(_)
            | ast::Statement::TSGlobalDeclaration(_)
            | ast::Statement::TSEnumDeclaration(_)
            | ast::Statement::TSInterfaceDeclaration(_)
            | ast::Statement::TSTypeAliasDeclaration(_) => {}
            ast::Statement::ForStatement(node) => {
                self.push_scope();
                if let Some(init) = &mut node.init {
                    if let ast::ForStatementInit::VariableDeclaration(var) = init {
                        self.variables(var, Some(Value::Local));
                    } else if let Some(expression) = init.as_expression_mut() {
                        self.visit_expression(expression);
                    }
                }
                if let Some(test) = &mut node.test {
                    self.visit_expression(test);
                }
                if let Some(update) = &mut node.update {
                    self.visit_expression(update);
                }
                self.visit_statement(&mut node.body);
                self.pop_scope();
            }
            ast::Statement::ForInStatement(node) => {
                self.visit_expression(&mut node.right);
                self.push_scope();
                self.for_left(&mut node.left);
                self.visit_statement(&mut node.body);
                self.pop_scope();
            }
            ast::Statement::ForOfStatement(node) => {
                self.visit_expression(&mut node.right);
                self.push_scope();
                self.for_left(&mut node.left);
                self.visit_statement(&mut node.body);
                self.pop_scope();
            }
            ast::Statement::SwitchStatement(node) => {
                self.visit_expression(&mut node.discriminant);
                self.push_scope();
                self.declare_block(node.cases.iter().flat_map(|case| case.consequent.iter()));
                for case in &mut node.cases {
                    if let Some(test) = &mut case.test {
                        self.visit_expression(test);
                    }
                    self.block_items(&mut case.consequent);
                }
                self.pop_scope();
            }
            ast::Statement::TryStatement(node) => {
                self.push_scope();
                self.body(&mut node.block.body);
                self.pop_scope();
                if let Some(handler) = &mut node.handler {
                    self.push_scope();
                    if let Some(param) = &mut handler.param {
                        self.binding_pat(&mut param.pattern, Some(Value::Local));
                    }
                    self.push_scope();
                    self.body(&mut handler.body.body);
                    self.pop_scope();
                    self.pop_scope();
                }
                if let Some(finalizer) = &mut node.finalizer {
                    self.push_scope();
                    self.body(&mut finalizer.body);
                    self.pop_scope();
                }
            }
            _ => walk_mut::walk_statement(self, statement),
        }
    }
    fn visit_expression(&mut self, expression: &mut ast::Expression<'a>) {
        match expression {
            ast::Expression::FunctionExpression(_) | ast::Expression::ClassExpression(_) => {}
            ast::Expression::ArrowFunctionExpression(arrow) => {
                let saved = self.enter_level(self.this_owner);
                self.push_scope();
                self.declare_type_params(arrow.type_parameters.as_deref_mut());
                self.parameters(&mut arrow.params, Some(Value::Parameter));
                if let Some(ty) = &mut arrow.return_type {
                    self.visit_ts_type(&mut ty.type_annotation);
                }
                match &mut arrow.body {
                    ast::ArrowFunctionBody::FunctionBody(body) => self.body(&mut body.statements),
                    body => {
                        if let Some(expression) = body.as_expression_mut() {
                            self.visit_expression(expression);
                        }
                    }
                }
                self.pop_scope();
                self.leave_level(saved);
            }
            ast::Expression::ThisExpression(this) => {
                if let (Some(owner), false) = (self.this_owner, self.apply) {
                    self.records[owner].reads.push((
                        this.span.start,
                        "reads `this` of its enclosing function".to_owned(),
                    ));
                }
            }
            _ => walk_mut::walk_expression(self, expression),
        }
    }
    fn visit_identifier_reference(&mut self, id: &mut ast::IdentifierReference<'a>) {
        self.value(id, false);
    }
    fn visit_assignment_target(&mut self, target: &mut ast::AssignmentTarget<'a>) {
        if let ast::AssignmentTarget::AssignmentTargetIdentifier(id) = target {
            self.value(id, true);
        } else {
            walk_mut::walk_assignment_target(self, target);
        }
    }
    fn visit_simple_assignment_target(&mut self, target: &mut ast::SimpleAssignmentTarget<'a>) {
        if let ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = target {
            self.value(id, true);
        } else {
            walk_mut::walk_simple_assignment_target(self, target);
        }
    }
    fn visit_assignment_target_property_identifier(
        &mut self,
        property: &mut ast::AssignmentTargetPropertyIdentifier<'a>,
    ) {
        self.value(&mut property.binding, true);
        if let Some(init) = &mut property.init {
            self.visit_expression(init);
        }
    }
    fn visit_object_property(&mut self, property: &mut ast::ObjectProperty<'a>) {
        if property.method || property.kind != ast::PropertyKind::Init {
            return;
        }
        if property.computed {
            self.visit_property_key(&mut property.key);
        }
        self.visit_expression(&mut property.value);
        if property.shorthand {
            if let (ast::PropertyKey::StaticIdentifier(key), ast::Expression::Identifier(value)) =
                (&property.key, &property.value)
            {
                if key.name != value.name {
                    property.shorthand = false;
                }
            }
        }
    }
    fn visit_ts_type(&mut self, ty: &mut ast::TSType<'a>) {
        match ty {
            ast::TSType::TSTypeReference(reference) => {
                let mut name = &reference.type_name;
                while let ast::TSTypeName::QualifiedName(qualified) = name {
                    name = &qualified.left;
                }
                if let ast::TSTypeName::IdentifierReference(id) = name {
                    self.type_name(id.name.as_str(), id.span.start);
                }
                if let Some(args) = &mut reference.type_arguments {
                    for arg in &mut args.params {
                        self.visit_ts_type(arg);
                    }
                }
            }
            ast::TSType::TSTypeQuery(query) => {
                if let ast::TSTypeQueryExprName::IdentifierReference(id) = &mut query.expr_name {
                    self.value(id, false);
                }
                if let Some(args) = &mut query.type_arguments {
                    for arg in &mut args.params {
                        self.visit_ts_type(arg);
                    }
                }
            }
            ast::TSType::TSFunctionType(function) => {
                self.this_parameter(function.this_param.as_deref_mut());
                self.signature_parameters(&mut function.params);
                self.visit_ts_type(&mut function.return_type.type_annotation);
            }
            ast::TSType::TSConstructorType(function) => {
                self.signature_parameters(&mut function.params);
                self.visit_ts_type(&mut function.return_type.type_annotation);
            }
            ast::TSType::TSTypeLiteral(literal) => {
                for member in &mut literal.members {
                    match member {
                        ast::TSSignature::TSPropertySignature(property) => {
                            if let Some(ty) = &mut property.type_annotation {
                                self.visit_ts_type(&mut ty.type_annotation);
                            }
                        }
                        ast::TSSignature::TSMethodSignature(method)
                            if method.kind == ast::TSMethodSignatureKind::Method =>
                        {
                            self.this_parameter(method.this_param.as_deref_mut());
                            self.signature_parameters(&mut method.params);
                            if let Some(ty) = &mut method.return_type {
                                self.visit_ts_type(&mut ty.type_annotation);
                            }
                        }
                        ast::TSSignature::TSIndexSignature(index) => {
                            self.visit_ts_type(
                                &mut index.parameter.type_annotation.type_annotation,
                            );
                            self.visit_ts_type(&mut index.type_annotation.type_annotation);
                        }
                        _ => {}
                    }
                }
            }
            ast::TSType::TSInferType(_)
            | ast::TSType::TSMappedType(_)
            | ast::TSType::TSLiteralType(_)
            | ast::TSType::TSTypePredicate(_)
            | ast::TSType::TSImportType(_) => {}
            _ => walk_mut::walk_ts_type(self, ty),
        }
    }
}

/// Moves capture-free local functions into module scope (compiler.md §207 and §341 rule 5).
pub(crate) fn lower_module<'a>(
    allocator: &'a Allocator,
    module: &mut ast::Program<'a>,
) -> LocalFunctions {
    let mut walker = Walker {
        allocator,
        apply: false,
        scopes: Vec::new(),
        level: 0,
        open: Vec::new(),
        this_owner: None,
        class: None,
        path: Vec::new(),
        ordinals: HashMap::new(),
        records: Vec::new(),
        by_declaration: HashMap::new(),
        references: HashMap::new(),
        lifted: Vec::new(),
        hoisted: Vec::new(),
    };
    for item in &mut module.body {
        walker.module_item(item);
    }
    let status = walker.decide();
    let mut facts = LocalFunctions::default();
    for (record, decision) in walker.records.iter().zip(&status) {
        let outcome = match decision {
            Decision::Lifted => {
                if let Some(class) = &record.class {
                    facts.classes.insert(record.declared_at, class.clone());
                }
                continue;
            }
            Decision::Buried => continue,
            Decision::Reads(at, read) => LocalFunctionOutcome::Reads(LocalFunctionRead {
                function: record.name.clone(),
                read: read.clone(),
                at: *at,
            }),
            Decision::Duplicate => LocalFunctionOutcome::Duplicate,
            Decision::Poisoned => LocalFunctionOutcome::Poisoned,
        };
        facts.outcomes.insert(record.declared_at, outcome);
    }
    walker.lifted = status
        .iter()
        .map(|decision| matches!(decision, Decision::Lifted))
        .collect();
    walker.apply = true;
    let items = std::mem::replace(&mut module.body, ArenaVec::new_in(&allocator));
    for mut item in items {
        walker.path.clear();
        walker.module_item(&mut item);
        for function in walker.hoisted.drain(..) {
            module
                .body
                .push(ast::Statement::FunctionDeclaration(function));
        }
        module.body.push(item);
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn parse<'a>(allocator: &'a Allocator, source: &'a str) -> ast::Program<'a> {
        let parsed = oxc_parser::Parser::new(
            allocator,
            source,
            oxc_span::SourceType::ts().with_module(true),
        )
        .parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.program
    }
    fn function_names(module: &ast::Program<'_>) -> Vec<String> {
        module
            .body
            .iter()
            .filter_map(|item| match item {
                ast::Statement::FunctionDeclaration(f) => {
                    f.id.as_ref().map(|id| id.name.to_string())
                }
                _ => None,
            })
            .collect()
    }
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn each_reference_takes_the_name_of_the_function_its_block_declares() {
        let allocator = Allocator::default();
        let mut module = parse(&allocator, SOURCE);
        let facts = lower_module(&allocator, &mut module);
        assert!(facts.outcomes.is_empty(), "{facts:?}");
        let names = function_names(&module);
        assert_eq!(names.len(), 4, "{names:?}");
        let (local_sq, local_g) = (&names[1], &names[2]);
        assert!(local_sq.starts_with("[[identity:local:") && local_sq.ends_with("]]sq"));
        assert!(local_g.starts_with("[[identity:local:") && local_g.ends_with("]]g"));
        let text = format!("{module:?}");
        assert_eq!(text.matches(&format!("{local_sq:?}")).count(), 3, "{text}");
        assert!(text.contains("shorthand: false"), "{text}");
        assert_eq!(text.matches(&format!("{local_g:?}")).count(), 2, "{text}");
        assert_eq!(names[0], "sq");
    }
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn captures_duplicates_poisoned_names_and_class_context_keep_their_facts() {
        let allocator = Allocator::default();
        for (source, kind) in [
            ("function f(p: i32) { function g() { return p; } }", "reads"),
            (
                "function f() { let n = 1; function g() { n++; } }",
                "writes",
            ),
            ("function f<T>() { function g(x: T) {} }", "type"),
            ("function f() { function g() { return this; } }", "this"),
            ("function f() { const g = 1; function g() {} }", "duplicate"),
            (
                "function f() { class C {} function g() { return C; } }",
                "poison",
            ),
            (
                "function f(p: i32) { function g() { return p; } function h() { return g(); } }",
                "transitive",
            ),
        ] {
            let mut module = parse(&allocator, source);
            let facts = lower_module(&allocator, &mut module);
            assert!(!facts.outcomes.is_empty(), "{source}");
            match kind {
                "duplicate" => assert!(facts
                    .outcomes
                    .values()
                    .any(|fact| matches!(fact, LocalFunctionOutcome::Duplicate))),
                "poison" => assert!(facts
                    .outcomes
                    .values()
                    .any(|fact| matches!(fact, LocalFunctionOutcome::Poisoned))),
                "transitive" => assert_eq!(facts.outcomes.len(), 2),
                _ => assert!(
                    facts
                        .outcomes
                        .values()
                        .any(|fact| matches!(fact, LocalFunctionOutcome::Reads(_))),
                    "{facts:?}"
                ),
            }
            assert_eq!(function_names(&module).len(), 1);
        }
        let mut module = parse(&allocator, "class C { private x: i32 = 1; m() { function g(c: C) { return c.x; } return g(this); } }");
        let facts = lower_module(&allocator, &mut module);
        assert!(facts.outcomes.is_empty());
        assert_eq!(facts.classes.values().collect::<Vec<_>>(), ["C"]);
        assert_eq!(function_names(&module).len(), 1);
    }
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn type_signature_constraints_and_unchecked_members_keep_their_capture_facts() {
        let allocator = Allocator::default();
        for annotation in [
            "<U extends T>(u: U) => void",
            "{ (): T }",
            "{ new(): T }",
            "{ get value(): T }",
        ] {
            let source =
                format!("function outer<T>() {{ function local(x: {annotation}): void {{}} }}");
            let mut module = parse(&allocator, allocator.alloc_str(&source));
            let facts = lower_module(&allocator, &mut module);
            assert!(facts.outcomes.is_empty(), "{annotation}: {facts:?}");
            assert_eq!(function_names(&module).len(), 2);
        }
        for annotation in [
            "T",
            "{ value: T }",
            "{ method(x: T): void }",
            "(x: T) => void",
            "{ [key: string]: T }",
        ] {
            let source =
                format!("function outer<T>() {{ function local(x: {annotation}): void {{}} }}");
            let mut module = parse(&allocator, allocator.alloc_str(&source));
            let facts = lower_module(&allocator, &mut module);
            assert!(facts.outcomes.values().any(|outcome| matches!(outcome, LocalFunctionOutcome::Reads(read) if read.read.contains("type parameter `T`"))), "{annotation}: {facts:?}");
            assert_eq!(function_names(&module).len(), 1);
        }
    }
    // Cost (debug): <0.01 s in the isolated front-end harness.
    #[test]
    fn module_variable_kinds_keep_hidden_name_paths() {
        let allocator = Allocator::default();
        for (kind, path) in [
            ("using", "top"),
            ("await using", "top"),
            ("const", "resource"),
        ] {
            let source = format!(
                "{kind} resource = (() => {{ function f() {{ return 1; }} return f(); }})();"
            );
            let mut module = parse(&allocator, allocator.alloc_str(&source));
            let facts = lower_module(&allocator, &mut module);
            assert!(facts.outcomes.is_empty());
            assert_eq!(
                function_names(&module),
                [format!(
                    "[[identity:local:{}]]f",
                    hex(&format!("{path}/f#0"))
                )]
            );
        }
    }
}
