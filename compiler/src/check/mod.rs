//! The semantic checker: enforces the collision rules (C1–C8) and the
//! Q-register resolutions, and produces the typed HIR.
//!
//! Structure: pass A collects top-level names per file and resolves
//! imports; pass B resolves declared signatures and class shapes; pass C
//! checks bodies in source order and builds the HIR. Generic
//! declarations are registered as templates in pass A/B and
//! monomorphized on first use (`identity<i32>`, `Box<f64>`).

mod expr;
mod fallthrough;
mod json;
mod layout;
pub(crate) mod pattern;
mod stmt;
mod tyres;

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
    Global(String),
    /// A foreign C-ABI function declared by an ambient mirror (§12.2);
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
    nullable: bool,
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
    pub scopes: Vec<Scope>,
    pub narrowed: HashSet<String>,
    pub loop_depth: u32,
    pub switch_depth: u32,
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
    pub diags: DiagnosticSink,
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
    pub top_level: Vec<hir::Stmt>,
    pub poison_missing_modules: HashSet<String>,
    pub poisoned_imports: Vec<hir::PoisonedImport>,
    pub cur_file: usize,
    pub subst: HashMap<String, Type>,
    /// Global ambient names contributed by ingested mirror (`.d.ts`)
    /// files (§12.2): handles, boundary structs, enums, foreign functions,
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
    /// Monotonic suffix for the storage that holds a binding pattern's
    /// source, which every pattern evaluates one time (§107.2).
    pub next_pattern_id: usize,
}

fn normalize_operation_parameter_types(
    target: &hir::OperationSignatureTarget,
    parameters: &mut [Type],
) {
    let array_element = parameters.first().and_then(|receiver| match receiver {
        Type::Array(element) => Some((**element).clone()),
        _ => None,
    });
    match target {
        hir::OperationSignatureTarget::BuiltinMethod(hir::BuiltinMethod::ArrayPush) => {
            if let (Some(element), Some(value)) = (array_element, parameters.get_mut(1)) {
                *value = element;
            }
        }
        hir::OperationSignatureTarget::Arr(function) => match function {
            hir::ArrFn::IndexOf
            | hir::ArrFn::LastIndexOf
            | hir::ArrFn::Includes
            | hir::ArrFn::Fill
            | hir::ArrFn::Unshift => {
                if let (Some(element), Some(value)) = (array_element, parameters.get_mut(1)) {
                    *value = element;
                }
            }
            hir::ArrFn::Reduce | hir::ArrFn::ReduceRight => {
                let accumulator = parameters.get(1).and_then(|callback| match callback {
                    Type::Func(signature) => signature.params.first().cloned(),
                    _ => None,
                });
                if let (Some(accumulator), Some(initial)) = (accumulator, parameters.get_mut(2)) {
                    *initial = accumulator;
                }
            }
            _ => {}
        },
        hir::OperationSignatureTarget::Map(function) => {
            let pair = parameters.first().and_then(|receiver| match receiver {
                Type::Map(key, value) => Some(((**key).clone(), (**value).clone())),
                _ => None,
            });
            if let Some((key, value)) = pair {
                if matches!(
                    function,
                    hir::MapFn::Get
                        | hir::MapFn::GetOr
                        | hir::MapFn::Set
                        | hir::MapFn::Has
                        | hir::MapFn::Delete
                ) {
                    if let Some(parameter) = parameters.get_mut(1) {
                        *parameter = key;
                    }
                }
                if matches!(function, hir::MapFn::GetOr | hir::MapFn::Set) {
                    if let Some(parameter) = parameters.get_mut(2) {
                        *parameter = value;
                    }
                }
            }
        }
        hir::OperationSignatureTarget::Set(function) => {
            let key = parameters.first().and_then(|receiver| match receiver {
                Type::Set(key) => Some((**key).clone()),
                _ => None,
            });
            if matches!(
                function,
                hir::SetFn::Add | hir::SetFn::Has | hir::SetFn::Delete
            ) {
                if let (Some(key), Some(parameter)) = (key, parameters.get_mut(1)) {
                    *parameter = key;
                }
            }
        }
        hir::OperationSignatureTarget::Worker(function) => {
            let message = parameters
                .first()
                .and_then(|receiver| match (function, receiver) {
                    (hir::WorkerFn::Post, Type::Worker(input, _)) => Some((**input).clone()),
                    (hir::WorkerFn::OutboxPost, Type::Outbox(message)) => Some((**message).clone()),
                    _ => None,
                });
            if let (Some(message), Some(parameter)) = (message, parameters.get_mut(1)) {
                *parameter = message;
            }
        }
        _ => {}
    }
}

