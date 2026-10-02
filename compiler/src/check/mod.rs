//! The semantic checker: enforces the collision rules (C1–C8) and the
//! Q-register resolutions, and produces the typed HIR.
//!
//! Structure: pass A collects top-level names per file and resolves
//! imports; pass B resolves declared signatures and class shapes; pass C
//! checks bodies in source order and builds the HIR. Generic
//! declarations are registered as templates in pass A/B and
//! monomorphized on first use (`identity<i32>`, `Box<f64>`).

mod host_entries;
mod identity;
mod init_effects;
mod init_order;
pub(crate) use crate::hir::source_name;
pub(crate) use container_argument::ContainerSlot;
use init_effects::module_initializer_diagnostics;
mod bindings;
mod bodies;
#[cfg(test)]
mod body_check_cost;
mod capture;
mod class_shape;
mod container_argument;
mod declarations;
pub(crate) mod exception;
mod exports;
mod expr;
pub(crate) mod fallthrough;
mod field_initializer;
mod generics;
mod instance_chain;
use instance_chain::InstanceArguments;
mod json;
mod layout;
mod lookup;
mod mirror_provenance;
mod narrowing;
mod opaque;
mod pipeline;
pub(crate) use pipeline::run;
pub(crate) mod pattern;
mod signatures;
mod stmt;
mod type_rules;
mod tyres;
mod using_scope;

#[cfg(test)]
pub(crate) use expr::{take_classified_places, PlaceKind};

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

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

/// Returns the object-literal mapping from the one wire-mapped alias form
/// (§50.1).
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
/// unary sign before returning its mathematical value (§56.1).
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
/// spellings are read exactly; a synthesized node goes through the f64
/// path.
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

