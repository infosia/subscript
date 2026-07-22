//! The semantic checker: enforces the collision rules (C1–C8) and the
//! Q-register resolutions, and produces the typed HIR.
//!
//! Structure: pass A collects top-level names per file and resolves
//! imports; pass B resolves declared signatures and class shapes; pass C
//! checks bodies in source order and builds the HIR. Generic
//! declarations are registered as templates in pass A/B and
//! monomorphized on first use (`identity<i32>`, `Box<f64>`).

mod expr;
mod stmt;
mod tyres;

use std::collections::{HashMap, HashSet};

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::hir;
use crate::parse::ParsedProgram;
use crate::types::{ClassId, EnumId, Type};

/// One declared parameter in a signature.
#[derive(Debug, Clone)]
pub(crate) struct ParamSig {
    pub name: String,
    pub ty: Type,
    pub has_default: bool,
}

/// A resolved function signature.
#[derive(Debug, Clone)]
pub(crate) struct FnSig {
    pub params: Vec<ParamSig>,
    /// Return type; `Generator<Y>` for generators once the yield type is
    /// inferred from the body.
    pub ret: Type,
    pub is_generator: bool,
    /// True once a generator's yield type has been inferred (generators
    /// are checked in source order; a call before that point is
    /// rejected).
    pub yield_known: bool,
}

/// Checker-side class information (the HIR `ClassDef` holds the fields;
/// this holds callable signatures).
#[derive(Debug, Clone, Default)]
pub(crate) struct ClassSig {
    pub ctor: Option<Vec<ParamSig>>,
    pub methods: HashMap<String, FnSig>,
}

/// A module-level variable's declared shape.
#[derive(Debug, Clone)]
pub(crate) struct GlobalSig {
    pub ty: Type,
    pub mutable: bool,
}

/// A generic function template awaiting monomorphization.
#[derive(Debug, Clone)]
pub(crate) struct GenericFn {
    pub file: usize,
    pub type_params: Vec<String>,
    pub function: ast::Function,
}

/// A generic class template awaiting monomorphization.
#[derive(Debug, Clone)]
pub(crate) struct GenericClass {
    pub file: usize,
    pub is_value: bool,
    pub type_params: Vec<String>,
    pub class: ast::Class,
    pub pos: Pos,
}

/// What a top-level name refers to inside one file's scope.
#[derive(Debug, Clone)]
pub(crate) enum ScopeItem {
    Func(String),
    GenericFunc(String),
    Class(ClassId),
    GenericClass(String),
    Enum(EnumId),
    Global(String),
}

/// A local binding inside a function body.
#[derive(Debug, Clone)]
pub(crate) struct Local {
    pub ty: Type,
    pub mutable: bool,
    /// True when the binding holds a capturing lambda; such a binding
    /// may be called and passed downward but may not escape (C5).
    pub holds_capturing: bool,
}

/// One lexical scope. `fn_boundary` marks the start of a lambda body:
/// lookups that cross it are captures.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    pub vars: HashMap<String, Local>,
    pub fn_boundary: bool,
}

/// One function (or lambda) frame.
#[derive(Debug)]
pub(crate) struct Frame {
    pub ret: Type,
    pub is_generator: bool,
    pub yield_ty: Option<Type>,
    pub is_lambda: bool,
    pub captures: Vec<String>,
    pub this_ty: Option<Type>,
}

/// Per-body checking state: scope stack, frames, and the C7 narrowing
/// set (path keys currently known non-null).
#[derive(Debug)]
pub(crate) struct FnCtx {
    pub frames: Vec<Frame>,
    pub scopes: Vec<Scope>,
    pub narrowed: HashSet<String>,
    pub loop_depth: u32,
    pub switch_depth: u32,
}

impl FnCtx {
    pub(crate) fn new(ret: Type, is_generator: bool, this_ty: Option<Type>) -> Self {
        FnCtx {
            frames: vec![Frame {
                ret,
                is_generator,
                yield_ty: None,
                is_lambda: false,
                captures: Vec::new(),
                this_ty,
            }],
            scopes: vec![Scope::default()],
            narrowed: HashSet::new(),
            loop_depth: 0,
            switch_depth: 0,
        }
    }

    pub(crate) fn declare(&mut self, name: &str, local: Local) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name.to_string(), local);
        }
    }

    /// Marks a bound local as holding a capturing lambda.
    pub(crate) fn taint_capturing(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(local) = scope.vars.get_mut(name) {
                local.holds_capturing = true;
                return;
            }
        }
    }
}

/// The checker.
pub(crate) struct Checker<'p> {
    pub prog: &'p ParsedProgram,
    pub diags: Vec<Diagnostic>,
    pub classes: Vec<hir::ClassDef>,
    pub class_sigs: Vec<ClassSig>,
    pub class_ids: HashMap<String, ClassId>,
    pub enums: Vec<hir::EnumDef>,
    pub enum_ids: HashMap<String, EnumId>,
    pub fn_sigs: HashMap<String, FnSig>,
    pub functions: Vec<hir::Function>,
    pub global_sigs: HashMap<String, GlobalSig>,
    pub globals: Vec<hir::Global>,
    pub generic_fns: HashMap<String, GenericFn>,
    pub generic_classes: HashMap<String, GenericClass>,
    pub file_scopes: Vec<HashMap<String, ScopeItem>>,
    pub exports: Vec<HashSet<String>>,
    pub exported_fns: HashSet<String>,
    pub top_level: Vec<hir::Stmt>,
    pub cur_file: usize,
    pub subst: HashMap<String, Type>,
}