fn operation_signatures(module: &mut hir::Module) -> Vec<hir::OperationSignature> {
    fn visit_child(child: hir::HirChild<'_>, signatures: &mut Vec<hir::OperationSignature>) {
        match child {
            hir::HirChild::Expr(expression) => visit_expr(expression, signatures),
            hir::HirChild::Stmt(statement) => visit_stmt(statement, signatures),
        }
    }

    fn visit_stmt(statement: &hir::Stmt, signatures: &mut Vec<hir::OperationSignature>) {
        for child in statement.children() {
            visit_child(child, signatures);
        }
    }

    fn visit_expr(expression: &hir::Expr, signatures: &mut Vec<hir::OperationSignature>) {
        for child in expression.children() {
            visit_child(child, signatures);
        }
        let hir::ExprKind::Call { callee, args } = &expression.kind else {
            return;
        };
        let Some((target, receiver)) = hir::operation_signature_target(callee) else {
            return;
        };
        let mut parameter_types = receiver
            .into_iter()
            .cloned()
            .chain(args.iter().map(|argument| argument.ty.clone()))
            .collect::<Vec<_>>();
        normalize_operation_parameter_types(&target, &mut parameter_types);
        let signature = hir::OperationSignature {
            target,
            parameter_types,
            return_type: (expression.ty != Type::Void).then(|| expression.ty.clone()),
        };
        if !signatures.contains(&signature) {
            signatures.push(signature);
        }
    }

    fn visit_body(body: &[hir::Stmt], signatures: &mut Vec<hir::OperationSignature>) {
        for statement in body {
            visit_stmt(statement, signatures);
        }
    }

    let mut signatures = Vec::new();
    for owner in module.expression_owners_mut() {
        match owner {
            hir::ExpressionOwnerMut::Expr(expression) => visit_expr(expression, &mut signatures),
            hir::ExpressionOwnerMut::Body(body) => visit_body(body, &mut signatures),
        }
    }
    signatures
}