/// The value of a mirror variable declarator in the one accepted form,
/// `declare const X = <integer literal>;` (compiler.md §136.1 rule 1).
/// Every other form gives `None`.
fn mirror_const_value(v: &ast::VarDecl, d: &ast::VarDeclarator) -> Option<i64> {
    let ast::Pat::Ident(binding) = &d.name else {
        return None;
    };
    if !v.declare || v.kind != ast::VarDeclKind::Const || binding.type_ann.is_some() {
        return None;
    }
    d.init.as_deref().and_then(int_literal_value)
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

fn static_member_symbol(id: ClassId, class: &str, member: &str) -> String {
    format!("[[identity:class:{}]]{class}.{member}", id.0)
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

impl GenericMethod {
    fn rejected(file: usize, function: &ast::Function) -> Option<Self> {
        let declaration = function.type_params.as_ref()?;
        Some(Self {
            file,
            type_params: declaration
                .params
                .iter()
                .map(|parameter| parameter.name.sym.to_string())
                .collect(),
            function: function.clone(),
            rejected: true,
        })
    }
}

/// A generic class template awaiting monomorphization.
#[derive(Debug, Clone)]
pub(crate) struct GenericClass {
    pub file: usize,
    pub is_value: bool,
    pub is_descriptor: bool,
    /// The template's `declare` keyword. compiler.md §108.1 rule 3: an
    /// instance inherits the template's ambient status.
    pub declared: bool,
    pub alignment_override: Option<hir::AlignmentOverride>,
    pub type_params: Vec<String>,
    pub has_static_member: bool,
    /// Rejected method templates, partitioned by the static namespace flag.
    pub rejected_generic_methods: HashMap<(Option<String>, bool), GenericMethod>,
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
    TypeAlias(Type),
    Global(String),
    /// A foreign C-ABI function declared by an ambient mirror (§12.2);
    /// callable but not usable as a value.
    Foreign(String),
}

/// A top-level scope entry retains its import status (compiler.md §127)
/// and whether a type-only import bound it (compiler.md §134).
#[derive(Debug, Clone)]
pub(crate) struct ScopeBinding {
    pub item: ScopeItem,
    pub imported: bool,
    /// The binding names a type only; a value use of it is S100.
    pub type_only: bool,
}

/// A local binding inside a function body.
#[derive(Debug, Clone)]
pub(crate) struct Local {
    pub ty: Type,
    pub mutable: bool,
    /// Async-handle creation obligations reachable through this value.
    pub async_origins: HashSet<u32>,
    /// True for a catch binding. Before a narrowing test it has two legal
    /// uses: the left operand of `instanceof` and the operand of `throw`
    /// (compiler.md §115.3 rule 4).
    pub caught: bool,
}

/// One lexical scope. `fn_boundary` marks the start of a lambda body:
/// lookups that cross it are captures.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    pub vars: HashMap<String, Local>,
    pub shared_narrowing_paths: HashMap<String, bool>,
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

/// The field name when `statement` is a plain `this.<name> = value`
/// (compiler.md §108.1 rule 2: the assignment that satisfies the rule).
fn this_field_assignment(statement: &hir::Stmt) -> Option<&str> {
    let hir::Stmt::Expr(expression) = statement else {
        return None;
    };
    let hir::ExprKind::Assign {
        op: None, target, ..
    } = &expression.kind
    else {
        return None;
    };
    let hir::ExprKind::Field { obj, name } = &target.kind else {
        return None;
    };
    matches!(obj.kind, hir::ExprKind::This).then_some(name.as_str())
}

/// How one class field is spelled in the source (compiler.md §108.1).
#[derive(Debug, Clone, Default)]
struct FieldSpelling {
    /// The `!` definite-assignment assertion.
    definite: bool,
    /// The `?` optional marker.
    optional: bool,
    /// The declared type as the source writes it, `None` for a field
    /// with no type annotation.
    declared_type: Option<String>,
}

/// True when `statement` holds a `return` at any statement depth below
/// it (compiler.md §108.1 rule 2: such a statement can leave the
/// constructor). A lambda body is a separate function, so a `return`
/// inside one returns from the lambda and the walk stops there.
fn leaves_the_function(statement: &hir::Stmt) -> bool {
    fn child_leaves(node: hir::HirChild<'_>) -> bool {
        match node {
            hir::HirChild::Stmt(hir::Stmt::Return { .. }) => true,
            hir::HirChild::Stmt(statement) => statement.children().into_iter().any(child_leaves),
            hir::HirChild::Expr(expression) => {
                !matches!(expression.kind, hir::ExprKind::Lambda { .. })
                    && expression.children().into_iter().any(child_leaves)
            }
        }
    }

    child_leaves(hir::HirChild::Stmt(statement))
}

/// Collects every field name that a plain `this.<name> = value` writes
/// anywhere below `node`, at any nesting.
fn collect_this_field_assignments(node: hir::HirChild<'_>, names: &mut HashSet<String>) {
    let children = match node {
        hir::HirChild::Stmt(statement) => {
            if let Some(name) = this_field_assignment(statement) {
                names.insert(name.to_string());
            }
            statement.children()
        }
        hir::HirChild::Expr(expression) => {
            if let hir::ExprKind::Assign {
                op: None, target, ..
            } = &expression.kind
            {
                if let hir::ExprKind::Field { obj, name } = &target.kind {
                    if matches!(obj.kind, hir::ExprKind::This) {
                        names.insert(name.clone());
                    }
                }
            }
            expression.children()
        }
    };
    for child in children {
        collect_this_field_assignments(child, names);
    }
}

/// One `this` that compiler.md §108.4 rule 6 rejects.
#[derive(Debug)]
enum PrefixThis {
    /// Site A: a read `this.g` of a field that holds no value.
    Read(String),
    /// Site B: a method or an accessor call on `this`.
    Call,
    /// Site B: `this` as a value.
    Value,
}

/// Collects every `this` that compiler.md §108.4 rule 6 rejects below
/// `node`, at every statement and expression depth. `held` names the
/// fields that hold a value at the statement that carries `node`.
///
/// Two forms are legal. Form (a) is the target of `this.f = …`, so the
/// walk reads the value of such an assignment and leaves the target.
/// Form (b) is a read `this.g` of a field in `held`: an operand, a
/// member write `this.g.x = …`, a compound assignment, an increment, an
/// argument, and the receiver of `this.g.m()` all lower to that shape.
/// A lambda body cannot mention `this`, so the walk finds none there.
fn prefix_this_violations(
    node: hir::HirChild<'_>,
    held: &HashSet<String>,
    out: &mut Vec<(Pos, PrefixThis)>,
) {
    let expression = match node {
        hir::HirChild::Stmt(statement) => {
            for child in statement.children() {
                prefix_this_violations(child, held, out);
            }
            return;
        }
        hir::HirChild::Expr(expression) => expression,
    };
    match &expression.kind {
        hir::ExprKind::Assign {
            op: None,
            target,
            value,
        } if matches!(&target.kind, hir::ExprKind::Field { obj, .. }
            if matches!(obj.kind, hir::ExprKind::This)) =>
        {
            prefix_this_violations(hir::HirChild::Expr(value), held, out);
        }
        hir::ExprKind::Field { obj, name } if matches!(obj.kind, hir::ExprKind::This) => {
            if !held.contains(name.as_str()) {
                out.push((obj.pos.clone(), PrefixThis::Read(name.clone())));
            }
        }
        hir::ExprKind::Call {
            callee: hir::Callee::Method { recv, .. },
            args,
        } if matches!(recv.kind, hir::ExprKind::This) => {
            out.push((recv.pos.clone(), PrefixThis::Call));
            for argument in args {
                prefix_this_violations(hir::HirChild::Expr(argument), held, out);
            }
        }
        hir::ExprKind::This => out.push((expression.pos.clone(), PrefixThis::Value)),
        _ => {
            for child in expression.children() {
                prefix_this_violations(child, held, out);
            }
        }
    }
}

/// One `this` that compiler.md §108.4 rule 6 rejects, with the rule-1
/// fields that hold no value at it.
#[derive(Debug)]
struct PrefixViolation {
    pos: Pos,
    kind: PrefixThis,
    /// The first rule-1 field that holds no value there.
    first_missing: String,
    /// The other rule-1 fields that hold no value there.
    other_missing: Vec<String>,
}

/// Moves every `this` that one statement of the prefix carries into
/// `collected`, beside the fields that hold no value at that statement.
fn record_prefix_violations(
    found: Vec<(Pos, PrefixThis)>,
    rule_one_fields: &[String],
    held: &HashSet<String>,
    collected: &mut Vec<PrefixViolation>,
) {
    if found.is_empty() {
        return;
    }
    let missing: Vec<String> = rule_one_fields
        .iter()
        .filter(|name| !held.contains(name.as_str()))
        .cloned()
        .collect();
    // If every rule-1 field holds a value here, the prefix ended before
    // this statement, and rule 6 does not reach the `this`.
    let Some((first, rest)) = missing.split_first() else {
        return;
    };
    for (pos, kind) in found {
        collected.push(PrefixViolation {
            pos,
            kind,
            first_missing: first.clone(),
            other_missing: rest.to_vec(),
        });
    }
}

/// The parts of a compiler.md §108.4 site B message that agree with the
/// count of the fields the message names.
struct NamedFields {
    /// The fields, as the subject of the clause.
    subject: String,
    /// The verb that agrees with the subject.
    verb: &'static str,
    /// The assignments the first spelling moves the use after.
    assignments: String,
    /// The second spelling, which gives each field an initializer.
    initializers: String,
}

/// Names the fields of a compiler.md §108.4 site B diagnostic. The list
/// is never empty: the prefix ends at the first statement after which
/// every rule-1 field holds a value.
fn field_list(first: &str, rest: &[String]) -> NamedFields {
    if rest.is_empty() {
        return NamedFields {
            subject: format!("field `{first}`"),
            verb: "holds",
            assignments: format!("the assignment of `{first}`"),
            initializers: format!("give `{first}` an initializer"),
        };
    }
    let listed = std::iter::once(first)
        .chain(rest.iter().map(String::as_str))
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    NamedFields {
        subject: format!("fields {listed}"),
        verb: "hold",
        assignments: format!("the assignments of {listed}"),
        initializers: format!("give {listed} initializers"),
    }
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

/// Per-body checking state: scope stack, frames, and the narrowing set of
/// path keys known non-null or present (C7, §43).
#[derive(Debug)]
pub(crate) struct FnCtx {
    pub frames: Vec<Frame>,
    field_initializer: Option<field_initializer::FieldInitializer>,
    pub scopes: Vec<Scope>,
    pub narrowed: HashSet<String>,
    pub ended_shared_narrowing: HashSet<String>,
    pub loop_depth: u32,
    pub switch_depth: u32,
    pub switch_break_facts: Vec<(u32, Vec<HashSet<String>>)>,
    /// Each async handle creation or async-handle parameter in this body.
    pub async_origins: Vec<(Pos, bool)>,
    /// Owner-scoped local declarations required by rewritten expressions.
    synthetic_owners: Vec<SyntheticPrefix>,
    diagnostics: DiagnosticSink,
}

/// Shared order-preserving diagnostic destination for the checker and its body contexts.
#[derive(Clone, Debug, Default)]
pub(crate) struct DiagnosticSink(Rc<RefCell<Vec<Diagnostic>>>);

impl DiagnosticSink {
    fn push(&self, diagnostic: Diagnostic) {
        self.0.borrow_mut().push(diagnostic);
    }

    fn extend(&self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.0.borrow_mut().extend(diagnostics);
    }

    fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    fn len(&self) -> usize {
        self.0.borrow().len()
    }

    fn take(&self) -> Vec<Diagnostic> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

/// The source construct that owns a synthetic prefix and its diagnostic position.
#[derive(Debug)]
enum SyntheticOwnerKind {
    Statement(Pos),
    Declarator(Pos),
    ForInit(Pos),
    ForCond(Pos),
    ForUpdate(Pos),
    ArrowBody(Pos),
    Initializer(Pos),
    SwitchCase(Pos),
}

impl SyntheticOwnerKind {
    fn pos(&self) -> &Pos {
        match self {
            Self::Statement(pos)
            | Self::Declarator(pos)
            | Self::ForInit(pos)
            | Self::ForCond(pos)
            | Self::ForUpdate(pos)
            | Self::ArrowBody(pos)
            | Self::Initializer(pos)
            | Self::SwitchCase(pos) => pos,
        }
    }
}

/// Synthetic declarations awaiting placement in their owner's statement list.
#[derive(Debug, Default)]
#[must_use]
struct SyntheticPrefix(Vec<hir::Stmt>);

impl SyntheticPrefix {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn push(&mut self, statement: hir::Stmt) {
        self.0.push(statement);
    }

    fn into_statements(self) -> Vec<hir::Stmt> {
        self.0
    }
}

impl IntoIterator for SyntheticPrefix {
    type Item = hir::Stmt;
    type IntoIter = std::vec::IntoIter<hir::Stmt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Explicit access to a body's expression for prefix rejection and its original diagnostic position.
trait SyntheticOwnerResult {
    fn expression_mut(&mut self) -> Option<&mut hir::Expr>;
}

impl SyntheticOwnerResult for hir::Expr {
    fn expression_mut(&mut self) -> Option<&mut hir::Expr> {
        Some(self)
    }
}

impl SyntheticOwnerResult for bool {
    fn expression_mut(&mut self) -> Option<&mut hir::Expr> {
        None
    }
}

impl SyntheticOwnerResult for Vec<hir::Stmt> {
    fn expression_mut(&mut self) -> Option<&mut hir::Expr> {
        None
    }
}

#[cfg(test)]
impl<T, E> SyntheticOwnerResult for Result<T, E> {
    fn expression_mut(&mut self) -> Option<&mut hir::Expr> {
        None
    }
}

impl FnCtx {
    pub(crate) fn new(
        ret: Type,
        is_generator: bool,
        this_ty: Option<Type>,
        diagnostics: DiagnosticSink,
    ) -> Self {
        FnCtx {
            field_initializer: None,
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
            ended_shared_narrowing: HashSet::new(),
            loop_depth: 0,
            switch_depth: 0,
            switch_break_facts: Vec::new(),
            async_origins: Vec::new(),
            synthetic_owners: Vec::new(),
            diagnostics,
        }
    }

    /// Runs one owner and returns its body result and prefix, including after an early return.
    ///
    /// Place `Statement` before its statement and `Declarator` before its own `Let`.
    /// Place `ForInit` before the loop, `ForCond` at the loop-body head, and `ForUpdate` in the step.
    /// Place `ArrowBody` at the body head. `Initializer` and `SwitchCase` reject prefixes and return an empty prefix.
    #[must_use = "use the body result and place the returned prefix in its owner's statement list"]
    fn with_synthetic_owner<R: SyntheticOwnerResult>(
        &mut self,
        kind: SyntheticOwnerKind,
        body: impl FnOnce(&mut Self) -> R,
    ) -> (R, SyntheticPrefix) {
        self.synthetic_owners.push(SyntheticPrefix::default());
        let mut result = body(self);
        let pos = result
            .expression_mut()
            .map(|expression| expression.pos.clone())
            .unwrap_or_else(|| kind.pos().clone());
        let rejects_prefix = matches!(
            kind,
            SyntheticOwnerKind::Initializer(_) | SyntheticOwnerKind::SwitchCase(_)
        );
        let mut prefix = self.synthetic_owners.pop().unwrap_or_default();
        if rejects_prefix && !prefix.is_empty() {
            let mut diagnostic = Diagnostic::new(
                RuleCode::S100,
                "a non-place receiver of `??` or `?.` cannot be used in an initializer",
                pos.clone(),
            );
            diagnostic.divergence = Some(Divergence::NonPlaceNullishInitializer);
            self.diagnostics.push(diagnostic);
            prefix = SyntheticPrefix::default();
            if let Some(expression) = result.expression_mut() {
                expression.kind = hir::ExprKind::Null;
                expression.ty = Type::Error;
            }
        }
        (result, prefix)
    }

    fn push_synthetic_prefix(&mut self, statement: hir::Stmt) {
        self.synthetic_owners
            .last_mut()
            .expect("a synthetic prefix must have an owner")
            .push(statement);
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
            let prefix = format!("{name}.");
            self.narrowed
                .retain(|key| key != name && !key.starts_with(&prefix));
            self.ended_shared_narrowing
                .retain(|key| key != name && !key.starts_with(&prefix));
        }
        true
    }

    /// Removes a declaration reservation after the declaration fails.
    pub(crate) fn discard_pending(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.pending.remove(name);
        }
    }
}

/// The checker.
pub(crate) struct Checker<'p> {
    narrowing_analysis: Option<narrowing::Analysis>,
    pub prog: &'p ParsedProgram,
    pub diags: DiagnosticSink,
    pub classes: Vec<hir::ClassDef>,
    pub class_sigs: Vec<ClassSig>,
    pub class_ids: HashMap<String, ClassId>,
    pub enums: Vec<hir::EnumDef>,
    pub string_aliases: Vec<hir::StringAliasDef>,
    pub fn_sigs: HashMap<String, FnSig>,
    pub functions: Vec<hir::Function>,
    pub worker_entries: Vec<hir::WorkerEntry>,
    pub global_sigs: HashMap<String, GlobalSig>,
    pub globals: Vec<hir::Global>,
    pub generic_fns: HashMap<String, GenericFn>,
    pub generic_classes: HashMap<String, GenericClass>,
    pub instance_symbols: HashMap<String, String>,
    pub file_scopes: Vec<HashMap<String, ScopeBinding>>,
    pub exports: Vec<HashMap<String, ScopeItem>>,
    export_definitions: Vec<BTreeMap<String, exports::ExportBinding>>,
    pub top_level: Vec<hir::Stmt>,
    pub poison_missing_modules: HashSet<String>,
    pub poisoned_imports: Vec<hir::PoisonedImport>,
    /// Use sites that already report a value use of a type-only import.
    pub type_only_value_uses: HashSet<(usize, u32, u32)>,
    pub cur_file: usize,
    pub subst: HashMap<String, Type>,
    /// Global ambient names contributed by ingested mirror (`.d.ts`)
    /// files (§12.2): handles, boundary structs, enums, foreign functions,
    /// ambient constants. Consulted after the per-file scope.
    pub ambient_scope: HashMap<String, ScopeBinding>,
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
    /// Class ids that an ambient `declare class` declares, in a mirror or
    /// in a program file. compiler.md §108.4 rule 5 rejects `new` on the
    /// program-file half; §108.1 rule 3 keeps every field outside rule 1.
    pub declared_classes: HashSet<ClassId>,
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
    /// True while resolving a boundary position that accepts a wire-mapped
    /// alias: a foreign-function signature, a boundary-struct member, or a
    /// mirror-class constructor parameter (§50.2, §52.2).
    pub allow_wire_alias_boundary: bool,
    /// True while resolving a `Map`/`Set` key argument. It lets the
    /// resolver preserve otherwise-banned key shapes (`object`,
    /// `T | null`) long enough to emit the Q24-specific S014 diagnostic
    /// instead of an unrelated general type diagnostic.
    pub in_assoc_key: bool,
    /// True while resolving the type arguments of a container
    /// construction whose contextual type is the error type. The
    /// declaration that gives the context reported the failure, so an
    /// affine argument at any depth reports nothing more (§132 rule 2).
    pub in_poisoned_context: bool,
    /// True only while checking the argument expression of
    /// `JSON.stringify`. Like `in_assoc_key`, it preserves a banned
    /// `object` assertion long enough for the Q28 call to issue its
    /// required S014 instead of an unrelated general-type diagnostic.
    pub in_json_argument: bool,
    /// True while the checker checks a `for…of` subject expression. It
    /// stays true for every expression in that subject, a closure body
    /// included. `compiler.md` §103.2 rule 4 owns this behaviour.
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
    /// Monotonic lambda unit identity (compiler.md §137 rule 5b).
    pub next_lambda_id: usize,
    /// Checker-generated module globals for regex-literal source sites.
    ///
    /// Each initializer compiles and allocates one rooted handle; every
    /// evaluation of the literal reads that handle.
    pub regex_literals: HashMap<(String, u32, u32), String>,
    /// Monotonic suffix for collision-free regex-literal global names.
    pub next_regex_literal_id: usize,
    /// Monotonic suffix for switch-body disposal storage.
    pub next_using_switch_id: usize,
    /// The shared Error-family class, declared first (compiler.md §119.1 rule 6).
    pub error_class: ClassId,
    /// This suffix keeps compound-write operand locals unique.
    pub next_compound_local_id: usize,
    /// Monotonic suffix for the storage that holds a binding pattern's
    /// source, which every pattern evaluates one time (§107.2).
    pub next_pattern_id: usize,
    /// The opaque type parameter types of the running opaque check
    /// (compiler.md §135.1 rule 1). Empty outside that check.
    pub opaque_params: HashMap<ClassId, opaque::OpaqueType>,
    /// The generic class instances that the running opaque check made at
    /// an opaque type argument (§143 rule 2a).
    pub opaque_instances: HashSet<ClassId>,
    /// True while the opaque check starts its root instance, whose body
    /// is checked (compiler.md §135.1 rule 1).
    pub opaque_root: bool,
    /// The diagnostic range of each per-instance check of a generic body,
    /// read by the merge of §135.1 rule 3.
    pub instance_diagnostic_ranges: Vec<std::ops::Range<usize>>,
    /// The template key and the type arguments of each generic class
    /// instance, read by the constraint relation of compiler.md §135.1
    /// rule 2b.
    pub instance_arguments: HashMap<ClassId, (String, Vec<Type>)>,
    /// Instance bodies wait until every module signature resolves (§138 rule 1).
    pub signatures_resolved: bool,
    /// Instances with bodies that wait for the signature pass (§138 rule 1).
    pub pending_instance_bodies: Vec<(ClassId, Vec<instance_chain::InstanceRequest>)>,
    /// The active requests for generic instances (compiler.md §140 rule 1).
    pub instance_chain: Vec<instance_chain::InstanceRequest>,
    /// One request site per cycle, including rotations of its templates (§140 acceptance 2).
    pub growing_cycles: Vec<(Vec<Pos>, Pos)>,
    /// Diagnostic indices and argument ranges for S011 priority (§140 acceptance 2).
    pub growth_reports: Vec<(usize, Pos, Pos)>,
    /// The loop narrowing effects of the opaque check's bodies, collected
    /// by the provisional run (compiler.md §135.1 rule 1).
    opaque_loop_effects: narrowing::Analysis,
}

#[cfg(test)]
mod tests {
    use crate::{check_program, RuleCode, SourceFile};

    #[test]
    fn synthetic_owner_returns_its_prefix_after_question_mark() {
        use super::{DiagnosticSink, FnCtx, SyntheticOwnerKind};
        use crate::{hir, types::Type, Pos};

        let diagnostics = DiagnosticSink::default();
        let mut fx = FnCtx::new(Type::Void, false, None, diagnostics.clone());
        let pos = Pos::new("test.ts", 1, 1);
        let statement = hir::Stmt::Let {
            name: "[[early]]".into(),
            ty: Type::Bool,
            mutable: false,
            dispose: false,
            init: hir::Expr {
                kind: hir::ExprKind::Bool(true),
                ty: Type::Bool,
                pos: pos.clone(),
            },
            pos: pos.clone(),
        };
        let entry_depth = fx.synthetic_owners.len();
        let (outer_result, outer_prefix) = fx.with_synthetic_owner(
            SyntheticOwnerKind::Statement(pos.clone()),
            |fx| -> Result<(), ()> {
                fx.push_synthetic_prefix(statement.clone());
                let inner_depth = fx.synthetic_owners.len();
                let (result, prefix) = fx.with_synthetic_owner(
                    SyntheticOwnerKind::Statement(pos.clone()),
                    |fx| -> Result<(), ()> {
                        fx.push_synthetic_prefix(statement.clone());
                        Err(())?;
                        Ok(())
                    },
                );
                assert_eq!(result, Err(()));
                assert_eq!(prefix.0, vec![statement.clone()]);
                assert_eq!(fx.synthetic_owners.len(), inner_depth);
                result?;
                Ok(())
            },
        );
        assert_eq!(outer_result, Err(()));
        assert_eq!(outer_prefix.0, vec![statement]);
        assert_eq!(fx.synthetic_owners.len(), entry_depth);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn reachable_import_of_an_absent_export_uses_s016() {
        let diagnostics = check_program(&[
            SourceFile::new("m.ts", "export const present: i32 = 1;\n"),
            SourceFile::entry(
                "main.ts",
                "import { missing } from \"./m\";\n\
                 export function main(): void { missing; }\n",
            ),
        ])
        .expect_err("the absent export must fail");

        assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S016);
        assert_eq!(diagnostics[0].message, "`missing` is not exported by `./m`");
        assert_eq!(diagnostics[0].pos.file, "main.ts");
        assert_eq!(diagnostics[0].pos.line, 1);
    }

    /// compiler.md §103.1: `new Set<K>(source)` takes each accepted
    /// source kind, and one HIR call carries the source operand.
    #[test]
    fn set_source_construction_accepts_each_source_kind() {
        use crate::hir::{Callee, ExprKind, SetFn};
        use crate::types::Type;

        let module = check_program(&[SourceFile::entry(
            "main.ts",
            "export function main(): void {\n\
               const values: i32[] = [1, 2];\n\
               const fixed: FixedArray<i32, 2> = [1, 2];\n\
               const text: string = \"ab\";\n\
               const a: Set<i32> = new Set<i32>(values);\n\
               const b: Set<i32> = new Set<i32>(fixed);\n\
               const c: Set<i32> = new Set<i32>(a);\n\
               const d: Set<string> = new Set<string>(text);\n\
               print(`${a.size}${b.size}${c.size}${d.size}`);\n\
             }\n",
        )])
        .expect("every accepted source kind checks clean");

        let body = &module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main")
            .body;
        let mut sources = Vec::new();
        for statement in body {
            let crate::hir::Stmt::Let { init, .. } = statement else {
                continue;
            };
            let ExprKind::Call {
                callee: Callee::Set(SetFn::New),
                args,
            } = &init.kind
            else {
                continue;
            };
            sources.push(args.first().map(|argument| argument.ty.clone()));
        }
        assert_eq!(
            sources,
            vec![
                Some(Type::Array(Box::new(Type::I32))),
                Some(Type::FixedArray(Box::new(Type::I32), 2)),
                Some(Type::Set(Box::new(Type::I32))),
                Some(Type::Str),
            ]
        );
    }

    /// compiler.md §103.1 rules 1, 5, and 6: each rejected source
    /// names its own reason.
    #[test]
    fn set_source_construction_rejects_a_map_and_a_generator() {
        for (source, needle) in [
            (
                "const m: Map<i32, string> = new Map<i32, string>();\n\
                 export function main(): void { const s: Set<i32> = new Set<i32>(m); }\n",
                "TS2769",
            ),
            (
                "function* one(): Generator<i32> { yield 1; }\n\
                 export function main(): void { const s: Set<i32> = new Set<i32>(one()); }\n",
                "single-use",
            ),
            (
                "export function main(): void { const s: Set<i32> = new Set<i32>(1); }\n",
                "accepts T[], FixedArray<T, N>, Set<T>, or string",
            ),
        ] {
            let diagnostics = check_program(&[SourceFile::entry("main.ts", source)])
                .expect_err("the rejected source must fail");
            assert_eq!(diagnostics[0].code, RuleCode::S014);
            assert!(
                diagnostics[0].message.contains(needle),
                "diagnostic does not name `{needle}`: {}",
                diagnostics[0].message
            );
        }
    }

    /// compiler.md §103.2: the view rules read the receiver type, so a
    /// user class keeps `keys`, `values`, and `entries` as members.
    #[test]
    fn a_user_receiver_keeps_the_three_view_names_as_members() {
        check_program(&[SourceFile::entry(
            "main.ts",
            "function* one(): Generator<i32> { yield 1; }\n\
             class Bag {\n\
               keys(): Generator<i32> { return one(); }\n\
               values(): Generator<i32> { return one(); }\n\
               entries(): Generator<i32> { return one(); }\n\
             }\n\
             export function main(): void {\n\
               const bag: Bag = new Bag();\n\
               for (const key of bag.keys()) { print(`${key}`); }\n\
               for (const value of bag.values()) { print(`${value}`); }\n\
               for (const entry of bag.entries()) { print(`${entry}`); }\n\
             }\n",
        )])
        .expect("a user receiver keeps the three names");

        // The control: the same three names on a §14.1 container stay
        // subject-only, and `entries()` stays rejected.
        for (source, needle) in [
            (
                "export function main(): void {\n\
                   const values: i32[] = [1];\n\
                   const view = values.keys();\n\
                   print(`${view}`);\n\
                 }\n",
                "direct subject",
            ),
            (
                "export function main(): void {\n\
                   const values: Map<i32, i32> = new Map<i32, i32>();\n\
                   for (const entry of values.entries()) { print(`${entry}`); }\n\
                 }\n",
                "no tuple type",
            ),
            (
                "export function main(): void {\n\
                   const values: FixedArray<i32, 1> = [1];\n\
                   for (const value of values.values()) { print(`${value}`); }\n\
                 }\n",
                "subject-only fused view",
            ),
        ] {
            let diagnostics = check_program(&[SourceFile::entry("main.ts", source)])
                .expect_err("a container receiver keeps the view rules");
            assert!(
                diagnostics[0].message.contains(needle),
                "diagnostic does not name `{needle}`: {}",
                diagnostics[0].message
            );
        }
    }

    /// compiler.md §103.2 rule 4: the `for…of` subject environment does
    /// not depend on the member name. One program under two spellings
    /// gives one outcome.
    #[test]
    fn the_for_of_subject_environment_does_not_depend_on_the_member_name() {
        const PROGRAM: &str = "function* one(): Generator<i32> { yield 1; }\n\
             class Bag {\n\
               MEMBER<T>(): Generator<i32> { return one(); }\n\
             }\n\
             export function main(): void {\n\
               const bag: Bag = new Bag();\n\
               for (const value of bag.MEMBER<object>()) { print(`${value}`); }\n\
             }\n";
        let outcome = |member: &str| -> Vec<String> {
            let source = PROGRAM.replace("MEMBER", member);
            match check_program(&[SourceFile::entry("main.ts", &source)]) {
                Ok(_) => Vec::new(),
                Err(diagnostics) => diagnostics
                    .iter()
                    .map(|diagnostic| format!("{:?} {}", diagnostic.code, diagnostic.message))
                    .collect(),
            }
        };
        let ordinary = outcome("each");
        assert!(
            ordinary.is_empty(),
            "an ordinary member name must check clean: {ordinary:?}"
        );
        assert_eq!(
            outcome("values"),
            ordinary,
            "the subject environment reads the member name"
        );
    }
}

mod text;