/// Runs the checker over a parsed program.
pub(crate) fn run(prog: &ParsedProgram) -> Result<hir::Module, Vec<Diagnostic>> {
    let mut ck = Checker {
        prog,
        diags: Vec::new(),
        classes: Vec::new(),
        class_sigs: Vec::new(),
        class_ids: HashMap::new(),
        enums: Vec::new(),
        enum_ids: HashMap::new(),
        fn_sigs: HashMap::new(),
        functions: Vec::new(),
        global_sigs: HashMap::new(),
        globals: Vec::new(),
        generic_fns: HashMap::new(),
        generic_classes: HashMap::new(),
        file_scopes: Vec::new(),
        exports: Vec::new(),
        exported_fns: HashSet::new(),
        top_level: Vec::new(),
        cur_file: 0,
        subst: HashMap::new(),
    };

    for i in 0..prog.files.len() {
        ck.cur_file = i;
        ck.collect_file(i);
    }
    ck.resolve_imports();
    for i in 0..prog.files.len() {
        ck.cur_file = i;
        ck.subst.clear();
        ck.resolve_signatures(i);
    }
    for i in 0..prog.files.len() {
        ck.cur_file = i;
        ck.subst.clear();
        ck.check_bodies(i);
    }

    if ck.diags.is_empty() {
        Ok(hir::Module {
            classes: ck.classes,
            enums: ck.enums,
            globals: ck.globals,
            functions: ck.functions,
            top_level: ck.top_level,
        })
    } else {
        Err(ck.diags)
    }
}

impl<'p> Checker<'p> {
    pub(crate) fn error(&mut self, code: RuleCode, message: impl Into<String>, pos: Pos) {
        self.diags.push(Diagnostic::new(code, message, pos));
    }

    pub(crate) fn pos(&self, span: swc_common::Span) -> Pos {
        self.prog.pos(span)
    }

