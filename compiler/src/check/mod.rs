//! The semantic checker: enforces the collision rules (C1–C8) and the
//! Q-register resolutions, and produces the typed HIR.
//!
//! Structure: pass A collects top-level names per file and resolves
//! imports; pass B resolves declared signatures and class shapes; pass C
//! checks bodies in source order and builds the HIR. Generic
//! declarations are registered as templates in pass A/B and
//! monomorphized on first use (`identity<i32>`, `Box<f64>`).

mod expr;
mod json;
mod layout;
mod stmt;
mod tyres;

use std::collections::{HashMap, HashSet};

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::hir;
use crate::parse::ParsedProgram;
use crate::provenance;
use crate::types::{ClassId, EnumId, StringAliasId, Type};

fn module_decl(item: &ast::ModuleItem) -> Option<&ast::Decl> {
    match item {
        ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => Some(decl),
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => Some(&export.decl),
        _ => None,
    }
}

fn parameter_name_from_pat(pat: &ast::Pat) -> Option<&str> {
    match pat {
        ast::Pat::Ident(binding) => Some(binding.id.sym.as_ref()),
        ast::Pat::Assign(assign) => parameter_name_from_pat(&assign.left),
        _ => None,
    }
}

fn type_reference_name(ty: Option<&ast::TsType>) -> Option<&str> {
    let ast::TsType::TsTypeRef(reference) = ty? else {
        return None;
    };
    let ast::TsEntityName::Ident(ident) = &reference.type_name else {
        return None;
    };
    Some(ident.sym.as_ref())
}

fn is_dispose_method_key(key: &ast::PropName) -> bool {
    let ast::PropName::Computed(computed) = key else {
        return false;
    };
    let mut expression = computed.expr.as_ref();
    while let ast::Expr::Paren(paren) = expression {
        expression = &paren.expr;
    }
    let ast::Expr::Member(member) = expression else {
        return false;
    };
    let ast::Expr::Ident(symbol) = member.obj.as_ref() else {
        return false;
    };
    let ast::MemberProp::Ident(dispose) = &member.prop else {
        return false;
    };
    symbol.sym.as_ref() == "Symbol" && dispose.sym.as_ref() == "dispose"
}

/// Extracts the declaration-ordered members of the one program type-alias
/// form admitted by Q32.
fn string_alias_members(ty: &ast::TsType) -> Option<Vec<String>> {
    let ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(union)) =
        ty
    else {
        return None;
    };
    if union.types.len() < 2 {
        return None;
    }
    union
        .types
        .iter()
        .map(|member| match &**member {
            ast::TsType::TsLitType(ast::TsLitType {
                lit: ast::TsLit::Str(value),
                ..
            }) => Some(value.value.to_string()),
            _ => None,
        })
        .collect()
}

/// Returns the object-literal mapping from the one R23 alias form.
fn wire_alias_literal(ty: &ast::TsType) -> Option<&ast::TsTypeLit> {
    let ast::TsType::TsTypeRef(reference) = ty else {
        return None;
    };
    let ast::TsEntityName::Ident(name) = &reference.type_name else {
        return None;
    };
    if name.sym.as_ref() != "CEnum" {
        return None;
    }
    let arguments = reference.type_params.as_ref()?;
    if arguments.params.len() != 1 {
        return None;
    }
    let ast::TsType::TsTypeLit(literal) = &*arguments.params[0] else {
        return None;
    };
    Some(literal)
}

/// Reads one parser-accepted integer spelling exactly, applying the folded
/// unary sign before returning its mathematical value (R26).
fn parse_integer_spelling(raw: &str, negate: bool) -> Option<i128> {
    let (radix, digits) =
        if let Some(digits) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
            (16, digits)
        } else if let Some(digits) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
            (2, digits)
        } else if let Some(digits) = raw.strip_prefix("0o").or_else(|| raw.strip_prefix("0O")) {
            (8, digits)
        } else {
            (10, raw)
        };
    let digits: String = digits.chars().filter(|c| *c != '_').collect();
    let magnitude = u128::from_str_radix(&digits, radix).ok()?;
    let magnitude = i128::try_from(magnitude).ok()?;
    if negate {
        magnitude.checked_neg()
    } else {
        Some(magnitude)
    }
}

/// The integer value of a non-negative numeric-literal expression (a flag
/// member initializer, §13.2), or `None` for any other expression. Source
/// spellings are read at u64 width; synthesized nodes retain the f64 path.
fn int_literal_value(e: &ast::Expr) -> Option<i64> {
    match e {
        ast::Expr::Lit(ast::Lit::Num(n)) => {
            if let Some(raw) = n.raw.as_deref() {
                let value = parse_integer_spelling(raw, false)?;
                return (value >= 0 && value <= i128::from(u64::MAX)).then_some(value as i64);
            }
            let v = n.value;
            if v.is_finite() && v.fract() == 0.0 && v >= 0.0 {
                Some(v as i64)
            } else {
                None
            }
        }
        ast::Expr::Paren(p) => int_literal_value(&p.expr),
        _ => None,
    }
}

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
    /// True for a Q34 poll-driven async function. `ret` is the fulfilled
    /// type inside the required source-level `Promise<ret>` annotation.
    pub is_async: bool,
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
    pub is_descriptor: bool,
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
    StringAlias(StringAliasId),
    Global(String),
    /// A foreign C-ABI function declared by an ambient mirror (P5.2);
    /// callable but not usable as a value.
    Foreign(String),
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

#[derive(Clone)]
struct UsingBinding {
    name: String,
    ty: Type,
    pos: Pos,
}

/// One function (or lambda) frame.
#[derive(Debug)]
pub(crate) struct Frame {
    pub ret: Type,
    pub is_generator: bool,
    pub is_async: bool,
    pub yield_ty: Option<Type>,
    pub is_lambda: bool,
    pub captures: Vec<hir::Capture>,
    pub this_ty: Option<Type>,
}