/// Runs the checker over a parsed program.
pub(crate) fn run(
    prog: &ParsedProgram,
    options: &CheckOptions,
) -> Result<hir::Module, Vec<Diagnostic>> {
    let mut ck = Checker {
        prog,
        diags: DiagnosticSink::default(),
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
        declared_classes: HashSet::new(),
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
        next_pattern_id: 0,
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
            operation_signatures: Vec::new(),
            foreign_fns: ck.foreign_defs,
            foreign_mirrors: ck.foreign_mirrors,
            top_level: ck.top_level,
        };
        module.operation_signatures = operation_signatures(&mut module);
        crate::trap_sites::decide_index_checks(&mut module);
        Ok(module)
    } else {
        Err(ck.diags.take())
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
                // A binding pattern reserves every name it binds, so a
                // read before it names the order, not the binding (C14).
                for binding in pattern::collect_names(&declarator.name) {
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

    /// Reports a rejected binding pattern one time, and binds every name
    /// in the pattern with the error type, so no `unknown name` follows
    /// it (§107.4).
    pub(crate) fn reject_pattern(
        &mut self,
        rejection: &pattern::PatternRejection<'_>,
        fx: &mut FnCtx,
    ) {
        let pos = self.pos(rejection.span);
        self.error_diverging(RuleCode::S100, rejection.message, pos, rejection.divergence);
        self.bind_error_names(&rejection.names, fx);
    }

    /// Binds each name with the error type. A read of one adds no
    /// diagnostic.
    pub(crate) fn bind_error_names(&mut self, names: &[&ast::BindingIdent], fx: &mut FnCtx) {
        for binding in names {
            let name = binding.id.sym.to_string();
            fx.discard_pending(&name);
            fx.declare(
                &name,
                Local {
                    ty: Type::Error,
                    mutable: true,
                    holds_capturing: false,
                    async_origins: HashSet::new(),
                },
            );
        }
    }

    /// True when the source type carries the pattern's reads: an array
    /// pattern reads by index, a field pattern reads by field name
    /// (§107.1).
    fn pattern_source_fits(&mut self, pattern: &pattern::Pattern<'_>, ty: &Type) -> bool {
        if matches!(ty, Type::Error) {
            return false;
        }
        let (fits, shape) = match pattern {
            pattern::Pattern::Array { .. } => (
                matches!(ty, Type::Array(_) | Type::FixedArray(_, _)),
                "an array binding pattern reads a `T[]` or a `FixedArray<T, N>`",
            ),
            pattern::Pattern::Fields { .. } => (
                matches!(ty, Type::Class(_)),
                "a field binding pattern reads a reference or value class",
            ),
            _ => return true,
        };
        if !fits {
            let name = self.type_name(ty);
            let pos = self.pos(pattern.span());
            self.error_diverging(
                RuleCode::S100,
                format!("{shape}; the source is `{name}`"),
                pos,
                Divergence::PatternSourceShape,
            );
        }
        fits
    }

    /// Binds an accepted pattern's names out of `source`, which the
    /// caller already evaluated into storage. Every read is the ordinary
    /// checked element read or field read (§107.2).
    pub(crate) fn bind_pattern_from(
        &mut self,
        pattern: &pattern::Pattern<'_>,
        source: &hir::Expr,
        mutable: bool,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) {
        let bindings = match pattern {
            pattern::Pattern::Array { bindings, .. }
            | pattern::Pattern::Fields { bindings, .. } => bindings,
            _ => return,
        };
        if !self.pattern_source_fits(pattern, &source.ty) {
            let names: Vec<&ast::BindingIdent> =
                bindings.iter().map(|binding| binding.binding).collect();
            self.bind_error_names(&names, fx);
            return;
        }
        for binding in bindings {
            let pos = self.pos(binding.binding.id.span);
            let value = match &binding.source {
                pattern::BindingSource::Element(index) => {
                    let index = hir::Expr {
                        kind: hir::ExprKind::Int(i64::from(*index)),
                        ty: Type::I32,
                        pos: pos.clone(),
                    };
                    self.check_index(source.clone(), index, pos.clone())
                }
                pattern::BindingSource::Field(field) => {
                    self.member_on(source.clone(), field, pos.clone(), false)
                }
            };
            let name = binding.binding.id.sym.to_string();
            let ty = value.ty.clone();
            let holds_capturing = self.is_capturing_value(&value, fx);
            let async_origins = self.expr_async_origins(&value, fx);
            self.declare_local(
                &name,
                Local {
                    ty: ty.clone(),
                    mutable,
                    holds_capturing,
                    async_origins,
                },
                pos.clone(),
                fx,
            );
            let prefix = format!("{name}.");
            fx.narrowed
                .retain(|key| key != &name && !key.starts_with(&prefix));
            out.push(hir::Stmt::Let {
                name,
                ty,
                mutable,
                dispose: false,
                init: value,
                pos,
            });
        }
    }

    /// Evaluates `source` one time into checker-generated storage, then
    /// binds the pattern's names out of it (§107.2).
    pub(crate) fn bind_pattern(
        &mut self,
        pattern: &pattern::Pattern<'_>,
        source: hir::Expr,
        mutable: bool,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) {
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        let name = format!("[[pattern#{id}.source]]");
        let ty = source.ty.clone();
        let pos = source.pos.clone();
        let place = hir::Expr {
            kind: hir::ExprKind::Local(name.clone()),
            ty: ty.clone(),
            pos: pos.clone(),
        };
        out.push(hir::Stmt::Let {
            name,
            ty,
            mutable: false,
            dispose: false,
            init: source,
            pos,
        });
        self.bind_pattern_from(pattern, &place, mutable, fx, out);
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
                    Some(method)
                }
                _ => None,
            });
            let mut rejected_generic_methods: HashMap<(Option<String>, bool), GenericMethod> =
                HashMap::new();
            for method in generic_methods {
                if let Some(template) = GenericMethod::rejected(file, &method.function) {
                    rejected_generic_methods.insert(
                        (Self::class_method_name(&method.key), method.is_static),
                        template,
                    );
                }
                // The static-member rule already reports a static method.
                if !method.is_static {
                    self.error_diverging(
                        RuleCode::S100,
                        "generic classes cannot declare generic methods",
                        self.pos(method.span),
                        Divergence::GenericMethodOnGenericClass,
                    );
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
                    declared: c.declare,
                    alignment_override,
                    type_params,
                    has_static_member,
                    rejected_generic_methods,
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
                self.reject_outer_pattern(file, &d.name);
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

    /// Reports a binding pattern in a declaration outside a function body
    /// one time, and poisons every name in the pattern, so no
    /// `unknown name` follows it (§107.4).
    fn reject_outer_pattern(&mut self, file: usize, pat: &ast::Pat) {
        self.error_diverging(
            RuleCode::S100,
            "a binding pattern binds inside a function body; a declaration outside one binds one name",
            self.pos(pat.span()),
            Divergence::ModuleLevelPattern,
        );
        for binding in pattern::collect_names(pat) {
            let name = binding.id.sym.to_string();
            let pos = self.pos(binding.id.span);
            self.register_scope_item(file, &name, ScopeItem::Poisoned, pos);
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

    /// Collects and validates one `CEnum<{ key: wire }>` alias (§50.1).
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

    // ----- mirror (`.d.ts`) ingestion -----

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
                self.reject_outer_pattern(file, &d.name);
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
                            pos.clone(),
                        );
                        additions.push((local, ScopeItem::Poisoned, pos));
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
            // A binding pattern parameter takes its type from the
            // pattern's own annotation, and binds into checker-generated
            // storage that the entry prologue reads (§107.2). A boundary
            // signature has no body, so it holds no such prologue.
            ast::Pat::Array(_) | ast::Pat::Object(_) if !self.in_boundary => {
                let annotation = match pat {
                    ast::Pat::Array(array) => array.type_ann.as_deref(),
                    ast::Pat::Object(object) => object.type_ann.as_deref(),
                    _ => None,
                };
                let ty = match annotation {
                    Some(annotation) => self.resolve_type(&annotation.type_ann),
                    None => {
                        let pos = self.pos(pat.span());
                        self.error(RuleCode::S100, "parameters require a type annotation", pos);
                        Type::Error
                    }
                };
                let id = self.next_pattern_id;
                self.next_pattern_id += 1;
                ParamSig {
                    name: format!("[[pattern#{id}.parameter]]"),
                    ty,
                    has_default: false,
                }
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

    /// Binds a parameter's pattern names at function entry, in parameter
    /// order (§107.2). Answers with the statements the body starts with.
    pub(super) fn bind_parameter_patterns(
        &mut self,
        params: Vec<(ParamSig, ast::Pat)>,
        fx: &mut FnCtx,
    ) -> Vec<hir::Stmt> {
        let mut prologue = Vec::new();
        for (signature, pat) in params {
            let pat = match &pat {
                ast::Pat::Assign(assign) => assign.left.as_ref(),
                other => other,
            };
            let pattern = pattern::classify(pat);
            match &pattern {
                pattern::Pattern::Rejected(rejection) => self.reject_pattern(rejection, fx),
                _ if pattern.is_destructuring() => {
                    let source = hir::Expr {
                        kind: hir::ExprKind::Local(signature.name.clone()),
                        ty: signature.ty.clone(),
                        pos: self.pos(pattern.span()),
                    };
                    self.bind_pattern_from(&pattern, &source, true, fx, &mut prologue);
                }
                _ => {}
            }
        }
        prologue
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

    /// The statically named member of a method declaration, including a
    /// literal key whose spelling the collection rules reject.
    fn class_method_name(key: &ast::PropName) -> Option<String> {
        if is_dispose_method_key(key) {
            return Some(hir::DISPOSE_METHOD_NAME.to_string());
        }
        match key {
            ast::PropName::Ident(key) => Some(key.sym.to_string()),
            ast::PropName::Str(key) => Some(key.value.to_string()),
            ast::PropName::Num(key) => Some(key.value.to_string()),
            ast::PropName::BigInt(key) => Some(key.value.to_string()),
            ast::PropName::Computed(key) => match &*key.expr {
                ast::Expr::Lit(ast::Lit::Str(key)) => Some(key.value.to_string()),
                ast::Expr::Lit(ast::Lit::Num(key)) => Some(key.value.to_string()),
                _ => None,
            },
        }
    }

    fn resolve_class_method(
        &mut self,
        id: ClassId,
        method: &ast::ClassMethod,
        declared: bool,
        write_accessors: &mut Vec<(String, Pos, bool)>,
    ) {
        let is_value = self.classes[id.0].is_value;
        let is_descriptor = self.classes[id.0].is_descriptor;
        let (name, key_pos, is_dispose) = match &method.key {
            ast::PropName::Ident(key) => (key.sym.to_string(), self.pos(key.span), false),
            key if is_dispose_method_key(key) => (
                hir::DISPOSE_METHOD_NAME.to_string(),
                self.pos(method.span),
                true,
            ),
            _ => {
                let pos = self.pos(method.span);
                self.error(RuleCode::S100, "computed method names are not decided", pos);
                return;
            }
        };
        if is_descriptor {
            if is_dispose {
                self.error_diverging(
                    RuleCode::S100,
                    "descriptor classes cannot declare `[Symbol.dispose]()`",
                    key_pos,
                    Divergence::UsingDeclaration,
                );
                return;
            }
            self.error(
                RuleCode::S100,
                if method.kind != ast::MethodKind::Method {
                    "descriptor classes cannot declare accessors"
                } else {
                    "descriptor classes cannot declare methods"
                },
                key_pos,
            );
            return;
        }
        if method.is_static && (self.in_boundary || self.classes[id.0].is_boundary) {
            self.error(
                RuleCode::S100,
                "mirror classes cannot declare static methods or accessors",
                key_pos,
            );
            return;
        }
        if is_dispose && method.is_static {
            self.error(
                RuleCode::S100,
                "`[Symbol.dispose]()` must be non-static",
                key_pos,
            );
            return;
        }
        if method.is_static && method.function.is_async {
            self.error_diverging(
                RuleCode::S100,
                "async static methods are not in the decided surface",
                self.pos(method.span),
                Divergence::AsyncFunctionShape,
            );
            return;
        }
        if is_dispose && is_value {
            self.error_diverging(
                RuleCode::S100,
                "value classes cannot declare `[Symbol.dispose]()`",
                key_pos,
                Divergence::UsingDeclaration,
            );
            return;
        }
        if method.kind != ast::MethodKind::Method && self.in_boundary {
            let pos = self.pos(method.span);
            self.error(
                RuleCode::S100,
                "mirror classes cannot declare accessors",
                pos,
            );
            return;
        }
        if method.kind == ast::MethodKind::Getter {
            if !self.claim_class_member_name(
                id,
                &name,
                ClassMemberDeclaration::ReadAccessor,
                method.is_static,
                key_pos.clone(),
            ) {
                return;
            }
            if !method.function.params.is_empty() {
                self.error(
                    RuleCode::S100,
                    "a read accessor must declare no parameters",
                    key_pos.clone(),
                );
                return;
            }
            let Some(return_type) = &method.function.return_type else {
                self.error(
                    RuleCode::S100,
                    "a read accessor requires an explicit return type",
                    key_pos,
                );
                return;
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
            return;
        }
        if method.kind == ast::MethodKind::Setter {
            let write_name = format!("{name}=");
            if is_value && !method.is_static {
                let class_name = self.classes[id.0].name.clone();
                self.error_diverging(
                    RuleCode::S100,
                    format!("value class `{class_name}` cannot declare a write accessor"),
                    key_pos,
                    Divergence::NamedAccessor,
                );
                return;
            }
            if !self.claim_class_member_name(
                id,
                &name,
                ClassMemberDeclaration::WriteAccessor,
                method.is_static,
                key_pos.clone(),
            ) {
                return;
            }
            if method.function.return_type.is_some() {
                self.error(
                    RuleCode::S100,
                    "a write accessor cannot declare a return type",
                    key_pos,
                );
                return;
            }
            let [parameter] = method.function.params.as_slice() else {
                self.error(
                    RuleCode::S100,
                    "a write accessor must declare exactly one parameter",
                    key_pos.clone(),
                );
                return;
            };
            let binding = match &parameter.pat {
                ast::Pat::Ident(binding) => binding,
                ast::Pat::Assign(_) => {
                    self.error(
                        RuleCode::S100,
                        "a write accessor parameter cannot have a default",
                        key_pos.clone(),
                    );
                    return;
                }
                _ => {
                    self.error(
                        RuleCode::S100,
                        "a write accessor parameter must be an identifier",
                        key_pos.clone(),
                    );
                    return;
                }
            };
            let Some(annotation) = &binding.type_ann else {
                self.error(
                    RuleCode::S100,
                    "a write accessor parameter requires a type annotation",
                    key_pos.clone(),
                );
                return;
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
                let symbol = static_member_symbol(&self.classes[id.0].name, &write_name);
                self.class_sigs[id.0]
                    .static_methods
                    .insert(write_name, sig.clone());
                self.fn_sigs.insert(symbol, sig);
            } else {
                self.class_sigs[id.0].methods.insert(write_name, sig);
            }
            return;
        }
        if !self.claim_class_member_name(
            id,
            &name,
            ClassMemberDeclaration::Method,
            method.is_static,
            key_pos.clone(),
        ) {
            return;
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
            return;
        }
        if method.function.is_async && is_value && !method.is_static {
            let pos = self.pos(method.span);
            self.error_diverging(
                RuleCode::S100,
                "async methods on `@CStruct` value classes are not in the decided surface",
                pos,
                Divergence::AsyncFunctionShape,
            );
            return;
        }
        // §82.4 rules 1 and 5: a method with type parameters
        // collects as a template. Each call instantiates it.
        if !(is_dispose || self.in_boundary || self.classes[id.0].is_boundary)
            && method.function.type_params.is_some()
        {
            let bodiless = method.function.body.is_none();
            if bodiless && declared && !method.function.is_async {
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
            return;
        }
        if is_dispose && method.function.is_async {
            self.error(
                RuleCode::S100,
                "`[Symbol.dispose]()` must be synchronous",
                key_pos,
            );
            return;
        }
        let sig = self.resolve_fn_sig(&method.function, key_pos.clone());
        if is_dispose && (!sig.params.is_empty() || sig.ret != Type::Void) {
            self.error(
                RuleCode::S100,
                "`[Symbol.dispose]()` takes no parameters and returns `void`",
                key_pos,
            );
            return;
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

    /// Resolves a class's fields and callable signatures (pass B), and
    /// enforces C2 (no inheritance for value classes; field whitelist).
    pub(crate) fn resolve_class_shape(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
        if declared {
            self.declared_classes.insert(id);
        }
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
                    let diagnostics_before = self.diags.len();
                    self.resolve_class_method(id, method, declared, &mut write_accessors);
                    // §93 rule 12: every collection rejection preserves a
                    // template, including exits before normal collection.
                    if self.diags.len() != diagnostics_before {
                        if let Some(template) =
                            GenericMethod::rejected(self.cur_file, &method.function)
                        {
                            if let Some(name) = Self::class_method_name(&method.key) {
                                let templates = if method.is_static {
                                    &mut self.class_sigs[id.0].static_generic_methods
                                } else {
                                    &mut self.class_sigs[id.0].generic_methods
                                };
                                templates.insert(name, template);
                            }
                        }
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
                    let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
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
            let mut fx = FnCtx::new(Type::Void, false, Some(this_ty.clone()), self.diags.clone());
            let checked = fx
                .with_synthetic_owner(
                    SyntheticOwnerKind::Initializer(self.pos(value.span())),
                    |fx| {
                        let checked = self.check_expr(value, Some(&field_ty), fx);
                        self.require_assignable(
                            &checked.ty.clone(),
                            &field_ty,
                            checked.pos.clone(),
                            "the descriptor member default",
                        );
                        checked
                    },
                )
                .0;
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
                    self.check_class_body(id, &c.class, c.declare);
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
                    let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
                    let init = match &d.init {
                        Some(init) => {
                            fx.with_synthetic_owner(
                                SyntheticOwnerKind::Initializer(self.pos(init.span())),
                                |fx| {
                                    let e = self.check_expr(init, Some(&sig.ty), fx);
                                    self.require_assignable(
                                        &e.ty.clone(),
                                        &sig.ty,
                                        e.pos.clone(),
                                        "the initializer",
                                    );
                                    e
                                },
                            )
                            .0
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
                let receiver_type = match &binding.ty {
                    Type::Nullable(inner) if binding.nullable => inner.as_ref(),
                    other => other,
                };
                let call = hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Call {
                        callee: hir::Callee::Method {
                            recv: Box::new(hir::Expr {
                                kind: hir::ExprKind::Local(binding.name.clone()),
                                ty: receiver_type.clone(),
                                pos: binding.pos.clone(),
                            }),
                            name: hir::DISPOSE_METHOD_NAME.to_string(),
                        },
                        args: Vec::new(),
                    },
                    ty: Type::Void,
                    pos: binding.pos.clone(),
                });
                let call = if binding.nullable {
                    hir::Stmt::If {
                        cond: hir::Expr {
                            kind: hir::ExprKind::Binary {
                                op: hir::BinOp::Ne,
                                left: Box::new(hir::Expr {
                                    kind: hir::ExprKind::Local(binding.name.clone()),
                                    ty: binding.ty.clone(),
                                    pos: binding.pos.clone(),
                                }),
                                right: Box::new(hir::Expr {
                                    kind: hir::ExprKind::Null,
                                    ty: Type::Null,
                                    pos: binding.pos.clone(),
                                }),
                            },
                            ty: Type::Bool,
                            pos: binding.pos.clone(),
                        },
                        then: vec![call],
                        els: None,
                        pos: binding.pos.clone(),
                    }
                } else {
                    call
                };
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
        let mut falls_through = true;
        for statement in statements {
            if !falls_through {
                break;
            }
            falls_through = fallthrough::can_fall_through(&statement);
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
                                nullable: matches!(ty, Type::Nullable(_)),
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
                        if fallthrough::sequence_can_fall_through(&last_case.body) {
                            last_case.body.extend(Self::make_disposal_statements(
                                std::slice::from_ref(&scope),
                                0,
                            ));
                        }
                    }
                    rewritten.push(hir::Stmt::Switch { disc, cases, pos });
                }
                other => rewritten.push(other),
            }
        }
        if open_scope {
            let scope = scopes.pop().unwrap_or_default();
            if falls_through && !scope.is_empty() {
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
        let mut fx = FnCtx::new(
            sig.ret.clone(),
            sig.is_generator,
            this_ty,
            self.diags.clone(),
        );
        fx.frames[0].missing_this_divergence = missing_this_divergence;
        fx.frames[0].is_async = sig.is_async;
        if sig.is_generator {
            if let Type::Generator(y) = &sig.ret {
                if sig.yield_known {
                    fx.frames[0].yield_ty = Some((**y).clone());
                }
            }
        }
        let (params, prologue) = self.bind_params(f, sig, &mut fx);
        let body = match &f.body {
            Some(block) => {
                self.reserve_block_declarations(&block.stmts, &mut fx);
                let mut out = prologue;
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

    /// Declares parameters as locals and checks default values. Answers
    /// with the parameters and the statements that bind every pattern
    /// parameter at entry (§107.2).
    fn bind_params(
        &mut self,
        f: &ast::Function,
        sig: &FnSig,
        fx: &mut FnCtx,
    ) -> (Vec<hir::Param>, Vec<hir::Stmt>) {
        let mut out = Vec::new();
        let mut patterns = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let Some(ps) = sig.params.get(i) else { break };
            let pos = self.pos(p.span);
            let default = match &p.pat {
                ast::Pat::Assign(a) => Some(
                    fx.with_synthetic_owner(
                        SyntheticOwnerKind::Initializer(self.pos(a.right.span())),
                        |fx| {
                            let e = self.check_expr(&a.right, Some(&ps.ty), fx);
                            self.require_assignable(
                                &e.ty.clone(),
                                &ps.ty,
                                e.pos.clone(),
                                "the default value",
                            );
                            e
                        },
                    )
                    .0,
                ),
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
            patterns.push((ps.clone(), p.pat.clone()));
        }
        let prologue = self.bind_parameter_patterns(patterns, fx);
        (out, prologue)
    }

    /// Checks field initializers, the constructor, and methods (pass C).
    pub(crate) fn check_class_body(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
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
                        let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
                        fx.frames[0].missing_this_divergence =
                            Some(Divergence::StaticMemberSurface);
                        let init = match &prop.value {
                            Some(value) => {
                                fx.with_synthetic_owner(
                                    SyntheticOwnerKind::Initializer(self.pos(value.span())),
                                    |fx| {
                                        let expression =
                                            self.check_expr(value, Some(&signature.ty), fx);
                                        self.require_assignable(
                                            &expression.ty.clone(),
                                            &signature.ty,
                                            expression.pos.clone(),
                                            "the static field initializer",
                                        );
                                        expression
                                    },
                                )
                                .0
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
                    let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
                    fx.frames[0].missing_this_divergence = Some(Divergence::ThisInFieldInitializer);
                    let e = fx
                        .with_synthetic_owner(
                            SyntheticOwnerKind::Initializer(self.pos(value.span())),
                            |fx| {
                                let e = self.check_expr(value, Some(&field_ty), fx);
                                self.require_assignable(
                                    &e.ty.clone(),
                                    &field_ty,
                                    e.pos.clone(),
                                    "the field initializer",
                                );
                                e
                            },
                        )
                        .0;
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
                    let mut fx =
                        FnCtx::new(Type::Void, false, Some(this_ty.clone()), self.diags.clone());
                    let mut hir_params = Vec::new();
                    let mut patterns = Vec::new();
                    for (i, p) in ctor.params.iter().enumerate() {
                        let ast::ParamOrTsParamProp::Param(param) = p else {
                            continue;
                        };
                        let Some(ps) = sig.params.get(i) else { break };
                        let default = match &param.pat {
                            ast::Pat::Assign(a) => Some(
                                fx.with_synthetic_owner(
                                    SyntheticOwnerKind::Initializer(self.pos(a.right.span())),
                                    |fx| self.check_expr(&a.right, Some(&ps.ty), fx),
                                )
                                .0,
                            ),
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
                        patterns.push((ps.clone(), param.pat.clone()));
                    }
                    let mut body = self.bind_parameter_patterns(patterns, &mut fx);
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
        self.require_field_values(id, class, declared);
    }

    /// compiler.md §108.1: a declared field carries a value before the
    /// constructor returns. A field with no initializer is assigned at
    /// the constructor's top level, or the class is rejected at the
    /// field. A `!` assertion does not satisfy the rule. Rule 3 keeps a
    /// mirror field, a `@Descriptor` member, and a static field outside:
    /// a mirror class and a `declare class` have no constructor body, a
    /// `@Descriptor` class never reaches this pass, and a static field
    /// is a global with S100's own initializer rule. An instance of a
    /// generic template inherits the template's ambient status, so an
    /// ambient template stays outside the rule.
    ///
    /// The rule reads the constructor's top level only, and a top-level
    /// assignment counts only when no statement before it holds a
    /// `return` (rule 2). The diagnostic distinguishes four shapes,
    /// because §79 rule 4 pairs each with its measured `tsc` class: a
    /// field that no statement assigns (`tsc` answers TS2564), a `!`
    /// field (`tsc` accepts), a field assigned only inside a nested
    /// statement (`tsc` follows definite assignment; this rule does
    /// not), and a field whose top-level assignment stands after a
    /// statement that holds a `return` (`tsc` answers TS2564).
    fn require_field_values(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
        if declared || self.classes[id.0].is_boundary {
            return;
        }
        let mut spellings: HashMap<String, FieldSpelling> = HashMap::new();
        for member in &class.body {
            let ast::ClassMember::ClassProp(prop) = member else {
                continue;
            };
            let ast::PropName::Ident(key) = &prop.key else {
                continue;
            };
            if !prop.is_static {
                spellings.insert(
                    key.sym.to_string(),
                    FieldSpelling {
                        definite: prop.definite,
                        optional: prop.is_optional,
                        // The declared text, not the resolved type: a
                        // generic template declares `T`, and the advice
                        // is written into the template.
                        declared_type: prop
                            .type_ann
                            .as_ref()
                            .and_then(|annotation| self.prog.snippet(annotation.type_ann.span())),
                    },
                );
            }
        }
        let (top_level, anywhere, after_return) = match &self.classes[id.0].ctor {
            Some(ctor) => {
                let mut top_level = HashSet::new();
                let mut after_return = HashSet::new();
                let mut left = false;
                for statement in &ctor.body {
                    if let Some(name) = this_field_assignment(statement) {
                        if left {
                            after_return.insert(name.to_string());
                        } else {
                            top_level.insert(name.to_string());
                        }
                    }
                    left = left || leaves_the_function(statement);
                }
                let mut anywhere = HashSet::new();
                for statement in &ctor.body {
                    collect_this_field_assignments(hir::HirChild::Stmt(statement), &mut anywhere);
                }
                (top_level, anywhere, after_return)
            }
            None => (HashSet::new(), HashSet::new(), HashSet::new()),
        };
        let class_name = self.classes[id.0].name.clone();
        let unassigned: Vec<(String, Type, Pos)> = self.classes[id.0]
            .fields
            .iter()
            .filter(|field| field.init.is_none() && field.ty != Type::Error)
            .map(|field| (field.name.clone(), field.ty.clone(), field.pos.clone()))
            .collect();
        for (name, ty, pos) in unassigned {
            let spelling = spellings.get(&name).cloned().unwrap_or_default();
            if spelling.optional || top_level.contains(&name) {
                continue;
            }
            // `…` names no field, so the advice never reads as an
            // assignment of the field to itself.
            let declared = spelling
                .declared_type
                .unwrap_or_else(|| self.type_name(&ty));
            let spellings = format!(
                "write `{name}: {declared} = …`, or assign `this.{name} = …` at the top level \
                 of the constructor"
            );
            if spelling.definite {
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` asserts with `!` a value that nothing \
                         assigns at the constructor's top level; {spellings}"
                    ),
                    pos,
                    Divergence::DefiniteAssignmentAssertion,
                );
            } else if after_return.contains(&name) {
                self.error(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` is assigned at the constructor's top \
                         level after a statement that holds a `return`, so the constructor can \
                         return before the assignment (stock `tsc` answers TS2564); {spellings}, \
                         before every statement that holds a `return`"
                    ),
                    pos,
                );
            } else if anywhere.contains(&name) {
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` is assigned inside a nested statement \
                         of the constructor, not at its top level; {spellings}"
                    ),
                    pos,
                    Divergence::NestedFieldAssignment,
                );
            } else {
                self.error(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` has no initializer, and no constructor \
                         statement assigns it (stock `tsc` answers TS2564); {spellings}"
                    ),
                    pos,
                );
            }
        }
        self.check_this_in_assignment_prefix(id, &spellings);
    }

    /// compiler.md §108.4 rule 6: `this` inside the constructor's
    /// assignment prefix appears in two forms only. Form (a) is the
    /// target of `this.f = …`, at any depth. Form (b) is a read `this.g`
    /// of a field that holds a value at that statement.
    ///
    /// The assignment prefix is the constructor's parameter defaults,
    /// followed by its top-level statements up to and including the last
    /// top-level statement that assigns a field rule 1 reaches. A field
    /// holds a value when it has an initializer, or when a top-level
    /// statement earlier in the prefix assigns it. That is rule 2's
    /// notion: a nested assignment does not count, and the statement's
    /// own target does not count. A class with no rule-1 field has an
    /// empty prefix, and after the prefix every rule-1 field holds a
    /// value.
    ///
    /// Site A is a read of a field that holds no value. Stock `tsc`
    /// answers TS2565 for the measured forms, so the site carries no
    /// variant. Site B is a method or an accessor call on `this`, or
    /// `this` as a value; `tsc` accepts those, because its
    /// definite-assignment analysis does not follow a call.
    fn check_this_in_assignment_prefix(
        &mut self,
        id: ClassId,
        spellings: &HashMap<String, FieldSpelling>,
    ) {
        let rule_one_fields: Vec<String> = self.classes[id.0]
            .fields
            .iter()
            .filter(|field| field.init.is_none() && field.ty != Type::Error)
            .filter(|field| !spellings.get(&field.name).is_some_and(|s| s.optional))
            .map(|field| field.name.clone())
            .collect();
        if rule_one_fields.is_empty() {
            return;
        }
        let mut held: HashSet<String> = self.classes[id.0]
            .fields
            .iter()
            .map(|field| field.name.clone())
            .filter(|name| !rule_one_fields.contains(name))
            .collect();
        let Some(ctor) = self.classes[id.0].ctor.as_ref() else {
            return;
        };
        let mut collected: Vec<PrefixViolation> = Vec::new();
        let mut found = Vec::new();
        for parameter in &ctor.params {
            if let Some(default) = &parameter.default {
                prefix_this_violations(hir::HirChild::Expr(default), &held, &mut found);
            }
        }
        record_prefix_violations(found, &rule_one_fields, &held, &mut collected);
        // The prefix ends with the first top-level statement after which
        // every rule-1 field holds a value. If no statement completes the
        // set, the prefix is the whole constructor and rule 1 reports as
        // well.
        for statement in &ctor.body {
            let mut found = Vec::new();
            prefix_this_violations(hir::HirChild::Stmt(statement), &held, &mut found);
            record_prefix_violations(found, &rule_one_fields, &held, &mut collected);
            if let Some(name) = this_field_assignment(statement) {
                held.insert(name.to_string());
            }
            if rule_one_fields.iter().all(|field| held.contains(field)) {
                break;
            }
        }
        let class_name = self.classes[id.0].name.clone();
        for violation in collected {
            let PrefixViolation {
                pos,
                kind,
                first_missing,
                other_missing,
            } = violation;
            let (use_site, advice) = match kind {
                PrefixThis::Read(name) => {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`this.{name}` reads field `{name}` of `{class_name}` before the \
                             constructor assigns it at its top level; move the read after \
                             `this.{name} = …`, or give `{name}` an initializer"
                        ),
                        pos,
                    );
                    continue;
                }
                PrefixThis::Call => ("calls a member of `this`", "call"),
                PrefixThis::Value => ("uses `this` as a value", "use"),
            };
            let named = field_list(&first_missing, &other_missing);
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "the constructor of `{class_name}` {use_site} before {} {} a value; move the \
                     {advice} after {}, or {}",
                    named.subject, named.verb, named.assignments, named.initializers
                ),
                pos,
                Divergence::ThisBeforeFieldValues,
            );
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
        if template.has_static_member || !template.rejected_generic_methods.is_empty() {
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
        self.resolve_class_shape(id, &template.class, template.declared);
        if template.is_descriptor {
            self.check_descriptor_defaults(id, &template.class);
        } else {
            self.check_class_body(id, &template.class, template.declared);
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        Some(id)
    }

    // ----- shared lookups -----

    /// Resolves a name against the current file's top-level scope, then
    /// the global ambient scope (mirror declarations, §12.2).
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
            SourceFile::new(
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

        let module = check_program(&[SourceFile::new(
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
            let diagnostics = check_program(&[SourceFile::new("main.ts", source)])
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
        check_program(&[SourceFile::new(
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
            let diagnostics = check_program(&[SourceFile::new("main.ts", source)])
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
            match check_program(&[SourceFile::new("main.ts", &source)]) {
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