    /// Renders a type with real class/enum names, for messages.
    pub(crate) fn type_name(&self, ty: &Type) -> String {
        crate::types::display_type(
            ty,
            &|id| {
                self.classes
                    .get(id.0)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| format!("<class #{}>", id.0))
            },
            &|id| {
                self.enums
                    .get(id.0)
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| format!("<enum #{}>", id.0))
            },
        )
    }

    pub(crate) fn is_value_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if self.classes[id.0].is_value)
    }

    pub(crate) fn is_reference_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if !self.classes[id.0].is_value)
    }

    /// Structural assignability under nominal semantics: exact type
    /// equality plus the decided widenings (`null`/`T` into `T | null`,
    /// reference classes into the boundary-opaque `object`).
    pub(crate) fn assignable(&self, from: &Type, to: &Type) -> bool {
        if matches!(from, Type::Error) || matches!(to, Type::Error) {
            return true;
        }
        if from == to {
            return true;
        }
        match (from, to) {
            (Type::Null, Type::Nullable(_)) => true,
            (f, Type::Nullable(inner)) => {
                f == &**inner || (self.is_reference_class(f) && **inner == Type::Object)
            }
            (f, Type::Object) => self.is_reference_class(f),
            _ => false,
        }
    }

    /// Emits the rule-specific diagnostic for a failed assignment.
    pub(crate) fn require_assignable(&mut self, from: &Type, to: &Type, pos: Pos, what: &str) {
        if self.assignable(from, to) {
            return;
        }
        let from_n = self.type_name(from);
        let to_n = self.type_name(to);
        let class_like = |t: &Type| match t {
            Type::Class(_) => true,
            Type::Nullable(inner) => matches!(**inner, Type::Class(_)),
            _ => false,
        };
        if class_like(from) && class_like(to) {
            self.error(
                RuleCode::S005,
                format!(
                    "nominal types are not interchangeable: {} expects `{}`, got `{}`",
                    what, to_n, from_n
                ),
                pos,
            );
        } else if from.is_numeric() && to.is_numeric() {
            self.error(
                RuleCode::S007,
                format!(
                    "implicit numeric conversion from `{}` to `{}`; spell it `as {}`",
                    from_n, to_n, to_n
                ),
                pos,
            );
        } else if self.is_value_class(from) && matches!(to, Type::Nullable(_)) {
            self.error(
                RuleCode::S011,
                format!("value class `{}` cannot be nullable", from_n),
                pos,
            );
        } else {
            self.error(
                RuleCode::S100,
                format!("type mismatch: {} expects `{}`, got `{}`", what, to_n, from_n),
                pos,
            );
        }
    }

    // ----- pass A: name collection -----

    fn register_scope_item(&mut self, file: usize, name: &str, item: ScopeItem, pos: Pos) {
        if self.file_scopes[file].contains_key(name) {
            self.error(
                RuleCode::S100,
                format!("duplicate top-level name `{}`", name),
                pos,
            );
            return;
        }
        self.file_scopes[file].insert(name.to_string(), item);
    }

    fn collect_file(&mut self, file: usize) {
        self.file_scopes.push(HashMap::new());
        self.exports.push(HashSet::new());
        let module = &self.prog.files[file].module;
        for item in &module.body {
            let (decl, exported) = match item {
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => (&e.decl, true),
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(_)) => continue,
                ast::ModuleItem::ModuleDecl(other) => {
                    let pos = self.pos(other.span());
                    self.error(
                        RuleCode::S100,
                        "only `export` declarations and named imports are in the decided surface",
                        pos,
                    );
                    continue;
                }
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => (d, false),
                ast::ModuleItem::Stmt(_) => continue,
            };
            self.collect_decl(file, decl, exported);
        }
    }

    fn collect_decl(&mut self, file: usize, decl: &ast::Decl, exported: bool) {
        match decl {
            ast::Decl::Class(c) => self.collect_class(file, c, exported),
            ast::Decl::Fn(f) => self.collect_fn(file, f, exported),
            ast::Decl::Var(v) => self.collect_globals(file, v, exported),
            ast::Decl::TsEnum(e) => self.collect_enum(file, e, exported),
            other => {
                let pos = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "declaration form outside the decided surface",
                    pos,
                );
            }
        }
    }

    fn class_decorators(&mut self, class: &ast::Class) -> bool {
        let mut is_value = false;
        for dec in &class.decorators {
            match &*dec.expr {
                ast::Expr::Ident(id) if id.sym.as_ref() == "value" => is_value = true,
                _ => {
                    let pos = self.pos(dec.span);
                    self.error(
                        RuleCode::S100,
                        "the only decided decorator is the ambient `@value`",
                        pos,
                    );
                }
            }
        }
        is_value
    }

    fn collect_class(&mut self, file: usize, c: &ast::ClassDecl, exported: bool) {
        let name = c.ident.sym.to_string();
        let pos = self.pos(c.ident.span);
        let is_value = self.class_decorators(&c.class);
        if let Some(tp) = &c.class.type_params {
            let type_params: Vec<String> =
                tp.params.iter().map(|p| p.name.sym.to_string()).collect();
            self.generic_classes.insert(
                name.clone(),
                GenericClass {
                    file,
                    is_value,
                    type_params,
                    class: (*c.class).clone(),
                    pos: pos.clone(),
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericClass(name.clone()), pos);
        } else {
            let id = self.new_class(&name, is_value, pos.clone());
            self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
        }
        if exported {
            self.exports[file].insert(name);
        }
    }

    pub(crate) fn new_class(&mut self, name: &str, is_value: bool, pos: Pos) -> ClassId {
        let id = ClassId(self.classes.len());
        self.classes.push(hir::ClassDef {
            name: name.to_string(),
            is_value,
            fields: Vec::new(),
            ctor: None,
            methods: Vec::new(),
            pos: pos.clone(),
        });
        self.class_sigs.push(ClassSig::default());
        if self.class_ids.contains_key(name) {
            // Cross-file collisions land here; same-file ones are also
            // caught by the per-file scope registration.
            self.error(
                RuleCode::S100,
                format!("duplicate class name `{}` in the program", name),
                pos,
            );
        } else {
            self.class_ids.insert(name.to_string(), id);
        }
        id
    }

    fn collect_fn(&mut self, file: usize, f: &ast::FnDecl, exported: bool) {
        let name = f.ident.sym.to_string();
        let pos = self.pos(f.ident.span);
        if let Some(tp) = &f.function.type_params {
            let type_params: Vec<String> =
                tp.params.iter().map(|p| p.name.sym.to_string()).collect();
            self.generic_fns.insert(
                name.clone(),
                GenericFn {
                    file,
                    type_params,
                    function: (*f.function).clone(),
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericFunc(name.clone()), pos);
        } else {
            if self.fn_sigs.contains_key(&name) {
                self.error(
                    RuleCode::S100,
                    format!("duplicate function name `{}` in the program", name),
                    pos.clone(),
                );
            }
            // Placeholder; pass B fills the real signature.
            self.fn_sigs.insert(
                name.clone(),
                FnSig {
                    params: Vec::new(),
                    ret: Type::Error,
                    is_generator: false,
                    yield_known: false,
                },
            );
            self.register_scope_item(file, &name, ScopeItem::Func(name.clone()), pos);
        }
        if exported {
            self.exports[file].insert(name.clone());
            self.exported_fns.insert(name);
        }
    }

    fn collect_globals(&mut self, file: usize, v: &ast::VarDecl, exported: bool) {
        for d in &v.decls {
            let ast::Pat::Ident(binding) = &d.name else {
                let pos = self.pos(d.span);
                self.error(RuleCode::S100, "destructuring is not in the decided surface", pos);
                continue;
            };
            let name = binding.id.sym.to_string();
            let pos = self.pos(binding.id.span);
            self.register_scope_item(file, &name, ScopeItem::Global(name.clone()), pos);
            if exported {
                self.exports[file].insert(name);
            }
        }
    }

    fn collect_enum(&mut self, file: usize, e: &ast::TsEnumDecl, exported: bool) {
        let name = e.id.sym.to_string();
        let pos = self.pos(e.id.span);
        let mut members = Vec::new();
        // `None` means the previous member's value + 1 overflows i64, so
        // the next implicit value has no representation.
        let mut next: Option<i64> = Some(0);
        for m in &e.members {
            let member_name = match &m.id {
                ast::TsEnumMemberId::Ident(id) => id.sym.to_string(),
                ast::TsEnumMemberId::Str(s) => {
                    let p = self.pos(s.span);
                    self.error(RuleCode::S100, "string enum member names are not decided", p);
                    continue;
                }
            };
            let value = match &m.init {
                None => match next {
                    Some(v) => v,
                    None => {
                        let p = self.pos(m.span);
                        self.error(
                            RuleCode::S008,
                            format!(
                                "implicit value for enum member `{}` overflows i64",
                                member_name
                            ),
                            p,
                        );
                        0
                    }
                },
                Some(init) => match self.const_int_of(init) {
                    Some(v) => v,
                    None => {
                        let p = self.pos(init.span());
                        self.error(
                            RuleCode::S100,
                            "enum members must have integer literal values",
                            p,
                        );
                        next.unwrap_or(0)
                    }
                },
            };
            next = value.checked_add(1);
            members.push((member_name, value));
        }
        let id = EnumId(self.enums.len());
        self.enums.push(hir::EnumDef {
            name: name.clone(),
            members,
            pos: pos.clone(),
        });
        self.enum_ids.insert(name.clone(), id);
        self.register_scope_item(file, &name, ScopeItem::Enum(id), pos);
        if exported {
            self.exports[file].insert(name);
        }
    }

    fn const_int_of(&self, e: &ast::Expr) -> Option<i64> {
        match e {
            ast::Expr::Lit(ast::Lit::Num(n)) if n.value.fract() == 0.0 => Some(n.value as i64),
            ast::Expr::Unary(u) if u.op == ast::UnaryOp::Minus => {
                self.const_int_of(&u.arg).map(|v| -v)
            }
            ast::Expr::Paren(p) => self.const_int_of(&p.expr),
            _ => None,
        }
    }

    // ----- imports -----

    fn resolve_imports(&mut self) {
        for file in 0..self.prog.files.len() {
            let module = &self.prog.files[file].module;
            let mut additions: Vec<(String, ScopeItem, Pos)> = Vec::new();
            for item in &module.body {
                let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) = item else {
                    continue;
                };
                let raw = import.src.value.to_string();
                let stem = raw
                    .trim_start_matches("./")
                    .trim_end_matches(".ts")
                    .to_string();
                let Some(target) = self
                    .prog
                    .files
                    .iter()
                    .position(|f| f.stem == stem)
                else {
                    let pos = self.pos(import.src.span);
                    self.error(
                        RuleCode::S100,
                        format!("imported module `{}` is not among the program's files", raw),
                        pos,
                    );
                    continue;
                };
                for spec in &import.specifiers {
                    let ast::ImportSpecifier::Named(named) = spec else {
                        let pos = self.pos(spec.span());
                        self.error(
                            RuleCode::S100,
                            "only named imports are in the decided surface",
                            pos,
                        );
                        continue;
                    };
                    let local = named.local.sym.to_string();
                    let pos = self.pos(named.local.span);
                    if !self.exports[target].contains(&local) {
                        self.error(
                            RuleCode::S100,
                            format!("`{}` is not exported by `{}`", local, raw),
                            pos,
                        );
                        continue;
                    }
                    match self.file_scopes[target].get(&local) {
                        Some(item) => additions.push((local, item.clone(), pos)),
                        None => {
                            self.error(
                                RuleCode::S100,
                                format!("`{}` is not defined in `{}`", local, raw),
                                pos,
                            );
                        }
                    }
                }
            }
            for (name, item, pos) in additions {
                self.register_scope_item(file, &name, item, pos);
            }
        }
    }

    // ----- pass B: signatures -----

    fn resolve_signatures(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        for item in &module.body {
            let decl = match item {
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
                _ => continue,
            };
            match decl {
                ast::Decl::Class(c) if c.class.type_params.is_none() => {
                    let name = c.ident.sym.to_string();
                    if let Some(&id) = self.class_ids.get(&name) {
                        self.resolve_class_shape(id, &c.class);
                    }
                }
                ast::Decl::Fn(f) if f.function.type_params.is_none() => {
                    let name = f.ident.sym.to_string();
                    let sig = self.resolve_fn_sig(&f.function, self.pos(f.ident.span));
                    self.fn_sigs.insert(name, sig);
                }
                ast::Decl::Var(v) => {
                    for d in &v.decls {
                        let ast::Pat::Ident(binding) = &d.name else { continue };
                        let name = binding.id.sym.to_string();
                        let ty = match &binding.type_ann {
                            Some(ann) => self.resolve_type(&ann.type_ann),
                            None => {
                                let pos = self.pos(binding.id.span);
                                self.error(
                                    RuleCode::S100,
                                    "module-level variables require a type annotation",
                                    pos,
                                );
                                Type::Error
                            }
                        };
                        self.global_sigs.insert(
                            name,
                            GlobalSig {
                                ty,
                                mutable: v.kind == ast::VarDeclKind::Let,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Resolves a function signature (pass B). Emits S013 for `async`.
    pub(crate) fn resolve_fn_sig(&mut self, f: &ast::Function, pos: Pos) -> FnSig {
        if f.is_async {
            self.error(
                RuleCode::S013,
                "`async` requires an event loop; the language has none (use coroutines)",
                pos.clone(),
            );
            return FnSig {
                params: Vec::new(),
                ret: Type::Error,
                is_generator: false,
                yield_known: true,
            };
        }
        let params = self.resolve_params(&f.params);
        if f.is_generator {
            // The yield type is inferred from the body (checked in
            // source order); a `Generator<T>` annotation, when present,
            // seeds it.
            let mut yield_ty = None;
            if let Some(ann) = &f.return_type {
                if let ast::TsType::TsTypeRef(r) = &*ann.type_ann {
                    if let ast::TsEntityName::Ident(id) = &r.type_name {
                        if id.sym.as_ref() == "Generator" {
                            if let Some(args) = &r.type_params {
                                if let Some(first) = args.params.first() {
                                    yield_ty = Some(self.resolve_type(first));
                                }
                            }
                        }
                    }
                }
            }
            let known = yield_ty.is_some();
            return FnSig {
                params,
                ret: Type::Generator(Box::new(yield_ty.unwrap_or(Type::Error))),
                is_generator: true,
                yield_known: known,
            };
        }
        let ret = match &f.return_type {
            Some(ann) => self.resolve_type(&ann.type_ann),
            None => {
                self.error(
                    RuleCode::S100,
                    "function return types must be annotated",
                    pos,
                );
                Type::Error
            }
        };
        FnSig {
            params,
            ret,
            is_generator: false,
            yield_known: true,
        }
    }

    pub(crate) fn resolve_params(&mut self, params: &[ast::Param]) -> Vec<ParamSig> {
        params
            .iter()
            .map(|p| self.resolve_param_pat(&p.pat))
            .collect()
    }

    pub(crate) fn resolve_param_pat(&mut self, pat: &ast::Pat) -> ParamSig {
        match pat {
            ast::Pat::Ident(binding) => {
                if binding.id.optional {
                    // C7: optional parameters without defaults imply an
                    // observable `undefined`.
                    let pos = self.pos(binding.id.span);
                    self.error(
                        RuleCode::S012,
                        "optional parameters imply `undefined`; use a default value or `T | null`",
                        pos,
                    );
                }
                let ty = match &binding.type_ann {
                    Some(ann) => self.resolve_type(&ann.type_ann),
                    None => {
                        let pos = self.pos(binding.id.span);
                        self.error(RuleCode::S100, "parameters require a type annotation", pos);
                        Type::Error
                    }
                };
                ParamSig {
                    name: binding.id.sym.to_string(),
                    ty,
                    has_default: false,
                }
            }
            ast::Pat::Assign(assign) => {
                let mut inner = self.resolve_param_pat(&assign.left);
                inner.has_default = true;
                inner
            }
            other => {
                let pos = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "parameter pattern outside the decided surface",
                    pos,
                );
                ParamSig {
                    name: String::new(),
                    ty: Type::Error,
                    has_default: false,
                }
            }
        }
    }

    /// Resolves a class's fields and callable signatures (pass B), and
    /// enforces C2 (no inheritance for value classes; field whitelist).
    pub(crate) fn resolve_class_shape(&mut self, id: ClassId, class: &ast::Class) {
        let is_value = self.classes[id.0].is_value;
        if let Some(sup) = &class.super_class {
            let pos = self.pos(sup.span());
            if is_value {
                self.error(RuleCode::S006, "value classes do not inherit", pos);
            } else {
                self.error(
                    RuleCode::S100,
                    "class inheritance is not in the decided surface",
                    pos,
                );
            }
        }
        for member in &class.body {
            match member {
                ast::ClassMember::ClassProp(prop) => {
                    let ast::PropName::Ident(key) = &prop.key else {
                        let pos = self.pos(prop.span);
                        self.error(
                            RuleCode::S100,
                            "computed or non-identifier field names are not decided",
                            pos,
                        );
                        continue;
                    };
                    if prop.is_static {
                        let pos = self.pos(prop.span);
                        self.error(RuleCode::S100, "static fields are not decided", pos);
                        continue;
                    }
                    if prop.is_optional {
                        let pos = self.pos(prop.span);
                        self.error(
                            RuleCode::S012,
                            "optional properties imply `undefined`; use `T | null`",
                            pos,
                        );
                    }
                    let pos = self.pos(key.span);
                    let ty = match &prop.type_ann {
                        Some(ann) => self.resolve_type(&ann.type_ann),
                        None => {
                            self.error(
                                RuleCode::S100,
                                "fields require a type annotation",
                                pos.clone(),
                            );
                            Type::Error
                        }
                    };
                    if is_value && !self.value_field_ok(&ty) {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "field type `{}` is outside the value-class whitelist \
                                 (sized numerics, boolean, value classes, FixedArray, enums)",
                                self.type_name(&ty)
                            ),
                            pos.clone(),
                        );
                    }
                    self.classes[id.0].fields.push(hir::Field {
                        name: key.sym.to_string(),
                        ty,
                        init: None,
                        pos,
                    });
                }
                ast::ClassMember::Constructor(ctor) => {
                    let mut params = Vec::new();
                    for p in &ctor.params {
                        match p {
                            ast::ParamOrTsParamProp::Param(param) => {
                                params.push(self.resolve_param_pat(&param.pat));
                            }
                            ast::ParamOrTsParamProp::TsParamProp(pp) => {
                                let pos = self.pos(pp.span);
                                self.error(
                                    RuleCode::S100,
                                    "constructor parameter properties are not decided",
                                    pos,
                                );
                            }
                        }
                    }
                    self.class_sigs[id.0].ctor = Some(params);
                }
                ast::ClassMember::Method(method) => {
                    let ast::PropName::Ident(key) = &method.key else {
                        let pos = self.pos(method.span);
                        self.error(RuleCode::S100, "computed method names are not decided", pos);
                        continue;
                    };
                    if method.is_static || method.kind != ast::MethodKind::Method {
                        let pos = self.pos(method.span);
                        self.error(
                            RuleCode::S100,
                            "static methods and accessors are not decided",
                            pos,
                        );
                        continue;
                    }
                    if method.function.is_generator {
                        let pos = self.pos(method.span);
                        self.error(RuleCode::S100, "generator methods are not decided", pos);
                        continue;
                    }
                    let sig = self.resolve_fn_sig(&method.function, self.pos(key.span));
                    let name = key.sym.to_string();
                    self.class_sigs[id.0].methods.insert(name, sig);
                }
                ast::ClassMember::Empty(_) => {}
                other => {
                    let pos = self.pos(other.span());
                    self.error(
                        RuleCode::S100,
                        "class member form outside the decided surface",
                        pos,
                    );
                }
            }
        }
    }

    fn value_field_ok(&self, ty: &Type) -> bool {
        match ty {
            Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::F32
            | Type::F64
            | Type::Bool
            | Type::Enum(_)
            | Type::Error => true,
            Type::Class(id) => self.classes[id.0].is_value,
            Type::FixedArray(elem, _) => self.value_field_ok(elem),
            _ => false,
        }
    }

    // ----- pass C: bodies -----

    fn check_bodies(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        for item in &module.body {
            match item {
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => {
                    self.check_body_decl(&e.decl, true);
                }
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => self.check_body_decl(d, false),
                ast::ModuleItem::Stmt(s) => {
                    let mut fx = FnCtx::new(Type::Void, false, None);
                    let mut out = Vec::new();
                    self.check_stmt(s, &mut fx, &mut out);
                    self.top_level.extend(out);
                }
                _ => {}
            }
        }
    }

    fn check_body_decl(&mut self, decl: &ast::Decl, exported: bool) {
        match decl {
            ast::Decl::Fn(f) if f.function.type_params.is_none() => {
                let name = f.ident.sym.to_string();
                let pos = self.pos(f.ident.span);
                let Some(sig) = self.fn_sigs.get(&name).cloned() else { return };
                let function =
                    self.check_function(&f.function, &name, exported, &sig, None, pos);
                if let Some(function) = function {
                    self.functions.push(function);
                }
            }
            ast::Decl::Class(c) if c.class.type_params.is_none() => {
                let name = c.ident.sym.to_string();
                if let Some(&id) = self.class_ids.get(&name) {
                    self.check_class_body(id, &c.class);
                }
            }
            ast::Decl::Var(v) => {
                for d in &v.decls {
                    let ast::Pat::Ident(binding) = &d.name else { continue };
                    let name = binding.id.sym.to_string();
                    let Some(sig) = self.global_sigs.get(&name).cloned() else { continue };
                    let pos = self.pos(binding.id.span);
                    let mut fx = FnCtx::new(Type::Void, false, None);
                    let init = match &d.init {
                        Some(init) => {
                            let e = self.check_expr(init, Some(&sig.ty), &mut fx);
                            self.require_assignable(
                                &e.ty.clone(),
                                &sig.ty,
                                e.pos.clone(),
                                "the initializer",
                            );
                            e
                        }
                        None => {
                            self.error(
                                RuleCode::S100,
                                "module-level variables require an initializer",
                                pos.clone(),
                            );
                            hir::Expr {
                                kind: hir::ExprKind::Null,
                                ty: Type::Error,
                                pos: pos.clone(),
                            }
                        }
                    };
                    self.globals.push(hir::Global {
                        name,
                        ty: sig.ty,
                        mutable: sig.mutable,
                        init,
                        pos,
                    });
                }
            }
            _ => {}
        }
    }

    /// Checks a function body against its resolved signature and builds
    /// the HIR function. Returns `None` for poisoned signatures.
    pub(crate) fn check_function(
        &mut self,
        f: &ast::Function,
        name: &str,
        exported: bool,
        sig: &FnSig,
        this_ty: Option<Type>,
        pos: Pos,
    ) -> Option<hir::Function> {
        if f.is_async {
            return None;
        }
        let mut fx = FnCtx::new(sig.ret.clone(), sig.is_generator, this_ty);
        if sig.is_generator {
            if let Type::Generator(y) = &sig.ret {
                if sig.yield_known {
                    fx.frames[0].yield_ty = Some((**y).clone());
                }
            }
        }
        let params = self.bind_params(f, sig, &mut fx);
        let body = match &f.body {
            Some(block) => {
                let mut out = Vec::new();
                for s in &block.stmts {
                    self.check_stmt(s, &mut fx, &mut out);
                }
                out
            }
            None => {
                self.error(RuleCode::S100, "function bodies are required", pos.clone());
                Vec::new()
            }
        };
        let ret = if sig.is_generator {
            let yield_ty = fx.frames[0].yield_ty.clone().unwrap_or(Type::Void);
            let ret = Type::Generator(Box::new(yield_ty));
            if let Some(entry) = self.fn_sigs.get_mut(name) {
                entry.ret = ret.clone();
                entry.yield_known = true;
            }
            ret
        } else {
            if f.body.is_some()
                && !matches!(sig.ret, Type::Void | Type::Error)
                && !stmt::always_returns(&body)
            {
                self.error(RuleCode::S100, "not all paths return a value", pos.clone());
            }
            sig.ret.clone()
        };
        Some(hir::Function {
            name: name.to_string(),
            exported,
            is_generator: sig.is_generator,
            params,
            ret,
            body,
            pos,
        })
    }

    /// Declares parameters as locals and checks default values.
    fn bind_params(&mut self, f: &ast::Function, sig: &FnSig, fx: &mut FnCtx) -> Vec<hir::Param> {
        let mut out = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let Some(ps) = sig.params.get(i) else { break };
            let pos = self.pos(p.span);
            let default = match &p.pat {
                ast::Pat::Assign(a) => {
                    let e = self.check_expr(&a.right, Some(&ps.ty), fx);
                    self.require_assignable(
                        &e.ty.clone(),
                        &ps.ty,
                        e.pos.clone(),
                        "the default value",
                    );
                    Some(e)
                }
                _ => None,
            };
            fx.declare(
                &ps.name,
                Local {
                    ty: ps.ty.clone(),
                    mutable: true,
                    holds_capturing: false,
                },
            );
            out.push(hir::Param {
                name: ps.name.clone(),
                ty: ps.ty.clone(),
                default,
                pos,
            });
        }
        out
    }

    /// Checks field initializers, the constructor, and methods (pass C).
    pub(crate) fn check_class_body(&mut self, id: ClassId, class: &ast::Class) {
        let this_ty = Type::Class(id);
        for member in &class.body {
            match member {
                ast::ClassMember::ClassProp(prop) => {
                    let ast::PropName::Ident(key) = &prop.key else { continue };
                    let Some(value) = &prop.value else { continue };
                    let field_ty = self.classes[id.0]
                        .fields
                        .iter()
                        .find(|f| f.name == key.sym.as_ref())
                        .map(|f| f.ty.clone());
                    let Some(field_ty) = field_ty else { continue };
                    let mut fx = FnCtx::new(Type::Void, false, Some(this_ty.clone()));
                    let e = self.check_expr(value, Some(&field_ty), &mut fx);
                    self.require_assignable(
                        &e.ty.clone(),
                        &field_ty,
                        e.pos.clone(),
                        "the field initializer",
                    );
                    if let Some(field) = self.classes[id.0]
                        .fields
                        .iter_mut()
                        .find(|f| f.name == key.sym.as_ref())
                    {
                        field.init = Some(e);
                    }
                }
                ast::ClassMember::Constructor(ctor) => {
                    let Some(params) = self.class_sigs[id.0].ctor.clone() else { continue };
                    let pos = self.pos(ctor.span);
                    let sig = FnSig {
                        params,
                        ret: Type::Void,
                        is_generator: false,
                        yield_known: true,
                    };
                    let mut fx = FnCtx::new(Type::Void, false, Some(this_ty.clone()));
                    let mut hir_params = Vec::new();
                    for (i, p) in ctor.params.iter().enumerate() {
                        let ast::ParamOrTsParamProp::Param(param) = p else { continue };
                        let Some(ps) = sig.params.get(i) else { break };
                        let default = match &param.pat {
                            ast::Pat::Assign(a) => {
                                let e = self.check_expr(&a.right, Some(&ps.ty), &mut fx);
                                Some(e)
                            }
                            _ => None,
                        };
                        fx.declare(
                            &ps.name,
                            Local {
                                ty: ps.ty.clone(),
                                mutable: true,
                                holds_capturing: false,
                            },
                        );
                        hir_params.push(hir::Param {
                            name: ps.name.clone(),
                            ty: ps.ty.clone(),
                            default,
                            pos: self.pos(param.span),
                        });
                    }
                    let mut body = Vec::new();
                    if let Some(block) = &ctor.body {
                        for s in &block.stmts {
                            self.check_stmt(s, &mut fx, &mut body);
                        }
                    }
                    self.classes[id.0].ctor = Some(hir::Function {
                        name: "constructor".to_string(),
                        exported: false,
                        is_generator: false,
                        params: hir_params,
                        ret: Type::Void,
                        body,
                        pos,
                    });
                }
                ast::ClassMember::Method(method) => {
                    let ast::PropName::Ident(key) = &method.key else { continue };
                    if method.is_static || method.kind != ast::MethodKind::Method {
                        continue;
                    }
                    let name = key.sym.to_string();
                    let Some(sig) = self.class_sigs[id.0].methods.get(&name).cloned() else {
                        continue;
                    };
                    let pos = self.pos(key.span);
                    if let Some(func) = self.check_function(
                        &method.function,
                        &name,
                        false,
                        &sig,
                        Some(this_ty.clone()),
                        pos,
                    ) {
                        self.classes[id.0].methods.push(func);
                    }
                }
                _ => {}
            }
        }
    }

    // ----- generic monomorphization (in HIR: templates never survive) -----

    /// Mangled instance name, e.g. `identity<i32>`.
    pub(crate) fn mono_name(&self, base: &str, args: &[Type]) -> String {
        let rendered: Vec<String> = args.iter().map(|t| self.type_name(t)).collect();
        format!("{}<{}>", base, rendered.join(", "))
    }

    /// Instantiates a generic function at explicit type arguments and
    /// checks its body immediately. Returns the instance name.
    pub(crate) fn instantiate_fn(
        &mut self,
        key: &str,
        args: &[Type],
        pos: Pos,
    ) -> Option<String> {
        let template = self.generic_fns.get(key)?.clone();
        if template.type_params.len() != args.len() {
            self.error(
                RuleCode::S100,
                format!(
                    "`{}` expects {} type argument(s), got {}",
                    key,
                    template.type_params.len(),
                    args.len()
                ),
                pos,
            );
            return None;
        }
        let name = self.mono_name(key, args);
        if self.fn_sigs.contains_key(&name) {
            return Some(name);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let sig = self.resolve_fn_sig(&template.function, pos.clone());
        self.fn_sigs.insert(name.clone(), sig.clone());
        let exported = self.exported_fns.contains(key);
        if let Some(function) =
            self.check_function(&template.function, &name, exported, &sig, None, pos)
        {
            self.functions.push(function);
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        Some(name)
    }

    /// Instantiates a generic class at explicit type arguments, checking
    /// its shape and bodies immediately. Returns the instance id.
    pub(crate) fn instantiate_class(
        &mut self,
        key: &str,
        args: &[Type],
        pos: Pos,
    ) -> Option<ClassId> {
        let template = self.generic_classes.get(key)?.clone();
        if template.type_params.len() != args.len() {
            self.error(
                RuleCode::S100,
                format!(
                    "`{}` expects {} type argument(s), got {}",
                    key,
                    template.type_params.len(),
                    args.len()
                ),
                pos,
            );
            return None;
        }
        let name = self.mono_name(key, args);
        if let Some(&id) = self.class_ids.get(&name) {
            return Some(id);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let id = self.new_class(&name, template.is_value, template.pos.clone());
        self.resolve_class_shape(id, &template.class);
        self.check_class_body(id, &template.class);
        self.cur_file = saved_file;
        self.subst = saved_subst;
        Some(id)
    }

    // ----- shared lookups -----

    /// Resolves a name against the current file's top-level scope.
    pub(crate) fn scope_item(&self, name: &str) -> Option<ScopeItem> {
        self.file_scopes
            .get(self.cur_file)
            .and_then(|scope| scope.get(name))
            .cloned()
    }

    /// Looks a name up in the local scope stack. A hit that crosses a
    /// lambda boundary is a capture: it is recorded on every crossed
    /// lambda frame and must refer to a `const` binding (C5).
    pub(crate) fn lookup_local(
        &mut self,
        name: &str,
        pos: &Pos,
        fx: &mut FnCtx,
    ) -> Option<Local> {
        let mut crossed = 0usize;
        let mut found: Option<(usize, Local)> = None;
        for scope in fx.scopes.iter().rev() {
            if let Some(local) = scope.vars.get(name) {
                found = Some((crossed, local.clone()));
                break;
            }
            if scope.fn_boundary {
                crossed += 1;
            }
        }
        let (crossed, local) = found?;
        if crossed > 0 {
            if local.mutable {
                self.error(
                    RuleCode::S009,
                    format!(
                        "lambda captures `{}`, which is not a `const` local; \
                         capturing lambdas may capture only const locals by value",
                        name
                    ),
                    pos.clone(),
                );
            }
            let mut remaining = crossed;
            for frame in fx.frames.iter_mut().rev() {
                if remaining == 0 {
                    break;
                }
                if frame.is_lambda {
                    if !frame.captures.iter().any(|c| c == name) {
                        frame.captures.push(name.to_string());
                    }
                    remaining -= 1;
                }
            }
        }
        Some(local)
    }

    /// True when an expression is (or can transport) a capturing
    /// lambda; such a value may only be called locally or passed
    /// downward (C5). Conditionals, assignment expressions, and array
    /// literals forward the taint of their value positions. Other kinds
    /// cannot carry one: parentheses are erased during checking, `||`
    /// requires boolean operands, and reading a capturing lambda back
    /// out of storage is impossible because storing one is rejected.
    pub(crate) fn is_capturing_value(&self, e: &hir::Expr, fx: &FnCtx) -> bool {
        match &e.kind {
            hir::ExprKind::Lambda { captures, .. } => !captures.is_empty(),
            hir::ExprKind::Local(name) => fx
                .scopes
                .iter()
                .rev()
                .find_map(|s| s.vars.get(name))
                .map(|l| l.holds_capturing)
                .unwrap_or(false),
            hir::ExprKind::Cond { then, els, .. } => {
                self.is_capturing_value(then, fx) || self.is_capturing_value(els, fx)
            }
            hir::ExprKind::Assign { value, .. } => self.is_capturing_value(value, fx),
            hir::ExprKind::ArrayLit(elems) => {
                elems.iter().any(|e| self.is_capturing_value(e, fx))
            }
            _ => false,
        }
    }
}
