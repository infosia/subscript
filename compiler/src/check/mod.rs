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

#[cfg(test)]
pub(crate) use expr::{take_classified_places, PlaceKind};

#[cfg(test)]
std::thread_local! {
    static SYNTHETIC_PREFIX_ESCAPE_HOOK: std::cell::Cell<bool> = const {
        std::cell::Cell::new(false)
    };
}

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir;
use crate::parse::ParsedProgram;
use crate::provenance;
use crate::types::{ClassId, EnumId, StringAliasId, Type};
use crate::CheckOptions;

fn normalize_module_specifier(specifier: &str) -> String {
    specifier
        .trim_start_matches("./")
        .trim_end_matches(".ts")
        .to_string()
}

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
/// spellings are read exactly; synthesized nodes retain the f64 path.
fn int_literal_value(e: &ast::Expr) -> Option<i64> {
    match e {
        ast::Expr::Lit(ast::Lit::Num(n)) => {
            if let Some(raw) = n.raw.as_deref() {
                let value = parse_integer_spelling(raw, false)?;
                return u64::try_from(value).ok().map(|value| value as i64);
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

impl ParamSig {
    fn positional(ty: Type) -> Self {
        Self {
            name: String::new(),
            ty,
            has_default: false,
        }
    }
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
    pub static_methods: HashMap<String, FnSig>,
    pub static_fields: HashMap<String, GlobalSig>,
    /// Generic instance methods, by declared name (§82.4). A template
    /// never reaches the HIR; each call instantiates one method.
    pub generic_methods: HashMap<String, GenericMethod>,
    /// Generic static methods, by declared name (§82.4).
    pub static_generic_methods: HashMap<String, GenericMethod>,
    member_namespace: HashMap<String, ClassMemberNamespaceEntry>,
    static_member_namespace: HashMap<String, ClassMemberNamespaceEntry>,
}

impl ClassSig {
    fn has_member(&self, name: &str) -> bool {
        self.member_namespace.contains_key(name)
    }

    fn has_accessor(&self, name: &str) -> bool {
        matches!(
            self.member_namespace.get(name),
            Some(ClassMemberNamespaceEntry::Accessor { .. })
        )
    }

    fn has_read_accessor(&self, name: &str) -> bool {
        matches!(
            self.member_namespace.get(name),
            Some(ClassMemberNamespaceEntry::Accessor { read: true, .. })
        )
    }

    fn has_static_accessor(&self, name: &str) -> bool {
        matches!(
            self.static_member_namespace.get(name),
            Some(ClassMemberNamespaceEntry::Accessor { .. })
        )
    }

    fn has_static_read_accessor(&self, name: &str) -> bool {
        matches!(
            self.static_member_namespace.get(name),
            Some(ClassMemberNamespaceEntry::Accessor { read: true, .. })
        )
    }

    fn has_static_member(&self, name: &str) -> bool {
        self.static_member_namespace.contains_key(name)
    }

    /// True when the class declares a generic method of this name in the
    /// namespace that `is_static` selects (§82.4).
    pub(crate) fn has_generic_method(&self, name: &str, is_static: bool) -> bool {
        if is_static {
            self.static_generic_methods.contains_key(name)
        } else {
            self.generic_methods.contains_key(name)
        }
    }

    pub(crate) fn generic_method_is_rejected(&self, name: &str, is_static: bool) -> bool {
        let template = if is_static {
            self.static_generic_methods.get(name)
        } else {
            self.generic_methods.get(name)
        };
        template.is_some_and(|template| template.rejected)
    }
}

#[derive(Debug, Clone, Copy)]
enum ClassMemberNamespaceEntry {
    Field,
    Method,
    Accessor { read: bool, write: bool },
}

#[derive(Debug, Clone, Copy)]
enum ClassMemberDeclaration {
    Field,
    Method,
    ReadAccessor,
    WriteAccessor,
}

fn static_member_symbol(class: &str, member: &str) -> String {
    format!("{class}.{member}")
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
    pub rejected: bool,
}

/// A generic method template awaiting monomorphization (§82.4).
#[derive(Debug, Clone)]
pub(crate) struct GenericMethod {
    pub file: usize,
    pub type_params: Vec<String>,
    pub function: ast::Function,
    pub rejected: bool,
}

/// A generic class template awaiting monomorphization.
#[derive(Debug, Clone)]
pub(crate) struct GenericClass {
    pub file: usize,
    pub is_value: bool,
    pub is_descriptor: bool,
    pub alignment_override: Option<hir::AlignmentOverride>,
    pub type_params: Vec<String>,
    pub has_static_member: bool,
    pub has_generic_method: bool,
    pub class: ast::Class,
    pub pos: Pos,
}

/// What a top-level name refers to inside one file's scope.
#[derive(Debug, Clone)]
pub(crate) enum ScopeItem {
    Poisoned,
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
    /// Async-handle creation obligations reachable through this value.
    pub async_origins: HashSet<u32>,
}

/// One lexical scope. `fn_boundary` marks the start of a lambda body:
/// lookups that cross it are captures.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    pub vars: HashMap<String, Local>,
    /// Names that declarations later in this scope own.
    pub pending: HashSet<String>,
    /// The first case that declares each name in a switch body.
    pub switch_declarations: HashMap<String, usize>,
    /// Names that already have a duplicate-declaration diagnostic.
    pub duplicate_declarations: HashSet<String>,
    /// The case that the checker currently checks.
    pub switch_case: Option<usize>,
    /// True when this scope contains one switch body.
    pub is_switch: bool,
    pub fn_boundary: bool,
}

#[derive(Clone)]
struct UsingBinding {
    name: String,
    ty: Type,
    pos: Pos,
    active: Option<String>,
}

struct SwitchUsingStorage {
    source: String,
    active: String,
    storage: String,
    ty: Type,
}

fn has_dispose_binding(statements: &[hir::Stmt]) -> bool {
    fn statement_has_dispose(statement: &hir::Stmt) -> bool {
        matches!(statement, hir::Stmt::Let { dispose: true, .. })
            || statement.children().into_iter().any(|child| match child {
                hir::HirChild::Expr(_) => false,
                hir::HirChild::Stmt(statement) => statement_has_dispose(statement),
            })
    }

    statements.iter().any(statement_has_dispose)
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
    pub missing_this_divergence: Option<Divergence>,
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
    /// Each async handle creation or async-handle parameter in this body.
    pub async_origins: Vec<(Pos, bool)>,
    /// Owner-scoped local declarations required by rewritten expressions.
    synthetic_owners: Vec<Vec<hir::Stmt>>,
}

#[derive(Debug)]
pub(crate) struct SyntheticOwner {
    depth: usize,
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
                missing_this_divergence: None,
            }],
            scopes: vec![Scope::default()],
            narrowed: HashSet::new(),
            loop_depth: 0,
            switch_depth: 0,
            async_origins: Vec::new(),
            synthetic_owners: Vec::new(),
        }
    }

    pub(crate) fn enter_synthetic_owner(&mut self) -> SyntheticOwner {
        let owner = SyntheticOwner {
            depth: self.synthetic_owners.len(),
        };
        self.synthetic_owners.push(Vec::new());
        owner
    }

    pub(crate) fn push_synthetic_prefix(&mut self, statement: hir::Stmt) {
        self.synthetic_owners
            .last_mut()
            .expect("a synthetic prefix must have an owner")
            .push(statement);
    }

    pub(crate) fn drain_synthetic_prefix(&mut self) -> Vec<hir::Stmt> {
        self.synthetic_owners
            .last_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    fn leave_synthetic_owner(&mut self, owner: SyntheticOwner, pos: Pos) -> Option<Pos> {
        if self.synthetic_owners.len() != owner.depth + 1 {
            self.synthetic_owners.truncate(owner.depth);
            return Some(pos);
        }
        #[cfg(test)]
        SYNTHETIC_PREFIX_ESCAPE_HOOK.with(|hook| {
            if hook.replace(false) {
                self.push_synthetic_prefix(hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Null,
                    ty: Type::Null,
                    pos: pos.clone(),
                }));
            }
        });
        let escaped = self
            .synthetic_owners
            .last()
            .and_then(|statements| statements.first())
            .map(|statement| match statement {
                hir::Stmt::Let { pos, .. }
                | hir::Stmt::Return { pos, .. }
                | hir::Stmt::If { pos, .. }
                | hir::Stmt::While { pos, .. }
                | hir::Stmt::For { pos, .. }
                | hir::Stmt::ForOf { pos, .. }
                | hir::Stmt::Switch { pos, .. }
                | hir::Stmt::Break(pos)
                | hir::Stmt::Continue(pos) => pos.clone(),
                hir::Stmt::Expr(expression) => expression.pos.clone(),
                hir::Stmt::Block(_) => pos.clone(),
            });
        self.synthetic_owners.pop();
        escaped
    }

    /// Registers one handle whose underlying computation needs one await.
    pub(crate) fn register_async_origin(&mut self, pos: Pos) -> u32 {
        let id = self.async_origins.len() as u32;
        self.async_origins.push((pos, false));
        id
    }

    /// Discharges every supplied handle origin through await, pass, or return.
    pub(crate) fn handle_async_origins(&mut self, origins: &HashSet<u32>) {
        for origin in origins {
            if let Some((_, handled)) = self.async_origins.get_mut(*origin as usize) {
                *handled = true;
            }
        }
    }

    /// Returns the async obligations carried by one already-resolved local.
    pub(crate) fn local_async_origins(&self, name: &str) -> HashSet<u32> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name))
            .map_or_else(HashSet::new, |local| local.async_origins.clone())
    }

    /// Replaces the obligations carried by a mutable local assignment.
    pub(crate) fn set_local_async_origins(&mut self, name: &str, origins: HashSet<u32>) {
        if let Some(local) = self
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.vars.get_mut(name))
        {
            local.async_origins = origins;
        }
    }

    /// Returns true if a local scope owns the name.
    pub(crate) fn owns_local_name(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| {
            scope.vars.contains_key(name)
                || scope.pending.contains(name)
                || scope.switch_declarations.contains_key(name)
        })
    }

    /// Declares a local. Returns false if the current scope already contains the name.
    pub(crate) fn declare(&mut self, name: &str, local: Local) -> bool {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.vars.contains_key(name) {
                return false;
            }
            scope.pending.remove(name);
            scope.vars.insert(name.to_string(), local);
        }
        true
    }

    /// Removes a declaration reservation after the declaration fails.
    pub(crate) fn discard_pending(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.pending.remove(name);
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
    pub operation_signatures: RefCell<Vec<hir::OperationSignature>>,
    pub global_sigs: HashMap<String, GlobalSig>,
    pub globals: Vec<hir::Global>,
    pub generic_fns: HashMap<String, GenericFn>,
    pub generic_classes: HashMap<String, GenericClass>,
    pub file_scopes: Vec<HashMap<String, ScopeItem>>,
    pub exports: Vec<HashSet<String>>,
    pub top_level: Vec<hir::Stmt>,
    pub poison_missing_modules: HashSet<String>,
    pub poisoned_imports: Vec<hir::PoisonedImport>,
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
    /// Runtime handle classification for each entry in `classes`.
    pub type_handle_classes: Vec<crate::types::HandleClass>,
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
    /// The divergence for an aggregate type in the current declaration.
    pub aggregate_type_divergence: Option<Divergence>,
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
    /// Monotonic suffix for switch-body disposal storage.
    pub next_using_switch_id: usize,
    /// This suffix keeps compound-write operand locals unique.
    pub next_compound_local_id: usize,
}