/// Per-body checking state: scope stack, frames, and the C7/R16 narrowing
/// set (path keys currently known non-null or present).
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
                is_async: false,
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
    pub string_aliases: Vec<hir::StringAliasDef>,
    pub fn_sigs: HashMap<String, FnSig>,
    pub functions: Vec<hir::Function>,
    pub worker_entries: Vec<hir::WorkerEntry>,
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
    /// Global ambient names contributed by ingested mirror (`.d.ts`)
    /// files (P5.2): handles, boundary structs, enums, foreign functions,
    /// ambient constants. Consulted after the per-file scope.
    pub ambient_scope: HashMap<String, ScopeItem>,
    /// Resolved signatures of foreign functions, keyed by symbol name.
    pub foreign_sigs: HashMap<String, FnSig>,
    /// Foreign function definitions, in mirror declaration order.
    pub foreign_defs: Vec<hir::ForeignFn>,
    /// Mirrors that contribute foreign functions, in source order.
    pub foreign_mirrors: Vec<hir::ForeignMirror>,
    /// Mirror id assigned to each ambient file that contributes functions.
    pub foreign_mirror_ids: HashMap<usize, hir::ForeignMirrorId>,
    /// Class ids that are opaque handles (empty branded nominal types).
    pub handle_classes: HashSet<ClassId>,
    /// Class ids that are boundary structs: value-layout structs whose
    /// fields may hold boundary types (`X | null`, `object | null`,
    /// function pointers) outside the ordinary C2 value-field whitelist.
    pub boundary_classes: HashSet<ClassId>,
    /// Mirror `type` aliases (function-pointer typedefs, flag-set `u64`
    /// aliases), resolved to language types.
    pub type_aliases: HashMap<String, Type>,
    /// True while resolving mirror declarations: the boundary null forms
    /// (`Struct | null`, `object`/`object | null`) are legal here and
    /// rejected elsewhere (C7).
    pub in_boundary: bool,
    /// True only while resolving a direct foreign-function signature.
    /// R23 wire aliases are admitted there, but not in boundary structs.
    pub allow_wire_alias_boundary: bool,
    /// True while resolving a `Map`/`Set` key argument. It lets the
    /// resolver preserve otherwise-banned key shapes (`object`,
    /// `T | null`) long enough to emit the Q24-specific S014 diagnostic
    /// instead of an unrelated general type diagnostic.
    pub in_assoc_key: bool,
    /// True only while checking the argument expression of
    /// `JSON.stringify`. Like `in_assoc_key`, it preserves a banned
    /// `object` assertion long enough for the Q28 call to issue its
    /// required S014 instead of an unrelated general-type diagnostic.
    pub in_json_argument: bool,
    /// True only while checking the expression to the right of
    /// `for…of`. It preserves a direct `as object` assertion long
    /// enough for P22's closed-list S014 instead of the general
    /// boundary-only-type diagnostic.
    pub in_for_of_subject: bool,
    /// Aggregate type annotations whose byte size depends on a class
    /// layout and must therefore be checked after signature resolution.
    pub pending_layouts: Vec<(Type, Pos, &'static str)>,
    /// Mirror flag members: an ambient `declare const X = <int literal>;`
    /// (§13.2) folds to its C `static const` value at each reference, so
    /// both tiers emit an immediate rather than reading a runtime global.
    /// Keyed by name → (value, `u64` flag type).
    pub ambient_int_consts: HashMap<String, (i64, Type)>,
    /// Monotonic id for checker-generated storage that stabilizes a
    /// `for…of` subject across the fused loop.
    pub next_for_of_id: usize,
    /// Checker-generated module globals for regex-literal source sites.
    ///
    /// Each initializer compiles and allocates one rooted handle; every
    /// evaluation of the literal reads that handle.
    pub regex_literals: HashMap<(String, u32, u32), String>,
    /// Monotonic suffix for collision-free regex-literal global names.
    pub next_regex_literal_id: usize,
    /// Monotonic suffix for return locals that preserve values across disposal.
    pub next_using_return_id: usize,
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
        string_aliases: Vec::new(),
        fn_sigs: HashMap::new(),
        functions: Vec::new(),
        worker_entries: Vec::new(),
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
        ambient_scope: HashMap::new(),
        foreign_sigs: HashMap::new(),
        foreign_defs: Vec::new(),
        foreign_mirrors: Vec::new(),
        foreign_mirror_ids: HashMap::new(),
        handle_classes: HashSet::new(),
        boundary_classes: HashSet::new(),
        type_aliases: HashMap::new(),
        in_boundary: false,
        allow_wire_alias_boundary: false,
        in_assoc_key: false,
        in_json_argument: false,
        in_for_of_subject: false,
        pending_layouts: Vec::new(),
        ambient_int_consts: HashMap::new(),
        next_for_of_id: 0,
        regex_literals: HashMap::new(),
        next_regex_literal_id: 0,
        next_using_return_id: 0,
    };

    // Parse-time provenance has a fixed shape; this pass binds each record
    // to declarations in its own mirror before type resolution discards
    // the source spelling.
    for i in 0..prog.files.len() {
        if prog.files[i].dts {
            ck.collect_mirror_provenance(i);
        }
    }

    // Pass A: collect top-level names. Mirror (`.d.ts`) declarations land
    // in the global ambient scope; program declarations in per-file scopes.
    for i in 0..prog.files.len() {
        ck.cur_file = i;
        ck.collect_file(i);
    }
    ck.resolve_imports();
    // Pass B: signatures. Mirror files first, in a boundary context (so
    // the boundary null forms resolve), then program files.
    ck.in_boundary = true;
    for i in 0..prog.files.len() {
        if prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.resolve_mirror_signatures(i);
        }
    }
    ck.in_boundary = false;
    for i in 0..prog.files.len() {
        if !prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.resolve_signatures(i);
        }
    }
    // Descriptor defaults need every class and function signature, but
    // constructing literals in ordinary bodies need the checked defaults.
    // Check all non-generic descriptor defaults in this intermediate pass.
    for i in 0..prog.files.len() {
        if !prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.check_descriptor_defaults_in_file(i);
        }
    }
    // Pass C: bodies (program files only; mirror declarations have none).
    for i in 0..prog.files.len() {
        if !prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.check_bodies(i);
        }
    }
    ck.validate_layouts();

    if ck.diags.is_empty() {
        let mut module = hir::Module {
            classes: ck.classes,
            enums: ck.enums,
            string_aliases: ck.string_aliases,
            globals: ck.globals,
            functions: ck.functions,
            worker_entries: ck.worker_entries,
            foreign_fns: ck.foreign_defs,
            foreign_mirrors: ck.foreign_mirrors,
            top_level: ck.top_level,
        };
        crate::trap_sites::decide_index_checks(&mut module);
        Ok(module)
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

    /// True for the three Q35 Context-affine runtime handle types, including
    /// their nullable local form.
    pub(crate) fn is_context_affine_type(ty: &Type) -> bool {
        match ty {
            Type::Worker(..) | Type::Inbox(_) | Type::Outbox(_) => true,
            Type::Nullable(inner) => Self::is_context_affine_type(inner),
            _ => false,
        }
    }

    /// Validates record targets in one ambient mirror and assigns its HIR
    /// header identity when it contributes foreign functions.
    fn collect_mirror_provenance(&mut self, file: usize) {
        let parsed = &self.prog.files[file];
        let mut functions = HashMap::new();
        let mut aliases = HashSet::new();
        for item in &parsed.module.body {
            let Some(decl) = module_decl(item) else {
                continue;
            };
            match decl {
                ast::Decl::Fn(function) => {
                    functions.insert(function.ident.sym.to_string(), &function.function);
                }
                ast::Decl::TsTypeAlias(alias) => {
                    aliases.insert(alias.id.sym.to_string());
                }
                _ => {}
            }
        }

        if !functions.is_empty() {
            let include = match &parsed.provenance.header {
                Some(record) => record.value.clone(),
                None => {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "mirror `{}` declares foreign functions but has no \
                             `@subscript-c-header` provenance record",
                            parsed.name
                        ),
                        Pos::new(parsed.name.clone(), 1, 1),
                    );
                    String::new()
                }
            };
            let id = hir::ForeignMirrorId(self.foreign_mirrors.len());
            self.foreign_mirrors.push(hir::ForeignMirror {
                source_name: parsed.name.clone(),
                include,
            });
            self.foreign_mirror_ids.insert(file, id);
        }

        for ((function_name, parameter_name), record) in &parsed.provenance.parameters {
            let exists = functions.get(function_name).is_some_and(|function| {
                function.params.iter().any(|parameter| {
                    parameter_name_from_pat(&parameter.pat)
                        .is_some_and(|name| name == parameter_name)
                })
            });
            if !exists {
                self.error(
                    RuleCode::S100,
                    format!(
                        "mirror `{}` has provenance record naming nonexistent \
                         parameter `{}.{}`: `{}`",
                        parsed.name, function_name, parameter_name, record.raw
                    ),
                    Pos::new(parsed.name.clone(), record.line, 1),
                );
            }
        }

        for (typedef_name, record) in &parsed.provenance.callbacks {
            if !aliases.contains(typedef_name) {
                self.error(
                    RuleCode::S100,
                    format!(
                        "mirror `{}` has provenance record naming nonexistent \
                         callback typedef `{}`: `{}`",
                        parsed.name, typedef_name, record.raw
                    ),
                    Pos::new(parsed.name.clone(), record.line, 1),
                );
            }
        }
    }

    /// Converts parameter provenance into the consumer-ready HIR shape and
    /// rejects missing or type-incompatible records.
    fn foreign_parameter_provenance(
        &mut self,
        file: usize,
        function_name: &str,
        parameter_name: &str,
        ty: &Type,
        pos: Pos,
    ) -> Option<hir::ForeignTypeProvenance> {
        let parsed = &self.prog.files[file];
        let key = (function_name.to_string(), parameter_name.to_string());
        let record = parsed.provenance.parameters.get(&key);
        match (ty, record.map(|record| &record.value)) {
            (
                Type::Array(_),
                Some(provenance::Parameter::Descriptor {
                    aggregate,
                    element,
                    element_const,
                }),
            ) => Some(hir::ForeignTypeProvenance::Descriptor {
                aggregate: aggregate.clone(),
                element: element.clone(),
                element_const: *element_const,
            }),
            (
                Type::Array(_),
                Some(provenance::Parameter::ScalarPair {
                    element,
                    element_const,
                }),
            ) => Some(hir::ForeignTypeProvenance::ScalarPair {
                element: element.clone(),
                element_const: *element_const,
            }),
            (Type::Str, Some(provenance::Parameter::StringView { aggregate })) => {
                Some(hir::ForeignTypeProvenance::StringView {
                    aggregate: aggregate.clone(),
                })
            }
            (Type::Array(_), None) => {
                self.error(
                    RuleCode::S100,
                    format!(
                        "mirror `{}` parameter `{}.{}` absorbs an array descriptor \
                         or scalar parameter pair but has no \
                         `@subscript-c-descriptor` or `@subscript-c-scalar-pair` \
                         provenance record",
                        parsed.name, function_name, parameter_name
                    ),
                    pos,
                );
                None
            }
            (Type::Str, None) => {
                self.error(
                    RuleCode::S100,
                    format!(
                        "mirror `{}` parameter `{}.{}` absorbs a string view but \
                         has no `@subscript-c-string-view` provenance record",
                        parsed.name, function_name, parameter_name
                    ),
                    pos,
                );
                None
            }
            (Type::Array(_), Some(_)) | (Type::Str, Some(_)) | (_, Some(_)) => {
                let raw = record.map(|record| record.raw.as_str()).unwrap_or_default();
                self.error(
                    RuleCode::S100,
                    format!(
                        "mirror `{}` has provenance record incompatible with \
                         parameter `{}.{}`: `{}`",
                        parsed.name, function_name, parameter_name, raw
                    ),
                    pos,
                );
                None
            }
            (_, None) => None,
        }
    }

    /// Resolves one mirrored function type directly to its C typedef.
    fn callback_provenance(
        &mut self,
        file: usize,
        type_ann: Option<&ast::TsType>,
        pos: Pos,
    ) -> Option<hir::ForeignTypeProvenance> {
        let parsed = &self.prog.files[file];
        let Some(typedef_name) = type_reference_name(type_ann) else {
            self.error(
                RuleCode::S100,
                format!(
                    "mirror `{}` has an anonymous callback type without \
                     `@subscript-c-callback` provenance",
                    parsed.name
                ),
                pos,
            );
            return None;
        };
        match parsed.provenance.callbacks.get(typedef_name) {
            Some(record) => Some(hir::ForeignTypeProvenance::Callback {
                typedef_name: record.value.clone(),
            }),
            None => {
                self.error(
                    RuleCode::S100,
                    format!(
                        "mirror `{}` callback type `{}` has no \
                         `@subscript-c-callback` provenance record",
                        parsed.name, typedef_name
                    ),
                    pos,
                );
                None
            }
        }
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
            &|id| {
                self.string_aliases
                    .get(id.0)
                    .map(|alias| alias.name.clone())
                    .unwrap_or_else(|| format!("<string alias #{}>", id.0))
            },
        )
    }

    pub(crate) fn is_value_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if self.classes[id.0].is_value)
    }

    pub(crate) fn is_reference_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if !self.classes[id.0].is_value)
            || matches!(ty, Type::Map(..) | Type::Set(_))
    }

    /// The Q24 hash/equality kind of a key, or `None` outside the
    /// whitelist.
    pub(crate) fn assoc_key_kind(&self, ty: &Type) -> Option<hir::AssocKeyKind> {
        hir::AssocKeyKind::of(ty, &|id| {
            self.classes.get(id.0).is_some_and(|class| class.is_value)
        })
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

    fn contains_string_alias(ty: &Type) -> bool {
        match ty {
            Type::StringAlias(_) => true,
            Type::FixedArray(element, _)
            | Type::Array(element)
            | Type::Set(element)
            | Type::Nullable(element)
            | Type::Generator(element)
            | Type::IterResult(element) => Self::contains_string_alias(element),
            Type::Map(key, value) => {
                Self::contains_string_alias(key) || Self::contains_string_alias(value)
            }
            Type::Func(function) => {
                function.params.iter().any(Self::contains_string_alias)
                    || Self::contains_string_alias(&function.ret)
            }
            _ => false,
        }
    }

    fn is_wire_alias(&self, ty: &Type) -> bool {
        matches!(ty, Type::StringAlias(alias) if self
            .string_aliases
            .get(alias.0)
            .is_some_and(|definition| definition.wire_values.is_some()))
    }

    /// The §52 boundary spellings whose storage is exactly one wire value,
    /// or a zero-copy descriptor of wire-value elements.
    fn supported_wire_alias_boundary_type(ty: &Type) -> bool {
        match ty {
            Type::StringAlias(_) => true,
            Type::Array(element) => matches!(&**element, Type::StringAlias(_)),
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
            Type::Class(_) | Type::Map(..) | Type::Set(_) => true,
            Type::Nullable(inner) => {
                matches!(**inner, Type::Class(_) | Type::Map(..) | Type::Set(_))
            }
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
                format!(
                    "type mismatch: {} expects `{}`, got `{}`",
                    what, to_n, from_n
                ),
                pos,
            );
        }
    }

    // ----- pass A: name collection -----

    fn register_scope_item(&mut self, file: usize, name: &str, item: ScopeItem, pos: Pos) {
        // Mirror (`.d.ts`) declarations populate the global ambient scope;
        // program declarations populate the per-file scope.
        let scope = if self.prog.files[file].dts {
            &mut self.ambient_scope
        } else {
            &mut self.file_scopes[file]
        };
        if scope.contains_key(name) {
            self.error(
                RuleCode::S100,
                format!("duplicate top-level name `{}`", name),
                pos,
            );
            return;
        }
        scope.insert(name.to_string(), item);
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
        if self.prog.files[file].dts {
            self.collect_mirror_decl(file, decl);
            return;
        }
        match decl {
            ast::Decl::Class(c) => self.collect_class(file, c, exported),
            ast::Decl::Fn(f) => self.collect_fn(file, f, exported),
            ast::Decl::Var(v) => self.collect_globals(file, v, exported),
            ast::Decl::TsEnum(e) => self.collect_enum(file, e, exported),
            ast::Decl::TsTypeAlias(alias) => self.collect_string_alias(file, alias, exported),
            ast::Decl::Using(using) => {
                self.error(
                    RuleCode::S100,
                    if using.is_await {
                        "module-level `await using` is not in the decided surface"
                    } else {
                        "module-level `using` is not in the decided surface"
                    },
                    self.pos(using.span),
                );
            }
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

    fn class_decorators(&mut self, class: &ast::Class) -> (bool, bool) {
        let mut is_value = false;
        let mut is_descriptor = false;
        for dec in &class.decorators {
            match &*dec.expr {
                ast::Expr::Ident(id) if id.sym.as_ref() == "CStruct" => is_value = true,
                ast::Expr::Ident(id) if id.sym.as_ref() == "Descriptor" => {
                    is_descriptor = true;
                }
                _ => {
                    let pos = self.pos(dec.span);
                    self.error(
                        RuleCode::S100,
                        "the only decided decorators are the ambient `@CStruct` and `@Descriptor`",
                        pos,
                    );
                }
            }
        }
        if is_value && is_descriptor {
            self.error(
                RuleCode::S100,
                "`@Descriptor` declares a reference class and cannot be combined with `@CStruct`",
                self.pos(class.span),
            );
        }
        (is_value, is_descriptor)
    }

    fn collect_class(&mut self, file: usize, c: &ast::ClassDecl, exported: bool) {
        let name = c.ident.sym.to_string();
        let pos = self.pos(c.ident.span);
        let (is_value, is_descriptor) = self.class_decorators(&c.class);
        if let Some(tp) = &c.class.type_params {
            if !is_descriptor {
                for member in &c.class.body {
                    let ast::ClassMember::Method(method) = member else {
                        continue;
                    };
                    if method.function.is_async {
                        self.error(
                            RuleCode::S100,
                            "async methods on generic class templates are not in the decided surface",
                            self.pos(method.span),
                        );
                    }
                }
            }
            let type_params: Vec<String> =
                tp.params.iter().map(|p| p.name.sym.to_string()).collect();
            self.generic_classes.insert(
                name.clone(),
                GenericClass {
                    file,
                    is_value,
                    is_descriptor,
                    type_params,
                    class: (*c.class).clone(),
                    pos: pos.clone(),
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericClass(name.clone()), pos);
        } else {
            let id = self.new_class(&name, is_value, is_descriptor, pos.clone());
            self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
        }
        if exported {
            self.exports[file].insert(name);
        }
    }

    pub(crate) fn new_class(
        &mut self,
        name: &str,
        is_value: bool,
        is_descriptor: bool,
        pos: Pos,
    ) -> ClassId {
        let id = ClassId(self.classes.len());
        self.classes.push(hir::ClassDef {
            name: name.to_string(),
            is_value,
            is_descriptor,
            is_boundary: false,
            fields: Vec::new(),
            ctor: None,
            methods: Vec::new(),
            index_signature: None,
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
                    is_async: false,
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
                self.error(
                    RuleCode::S100,
                    "destructuring is not in the decided surface",
                    pos,
                );
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
                    self.error(
                        RuleCode::S100,
                        "string enum member names are not decided",
                        p,
                    );
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

    fn collect_string_alias(&mut self, file: usize, alias: &ast::TsTypeAliasDecl, exported: bool) {
        let name = alias.id.sym.to_string();
        let pos = self.pos(alias.id.span);
        if alias.type_params.is_some() {
            self.error(
                RuleCode::S100,
                "string-literal union aliases cannot be generic",
                pos,
            );
            return;
        }
        if let Some(mapping) = wire_alias_literal(&alias.type_ann) {
            self.collect_wire_string_alias(file, alias, mapping, exported);
            return;
        }
        let Some(members) = string_alias_members(&alias.type_ann) else {
            self.error(
                RuleCode::S100,
                "type aliases are limited to a union of two or more string literals",
                pos,
            );
            return;
        };
        if members.len() > i32::MAX as usize {
            self.error(
                RuleCode::S100,
                "string-literal union has more members than fit its i32 discriminant",
                self.pos(alias.type_ann.span()),
            );
            return;
        }
        let mut seen = HashSet::new();
        if let Some(duplicate) = members
            .iter()
            .find(|member| !seen.insert((*member).clone()))
        {
            self.error(
                RuleCode::S100,
                format!("duplicate string-literal union member `{duplicate}`"),
                self.pos(alias.type_ann.span()),
            );
            return;
        }
        let id = StringAliasId(self.string_aliases.len());
        self.string_aliases.push(hir::StringAliasDef {
            name: name.clone(),
            members,
            wire_values: None,
            pos: pos.clone(),
        });
        self.register_scope_item(file, &name, ScopeItem::StringAlias(id), pos);
        if exported {
            self.exports[file].insert(name);
        }
    }

    /// Collects and validates one R23 `CEnum<{ key: wire }>` alias.
    fn collect_wire_string_alias(
        &mut self,
        file: usize,
        alias: &ast::TsTypeAliasDecl,
        mapping: &ast::TsTypeLit,
        exported: bool,
    ) {
        let name = alias.id.sym.to_string();
        let pos = self.pos(alias.id.span);
        if mapping.members.is_empty() {
            self.error(
                RuleCode::S100,
                "wire-mapped string-literal union must have at least one member",
                self.pos(mapping.span),
            );
            return;
        }
        if mapping.members.len() > i32::MAX as usize {
            self.error(
                RuleCode::S100,
                "wire-mapped string-literal union has more members than fit its i32 discriminant",
                self.pos(mapping.span),
            );
            return;
        }

        let mut members = Vec::with_capacity(mapping.members.len());
        let mut wire_values = Vec::with_capacity(mapping.members.len());
        let mut seen_members = HashSet::new();
        let mut seen_wires: HashMap<i32, String> = HashMap::new();
        for element in &mapping.members {
            let ast::TsTypeElement::TsPropertySignature(property) = element else {
                self.error(
                    RuleCode::S100,
                    "CEnum mappings contain only named properties with integer-literal values",
                    self.pos(element.span()),
                );
                return;
            };
            let member = match &*property.key {
                ast::Expr::Lit(ast::Lit::Str(value)) => value.value.to_string(),
                ast::Expr::Ident(value) if !property.computed => value.sym.to_string(),
                _ => {
                    self.error(
                        RuleCode::S100,
                        "CEnum member keys must be string literals or identifiers",
                        self.pos(property.key.span()),
                    );
                    return;
                }
            };
            if !seen_members.insert(member.clone()) {
                self.error(
                    RuleCode::S100,
                    format!("duplicate string-literal union member `{member}`"),
                    self.pos(property.key.span()),
                );
                return;
            }
            let Some(annotation) = &property.type_ann else {
                self.error(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(property.span),
                );
                return;
            };
            let ast::TsType::TsLitType(ast::TsLitType {
                lit: ast::TsLit::Number(number),
                ..
            }) = &*annotation.type_ann
            else {
                self.error(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(annotation.type_ann.span()),
                );
                return;
            };
            if !number.value.is_finite() || number.value.fract() != 0.0 {
                self.error(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(number.span),
                );
                return;
            }
            if number.value < f64::from(i32::MIN) || number.value > f64::from(i32::MAX) {
                let spelling = number
                    .raw
                    .as_ref()
                    .map_or_else(|| number.value.to_string(), ToString::to_string);
                self.error(
                    RuleCode::S100,
                    format!(
                        "wire value {spelling} for CEnum member `{member}` is outside the i32 range"
                    ),
                    self.pos(number.span),
                );
                return;
            }
            let wire = number.value as i32;
            if let Some(first) = seen_wires.insert(wire, member.clone()) {
                self.error(
                    RuleCode::S100,
                    format!(
                        "duplicate CEnum wire value {wire} for members `{first}` and `{member}`"
                    ),
                    self.pos(number.span),
                );
                return;
            }
            members.push(member);
            wire_values.push(wire);
        }

        let id = StringAliasId(self.string_aliases.len());
        self.string_aliases.push(hir::StringAliasDef {
            name: name.clone(),
            members,
            wire_values: Some(wire_values),
            pos: pos.clone(),
        });
        self.register_scope_item(file, &name, ScopeItem::StringAlias(id), pos);
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

    // ----- mirror (`.d.ts`) ingestion (P5.2) -----

    /// Pass A for a mirror declaration: registers the name (handle,
    /// boundary struct, enum, type alias, foreign function, or ambient
    /// const) into the global ambient scope. Shapes/signatures are
    /// resolved in [`Self::resolve_mirror_signatures`].
    fn collect_mirror_decl(&mut self, file: usize, decl: &ast::Decl) {
        match decl {
            ast::Decl::TsInterface(i) => self.collect_handle(file, i),
            ast::Decl::Class(c) if c.class.type_params.is_none() => {
                self.collect_boundary_struct(file, c)
            }
            ast::Decl::TsTypeAlias(t) => {
                if string_alias_members(&t.type_ann).is_some()
                    || wire_alias_literal(&t.type_ann).is_some()
                {
                    self.collect_string_alias(file, t, false);
                } else {
                    // Reserve the name; the aliased type is resolved in pass B.
                    self.type_aliases
                        .entry(t.id.sym.to_string())
                        .or_insert(Type::Error);
                }
            }
            ast::Decl::Fn(f) => {
                let name = f.ident.sym.to_string();
                let pos = self.pos(f.ident.span);
                self.register_scope_item(file, &name, ScopeItem::Foreign(name.clone()), pos);
            }
            ast::Decl::Var(v) => self.collect_ambient_consts(file, v),
            ast::Decl::TsEnum(e) => self.collect_enum(file, e, false),
            other => {
                let pos = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "mirror declaration form outside the decided surface",
                    pos,
                );
            }
        }
    }

    /// An ambient `interface` in a mirror is an opaque handle (Q13): an
    /// empty branded nominal type. Its members (the phantom brand) carry
    /// no in-language meaning and are ignored; it lowers to a
    /// pointer-sized handle (a reference-shaped nominal, non-value).
    fn collect_handle(&mut self, file: usize, i: &ast::TsInterfaceDecl) {
        let name = i.id.sym.to_string();
        let pos = self.pos(i.id.span);
        let id = self.new_class(&name, false, false, pos.clone());
        self.handle_classes.insert(id);
        self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
    }

    /// A mirror `declare class` is a boundary struct: a C-layout value
    /// type (Q13) whose fields may hold boundary types. Shape resolved
    /// in pass B.
    fn collect_boundary_struct(&mut self, file: usize, c: &ast::ClassDecl) {
        let name = c.ident.sym.to_string();
        let pos = self.pos(c.ident.span);
        let id = self.new_class(&name, true, false, pos.clone());
        self.boundary_classes.insert(id);
        self.classes[id.0].is_boundary = true;
        self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
    }

    /// A mirror `declare const` (enum/flag constant, Q13): a read-only
    /// ambient global of the given type. Type resolved in pass B.
    fn collect_ambient_consts(&mut self, file: usize, v: &ast::VarDecl) {
        for d in &v.decls {
            let ast::Pat::Ident(binding) = &d.name else {
                let pos = self.pos(d.span);
                self.error(
                    RuleCode::S100,
                    "destructuring is not in the decided surface",
                    pos,
                );
                continue;
            };
            let name = binding.id.sym.to_string();
            let pos = self.pos(binding.id.span);
            self.register_scope_item(file, &name, ScopeItem::Global(name.clone()), pos);
        }
    }

    /// Pass B for a mirror file: resolves type aliases first (so later
    /// declarations may reference them), then boundary-struct shapes,
    /// foreign-function signatures, and ambient-constant types. Runs with
    /// `in_boundary` set so the boundary null forms are legal.
    fn resolve_mirror_signatures(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        // Sub-pass 1: type aliases.
        for item in &module.body {
            let decl = match item {
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
                _ => continue,
            };
            if let ast::Decl::TsTypeAlias(t) = decl {
                if string_alias_members(&t.type_ann).is_some()
                    || wire_alias_literal(&t.type_ann).is_some()
                {
                    continue;
                }
                let ty = self.resolve_type(&t.type_ann);
                self.type_aliases.insert(t.id.sym.to_string(), ty);
            }
        }
        // Sub-pass 2: struct shapes, foreign signatures, ambient consts.
        for item in &module.body {
            let decl = match item {
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
                _ => continue,
            };
            match decl {
                ast::Decl::Class(c) if c.class.type_params.is_none() => {
                    let name = c.ident.sym.to_string();
                    if let Some(&id) = self.class_ids.get(&name) {
                        self.resolve_class_shape(id, &c.class);
                    }
                }
                ast::Decl::Fn(f) => {
                    let name = f.ident.sym.to_string();
                    let pos = self.pos(f.ident.span);
                    self.allow_wire_alias_boundary = true;
                    let sig = self.resolve_fn_sig(&f.function, pos.clone());
                    self.allow_wire_alias_boundary = false;
                    for parameter in &sig.params {
                        if Self::contains_string_alias(&parameter.ty)
                            && !Self::supported_wire_alias_boundary_type(&parameter.ty)
                        {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "wire-mapped aliases are supported only as direct foreign-function parameters or array-descriptor elements; `{}` nests one inside another boundary type",
                                    parameter.name
                                ),
                                pos.clone(),
                            );
                        }
                    }
                    if Self::contains_string_alias(&sig.ret)
                        && !matches!(sig.ret, Type::StringAlias(_))
                    {
                        self.error(
                            RuleCode::S100,
                            "wire-mapped aliases are supported only as direct foreign-function returns",
                            pos.clone(),
                        );
                    }
                    let mut params = Vec::with_capacity(sig.params.len());
                    for (index, parameter) in sig.params.iter().enumerate() {
                        let ast_parameter = f.function.params.get(index);
                        let parameter_pos =
                            ast_parameter.map_or_else(|| pos.clone(), |p| self.pos(p.span));
                        let foreign_provenance = self.foreign_parameter_provenance(
                            file,
                            &name,
                            &parameter.name,
                            &parameter.ty,
                            parameter_pos.clone(),
                        );
                        if matches!(parameter.ty, Type::Func(_)) {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "mirror `{}` foreign function `{}` parameter `{}` is a \
                                     direct callback; callbacks are supported only as fields \
                                     of mirrored boundary structs",
                                    self.prog.files[file].name, name, parameter.name
                                ),
                                parameter_pos.clone(),
                            );
                        }
                        params.push(hir::Param {
                            name: parameter.name.clone(),
                            ty: parameter.ty.clone(),
                            default: None,
                            foreign_provenance,
                            pos: parameter_pos,
                        });
                    }
                    let unsupported_return = match &sig.ret {
                        Type::Str => Some("a string view"),
                        Type::Array(_) => Some("an array descriptor"),
                        Type::Func(_) => Some("a direct callback"),
                        _ => None,
                    };
                    if let Some(kind) = unsupported_return {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "mirror `{}` foreign function `{}` returns {kind}; foreign \
                                 string-view, descriptor, and callback returns are unsupported \
                                 because return provenance cannot be represented by the boundary \
                                 vocabulary",
                                self.prog.files[file].name, name
                            ),
                            pos.clone(),
                        );
                    }
                    let Some(mirror) = self.foreign_mirror_ids.get(&file).copied() else {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "mirror `{}` has no header identity for foreign function `{}`",
                                self.prog.files[file].name, name
                            ),
                            pos,
                        );
                        continue;
                    };
                    self.foreign_defs.push(hir::ForeignFn {
                        name: name.clone(),
                        params,
                        ret: sig.ret.clone(),
                        mirror,
                        pos,
                    });
                    self.foreign_sigs.insert(name, sig);
                }
                ast::Decl::Var(v) => {
                    for d in &v.decls {
                        let ast::Pat::Ident(binding) = &d.name else {
                            continue;
                        };
                        let name = binding.id.sym.to_string();
                        let ty = match &binding.type_ann {
                            Some(ann) => self.resolve_type(&ann.type_ann),
                            None => match d.init.as_deref().and_then(int_literal_value) {
                                // A mirror flag member (§13.2):
                                // `declare const X = <int literal>;`. tsc
                                // accepts a bare literal initializer on an
                                // ambient const only without a type
                                // annotation, so the value travels here and
                                // the `u64` flag type is supplied by rule.
                                Some(value) => {
                                    self.ambient_int_consts
                                        .insert(name.clone(), (value, Type::U64));
                                    Type::U64
                                }
                                None => {
                                    let pos = self.pos(binding.id.span);
                                    self.error(
                                        RuleCode::S100,
                                        "ambient constants require a type annotation \
                                         or an integer-literal initializer",
                                        pos,
                                    );
                                    Type::Error
                                }
                            },
                        };
                        self.global_sigs
                            .insert(name, GlobalSig { ty, mutable: false });
                    }
                }
                _ => {}
            }
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
                let Some(target) = self.prog.files.iter().position(|f| f.stem == stem) else {
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
                    if self.exports[file].contains(&name) {
                        if sig.is_async && (!sig.params.is_empty() || sig.ret != Type::Void) {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "exported async function `{name}` must have the host entry signature `(): Promise<void>`"
                                ),
                                self.pos(f.ident.span),
                            );
                        }
                        let aliases_boundary = sig.params.iter().any(|parameter| {
                            Self::contains_string_alias(&parameter.ty)
                                && !self.is_wire_alias(&parameter.ty)
                        }) || Self::contains_string_alias(&sig.ret);
                        if aliases_boundary {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "exported function `{name}` has a string-literal union \
                                     alias in its boundary signature"
                                ),
                                self.pos(f.ident.span),
                            );
                        }
                    }
                    self.fn_sigs.insert(name, sig);
                }
                ast::Decl::Var(v) => {
                    for d in &v.decls {
                        let ast::Pat::Ident(binding) = &d.name else {
                            continue;
                        };
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
                        if Self::is_context_affine_type(&ty) {
                            self.error(
                                RuleCode::S100,
                                "Worker, Inbox, and Outbox values may not be module globals",
                                self.pos(binding.id.span),
                            );
                        }
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

    /// Resolves a function signature (pass B), including Q34's required
    /// `Promise<T>` view for async declarations.
    pub(crate) fn resolve_fn_sig(&mut self, f: &ast::Function, pos: Pos) -> FnSig {
        let params = self.resolve_params(&f.params);
        if f.is_async {
            if f.is_generator {
                self.error(
                    RuleCode::S100,
                    "a function cannot be both async and a generator",
                    pos,
                );
                return FnSig {
                    params,
                    ret: Type::Error,
                    is_generator: false,
                    is_async: true,
                    yield_known: true,
                };
            }
            let ret = match &f.return_type {
                Some(ann) => self.resolve_async_return(&ann.type_ann),
                None => {
                    self.error(
                        RuleCode::S100,
                        "async functions require an explicit `Promise<T>` return annotation",
                        pos,
                    );
                    Type::Error
                }
            };
            return FnSig {
                params,
                ret,
                is_generator: false,
                is_async: true,
                yield_known: true,
            };
        }
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
                is_async: false,
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
            is_async: false,
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
        let is_descriptor = self.classes[id.0].is_descriptor;
        let mut index_signature_pos = None;
        if let Some(sup) = &class.super_class {
            let pos = self.pos(sup.span());
            if is_value {
                self.error(RuleCode::S006, "value classes do not inherit", pos);
            } else if is_descriptor {
                self.error(RuleCode::S100, "descriptor classes do not inherit", pos);
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
                    let is_defaulted =
                        is_descriptor && prop.is_optional && prop.value.is_some() && !prop.definite;
                    if is_descriptor {
                        match (prop.definite, prop.is_optional, prop.value.is_some()) {
                            (true, false, false) | (false, true, true) | (false, true, false) => {}
                            (_, true, false) => {
                                let pos = self.pos(prop.span);
                                self.error(
                                    RuleCode::S012,
                                    "optional descriptor members require a default initializer",
                                    pos,
                                );
                            }
                            (true, _, true) => {
                                let pos = self.pos(prop.span);
                                self.error(
                                    RuleCode::S100,
                                    "a required descriptor member (`name!: T`) cannot have an initializer",
                                    pos,
                                );
                            }
                            (false, false, true) => {
                                let pos = self.pos(prop.span);
                                self.error(
                                    RuleCode::S100,
                                    "a descriptor member initializer requires the optional `?` spelling",
                                    pos,
                                );
                            }
                            _ => {
                                let pos = self.pos(prop.span);
                                self.error(
                                    RuleCode::S100,
                                    "required descriptor members must be spelled `name!: T`",
                                    pos,
                                );
                            }
                        }
                    } else if prop.is_optional {
                        let pos = self.pos(prop.span);
                        self.error(
                            RuleCode::S012,
                            "optional properties imply `undefined`; use `T | null`",
                            pos,
                        );
                    }
                    let pos = self.pos(key.span);
                    let ty = match &prop.type_ann {
                        Some(ann) => {
                            let allow_wire =
                                self.in_boundary && self.boundary_classes.contains(&id);
                            self.allow_wire_alias_boundary = allow_wire;
                            let ty = self.resolve_type(&ann.type_ann);
                            self.allow_wire_alias_boundary = false;
                            ty
                        }
                        None => {
                            self.error(
                                RuleCode::S100,
                                "fields require a type annotation",
                                pos.clone(),
                            );
                            Type::Error
                        }
                    };
                    if self.in_boundary
                        && Self::contains_string_alias(&ty)
                        && !Self::supported_wire_alias_boundary_type(&ty)
                    {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "wire-mapped aliases are supported only as direct boundary-struct members or array-pair elements; member `{}` nests one inside another boundary type",
                                key.sym
                            ),
                            pos.clone(),
                        );
                    }
                    let is_absence_capable = is_descriptor
                        && !prop.definite
                        && prop.is_optional
                        && prop.value.is_none()
                        && matches!(ty, Type::StringAlias(_));
                    if is_descriptor
                        && !prop.definite
                        && prop.is_optional
                        && prop.value.is_none()
                        && !matches!(ty, Type::StringAlias(_) | Type::Error)
                    {
                        self.error(
                            RuleCode::S012,
                            "optional descriptor members require a default initializer",
                            self.pos(prop.span),
                        );
                    }
                    let context_affine = Self::is_context_affine_type(&ty);
                    if context_affine {
                        self.error(
                            RuleCode::S100,
                            "Worker, Inbox, and Outbox values may not be class fields",
                            pos.clone(),
                        );
                    }
                    let foreign_provenance = if self.in_boundary && matches!(ty, Type::Func(_)) {
                        self.callback_provenance(
                            self.cur_file,
                            prop.type_ann
                                .as_deref()
                                .map(|annotation| annotation.type_ann.as_ref()),
                            pos.clone(),
                        )
                    } else {
                        None
                    };
                    // Boundary structs (mirror-ingested) relax the C2
                    // value-field whitelist: they may carry `X | null`,
                    // `object | null`, and function-pointer fields.
                    if is_value
                        && !self.boundary_classes.contains(&id)
                        && !context_affine
                        && !self.value_field_ok(&ty)
                    {
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
                        is_defaulted,
                        is_absence_capable,
                        init: None,
                        foreign_provenance,
                        pos,
                    });
                }
                ast::ClassMember::Constructor(ctor) => {
                    if is_descriptor {
                        self.error(
                            RuleCode::S100,
                            "descriptor classes cannot declare constructors",
                            self.pos(ctor.span),
                        );
                        continue;
                    }
                    let mut params = Vec::new();
                    for p in &ctor.params {
                        match p {
                            ast::ParamOrTsParamProp::Param(param) => {
                                self.allow_wire_alias_boundary = self.in_boundary;
                                let resolved = self.resolve_param_pat(&param.pat);
                                self.allow_wire_alias_boundary = false;
                                if self.in_boundary
                                    && Self::contains_string_alias(&resolved.ty)
                                    && !Self::supported_wire_alias_boundary_type(&resolved.ty)
                                {
                                    self.error(
                                        RuleCode::S100,
                                        format!(
                                            "wire-mapped aliases are supported only as direct mirror-constructor parameters or array-pair elements; parameter `{}` nests one inside another boundary type",
                                            resolved.name
                                        ),
                                        self.pos(param.span),
                                    );
                                }
                                params.push(resolved);
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
                    let (name, key_pos, is_dispose) = match &method.key {
                        ast::PropName::Ident(key) => {
                            (key.sym.to_string(), self.pos(key.span), false)
                        }
                        key if is_dispose_method_key(key) => (
                            hir::DISPOSE_METHOD_NAME.to_string(),
                            self.pos(method.span),
                            true,
                        ),
                        _ => {
                            let pos = self.pos(method.span);
                            self.error(
                                RuleCode::S100,
                                "computed method names are not decided",
                                pos,
                            );
                            continue;
                        }
                    };
                    if is_descriptor {
                        self.error(
                            RuleCode::S100,
                            if is_dispose {
                                "descriptor classes cannot declare `[Symbol.dispose]()`"
                            } else {
                                "descriptor classes cannot declare methods"
                            },
                            key_pos,
                        );
                        continue;
                    }
                    if is_dispose && is_value {
                        self.error(
                            RuleCode::S100,
                            "value classes cannot declare `[Symbol.dispose]()`",
                            key_pos,
                        );
                        continue;
                    }
                    if method.is_static {
                        let pos = self.pos(method.span);
                        self.error(
                            RuleCode::S100,
                            if is_dispose {
                                "`[Symbol.dispose]()` must be non-static"
                            } else if method.function.is_async {
                                "async static methods are not in the decided surface"
                            } else {
                                "static methods and accessors are not decided"
                            },
                            pos,
                        );
                        continue;
                    }
                    if method.kind != ast::MethodKind::Method {
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
                        self.error(
                            RuleCode::S100,
                            if method.function.is_async {
                                "async generator methods are not in the decided surface"
                            } else {
                                "generator methods are not in the decided surface"
                            },
                            pos,
                        );
                        continue;
                    }
                    if is_dispose && method.function.is_async {
                        self.error(
                            RuleCode::S100,
                            "`[Symbol.dispose]()` must be synchronous",
                            key_pos,
                        );
                        continue;
                    }
                    if method.function.is_async && is_value {
                        let pos = self.pos(method.span);
                        self.error(
                            RuleCode::S100,
                            "async methods on `@CStruct` value classes are not in the decided surface",
                            pos,
                        );
                        continue;
                    }
                    let sig = self.resolve_fn_sig(&method.function, key_pos.clone());
                    if is_dispose && (!sig.params.is_empty() || sig.ret != Type::Void) {
                        self.error(
                            RuleCode::S100,
                            "`[Symbol.dispose]()` takes no parameters and returns `void`",
                            key_pos,
                        );
                        continue;
                    }
                    self.class_sigs[id.0].methods.insert(name, sig);
                }
                ast::ClassMember::TsIndexSignature(signature) if !self.in_boundary => {
                    let pos = self.pos(signature.span);
                    if index_signature_pos.is_some() {
                        self.error(
                            RuleCode::S100,
                            "a class can declare at most one index signature",
                            pos,
                        );
                        continue;
                    }
                    index_signature_pos = Some(pos.clone());
                    if is_value || is_descriptor {
                        self.error(
                            RuleCode::S100,
                            "only reference classes can declare an index signature",
                            pos.clone(),
                        );
                    }
                    if signature.is_static {
                        self.error(
                            RuleCode::S100,
                            "a class index signature cannot be static",
                            pos.clone(),
                        );
                    }
                    let index_ty = match signature.params.as_slice() {
                        [ast::TsFnParam::Ident(binding)] => match &binding.type_ann {
                            Some(annotation) => self.resolve_type(&annotation.type_ann),
                            None => {
                                self.error(
                                    RuleCode::S100,
                                    "a class index signature parameter requires a type annotation",
                                    pos.clone(),
                                );
                                Type::Error
                            }
                        },
                        _ => {
                            self.error(
                                RuleCode::S100,
                                "a class index signature requires one identifier parameter",
                                pos.clone(),
                            );
                            Type::Error
                        }
                    };
                    if !matches!(index_ty, Type::I32 | Type::U32 | Type::Error) {
                        let actual = self.type_name(&index_ty);
                        self.error(
                            RuleCode::S100,
                            format!(
                                "a class index signature requires an `i32` or `u32` index, got `{actual}`"
                            ),
                            pos.clone(),
                        );
                    }
                    let element_ty = match &signature.type_ann {
                        Some(annotation) => self.resolve_type(&annotation.type_ann),
                        None => {
                            self.error(
                                RuleCode::S100,
                                "a class index signature requires an element type",
                                pos.clone(),
                            );
                            Type::Error
                        }
                    };
                    self.classes[id.0].index_signature = Some(hir::IndexSignature {
                        index_ty,
                        element_ty,
                        readonly: signature.readonly,
                    });
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
        if let Some(pos) = index_signature_pos {
            self.validate_class_index_accessors(id, pos);
        }
    }

    fn validate_class_index_accessors(&mut self, id: ClassId, pos: Pos) {
        let Some(signature) = self.classes[id.0].index_signature.clone() else {
            return;
        };
        let get_matches = self.class_sigs[id.0]
            .methods
            .get("get")
            .is_some_and(|method| {
                !method.is_async
                    && !method.is_generator
                    && method.params.len() == 1
                    && !method.params[0].has_default
                    && method.params[0].ty == signature.index_ty
                    && method.ret == signature.element_ty
            });
        if !get_matches {
            let index = self.type_name(&signature.index_ty);
            let element = self.type_name(&signature.element_ty);
            self.error(
                RuleCode::S100,
                format!(
                    "the index signature requires `get(index: {index}): {element}` with exactly matching types"
                ),
                pos.clone(),
            );
        }
        if signature.readonly {
            return;
        }
        let set_matches = self.class_sigs[id.0]
            .methods
            .get("set")
            .is_some_and(|method| {
                !method.is_async
                    && !method.is_generator
                    && method.params.len() == 2
                    && method.params.iter().all(|parameter| !parameter.has_default)
                    && method.params[0].ty == signature.index_ty
                    && method.params[1].ty == signature.element_ty
                    && method.ret == Type::Void
            });
        if !set_matches {
            let index = self.type_name(&signature.index_ty);
            let element = self.type_name(&signature.element_ty);
            self.error(
                RuleCode::S100,
                format!(
                    "the index signature requires `set(index: {index}, value: {element}): void` with exactly matching types"
                ),
                pos,
            );
        }
    }

    fn value_field_ok(&self, ty: &Type) -> bool {
        match ty {
            Type::I8
            | Type::U8
            | Type::I16
            | Type::U16
            | Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::F16
            | Type::F32
            | Type::F64
            | Type::Bool
            | Type::Enum(_)
            | Type::StringAlias(_)
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

    fn check_descriptor_defaults_in_file(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        for item in &module.body {
            let decl = match item {
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
                ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => decl,
                _ => continue,
            };
            let ast::Decl::Class(class) = decl else {
                continue;
            };
            if class.class.type_params.is_some() {
                continue;
            }
            let Some(&id) = self.class_ids.get(class.ident.sym.as_ref()) else {
                continue;
            };
            if self.classes[id.0].is_descriptor {
                self.check_descriptor_defaults(id, &class.class);
            }
        }
    }

    fn check_descriptor_defaults(&mut self, id: ClassId, class: &ast::Class) {
        let this_ty = Type::Class(id);
        for member in &class.body {
            let ast::ClassMember::ClassProp(prop) = member else {
                continue;
            };
            let ast::PropName::Ident(key) = &prop.key else {
                continue;
            };
            let Some(value) = &prop.value else {
                continue;
            };
            let field_ty = self.classes[id.0]
                .fields
                .iter()
                .find(|field| field.name == key.sym.as_ref() && field.is_defaulted)
                .map(|field| field.ty.clone());
            let Some(field_ty) = field_ty else {
                continue;
            };
            let mut fx = FnCtx::new(Type::Void, false, Some(this_ty.clone()));
            let checked = self.check_expr(value, Some(&field_ty), &mut fx);
            self.require_assignable(
                &checked.ty.clone(),
                &field_ty,
                checked.pos.clone(),
                "the descriptor member default",
            );
            if let Some(field) = self.classes[id.0]
                .fields
                .iter_mut()
                .find(|field| field.name == key.sym.as_ref())
            {
                field.init = Some(checked);
            }
        }
    }

    fn check_body_decl(&mut self, decl: &ast::Decl, exported: bool) {
        match decl {
            ast::Decl::Fn(f) if f.function.type_params.is_none() => {
                let name = f.ident.sym.to_string();
                let pos = self.pos(f.ident.span);
                let Some(sig) = self.fn_sigs.get(&name).cloned() else {
                    return;
                };
                let function = self.check_function(&f.function, &name, exported, &sig, None, pos);
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
                    let ast::Pat::Ident(binding) = &d.name else {
                        continue;
                    };
                    let name = binding.id.sym.to_string();
                    let Some(sig) = self.global_sigs.get(&name).cloned() else {
                        continue;
                    };
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

    fn lower_using_function(&mut self, function: &ast::Function) -> (ast::Function, Vec<Pos>) {
        let mut lowered = function.clone();
        let mut bindings = Vec::new();
        if let Some(body) = &mut lowered.body {
            self.lower_using_statements(&mut body.stmts, &mut bindings);
        }
        (lowered, bindings)
    }

    fn lower_using_statements(&mut self, statements: &mut [ast::Stmt], bindings: &mut Vec<Pos>) {
        for statement in statements {
            match statement {
                ast::Stmt::Decl(ast::Decl::Using(using)) => {
                    if using.is_await {
                        self.error(
                            RuleCode::S100,
                            "`await using` is not in the decided surface",
                            self.pos(using.span),
                        );
                    } else {
                        for declarator in &using.decls {
                            if let ast::Pat::Ident(binding) = &declarator.name {
                                bindings.push(self.pos(binding.id.span));
                            }
                        }
                    }
                    *statement = ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
                        span: using.span,
                        ctxt: Default::default(),
                        kind: ast::VarDeclKind::Const,
                        declare: false,
                        decls: using.decls.clone(),
                    })));
                }
                ast::Stmt::Block(block) => {
                    self.lower_using_statements(&mut block.stmts, bindings);
                }
                ast::Stmt::If(if_statement) => {
                    self.lower_using_statement(&mut if_statement.cons, bindings);
                    if let Some(alternate) = &mut if_statement.alt {
                        self.lower_using_statement(alternate, bindings);
                    }
                }
                ast::Stmt::While(while_statement) => {
                    self.lower_using_statement(&mut while_statement.body, bindings);
                }
                ast::Stmt::For(for_statement) => {
                    self.lower_using_statement(&mut for_statement.body, bindings);
                }
                ast::Stmt::ForOf(for_of) => {
                    if let ast::ForHead::UsingDecl(using) = &for_of.left {
                        self.error(
                            RuleCode::S100,
                            if using.is_await {
                                "`await using` in a `for` head is not in the decided surface"
                            } else {
                                "`using` in a `for` head is not in the decided surface"
                            },
                            self.pos(using.span),
                        );
                        for_of.left = ast::ForHead::VarDecl(Box::new(ast::VarDecl {
                            span: using.span,
                            ctxt: Default::default(),
                            kind: ast::VarDeclKind::Const,
                            declare: false,
                            decls: using.decls.clone(),
                        }));
                    }
                    self.lower_using_statement(&mut for_of.body, bindings);
                }
                ast::Stmt::Switch(switch_statement) => {
                    for case in &mut switch_statement.cases {
                        self.lower_using_statements(&mut case.cons, bindings);
                    }
                }
                _ => {}
            }
        }
    }

    fn lower_using_statement(&mut self, statement: &mut ast::Stmt, bindings: &mut Vec<Pos>) {
        self.lower_using_statements(std::slice::from_mut(statement), bindings);
    }

    fn is_using_position(positions: &[Pos], pos: &Pos) -> bool {
        positions.iter().any(|candidate| candidate == pos)
    }

    fn dispose_calls(scopes: &[Vec<UsingBinding>], first_scope: usize) -> Vec<hir::Stmt> {
        let mut calls = Vec::new();
        for scope in scopes[first_scope..].iter().rev() {
            for binding in scope.iter().rev() {
                calls.push(hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Call {
                        callee: hir::Callee::Method {
                            recv: Box::new(hir::Expr {
                                kind: hir::ExprKind::Local(binding.name.clone()),
                                ty: binding.ty.clone(),
                                pos: binding.pos.clone(),
                            }),
                            name: hir::DISPOSE_METHOD_NAME.to_string(),
                        },
                        args: Vec::new(),
                    },
                    ty: Type::Void,
                    pos: binding.pos.clone(),
                }));
            }
        }
        calls
    }

    fn rewrite_using_body(
        &mut self,
        body: Vec<hir::Stmt>,
        positions: &[Pos],
        ret: &Type,
    ) -> Vec<hir::Stmt> {
        if positions.is_empty() {
            return body;
        }
        let mut scopes = Vec::new();
        self.rewrite_using_scope(body, positions, ret, &mut scopes, None, None)
    }

    fn rewrite_using_scope(
        &mut self,
        statements: Vec<hir::Stmt>,
        positions: &[Pos],
        ret: &Type,
        scopes: &mut Vec<Vec<UsingBinding>>,
        break_scope: Option<usize>,
        continue_scope: Option<usize>,
    ) -> Vec<hir::Stmt> {
        scopes.push(Vec::new());
        let mut rewritten = Vec::new();
        for statement in statements {
            match statement {
                hir::Stmt::Let {
                    name,
                    ty,
                    mutable,
                    init,
                    pos,
                } => {
                    let is_using = Self::is_using_position(positions, &pos);
                    if is_using && !matches!(ty, Type::Error) {
                        let valid = match &ty {
                            Type::Class(id) => {
                                !self.classes[id.0].is_value
                                    && !self.classes[id.0].is_descriptor
                                    && self.class_sigs[id.0]
                                        .methods
                                        .contains_key(hir::DISPOSE_METHOD_NAME)
                            }
                            _ => false,
                        };
                        if !valid {
                            self.error(
                                RuleCode::S100,
                                "a `using` initializer must be a non-null reference class that declares `[Symbol.dispose](): void`; narrow nullable values first",
                                pos.clone(),
                            );
                        }
                    }
                    rewritten.push(hir::Stmt::Let {
                        name: name.clone(),
                        ty: ty.clone(),
                        mutable,
                        init,
                        pos: pos.clone(),
                    });
                    if is_using {
                        scopes
                            .last_mut()
                            .expect("using scope exists")
                            .push(UsingBinding { name, ty, pos });
                    }
                }
                hir::Stmt::Return { value, pos } => {
                    if let Some(value) = value {
                        if !matches!(ret, Type::Void | Type::Error) {
                            let id = self.next_using_return_id;
                            self.next_using_return_id += 1;
                            let name = format!("[[using.return#{id}]]");
                            rewritten.push(hir::Stmt::Let {
                                name: name.clone(),
                                ty: ret.clone(),
                                mutable: false,
                                init: value,
                                pos: pos.clone(),
                            });
                            rewritten.extend(Self::dispose_calls(scopes, 0));
                            rewritten.push(hir::Stmt::Return {
                                value: Some(hir::Expr {
                                    kind: hir::ExprKind::Local(name),
                                    ty: ret.clone(),
                                    pos: pos.clone(),
                                }),
                                pos,
                            });
                        } else {
                            rewritten.extend(Self::dispose_calls(scopes, 0));
                            rewritten.push(hir::Stmt::Return {
                                value: Some(value),
                                pos,
                            });
                        }
                    } else {
                        rewritten.extend(Self::dispose_calls(scopes, 0));
                        rewritten.push(hir::Stmt::Return { value: None, pos });
                    }
                }
                hir::Stmt::Break(pos) => {
                    if let Some(first_scope) = break_scope {
                        rewritten.extend(Self::dispose_calls(scopes, first_scope));
                    }
                    rewritten.push(hir::Stmt::Break(pos));
                }
                hir::Stmt::Continue(pos) => {
                    if let Some(first_scope) = continue_scope {
                        rewritten.extend(Self::dispose_calls(scopes, first_scope));
                    }
                    rewritten.push(hir::Stmt::Continue(pos));
                }
                hir::Stmt::Block(body) => {
                    let body = self.rewrite_using_scope(
                        body,
                        positions,
                        ret,
                        scopes,
                        break_scope,
                        continue_scope,
                    );
                    rewritten.push(hir::Stmt::Block(body));
                }
                hir::Stmt::If {
                    cond,
                    then,
                    els,
                    pos,
                } => {
                    let then = self.rewrite_using_scope(
                        then,
                        positions,
                        ret,
                        scopes,
                        break_scope,
                        continue_scope,
                    );
                    let els = els.map(|body| {
                        self.rewrite_using_scope(
                            body,
                            positions,
                            ret,
                            scopes,
                            break_scope,
                            continue_scope,
                        )
                    });
                    rewritten.push(hir::Stmt::If {
                        cond,
                        then,
                        els,
                        pos,
                    });
                }
                hir::Stmt::While { cond, body, pos } => {
                    let loop_scope = scopes.len();
                    let body = self.rewrite_using_scope(
                        body,
                        positions,
                        ret,
                        scopes,
                        Some(loop_scope),
                        Some(loop_scope),
                    );
                    rewritten.push(hir::Stmt::While { cond, body, pos });
                }
                hir::Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                    pos,
                } => {
                    let loop_scope = scopes.len();
                    let body = self.rewrite_using_scope(
                        body,
                        positions,
                        ret,
                        scopes,
                        Some(loop_scope),
                        Some(loop_scope),
                    );
                    rewritten.push(hir::Stmt::For {
                        init,
                        cond,
                        step,
                        body,
                        pos,
                    });
                }
                hir::Stmt::ForOf {
                    name,
                    ty,
                    subject,
                    kind,
                    body,
                    pos,
                } => {
                    let loop_scope = scopes.len();
                    let body = self.rewrite_using_scope(
                        body,
                        positions,
                        ret,
                        scopes,
                        Some(loop_scope),
                        Some(loop_scope),
                    );
                    rewritten.push(hir::Stmt::ForOf {
                        name,
                        ty,
                        subject,
                        kind,
                        body,
                        pos,
                    });
                }
                hir::Stmt::Switch { disc, cases, pos } => {
                    let switch_scope = scopes.len();
                    let cases = cases
                        .into_iter()
                        .map(|case| hir::SwitchCase {
                            test: case.test,
                            body: self.rewrite_using_scope(
                                case.body,
                                positions,
                                ret,
                                scopes,
                                Some(switch_scope),
                                continue_scope,
                            ),
                            pos: case.pos,
                        })
                        .collect();
                    rewritten.push(hir::Stmt::Switch { disc, cases, pos });
                }
                other => rewritten.push(other),
            }
        }
        let scope = scopes.pop().expect("using scope exists");
        if !scope.is_empty() {
            rewritten.extend(Self::dispose_calls(std::slice::from_ref(&scope), 0));
        }
        rewritten
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
        let (lowered, using_positions) = self.lower_using_function(f);
        let f = &lowered;
        let mut fx = FnCtx::new(sig.ret.clone(), sig.is_generator, this_ty);
        fx.frames[0].is_async = sig.is_async;
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
        let body = self.rewrite_using_body(body, &using_positions, &sig.ret);
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
            is_async: sig.is_async,
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
                foreign_provenance: None,
                pos,
            });
        }
        out
    }

    /// Checks field initializers, the constructor, and methods (pass C).
    pub(crate) fn check_class_body(&mut self, id: ClassId, class: &ast::Class) {
        if self.classes[id.0].is_descriptor {
            return;
        }
        let this_ty = Type::Class(id);
        for member in &class.body {
            match member {
                ast::ClassMember::ClassProp(prop) => {
                    let ast::PropName::Ident(key) = &prop.key else {
                        continue;
                    };
                    let Some(value) = &prop.value else { continue };
                    let field_ty = self.classes[id.0]
                        .fields
                        .iter()
                        .find(|f| f.name == key.sym.as_ref())
                        .map(|f| f.ty.clone());
                    let Some(field_ty) = field_ty else { continue };
                    let mut fx = FnCtx::new(Type::Void, false, None);
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
                    let Some(params) = self.class_sigs[id.0].ctor.clone() else {
                        continue;
                    };
                    let pos = self.pos(ctor.span);
                    let sig = FnSig {
                        params,
                        ret: Type::Void,
                        is_generator: false,
                        is_async: false,
                        yield_known: true,
                    };
                    let mut fx = FnCtx::new(Type::Void, false, Some(this_ty.clone()));
                    let mut hir_params = Vec::new();
                    for (i, p) in ctor.params.iter().enumerate() {
                        let ast::ParamOrTsParamProp::Param(param) = p else {
                            continue;
                        };
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
                            foreign_provenance: None,
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
                        is_async: false,
                        params: hir_params,
                        ret: Type::Void,
                        body,
                        pos,
                    });
                }
                ast::ClassMember::Method(method) => {
                    let (name, pos) = match &method.key {
                        ast::PropName::Ident(key) => (key.sym.to_string(), self.pos(key.span)),
                        key if is_dispose_method_key(key) => {
                            (hir::DISPOSE_METHOD_NAME.to_string(), self.pos(method.span))
                        }
                        _ => continue,
                    };
                    if method.is_static || method.kind != ast::MethodKind::Method {
                        continue;
                    }
                    let Some(sig) = self.class_sigs[id.0].methods.get(&name).cloned() else {
                        continue;
                    };
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
    pub(crate) fn instantiate_fn(&mut self, key: &str, args: &[Type], pos: Pos) -> Option<String> {
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
        let id = self.new_class(
            &name,
            template.is_value,
            template.is_descriptor,
            template.pos.clone(),
        );
        self.resolve_class_shape(id, &template.class);
        if template.is_descriptor {
            self.check_descriptor_defaults(id, &template.class);
        } else {
            self.check_class_body(id, &template.class);
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        Some(id)
    }

    // ----- shared lookups -----

    /// Resolves a name against the current file's top-level scope, then
    /// the global ambient scope (mirror declarations, P5.2).
    pub(crate) fn scope_item(&self, name: &str) -> Option<ScopeItem> {
        self.file_scopes
            .get(self.cur_file)
            .and_then(|scope| scope.get(name))
            .or_else(|| self.ambient_scope.get(name))
            .cloned()
    }

    /// Looks a name up in the local scope stack. A hit that crosses a
    /// lambda boundary is a capture: it is recorded on every crossed
    /// lambda frame and must refer to a `const` binding (C5).
    pub(crate) fn lookup_local(&mut self, name: &str, pos: &Pos, fx: &mut FnCtx) -> Option<Local> {
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
            if Self::is_context_affine_type(&local.ty) {
                self.error(
                    RuleCode::S100,
                    format!(
                        "lambda captures Context-affine `{name}`; Worker, Inbox, and Outbox values may not be captured"
                    ),
                    pos.clone(),
                );
            }
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
                    if !frame.captures.iter().any(|capture| capture.name == name) {
                        frame.captures.push(hir::Capture {
                            name: name.to_string(),
                            ty: local.ty.clone(),
                        });
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
            hir::ExprKind::ArrayLit(elems) => elems.iter().any(|e| self.is_capturing_value(e, fx)),
            hir::ExprKind::ArraySpreadLit(elems) => elems
                .iter()
                .any(|e| e.spread.is_none() && self.is_capturing_value(&e.expr, fx)),
            _ => false,
        }
    }
}