/// Runs the checker over a parsed program.
pub(crate) fn run(
    prog: &ParsedProgram,
    options: &CheckOptions,
) -> Result<hir::Module, Vec<Diagnostic>> {
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
        operation_signatures: RefCell::new(Vec::new()),
        global_sigs: HashMap::new(),
        globals: Vec::new(),
        generic_fns: HashMap::new(),
        generic_classes: HashMap::new(),
        file_scopes: Vec::new(),
        exports: Vec::new(),
        top_level: Vec::new(),
        poison_missing_modules: options
            .poison_missing_modules
            .iter()
            .map(|specifier| normalize_module_specifier(specifier))
            .collect(),
        poisoned_imports: Vec::new(),
        cur_file: 0,
        subst: HashMap::new(),
        ambient_scope: HashMap::new(),
        foreign_sigs: HashMap::new(),
        foreign_defs: Vec::new(),
        foreign_mirrors: Vec::new(),
        foreign_mirror_ids: HashMap::new(),
        handle_classes: HashSet::new(),
        type_handle_classes: Vec::new(),
        boundary_classes: HashSet::new(),
        type_aliases: HashMap::new(),
        in_boundary: false,
        allow_wire_alias_boundary: false,
        in_assoc_key: false,
        in_json_argument: false,
        in_for_of_subject: false,
        aggregate_type_divergence: None,
        pending_layouts: Vec::new(),
        ambient_int_consts: HashMap::new(),
        next_for_of_id: 0,
        regex_literals: HashMap::new(),
        next_regex_literal_id: 0,
        next_using_return_id: 0,
        next_using_switch_id: 0,
        next_compound_local_id: 0,
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
    let initializer_diags = module_initializer_diagnostics(&ck);
    ck.diags.extend(initializer_diags);
    ck.validate_layouts();

    if ck.diags.is_empty() {
        let mut module = hir::Module {
            poisoned_imports: ck.poisoned_imports,
            classes: ck.classes,
            enums: ck.enums,
            string_aliases: ck.string_aliases,
            globals: ck.globals,
            functions: ck.functions,
            worker_entries: ck.worker_entries,
            operation_signatures: ck.operation_signatures.into_inner(),
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ModuleFunction {
    Free(String),
    Constructor(ClassId),
    Method(ClassId, String),
}

#[derive(Clone, Default)]
struct ModuleEffects {
    accesses: BTreeMap<String, Vec<String>>,
    calls: Vec<(ModuleFunction, String)>,
}

struct ModuleEffectScanner<'a> {
    bindings: &'a [String],
    classes: &'a [hir::ClassDef],
    effects: ModuleEffects,
}

impl<'a> ModuleEffectScanner<'a> {
    fn new(bindings: &'a [String], classes: &'a [hir::ClassDef]) -> Self {
        Self {
            bindings,
            classes,
            effects: ModuleEffects::default(),
        }
    }

    fn function(mut self, function: &hir::Function) -> ModuleEffects {
        for parameter in &function.params {
            if let Some(default) = &parameter.default {
                self.expr(default);
            }
        }
        self.stmts(&function.body);
        self.effects
    }

    fn constructor(mut self, class: &hir::ClassDef) -> ModuleEffects {
        for field in &class.fields {
            if let Some(initializer) = &field.init {
                self.expr(initializer);
            }
        }
        if let Some(constructor) = &class.ctor {
            for parameter in &constructor.params {
                if let Some(default) = &parameter.default {
                    self.expr(default);
                }
            }
            self.stmts(&constructor.body);
        }
        self.effects
    }

    fn record_access(&mut self, name: &str, path: Vec<String>) {
        let replace = self
            .effects
            .accesses
            .get(name)
            .is_none_or(|current| path_is_better(&path, current));
        if replace {
            self.effects.accesses.insert(name.to_string(), path);
        }
    }

    fn record_indirect_call(&mut self) {
        for binding in self.bindings {
            self.record_access(binding, vec!["[indirect call]".to_string()]);
        }
    }

    fn record_call(&mut self, function: ModuleFunction, label: String) {
        self.effects.calls.push((function, label));
    }

    fn class_of(ty: &Type) -> Option<ClassId> {
        match ty {
            Type::Class(id) => Some(*id),
            Type::Nullable(inner) => Self::class_of(inner),
            _ => None,
        }
    }

    fn method_call(&mut self, receiver: &hir::Expr, name: &str) {
        let Some(class_id) = Self::class_of(&receiver.ty) else {
            return;
        };
        let Some(class) = self.classes.get(class_id.0) else {
            return;
        };
        if class.methods.iter().any(|method| method.name == name) {
            self.record_call(
                ModuleFunction::Method(class_id, name.to_string()),
                format!("{}.{}", class.name, name),
            );
        }
    }

    fn async_callee(&mut self, callee: &hir::AsyncCallee) {
        match callee {
            hir::AsyncCallee::Function(name) => {
                self.record_call(ModuleFunction::Free(name.clone()), name.clone());
            }
            hir::AsyncCallee::Method {
                class,
                receiver: _,
                name,
            } => {
                let label = self.classes.get(class.0).map_or_else(
                    || name.clone(),
                    |definition| format!("{}.{}", definition.name, name),
                );
                self.record_call(ModuleFunction::Method(*class, name.clone()), label);
            }
        }
    }

    fn stmts(&mut self, statements: &[hir::Stmt]) {
        for statement in statements {
            self.stmt(statement);
        }
    }

    fn stmt(&mut self, statement: &hir::Stmt) {
        for child in statement.children() {
            match child {
                hir::HirChild::Expr(expression) => self.expr(expression),
                hir::HirChild::Stmt(statement) => self.stmt(statement),
            }
        }
    }

    fn expr(&mut self, expression: &hir::Expr) {
        use hir::ExprKind as K;

        match &expression.kind {
            K::Global(name) => self.record_access(name, Vec::new()),
            K::Call { callee, .. } => {
                match callee {
                    hir::Callee::Func(name) => {
                        self.record_call(ModuleFunction::Free(name.clone()), name.clone());
                    }
                    hir::Callee::Value(_) => self.record_indirect_call(),
                    hir::Callee::Method { recv, name } => self.method_call(recv, name),
                    _ => {}
                }
                let invokes_callback = match callee {
                    hir::Callee::Arr(function) => function.takes_callback(),
                    hir::Callee::Map(function) => {
                        matches!(function, hir::MapFn::ForEach | hir::MapFn::GroupBy)
                    }
                    hir::Callee::Set(function) => matches!(function, hir::SetFn::ForEach),
                    _ => false,
                };
                if invokes_callback {
                    self.record_indirect_call();
                }
            }
            K::New { class, .. } => {
                let label = self.classes.get(class.0).map_or_else(
                    || "constructor".to_string(),
                    |definition| format!("{}.constructor", definition.name),
                );
                self.record_call(ModuleFunction::Constructor(*class), label);
            }
            K::DescriptorLit { class, fields } => {
                let defaults = self
                    .classes
                    .get(class.0)
                    .map(|definition| &definition.fields);
                for (index, field) in fields.iter().enumerate() {
                    if field.is_none() {
                        if let Some(default) = defaults
                            .and_then(|fields| fields.get(index))
                            .and_then(|field| field.init.as_ref())
                        {
                            self.expr(default);
                        }
                    }
                }
            }
            K::AsyncCall { callee, .. } | K::AsyncHandleCreate { callee, .. } => {
                self.async_callee(callee);
            }
            K::Lambda { .. } => return,
            _ => {}
        }
        for child in expression.children() {
            match child {
                hir::HirChild::Expr(expression) => self.expr(expression),
                hir::HirChild::Stmt(statement) => self.stmt(statement),
            }
        }
    }
}

fn path_is_better(candidate: &[String], current: &[String]) -> bool {
    candidate.len() < current.len() || (candidate.len() == current.len() && candidate < current)
}

fn merge_path(accesses: &mut BTreeMap<String, Vec<String>>, name: &str, path: Vec<String>) -> bool {
    let replace = accesses
        .get(name)
        .is_none_or(|current| path_is_better(&path, current));
    if replace {
        accesses.insert(name.to_string(), path);
    }
    replace
}

fn resolve_module_effects(
    effects: &ModuleEffects,
    summaries: &HashMap<ModuleFunction, BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, Vec<String>> {
    let mut accesses = effects.accesses.clone();
    for (callee, label) in &effects.calls {
        let Some(callee_accesses) = summaries.get(callee) else {
            continue;
        };
        for (name, path) in callee_accesses {
            let mut candidate = Vec::with_capacity(path.len() + 1);
            candidate.push(label.clone());
            candidate.extend(path.iter().cloned());
            merge_path(&mut accesses, name, candidate);
        }
    }
    accesses
}

fn module_data_bindings(checker: &Checker<'_>) -> Vec<String> {
    let mut bindings = Vec::new();
    for (file_index, file) in checker.prog.files.iter().enumerate() {
        if file.dts {
            continue;
        }
        for item in &file.module.body {
            let Some(declaration) = module_decl(item) else {
                continue;
            };
            match declaration {
                ast::Decl::Var(variables) if variables.kind != ast::VarDeclKind::Var => {
                    for declarator in &variables.decls {
                        if let ast::Pat::Ident(binding) = &declarator.name {
                            bindings.push(binding.id.sym.to_string());
                        }
                    }
                }
                ast::Decl::Using(using) => {
                    for declarator in &using.decls {
                        if let ast::Pat::Ident(binding) = &declarator.name {
                            bindings.push(binding.id.sym.to_string());
                        }
                    }
                }
                ast::Decl::Class(class) if class.class.type_params.is_none() => {
                    let class_name = class.ident.sym.as_ref();
                    let class_id = checker.file_scopes[file_index].get(class_name).and_then(
                        |item| match item {
                            ScopeItem::Class(id) => Some(*id),
                            _ => None,
                        },
                    );
                    let Some(class_id) = class_id else {
                        continue;
                    };
                    for member in &class.class.body {
                        let ast::ClassMember::ClassProp(property) = member else {
                            continue;
                        };
                        if !property.is_static {
                            continue;
                        }
                        let ast::PropName::Ident(name) = &property.key else {
                            continue;
                        };
                        if checker.class_sigs[class_id.0]
                            .static_fields
                            .contains_key(name.sym.as_ref())
                        {
                            bindings.push(static_member_symbol(class_name, name.sym.as_ref()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    bindings
}

fn module_initializer_diagnostics(checker: &Checker<'_>) -> Vec<Diagnostic> {
    let bindings = module_data_bindings(checker);
    let binding_order: HashMap<&str, usize> = bindings
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect();
    let mut effects = HashMap::new();

    for function in &checker.functions {
        let direct = ModuleEffectScanner::new(&bindings, &checker.classes).function(function);
        effects.insert(ModuleFunction::Free(function.name.clone()), direct);
    }
    for (index, class) in checker.classes.iter().enumerate() {
        let class_id = ClassId(index);
        let constructor = ModuleEffectScanner::new(&bindings, &checker.classes).constructor(class);
        effects.insert(ModuleFunction::Constructor(class_id), constructor);
        for method in &class.methods {
            let direct = ModuleEffectScanner::new(&bindings, &checker.classes).function(method);
            effects.insert(
                ModuleFunction::Method(class_id, method.name.clone()),
                direct,
            );
        }
    }

    let mut summaries: HashMap<_, _> = effects
        .iter()
        .map(|(function, effect)| (function.clone(), effect.accesses.clone()))
        .collect();
    loop {
        let mut changed = false;
        for (function, effect) in &effects {
            let resolved = resolve_module_effects(effect, &summaries);
            let Some(summary) = summaries.get_mut(function) else {
                continue;
            };
            for (name, path) in resolved {
                changed |= merge_path(summary, &name, path);
            }
        }
        if !changed {
            break;
        }
    }

    let mut diagnostics = Vec::new();
    for global in &checker.globals {
        let Some(&initializer_index) = binding_order.get(global.name.as_str()) else {
            continue;
        };
        let mut scanner = ModuleEffectScanner::new(&bindings, &checker.classes);
        scanner.expr(&global.init);
        let accesses = resolve_module_effects(&scanner.effects, &summaries);
        let violation = bindings
            .iter()
            .skip(initializer_index)
            .find_map(|binding| accesses.get(binding).map(|path| (binding, path)));
        let Some((binding, path)) = violation else {
            continue;
        };
        let route = if path.is_empty() {
            "directly from this initializer".to_string()
        } else {
            let path = path
                .iter()
                .map(|step| {
                    if step == "[indirect call]" {
                        "an indirect call".to_string()
                    } else {
                        format!("`{step}`")
                    }
                })
                .collect::<Vec<_>>()
                .join(" -> ");
            format!("through {path}")
        };
        let mut diagnostic = Diagnostic::new(
            RuleCode::S100,
            format!("`{binding}` is accessed before its declaration, {route}"),
            global.init.pos.clone(),
        );
        if !path.is_empty() {
            diagnostic.divergence = Some(Divergence::ModuleInitializerOrder);
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

impl<'p> Checker<'p> {
    pub(crate) fn finish_synthetic_owner(
        &mut self,
        fx: &mut FnCtx,
        owner: SyntheticOwner,
        pos: Pos,
    ) {
        if let Some(pos) = fx.leave_synthetic_owner(owner, pos) {
            self.error(
                RuleCode::S100,
                "internal: synthetic prefix escaped its owner",
                pos,
            );
        }
    }

    /// Reserves the names that declarations own in one statement list.
    pub(crate) fn reserve_block_declarations(&self, statements: &[ast::Stmt], fx: &mut FnCtx) {
        let Some(scope) = fx.scopes.last_mut() else {
            return;
        };
        for statement in statements {
            let ast::Stmt::Decl(declaration) = statement else {
                continue;
            };
            let declarators = match declaration {
                ast::Decl::Var(declaration) if declaration.kind != ast::VarDeclKind::Var => {
                    &declaration.decls
                }
                ast::Decl::Using(declaration) => &declaration.decls,
                _ => continue,
            };
            for declarator in declarators {
                if let ast::Pat::Ident(binding) = &declarator.name {
                    let name = binding.id.sym.to_string();
                    if !scope.vars.contains_key(&name) {
                        scope.pending.insert(name);
                    }
                }
            }
        }
    }

    /// Declares one local and reports a duplicate in the current scope.
    pub(crate) fn declare_local(&mut self, name: &str, local: Local, pos: Pos, fx: &mut FnCtx) {
        let in_switch = fx.scopes.last().is_some_and(|scope| scope.is_switch);
        if !fx.declare(name, local) {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.duplicate_declarations.insert(name.to_string());
            }
            let message = if in_switch {
                format!("duplicate declaration of `{name}` in one switch body")
            } else {
                format!("duplicate declaration of `{name}` in one scope")
            };
            self.error(RuleCode::S017, message, pos);
        }
    }

    pub(crate) fn error(&mut self, code: RuleCode, message: impl Into<String>, pos: Pos) {
        self.diags.push(Diagnostic::new(code, message, pos));
    }

    pub(crate) fn error_diverging(
        &mut self,
        code: RuleCode,
        message: impl Into<String>,
        pos: Pos,
        divergence: Divergence,
    ) {
        let mut diagnostic = Diagnostic::new(code, message, pos);
        diagnostic.divergence = Some(divergence);
        self.diags.push(diagnostic);
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
        ty.uses_reference_identity(&self.type_handle_classes)
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
        self.require_assignable_with(from, to, pos, what, None);
    }

    pub(crate) fn require_assignable_with(
        &mut self,
        from: &Type,
        to: &Type,
        pos: Pos,
        what: &str,
        divergence: Option<Divergence>,
    ) {
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
            let message = format!(
                "nominal types are not interchangeable: {} expects `{}`, got `{}`",
                what, to_n, from_n
            );
            if matches!((from, to), (Type::Class(_), Type::Class(_))) {
                self.error_diverging(
                    RuleCode::S005,
                    message,
                    pos,
                    Divergence::NominalClassIdentity,
                );
            } else {
                self.error(RuleCode::S005, message, pos);
            }
        } else if from.is_numeric() && to.is_numeric() {
            self.error_diverging(
                RuleCode::S007,
                format!(
                    "implicit numeric conversion from `{}` to `{}`; spell it `as {}`",
                    from_n, to_n, to_n
                ),
                pos,
                Divergence::SizedOperandWidths,
            );
        } else if self.is_value_class(from) && matches!(to, Type::Nullable(_)) {
            self.error(
                RuleCode::S011,
                format!("value class `{}` cannot be nullable", from_n),
                pos,
            );
        } else {
            let message = format!(
                "type mismatch: {} expects `{}`, got `{}`",
                what, to_n, from_n
            );
            let divergence = divergence.or_else(|| {
                matches!((from, to), (Type::StringAlias(_), Type::StringAlias(_)))
                    .then_some(Divergence::LiteralUnionAlias)
            });
            if let Some(divergence) = divergence {
                self.error_diverging(RuleCode::S100, message, pos, divergence);
            } else {
                self.error(RuleCode::S100, message, pos);
            }
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
                RuleCode::S017,
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

    fn cstruct_alignment(call: &ast::CallExpr) -> Result<u32, &'static str> {
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            return Err("`@CStruct` accepts exactly one object-literal argument");
        }
        let ast::Expr::Object(options) = &*call.args[0].expr else {
            return Err("`@CStruct` accepts exactly one object-literal argument");
        };
        if options.props.len() != 1 {
            return Err("`@CStruct` options must contain only the `align` key");
        }
        let ast::PropOrSpread::Prop(prop) = &options.props[0] else {
            return Err("`@CStruct` options must contain only the `align` key");
        };
        let ast::Prop::KeyValue(property) = &**prop else {
            return Err("`@CStruct` options must contain only the `align` key");
        };
        let is_align = match &property.key {
            ast::PropName::Ident(key) => key.sym.as_ref() == "align",
            ast::PropName::Str(key) => key.value.as_str() == "align",
            _ => false,
        };
        if !is_align {
            return Err("`@CStruct` options must contain only the `align` key");
        }
        let ast::Expr::Lit(ast::Lit::Num(number)) = &*property.value else {
            return Err("`@CStruct` alignment must be an integer literal in {2, 4, 8, 16}");
        };
        let value = number.value;
        if value.fract() != 0.0 || !matches!(value as u32, 2 | 4 | 8 | 16) {
            return Err("`@CStruct` alignment must be an integer literal in {2, 4, 8, 16}");
        }
        Ok(value as u32)
    }

    fn class_decorators(
        &mut self,
        class: &ast::Class,
    ) -> (bool, bool, Option<hir::AlignmentOverride>) {
        let mut is_value = false;
        let mut is_descriptor = false;
        let mut alignment_override = None;
        for dec in &class.decorators {
            match &*dec.expr {
                ast::Expr::Ident(id) if id.sym.as_ref() == "CStruct" => is_value = true,
                ast::Expr::Ident(id) if id.sym.as_ref() == "Descriptor" => {
                    is_descriptor = true;
                }
                ast::Expr::Call(call)
                    if matches!(
                        &call.callee,
                        ast::Callee::Expr(callee)
                            if matches!(&**callee, ast::Expr::Ident(id) if id.sym.as_ref() == "CStruct")
                    ) =>
                {
                    is_value = true;
                    match Self::cstruct_alignment(call) {
                        Ok(value) => {
                            alignment_override = Some(hir::AlignmentOverride {
                                value,
                                pos: self.pos(dec.span),
                            });
                        }
                        Err(message) => {
                            self.error(RuleCode::S100, message, self.pos(dec.span));
                        }
                    }
                }
                ast::Expr::Call(call)
                    if matches!(
                        &call.callee,
                        ast::Callee::Expr(callee)
                            if matches!(&**callee, ast::Expr::Ident(id) if id.sym.as_ref() == "Descriptor")
                    ) =>
                {
                    is_descriptor = true;
                    self.error(
                        RuleCode::S100,
                        "`@Descriptor` does not accept options",
                        self.pos(dec.span),
                    );
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
        (is_value, is_descriptor, alignment_override)
    }

    fn collect_class(&mut self, file: usize, c: &ast::ClassDecl, exported: bool) {
        let name = c.ident.sym.to_string();
        let pos = self.pos(c.ident.span);
        let (is_value, is_descriptor, alignment_override) = self.class_decorators(&c.class);
        if let Some(tp) = &c.class.type_params {
            let static_members = c.class.body.iter().filter_map(|member| match member {
                ast::ClassMember::ClassProp(property) if property.is_static => Some(property.span),
                ast::ClassMember::Method(method) if method.is_static => Some(method.span),
                _ => None,
            });
            let mut has_static_member = false;
            for span in static_members {
                has_static_member = true;
                self.error_diverging(
                    RuleCode::S100,
                    "generic classes cannot declare static members",
                    self.pos(span),
                    Divergence::StaticMemberSurface,
                );
            }
            // §82.4 rule 5: the checker holds one substitution, so a
            // generic method on a generic class is out of the surface.
            let generic_methods = c.class.body.iter().filter_map(|member| match member {
                ast::ClassMember::Method(method)
                    if method.kind == ast::MethodKind::Method
                        && method.function.type_params.is_some() =>
                {
                    Some(method.span)
                }
                _ => None,
            });
            let mut has_generic_method = false;
            for span in generic_methods {
                has_generic_method = true;
                self.error_diverging(
                    RuleCode::S100,
                    "generic classes cannot declare generic methods",
                    self.pos(span),
                    Divergence::GenericMethodOnGenericClass,
                );
            }
            let type_params: Vec<String> =
                tp.params.iter().map(|p| p.name.sym.to_string()).collect();
            self.generic_classes.insert(
                name.clone(),
                GenericClass {
                    file,
                    is_value,
                    is_descriptor,
                    alignment_override,
                    type_params,
                    has_static_member,
                    has_generic_method,
                    class: (*c.class).clone(),
                    pos: pos.clone(),
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericClass(name.clone()), pos);
        } else {
            let id = self.new_class(
                &name,
                is_value,
                is_descriptor,
                alignment_override,
                pos.clone(),
            );
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
        alignment_override: Option<hir::AlignmentOverride>,
        pos: Pos,
    ) -> ClassId {
        let id = ClassId(self.classes.len());
        self.classes.push(hir::ClassDef {
            name: name.to_string(),
            is_value,
            alignment_override,
            is_descriptor,
            is_boundary: false,
            fields: Vec::new(),
            ctor: None,
            methods: Vec::new(),
            index_signature: None,
            pos: pos.clone(),
        });
        self.type_handle_classes
            .push(crate::types::HandleClass::from(&self.classes[id.0]));
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
            let bodiless = f.function.body.is_none();
            if bodiless {
                self.error(RuleCode::S100, "function bodies are required", pos.clone());
            }
            let (type_params, duplicate_type_parameter) = self.collect_type_parameter_names(tp);
            self.generic_fns.insert(
                name.clone(),
                GenericFn {
                    file,
                    type_params,
                    function: (*f.function).clone(),
                    rejected: bodiless || duplicate_type_parameter,
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericFunc(name.clone()), pos);
        } else {
            if self.fn_sigs.contains_key(&name) {
                self.error(
                    RuleCode::S017,
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
        }
    }

    fn collect_type_parameter_names(
        &mut self,
        params: &ast::TsTypeParamDecl,
    ) -> (Vec<String>, bool) {
        let mut names = HashSet::new();
        let mut duplicate = false;
        let names = params
            .params
            .iter()
            .map(|parameter| {
                let name = parameter.name.sym.to_string();
                if !names.insert(name.clone()) {
                    duplicate = true;
                    self.error(
                        RuleCode::S017,
                        format!("duplicate type parameter `{name}`"),
                        self.pos(parameter.name.span),
                    );
                }
                name
            })
            .collect();
        (names, duplicate)
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
        // `None` means the previous member's value + 1 overflows i32, so
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
                                "implicit value for enum member `{}` overflows i32",
                                member_name
                            ),
                            p,
                        );
                        0
                    }
                },
                Some(init) => match self.const_int_of(init) {
                    Some(v) => i64::from(v),
                    None => {
                        let p = self.pos(init.span());
                        self.error_diverging(
                            RuleCode::S100,
                            "enum members must have integer literal values",
                            p,
                            Divergence::IntegerLiteralRange,
                        );
                        next.unwrap_or(0)
                    }
                },
            };
            next = i32::try_from(value)
                .ok()
                .and_then(|value| value.checked_add(1))
                .map(i64::from);
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
                self.error_diverging(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(property.span),
                    Divergence::WireEnumValues,
                );
                return;
            };
            let ast::TsType::TsLitType(ast::TsLitType {
                lit: ast::TsLit::Number(number),
                ..
            }) = &*annotation.type_ann
            else {
                self.error_diverging(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(annotation.type_ann.span()),
                    Divergence::WireEnumValues,
                );
                return;
            };
            if !number.value.is_finite() || number.value.fract() != 0.0 {
                self.error_diverging(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(number.span),
                    Divergence::WireEnumValues,
                );
                return;
            }
            if number.value < f64::from(i32::MIN) || number.value > f64::from(i32::MAX) {
                let spelling = number
                    .raw
                    .as_ref()
                    .map_or_else(|| number.value.to_string(), ToString::to_string);
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "wire value {spelling} for CEnum member `{member}` is outside the i32 range"
                    ),
                    self.pos(number.span),
                    Divergence::WireEnumValues,
                );
                return;
            }
            let wire = number.value as i32;
            if let Some(first) = seen_wires.insert(wire, member.clone()) {
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "duplicate CEnum wire value {wire} for members `{first}` and `{member}`"
                    ),
                    self.pos(number.span),
                    Divergence::WireEnumValues,
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

    fn const_int_of(&self, e: &ast::Expr) -> Option<i32> {
        fn read(e: &ast::Expr, negate: bool) -> Option<i32> {
            match e {
                ast::Expr::Lit(ast::Lit::Num(number)) => {
                    let raw = number.raw.as_deref()?;
                    i32::try_from(parse_integer_spelling(raw, negate)?).ok()
                }
                ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::Minus => {
                    read(&unary.arg, !negate)
                }
                ast::Expr::Paren(paren) => read(&paren.expr, negate),
                _ => None,
            }
        }

        read(e, false)
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
        let id = self.new_class(&name, false, false, None, pos.clone());
        self.handle_classes.insert(id);
        self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
    }

    /// A mirror `declare class` is a boundary struct: a C-layout value
    /// type (Q13) whose fields may hold boundary types. Shape resolved
    /// in pass B.
    fn collect_boundary_struct(&mut self, file: usize, c: &ast::ClassDecl) {
        let name = c.ident.sym.to_string();
        let pos = self.pos(c.ident.span);
        let id = self.new_class(&name, true, false, None, pos.clone());
        self.boundary_classes.insert(id);
        self.classes[id.0].is_boundary = true;
        self.type_handle_classes[id.0] = crate::types::HandleClass::BoundaryValue;
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
            let Some(decl) = module_decl(item) else {
                continue;
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
            let Some(decl) = module_decl(item) else {
                continue;
            };
            match decl {
                ast::Decl::Class(c) if c.class.type_params.is_none() => {
                    let name = c.ident.sym.to_string();
                    if let Some(&id) = self.class_ids.get(&name) {
                        self.resolve_class_shape(id, &c.class, c.declare);
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
                let stem = normalize_module_specifier(&raw);
                let Some(target) = self.prog.files.iter().position(|f| f.stem == stem) else {
                    let pos = self.pos(import.src.span);
                    let missing_message =
                        format!("imported module `{raw}` is not among the program's files");
                    if self.poison_missing_modules.contains(&stem) {
                        if import.specifiers.is_empty() {
                            self.error(RuleCode::S100, missing_message, pos);
                            continue;
                        }
                        let mut names = Vec::new();
                        for spec in &import.specifiers {
                            let ast::ImportSpecifier::Named(named) = spec else {
                                self.error(
                                    RuleCode::S100,
                                    "only named imports are in the decided surface",
                                    self.pos(spec.span()),
                                );
                                continue;
                            };
                            let local = named.local.sym.to_string();
                            let imported = named
                                .imported
                                .as_ref()
                                .map_or_else(|| local.clone(), |name| name.atom().to_string());
                            additions.push((
                                local.clone(),
                                ScopeItem::Poisoned,
                                self.pos(named.local.span),
                            ));
                            names.push((imported, local));
                        }
                        if !names.is_empty() {
                            self.poisoned_imports.push(hir::PoisonedImport {
                                module: raw,
                                names,
                                pos,
                            });
                        }
                    } else {
                        self.error(RuleCode::S100, missing_message, pos);
                    }
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
                            RuleCode::S016,
                            format!("`{}` is not exported by `{}`", local, raw),
                            pos,
                        );
                        continue;
                    }
                    match self.file_scopes[target].get(&local) {
                        Some(item) => additions.push((local, item.clone(), pos)),
                        None => {
                            self.error(
                                RuleCode::S016,
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
            let Some(decl) = module_decl(item) else {
                continue;
            };
            match decl {
                ast::Decl::Class(c) if c.class.type_params.is_none() => {
                    let name = c.ident.sym.to_string();
                    if let Some(&id) = self.class_ids.get(&name) {
                        self.resolve_class_shape(id, &c.class, c.declare);
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
                            self.error_diverging(
                                RuleCode::S100,
                                format!(
                                    "exported function `{name}` has a string-literal union \
                                     alias in its boundary signature"
                                ),
                                self.pos(f.ident.span),
                                Divergence::EntryParameterType,
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
                            self.error_diverging(
                                RuleCode::S100,
                                "Worker, Inbox, and Outbox values may not be module globals",
                                self.pos(binding.id.span),
                                Divergence::WorkerContextAffinity,
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

    fn claim_class_member_name(
        &mut self,
        id: ClassId,
        name: &str,
        declaration: ClassMemberDeclaration,
        is_static: bool,
        pos: Pos,
    ) -> bool {
        use ClassMemberDeclaration::{Field, Method, ReadAccessor, WriteAccessor};
        use ClassMemberNamespaceEntry::Accessor;

        let namespace = if is_static {
            &self.class_sigs[id.0].static_member_namespace
        } else {
            &self.class_sigs[id.0].member_namespace
        };
        let existing = namespace.get(name).copied();
        let entry = match (existing, declaration) {
            (None, Field) => ClassMemberNamespaceEntry::Field,
            (None, Method) => ClassMemberNamespaceEntry::Method,
            (None, ReadAccessor) => Accessor {
                read: true,
                write: false,
            },
            (None, WriteAccessor) => Accessor {
                read: false,
                write: true,
            },
            (Some(Accessor { read: true, .. }), ReadAccessor) => {
                self.error(
                    RuleCode::S017,
                    format!(
                        "two {}accessors cannot declare the read member `{name}`",
                        if is_static { "static " } else { "" }
                    ),
                    pos,
                );
                return false;
            }
            (Some(Accessor { write: true, .. }), WriteAccessor) => {
                self.error(
                    RuleCode::S017,
                    format!(
                        "two {}accessors cannot declare the write member `{name}`",
                        if is_static { "static " } else { "" }
                    ),
                    pos,
                );
                return false;
            }
            (Some(Accessor { write, .. }), ReadAccessor) => Accessor { read: true, write },
            (Some(Accessor { read, .. }), WriteAccessor) => Accessor { read, write: true },
            (Some(existing), declaration) => {
                let existing_kind = match existing {
                    ClassMemberNamespaceEntry::Field => "field",
                    ClassMemberNamespaceEntry::Method => "method",
                    Accessor { .. } => "accessor",
                };
                let declared_kind = match declaration {
                    Field => "field",
                    Method => "method",
                    ReadAccessor | WriteAccessor => "accessor",
                };
                let message = match (existing_kind, declared_kind) {
                    ("accessor", "field") | ("field", "accessor") => {
                        format!("a field and an accessor cannot share the member name `{name}`")
                    }
                    ("accessor", "method") | ("method", "accessor") => {
                        format!("a method and an accessor cannot share the member name `{name}`")
                    }
                    _ => format!(
                        "a {declared_kind} cannot share the member name `{name}` with a {existing_kind}"
                    ),
                };
                let message = if is_static {
                    message.replacen("a ", "a static ", 1)
                } else {
                    message
                };
                self.error(RuleCode::S017, message, pos);
                return false;
            }
        };
        if is_static {
            self.class_sigs[id.0]
                .static_member_namespace
                .insert(name.to_string(), entry);
        } else {
            self.class_sigs[id.0]
                .member_namespace
                .insert(name.to_string(), entry);
        }
        true
    }

    /// Resolves a class's fields and callable signatures (pass B), and
    /// enforces C2 (no inheritance for value classes; field whitelist).
    pub(crate) fn resolve_class_shape(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
        let is_value = self.classes[id.0].is_value;
        let is_descriptor = self.classes[id.0].is_descriptor;
        let mut index_signature_pos = None;
        let mut write_accessors = Vec::new();
        if let Some(sup) = &class.super_class {
            let pos = self.pos(sup.span());
            if is_value {
                self.error_diverging(
                    RuleCode::S006,
                    "value classes do not inherit",
                    pos,
                    Divergence::ValueClassLayout,
                );
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
                        let pos = self.pos(key.span);
                        if is_descriptor {
                            self.error(
                                RuleCode::S100,
                                "descriptor classes cannot declare static fields",
                                pos,
                            );
                            continue;
                        }
                        if self.in_boundary || self.classes[id.0].is_boundary {
                            self.error(
                                RuleCode::S100,
                                "mirror classes cannot declare static fields",
                                pos,
                            );
                            continue;
                        }
                        let name = key.sym.to_string();
                        if !self.claim_class_member_name(
                            id,
                            &name,
                            ClassMemberDeclaration::Field,
                            true,
                            self.pos(key.span),
                        ) {
                            continue;
                        }
                        if prop.is_optional {
                            self.error(
                                RuleCode::S012,
                                "optional static fields imply `undefined`; use `T | null`",
                                self.pos(prop.span),
                            );
                        }
                        let ty = match &prop.type_ann {
                            Some(annotation) => self.resolve_type(&annotation.type_ann),
                            None => {
                                self.error(
                                    RuleCode::S100,
                                    "static fields require a type annotation",
                                    self.pos(key.span),
                                );
                                Type::Error
                            }
                        };
                        if Self::is_context_affine_type(&ty) {
                            self.error(
                                RuleCode::S100,
                                "Worker, Inbox, and Outbox values may not be static fields",
                                self.pos(key.span),
                            );
                        }
                        let signature = GlobalSig {
                            ty,
                            mutable: !prop.readonly,
                        };
                        let symbol = static_member_symbol(&self.classes[id.0].name, &name);
                        self.global_sigs.insert(symbol, signature.clone());
                        self.class_sigs[id.0].static_fields.insert(name, signature);
                        continue;
                    }
                    let name = key.sym.to_string();
                    if !self.claim_class_member_name(
                        id,
                        &name,
                        ClassMemberDeclaration::Field,
                        false,
                        self.pos(key.span),
                    ) {
                        continue;
                    }
                    let is_defaulted =
                        is_descriptor && prop.is_optional && prop.value.is_some() && !prop.definite;
                    if is_descriptor {
                        match (prop.definite, prop.is_optional, prop.value.is_some()) {
                            (true, false, false) | (false, true, true) | (false, true, false) => {}
                            (_, true, false) => {
                                let pos = self.pos(prop.span);
                                self.error_diverging(
                                    RuleCode::S012,
                                    "optional descriptor members require a default initializer",
                                    pos,
                                    Divergence::OptionalDescriptorMember,
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
                        self.error_diverging(
                            RuleCode::S012,
                            "optional descriptor members require a default initializer",
                            self.pos(prop.span),
                            Divergence::OptionalDescriptorMember,
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
                            } else if method.kind != ast::MethodKind::Method {
                                "descriptor classes cannot declare accessors"
                            } else {
                                "descriptor classes cannot declare methods"
                            },
                            key_pos,
                        );
                        continue;
                    }
                    if method.is_static && (self.in_boundary || self.classes[id.0].is_boundary) {
                        self.error(
                            RuleCode::S100,
                            "mirror classes cannot declare static methods or accessors",
                            key_pos,
                        );
                        continue;
                    }
                    if is_dispose && method.is_static {
                        self.error(
                            RuleCode::S100,
                            "`[Symbol.dispose]()` must be non-static",
                            key_pos,
                        );
                        continue;
                    }
                    if method.is_static && method.function.is_async {
                        self.error_diverging(
                            RuleCode::S100,
                            "async static methods are not in the decided surface",
                            self.pos(method.span),
                            Divergence::AsyncFunctionShape,
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
                    if method.kind != ast::MethodKind::Method && self.in_boundary {
                        let pos = self.pos(method.span);
                        self.error(
                            RuleCode::S100,
                            "mirror classes cannot declare accessors",
                            pos,
                        );
                        continue;
                    }
                    if method.kind == ast::MethodKind::Getter {
                        if !self.claim_class_member_name(
                            id,
                            &name,
                            ClassMemberDeclaration::ReadAccessor,
                            method.is_static,
                            key_pos.clone(),
                        ) {
                            continue;
                        }
                        if !method.function.params.is_empty() {
                            self.error(
                                RuleCode::S100,
                                "a read accessor must declare no parameters",
                                key_pos.clone(),
                            );
                            continue;
                        }
                        let Some(return_type) = &method.function.return_type else {
                            self.error(
                                RuleCode::S100,
                                "a read accessor requires an explicit return type",
                                key_pos,
                            );
                            continue;
                        };
                        let sig = FnSig {
                            params: Vec::new(),
                            ret: self.resolve_type(&return_type.type_ann),
                            is_generator: false,
                            is_async: false,
                            yield_known: true,
                        };
                        if method.is_static {
                            let symbol = static_member_symbol(&self.classes[id.0].name, &name);
                            self.class_sigs[id.0]
                                .static_methods
                                .insert(name, sig.clone());
                            self.fn_sigs.insert(symbol, sig);
                        } else {
                            self.class_sigs[id.0].methods.insert(name, sig);
                        }
                        continue;
                    }
                    if method.kind == ast::MethodKind::Setter {
                        let write_name = format!("{name}=");
                        if is_value && !method.is_static {
                            let class_name = self.classes[id.0].name.clone();
                            self.error_diverging(
                                RuleCode::S100,
                                format!(
                                    "value class `{class_name}` cannot declare a write accessor"
                                ),
                                key_pos,
                                Divergence::NamedAccessor,
                            );
                            continue;
                        }
                        if !self.claim_class_member_name(
                            id,
                            &name,
                            ClassMemberDeclaration::WriteAccessor,
                            method.is_static,
                            key_pos.clone(),
                        ) {
                            continue;
                        }
                        if method.function.return_type.is_some() {
                            self.error(
                                RuleCode::S100,
                                "a write accessor cannot declare a return type",
                                key_pos,
                            );
                            continue;
                        }
                        let [parameter] = method.function.params.as_slice() else {
                            self.error(
                                RuleCode::S100,
                                "a write accessor must declare exactly one parameter",
                                key_pos.clone(),
                            );
                            continue;
                        };
                        let binding = match &parameter.pat {
                            ast::Pat::Ident(binding) => binding,
                            ast::Pat::Assign(_) => {
                                self.error(
                                    RuleCode::S100,
                                    "a write accessor parameter cannot have a default",
                                    key_pos.clone(),
                                );
                                continue;
                            }
                            _ => {
                                self.error(
                                    RuleCode::S100,
                                    "a write accessor parameter must be an identifier",
                                    key_pos.clone(),
                                );
                                continue;
                            }
                        };
                        let Some(annotation) = &binding.type_ann else {
                            self.error(
                                RuleCode::S100,
                                "a write accessor parameter requires a type annotation",
                                key_pos.clone(),
                            );
                            continue;
                        };
                        let sig = FnSig {
                            params: vec![ParamSig {
                                name: binding.id.sym.to_string(),
                                ty: self.resolve_type(&annotation.type_ann),
                                has_default: false,
                            }],
                            ret: Type::Void,
                            is_generator: false,
                            is_async: false,
                            yield_known: true,
                        };
                        write_accessors.push((name.clone(), key_pos, method.is_static));
                        if method.is_static {
                            let symbol =
                                static_member_symbol(&self.classes[id.0].name, &write_name);
                            self.class_sigs[id.0]
                                .static_methods
                                .insert(write_name, sig.clone());
                            self.fn_sigs.insert(symbol, sig);
                        } else {
                            self.class_sigs[id.0].methods.insert(write_name, sig);
                        }
                        continue;
                    }
                    if !self.claim_class_member_name(
                        id,
                        &name,
                        ClassMemberDeclaration::Method,
                        method.is_static,
                        key_pos.clone(),
                    ) {
                        continue;
                    }
                    if method.function.is_generator && !method.is_static {
                        let pos = self.pos(method.span);
                        if method.function.is_async {
                            self.error_diverging(
                                RuleCode::S100,
                                "async generator methods are not in the decided surface",
                                pos,
                                Divergence::AsyncFunctionShape,
                            );
                        } else {
                            self.error(
                                RuleCode::S100,
                                "generator methods are not in the decided surface",
                                pos,
                            );
                        }
                        continue;
                    }
                    // §82.4 rules 1 and 5: a method with type parameters
                    // collects as a template. Each call instantiates it.
                    if !(is_dispose || self.in_boundary || self.classes[id.0].is_boundary)
                        && method.function.type_params.is_some()
                    {
                        if method.function.is_async {
                            self.error_diverging(
                                RuleCode::S100,
                                "async generic methods are not in the decided surface",
                                self.pos(method.span),
                                Divergence::AsyncGenericMethod,
                            );
                            continue;
                        }
                        let bodiless = method.function.body.is_none();
                        if bodiless && declared {
                            self.error_diverging(
                                RuleCode::S100,
                                "function bodies are required",
                                key_pos.clone(),
                                Divergence::BodilessDeclareGenericMethod,
                            );
                        } else if bodiless {
                            self.error(
                                RuleCode::S100,
                                "function bodies are required",
                                key_pos.clone(),
                            );
                        }
                        let (type_params, duplicate_type_parameter) = method
                            .function
                            .type_params
                            .as_deref()
                            .map(|declaration| self.collect_type_parameter_names(declaration))
                            .unwrap_or_default();
                        let template = GenericMethod {
                            file: self.cur_file,
                            type_params,
                            function: (*method.function).clone(),
                            rejected: bodiless || duplicate_type_parameter,
                        };
                        if method.is_static {
                            self.class_sigs[id.0]
                                .static_generic_methods
                                .insert(name, template);
                        } else {
                            self.class_sigs[id.0].generic_methods.insert(name, template);
                        }
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
                    if method.function.is_async && is_value && !method.is_static {
                        let pos = self.pos(method.span);
                        self.error_diverging(
                            RuleCode::S100,
                            "async methods on `@CStruct` value classes are not in the decided surface",
                            pos,
                            Divergence::AsyncFunctionShape,
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
                    if method.is_static {
                        let symbol = static_member_symbol(&self.classes[id.0].name, &name);
                        self.class_sigs[id.0]
                            .static_methods
                            .insert(name, sig.clone());
                        self.fn_sigs.insert(symbol, sig);
                    } else {
                        self.class_sigs[id.0].methods.insert(name, sig);
                    }
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
        for (name, pos, is_static) in write_accessors {
            let has_read = if is_static {
                self.class_sigs[id.0].has_static_read_accessor(&name)
            } else {
                self.class_sigs[id.0].has_read_accessor(&name)
            };
            if !has_read {
                self.error(
                    RuleCode::S100,
                    format!(
                        "{}write accessor `{name}` requires a read accessor with the same name",
                        if is_static { "static " } else { "" }
                    ),
                    pos,
                );
                continue;
            }
            let methods = if is_static {
                &self.class_sigs[id.0].static_methods
            } else {
                &self.class_sigs[id.0].methods
            };
            let read_type = methods.get(&name).map(|signature| signature.ret.clone());
            let write_type = methods
                .get(&format!("{name}="))
                .and_then(|signature| signature.params.first())
                .map(|parameter| parameter.ty.clone());
            if let (Some(read_type), Some(write_type)) = (read_type, write_type) {
                if read_type != write_type {
                    self.error(
                        RuleCode::S100,
                        format!("the read and write accessors of `{name}` must have the same type"),
                        pos,
                    );
                }
            }
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
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "the index signature requires `get(index: {index}): {element}` with exactly matching types"
                ),
                pos.clone(),
                Divergence::ClassIndexSignature,
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

    pub(crate) fn plain_value_leaf(&self, ty: &Type) -> bool {
        matches!(
            ty,
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
                | Type::Error
        )
    }

    fn value_field_ok(&self, ty: &Type) -> bool {
        if self.plain_value_leaf(ty) {
            return true;
        }
        match ty {
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
            let Some(decl) = module_decl(item) else {
                continue;
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
            let owner = fx.enter_synthetic_owner();
            let checked = self.check_expr(value, Some(&field_ty), &mut fx);
            self.require_assignable(
                &checked.ty.clone(),
                &field_ty,
                checked.pos.clone(),
                "the descriptor member default",
            );
            let checked = self.close_synthetic_expression(checked, &mut fx, owner);
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
                let function =
                    self.check_function(&f.function, &name, exported, &sig, (None, None), pos);
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
                if v.kind == ast::VarDeclKind::Var {
                    let pos = self.pos(v.span);
                    self.error(
                        RuleCode::S100,
                        "`var` is not in the language; use `let` or `const`",
                        pos,
                    );
                    return;
                }
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
                            let owner = fx.enter_synthetic_owner();
                            let e = self.check_expr(init, Some(&sig.ty), &mut fx);
                            self.require_assignable(
                                &e.ty.clone(),
                                &sig.ty,
                                e.pos.clone(),
                                "the initializer",
                            );
                            self.close_synthetic_expression(e, &mut fx, owner)
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
                        initializer_index: self.top_level.len(),
                        pos,
                    });
                }
            }
            _ => {}
        }
    }

    fn make_disposal_statements(
        scopes: &[Vec<UsingBinding>],
        first_scope: usize,
    ) -> Vec<hir::Stmt> {
        let mut calls = Vec::new();
        for scope in scopes[first_scope..].iter().rev() {
            for binding in scope.iter().rev() {
                let call = hir::Stmt::Expr(hir::Expr {
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
                });
                if let Some(active) = &binding.active {
                    calls.push(hir::Stmt::If {
                        cond: hir::Expr {
                            kind: hir::ExprKind::Local(active.clone()),
                            ty: Type::Bool,
                            pos: binding.pos.clone(),
                        },
                        then: vec![call],
                        els: None,
                        pos: binding.pos.clone(),
                    });
                } else {
                    calls.push(call);
                }
            }
        }
        calls
    }

    fn insert_scope_exit_disposals(
        &mut self,
        statements: Vec<hir::Stmt>,
        ret: &Type,
        scopes: &mut Vec<Vec<UsingBinding>>,
        control_scopes: (Option<usize>, Option<usize>),
        scope_mode: (bool, &[SwitchUsingStorage]),
    ) -> Vec<hir::Stmt> {
        let (break_scope, continue_scope) = control_scopes;
        let (open_scope, switch_storage) = scope_mode;
        if open_scope {
            scopes.push(Vec::new());
        }
        let mut rewritten = Vec::new();
        for statement in statements {
            match statement {
                hir::Stmt::Let {
                    name,
                    ty,
                    mutable,
                    dispose,
                    init,
                    pos,
                } => {
                    rewritten.push(hir::Stmt::Let {
                        name: name.clone(),
                        ty: ty.clone(),
                        mutable,
                        dispose,
                        init,
                        pos: pos.clone(),
                    });
                    if dispose {
                        let storage = switch_storage.iter().find(|storage| storage.source == name);
                        let dispose_name = storage
                            .map(|storage| storage.storage.clone())
                            .unwrap_or_else(|| name.clone());
                        let active = storage.map(|storage| storage.active.clone());
                        scopes
                            .last_mut()
                            .expect("using scope exists")
                            .push(UsingBinding {
                                name: dispose_name,
                                ty: ty.clone(),
                                pos: pos.clone(),
                                active,
                            });
                        if let Some(storage) = storage {
                            rewritten.push(hir::Stmt::Expr(hir::Expr {
                                kind: hir::ExprKind::Assign {
                                    op: None,
                                    target: Box::new(hir::Expr {
                                        kind: hir::ExprKind::Local(storage.storage.clone()),
                                        ty: ty.clone(),
                                        pos: pos.clone(),
                                    }),
                                    value: Box::new(hir::Expr {
                                        kind: hir::ExprKind::Local(name),
                                        ty,
                                        pos: pos.clone(),
                                    }),
                                },
                                ty: storage.ty.clone(),
                                pos: pos.clone(),
                            }));
                            rewritten.push(hir::Stmt::Expr(hir::Expr {
                                kind: hir::ExprKind::Assign {
                                    op: None,
                                    target: Box::new(hir::Expr {
                                        kind: hir::ExprKind::Local(storage.active.clone()),
                                        ty: Type::Bool,
                                        pos: pos.clone(),
                                    }),
                                    value: Box::new(hir::Expr {
                                        kind: hir::ExprKind::Bool(true),
                                        ty: Type::Bool,
                                        pos: pos.clone(),
                                    }),
                                },
                                ty: Type::Bool,
                                pos,
                            }));
                        }
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
                                dispose: false,
                                init: value,
                                pos: pos.clone(),
                            });
                            rewritten.extend(Self::make_disposal_statements(scopes, 0));
                            rewritten.push(hir::Stmt::Return {
                                value: Some(hir::Expr {
                                    kind: hir::ExprKind::Local(name),
                                    ty: ret.clone(),
                                    pos: pos.clone(),
                                }),
                                pos,
                            });
                        } else {
                            rewritten.extend(Self::make_disposal_statements(scopes, 0));
                            rewritten.push(hir::Stmt::Return {
                                value: Some(value),
                                pos,
                            });
                        }
                    } else {
                        rewritten.extend(Self::make_disposal_statements(scopes, 0));
                        rewritten.push(hir::Stmt::Return { value: None, pos });
                    }
                }
                hir::Stmt::Break(pos) => {
                    if let Some(first_scope) = break_scope {
                        rewritten.extend(Self::make_disposal_statements(scopes, first_scope));
                    }
                    rewritten.push(hir::Stmt::Break(pos));
                }
                hir::Stmt::Continue(pos) => {
                    if let Some(first_scope) = continue_scope {
                        rewritten.extend(Self::make_disposal_statements(scopes, first_scope));
                    }
                    rewritten.push(hir::Stmt::Continue(pos));
                }
                hir::Stmt::Block(body) => {
                    let body = self.insert_scope_exit_disposals(
                        body,
                        ret,
                        scopes,
                        (break_scope, continue_scope),
                        (true, &[]),
                    );
                    rewritten.push(hir::Stmt::Block(body));
                }
                hir::Stmt::If {
                    cond,
                    then,
                    els,
                    pos,
                } => {
                    let then = self.insert_scope_exit_disposals(
                        then,
                        ret,
                        scopes,
                        (break_scope, continue_scope),
                        (true, &[]),
                    );
                    let els = els.map(|body| {
                        self.insert_scope_exit_disposals(
                            body,
                            ret,
                            scopes,
                            (break_scope, continue_scope),
                            (true, &[]),
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
                    let body = self.insert_scope_exit_disposals(
                        body,
                        ret,
                        scopes,
                        (Some(loop_scope), Some(loop_scope)),
                        (true, &[]),
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
                    let body = self.insert_scope_exit_disposals(
                        body,
                        ret,
                        scopes,
                        (Some(loop_scope), Some(loop_scope)),
                        (true, &[]),
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
                    let body = self.insert_scope_exit_disposals(
                        body,
                        ret,
                        scopes,
                        (Some(loop_scope), Some(loop_scope)),
                        (true, &[]),
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
                    let mut switch_bindings = Vec::new();
                    for case in &cases {
                        for statement in &case.body {
                            let hir::Stmt::Let {
                                name,
                                ty,
                                dispose: true,
                                pos,
                                ..
                            } = statement
                            else {
                                continue;
                            };
                            let id = self.next_using_switch_id;
                            self.next_using_switch_id += 1;
                            let active = format!("[[using.active#{id}]]");
                            let storage = format!("[[using.value#{id}]]");
                            rewritten.push(hir::Stmt::Let {
                                name: active.clone(),
                                ty: Type::Bool,
                                mutable: true,
                                dispose: false,
                                init: hir::Expr {
                                    kind: hir::ExprKind::Bool(false),
                                    ty: Type::Bool,
                                    pos: pos.clone(),
                                },
                                pos: pos.clone(),
                            });
                            rewritten.push(hir::Stmt::Let {
                                name: storage.clone(),
                                ty: ty.clone(),
                                mutable: true,
                                dispose: false,
                                init: hir::Expr {
                                    kind: hir::ExprKind::Null,
                                    ty: ty.clone(),
                                    pos: pos.clone(),
                                },
                                pos: pos.clone(),
                            });
                            switch_bindings.push(SwitchUsingStorage {
                                source: name.clone(),
                                active,
                                storage,
                                ty: ty.clone(),
                            });
                        }
                    }
                    scopes.push(Vec::new());
                    let mut cases = cases
                        .into_iter()
                        .map(|case| hir::SwitchCase {
                            test: case.test,
                            body: self.insert_scope_exit_disposals(
                                case.body,
                                ret,
                                scopes,
                                (Some(switch_scope), continue_scope),
                                (false, &switch_bindings),
                            ),
                            pos: case.pos,
                        })
                        .collect::<Vec<_>>();
                    let scope = scopes.pop().unwrap_or_default();
                    if let Some(last_case) = cases.last_mut() {
                        last_case.body.extend(Self::make_disposal_statements(
                            std::slice::from_ref(&scope),
                            0,
                        ));
                    }
                    rewritten.push(hir::Stmt::Switch { disc, cases, pos });
                }
                other => rewritten.push(other),
            }
        }
        if open_scope {
            let scope = scopes.pop().unwrap_or_default();
            if !scope.is_empty() {
                rewritten.extend(Self::make_disposal_statements(
                    std::slice::from_ref(&scope),
                    0,
                ));
            }
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
        this: (Option<Type>, Option<Divergence>),
        pos: Pos,
    ) -> Option<hir::Function> {
        let (this_ty, missing_this_divergence) = this;
        let mut fx = FnCtx::new(sig.ret.clone(), sig.is_generator, this_ty);
        fx.frames[0].missing_this_divergence = missing_this_divergence;
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
                self.reserve_block_declarations(&block.stmts, &mut fx);
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
        let unhandled = fx
            .async_origins
            .iter()
            .filter(|(_, handled)| !*handled)
            .map(|(pos, _)| pos.clone())
            .collect::<Vec<_>>();
        for origin in unhandled {
            self.error_diverging(
                RuleCode::S013,
                "an async handle is dropped without any await of its completion",
                origin,
                Divergence::DroppedAsyncHandle,
            );
        }
        let body = if has_dispose_binding(&body) {
            self.insert_scope_exit_disposals(
                body,
                &sig.ret,
                &mut Vec::new(),
                (None, None),
                (true, &[]),
            )
        } else {
            body
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
                    let owner = fx.enter_synthetic_owner();
                    let e = self.check_expr(&a.right, Some(&ps.ty), fx);
                    self.require_assignable(
                        &e.ty.clone(),
                        &ps.ty,
                        e.pos.clone(),
                        "the default value",
                    );
                    Some(self.close_synthetic_expression(e, fx, owner))
                }
                _ => None,
            };
            self.declare_local(
                &ps.name,
                Local {
                    ty: ps.ty.clone(),
                    mutable: true,
                    holds_capturing: false,
                    async_origins: if ps.ty.carries_async_handle() {
                        HashSet::from([fx.register_async_origin(pos.clone())])
                    } else {
                        HashSet::new()
                    },
                },
                pos.clone(),
                fx,
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
        let mut checked_read_accessors = HashSet::new();
        let mut checked_write_accessors = HashSet::new();
        for member in &class.body {
            match member {
                ast::ClassMember::ClassProp(prop) => {
                    let ast::PropName::Ident(key) = &prop.key else {
                        continue;
                    };
                    if prop.is_static {
                        let name = key.sym.to_string();
                        let Some(signature) =
                            self.class_sigs[id.0].static_fields.get(&name).cloned()
                        else {
                            continue;
                        };
                        let pos = self.pos(key.span);
                        let mut fx = FnCtx::new(Type::Void, false, None);
                        fx.frames[0].missing_this_divergence =
                            Some(Divergence::StaticMemberSurface);
                        let init = match &prop.value {
                            Some(value) => {
                                let owner = fx.enter_synthetic_owner();
                                let expression =
                                    self.check_expr(value, Some(&signature.ty), &mut fx);
                                self.require_assignable(
                                    &expression.ty.clone(),
                                    &signature.ty,
                                    expression.pos.clone(),
                                    "the static field initializer",
                                );
                                self.close_synthetic_expression(expression, &mut fx, owner)
                            }
                            None => {
                                self.error(
                                    RuleCode::S100,
                                    "static fields require an initializer",
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
                            name: static_member_symbol(&self.classes[id.0].name, &name),
                            ty: signature.ty,
                            mutable: signature.mutable,
                            init,
                            initializer_index: self.top_level.len(),
                            pos,
                        });
                        continue;
                    }
                    let Some(value) = &prop.value else { continue };
                    let field_ty = self.classes[id.0]
                        .fields
                        .iter()
                        .find(|f| f.name == key.sym.as_ref())
                        .map(|f| f.ty.clone());
                    let Some(field_ty) = field_ty else { continue };
                    let mut fx = FnCtx::new(Type::Void, false, None);
                    fx.frames[0].missing_this_divergence = Some(Divergence::ThisInFieldInitializer);
                    let owner = fx.enter_synthetic_owner();
                    let e = self.check_expr(value, Some(&field_ty), &mut fx);
                    self.require_assignable(
                        &e.ty.clone(),
                        &field_ty,
                        e.pos.clone(),
                        "the field initializer",
                    );
                    let e = self.close_synthetic_expression(e, &mut fx, owner);
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
                                let owner = fx.enter_synthetic_owner();
                                let e = self.check_expr(&a.right, Some(&ps.ty), &mut fx);
                                Some(self.close_synthetic_expression(e, &mut fx, owner))
                            }
                            _ => None,
                        };
                        let param_pos = self.pos(param.span);
                        self.declare_local(
                            &ps.name,
                            Local {
                                ty: ps.ty.clone(),
                                mutable: true,
                                holds_capturing: false,
                                async_origins: HashSet::new(),
                            },
                            param_pos.clone(),
                            &mut fx,
                        );
                        hir_params.push(hir::Param {
                            name: ps.name.clone(),
                            ty: ps.ty.clone(),
                            default,
                            foreign_provenance: None,
                            pos: param_pos,
                        });
                    }
                    let mut body = Vec::new();
                    if let Some(block) = &ctor.body {
                        self.reserve_block_declarations(&block.stmts, &mut fx);
                        for s in &block.stmts {
                            self.check_stmt(s, &mut fx, &mut body);
                        }
                    }
                    if has_dispose_binding(&body) {
                        body = self.insert_scope_exit_disposals(
                            body,
                            &Type::Void,
                            &mut Vec::new(),
                            (None, None),
                            (true, &[]),
                        );
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
                    let (mut name, pos) = match &method.key {
                        ast::PropName::Ident(key) => (key.sym.to_string(), self.pos(key.span)),
                        key if is_dispose_method_key(key) => {
                            (hir::DISPOSE_METHOD_NAME.to_string(), self.pos(method.span))
                        }
                        _ => continue,
                    };
                    match method.kind {
                        ast::MethodKind::Getter => {
                            let has_accessor = if method.is_static {
                                self.class_sigs[id.0].has_static_accessor(&name)
                            } else {
                                self.class_sigs[id.0].has_accessor(&name)
                            };
                            if !has_accessor
                                || !checked_read_accessors.insert((method.is_static, name.clone()))
                            {
                                continue;
                            }
                        }
                        ast::MethodKind::Setter => {
                            if !checked_write_accessors.insert((method.is_static, name.clone())) {
                                continue;
                            }
                            name.push('=');
                        }
                        ast::MethodKind::Method => {
                            let has_accessor = if method.is_static {
                                self.class_sigs[id.0].has_static_accessor(&name)
                            } else {
                                self.class_sigs[id.0].has_accessor(&name)
                            };
                            if has_accessor {
                                continue;
                            }
                            // §82.4 rule 3: a template has no body of its
                            // own. `instantiate_method` checks each
                            // instance at its first call.
                            if self.class_sigs[id.0].has_generic_method(&name, method.is_static) {
                                continue;
                            }
                        }
                    }
                    let sig = if method.is_static {
                        self.class_sigs[id.0].static_methods.get(&name).cloned()
                    } else {
                        self.class_sigs[id.0].methods.get(&name).cloned()
                    };
                    let Some(sig) = sig else {
                        continue;
                    };
                    let function_name = if method.is_static {
                        static_member_symbol(&self.classes[id.0].name, &name)
                    } else {
                        name.clone()
                    };
                    if let Some(func) = self.check_function(
                        &method.function,
                        &function_name,
                        false,
                        &sig,
                        (
                            (!method.is_static).then(|| this_ty.clone()),
                            method.is_static.then_some(Divergence::StaticMemberSurface),
                        ),
                        pos,
                    ) {
                        if method.is_static {
                            self.functions.push(func);
                        } else {
                            self.classes[id.0].methods.push(func);
                        }
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
        if template.rejected {
            return None;
        }
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
        if let Some(function) =
            self.check_function(&template.function, &name, false, &sig, (None, None), pos)
        {
            self.functions.push(function);
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        Some(name)
    }

    /// Instantiates a generic method at explicit type arguments and
    /// checks its body immediately (§82.4 rule 3). Returns the instance
    /// name, which is the monomorphized name `m<A>`.
    ///
    /// The instance is an ordinary method of the class in the namespace
    /// that `is_static` selects. Every consumer of a method name sees the
    /// instance name; no template reaches the HIR.
    pub(crate) fn instantiate_method(
        &mut self,
        id: ClassId,
        name: &str,
        args: &[Type],
        is_static: bool,
        pos: Pos,
    ) -> Option<String> {
        let template = if is_static {
            self.class_sigs[id.0]
                .static_generic_methods
                .get(name)?
                .clone()
        } else {
            self.class_sigs[id.0].generic_methods.get(name)?.clone()
        };
        if template.rejected {
            return None;
        }
        if template.type_params.len() != args.len() {
            self.error(
                RuleCode::S100,
                format!(
                    "`{}` expects {} type argument(s), got {}",
                    name,
                    template.type_params.len(),
                    args.len()
                ),
                pos,
            );
            return None;
        }
        let instance = self.mono_name(name, args);
        let known = if is_static {
            self.class_sigs[id.0].static_methods.contains_key(&instance)
        } else {
            self.class_sigs[id.0].methods.contains_key(&instance)
        };
        if known {
            return Some(instance);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let sig = self.resolve_fn_sig(&template.function, pos.clone());
        // The signature lands before the body check, so a recursive call
        // inside the body resolves against this instance.
        let function_name = if is_static {
            let symbol = static_member_symbol(&self.classes[id.0].name, &instance);
            self.class_sigs[id.0]
                .static_methods
                .insert(instance.clone(), sig.clone());
            self.fn_sigs.insert(symbol.clone(), sig.clone());
            symbol
        } else {
            self.class_sigs[id.0]
                .methods
                .insert(instance.clone(), sig.clone());
            instance.clone()
        };
        if let Some(function) = self.check_function(
            &template.function,
            &function_name,
            false,
            &sig,
            (
                (!is_static).then_some(Type::Class(id)),
                is_static.then_some(Divergence::StaticMemberSurface),
            ),
            pos,
        ) {
            if is_static {
                self.functions.push(function);
            } else {
                self.classes[id.0].methods.push(function);
            }
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        Some(instance)
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
        if template.has_static_member || template.has_generic_method {
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
            template.alignment_override,
            template.pos.clone(),
        );
        self.resolve_class_shape(id, &template.class, false);
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
        self.lookup_local_access(name, pos, fx, true)
    }

    /// Looks up a local assignment target without a read-before-declaration check.
    pub(crate) fn lookup_local_for_write(
        &mut self,
        name: &str,
        pos: &Pos,
        fx: &mut FnCtx,
    ) -> Option<Local> {
        self.lookup_local_access(name, pos, fx, false)
    }

    fn lookup_local_access(
        &mut self,
        name: &str,
        pos: &Pos,
        fx: &mut FnCtx,
        for_read: bool,
    ) -> Option<Local> {
        let mut crossed = 0usize;
        let mut found: Option<(usize, Local)> = None;
        for scope in fx.scopes.iter().rev() {
            let owns_name = scope.vars.contains_key(name)
                || scope.pending.contains(name)
                || scope.switch_declarations.contains_key(name);
            let scope_name = if scope.is_switch {
                "this switch body"
            } else {
                "this block"
            };
            if owns_name && !scope.duplicate_declarations.contains(name) {
                if let (Some(declaration_case), Some(current_case)) =
                    (scope.switch_declarations.get(name), scope.switch_case)
                {
                    if *declaration_case != current_case {
                        let message = if for_read {
                            format!("`{name}` is read from a different switch case")
                        } else {
                            format!("`{name}` is assigned in a case that does not declare it")
                        };
                        if for_read {
                            self.error(RuleCode::S100, message, pos.clone());
                        } else {
                            self.error_diverging(
                                RuleCode::S100,
                                message,
                                pos.clone(),
                                Divergence::DeclarationScope,
                            );
                        }
                        return Some(Local {
                            ty: Type::Error,
                            mutable: true,
                            holds_capturing: false,
                            async_origins: HashSet::new(),
                        });
                    }
                }
            }
            if owns_name && for_read && scope.pending.contains(name) {
                let message = format!("`{name}` is read before its declaration in {scope_name}");
                let shadows_program_item = matches!(
                    self.scope_item(name),
                    Some(ScopeItem::Class(_) | ScopeItem::GenericClass(_) | ScopeItem::Func(_))
                );
                let ambient_namespace = matches!(
                    name,
                    "Math" | "Date" | "Number" | "JSON" | "Context" | "Promise"
                );
                if shadows_program_item || ambient_namespace {
                    self.error(RuleCode::S100, message, pos.clone());
                } else {
                    self.error_diverging(
                        RuleCode::S100,
                        message,
                        pos.clone(),
                        Divergence::DeclarationScope,
                    );
                }
                return Some(Local {
                    ty: Type::Error,
                    mutable: true,
                    holds_capturing: false,
                    async_origins: HashSet::new(),
                });
            }
            if let Some(local) = scope.vars.get(name) {
                found = Some((crossed, local.clone()));
                break;
            }
            if scope.pending.contains(name) {
                self.error(
                    RuleCode::S100,
                    format!("`{name}` is assigned before its declaration in {scope_name}"),
                    pos.clone(),
                );
                return Some(Local {
                    ty: Type::Error,
                    mutable: true,
                    holds_capturing: false,
                    async_origins: HashSet::new(),
                });
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
        e.flow_leaves().any(|leaf| match &leaf.kind {
            hir::ExprKind::Lambda { captures, .. } => !captures.is_empty(),
            hir::ExprKind::Local(name) => fx
                .scopes
                .iter()
                .rev()
                .find_map(|scope| scope.vars.get(name))
                .is_some_and(|local| local.holds_capturing),
            hir::ExprKind::ArraySpreadLit(elements) => elements.iter().any(|element| {
                element.spread.is_none() && self.is_capturing_value(&element.expr, fx)
            }),
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_program, RuleCode, SourceFile};

    #[test]
    fn synthetic_prefix_owner_boundary_reports_an_escape() {
        super::SYNTHETIC_PREFIX_ESCAPE_HOOK.with(|hook| hook.set(true));
        let diagnostics = check_program(&[SourceFile::new(
            "test.ts",
            "export function main(): void { 1; }\n",
        )])
        .expect_err("the synthetic prefix hook must fail");

        assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(
            diagnostics[0].message,
            "internal: synthetic prefix escaped its owner"
        );
        assert_eq!(diagnostics[0].pos.file, "test.ts");
        assert_eq!(diagnostics[0].pos.line, 1);
    }

    #[test]
    fn reachable_import_of_an_absent_export_uses_s016() {
        let diagnostics = check_program(&[
            SourceFile::new("m.ts", "export const present: i32 = 1;\n"),
            SourceFile::new(
                "main.ts",
                "import { missing } from \"./m\";\nexport function main(): void {}\n",
            ),
        ])
        .expect_err("the absent export must fail");

        assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S016);
        assert_eq!(diagnostics[0].message, "`missing` is not exported by `./m`");
        assert_eq!(diagnostics[0].pos.file, "main.ts");
        assert_eq!(diagnostics[0].pos.line, 1);
    }
}
