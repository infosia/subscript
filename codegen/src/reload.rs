//! Dev-tier hot reload (`specs/blocks/compiler.md` §1, §8.2).
//!
//! # Reload eligibility
//!
//! A swap is accepted only when the module's **declaration hash** is
//! unchanged. The hash covers every class (field names, types, and
//! order), every enum member value, every `FixedArray` shape (as part
//! of the type spelling it appears in), every module-level variable's
//! name and type, every function signature, and the ordered source
//! module run identities (compiler.md §137 rule 3c). Function *bodies* are
//! excluded by construction: the hash walks declarations only and
//! never descends into a body. So only body edits reload; anything
//! else is refused, with the first differing declaration named.
//!
//! # Mechanism
//!
//! Reload-mode lowering ([`crate::lower::LowerOptions::reload`]) makes
//! the three code-generation changes below so a body can be replaced
//! under a live Context. It also permits an entry-less module because
//! the host drives named exports.
//!
//! - **Indirection table.** Every script call loads its target from a
//!   host-owned table of code addresses, reached through the Context.
//!   Slot numbers are a function of the declarations alone, so the
//!   recompile of an accepted swap yields the same slot for the same
//!   function; a swap rewrites the table entries in place.
//! - **Host-owned globals.** Module-level variables live in a block
//!   the session owns rather than in module data, which would die with
//!   the module. Their values, and the collection roots registered
//!   over them, survive a swap untouched.
//! - **Coroutine epoch.** Each coroutine frame records the reload
//!   epoch it was created in and `.next()` compares it against the
//!   Context's; a swap bumps the epoch, so resuming a coroutine
//!   suspended in a replaced body traps `stale coroutine after reload`
//!   at the resume position.
//!
//! # Frame boundary
//!
//! Swaps are applied only between host calls into script. The rule is
//! structural — [`ReloadSession::reload`] takes `&mut self`, so it
//! cannot run while a call through the same session is on the stack —
//! and additionally checked against the Context's script depth, which
//! [`ReloadSession::call_export`] raises for the duration of a call.
//!
//! # Retained code
//!
//! Pre-swap modules are kept alive for the session's lifetime, so a
//! function value taken before a swap stays a valid code address. Such
//! a value reaches the post-swap body: it points at the env wrapper,
//! which forwards through the indirection table. Lambda values are the
//! exception — a lambda is not a declaration, has no stable identity
//! across recompiles, and therefore keeps running its pre-swap body if
//! it was stored in Context state before the swap.
//!
//! Retention has a cost: each generation's code, data, and its string
//! literals (interned by data address, so a post-swap literal interns
//! afresh) stay allocated until the session is dropped. A session is a
//! development-tier object with a bounded edit count, not a shipped
//! one.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::HashMap;
use std::ffi::c_void;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::FuncId;
use subscript_compiler::{hir, Diagnostic, SourceFile, Type};
use subscript_runtime::Context;

use crate::jit::{install_reservation, register_runtime, RunError, TrapReport};
use crate::lower::{dev_flags, internal, lower_module_with_positions, LowerOptions};
use crate::native::{missing_symbol, register_symbols};
use crate::position_table::PositionTable;
use crate::NativeLibrary;

// ----- declaration hash -----

/// FNV-1a over 64 bits. Chosen for being fully specified here: the
/// hash must be reproducible for identical declarations across
/// recompiles and across processes, which the standard library's
/// default hasher does not promise.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The declaration fingerprint of a checked module: one entry per
/// declaration, in declaration order, plus their combined hash.
///
/// Two modules whose fingerprints are equal differ only in function
/// bodies, and only such a pair may be hot-swapped.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeclarationHash {
    entries: Vec<(String, u64)>,
    value: u64,
}

impl DeclarationHash {
    /// The combined hash of every declaration.
    #[must_use]
    pub fn value(&self) -> u64 {
        self.value
    }

    /// Human-readable names of the hashed declarations, in order.
    #[must_use]
    pub fn declarations(&self) -> Vec<&str> {
        self.entries.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// Names the first declaration that differs from `other`, or
    /// `None` when the two fingerprints are equal.
    #[must_use]
    pub fn first_difference(&self, other: &DeclarationHash) -> Option<String> {
        for (a, b) in self.entries.iter().zip(&other.entries) {
            if a.0 != b.0 {
                return Some(format!("{} (was {})", b.0, a.0));
            }
            if a.1 != b.1 {
                return Some(a.0.clone());
            }
        }
        match self.entries.len().cmp(&other.entries.len()) {
            std::cmp::Ordering::Less => other.entries.get(self.entries.len()).map(|e| e.0.clone()),
            std::cmp::Ordering::Greater => {
                self.entries.get(other.entries.len()).map(|e| e.0.clone())
            }
            std::cmp::Ordering::Equal => None,
        }
    }
}

/// Spells a function signature: name, parameter types in order, return
/// type, and the two shape bits that change the entry surface.
fn signature_text(f: &hir::Function) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{:?}:escape_kind={:?}", p.ty, p.escape_kind))
        .collect();
    format!(
        "{}({}) -> {:?}{}{}{}",
        f.name,
        params.join(","),
        f.ret,
        if f.exported { " export" } else { "" },
        if f.is_generator { " generator" } else { "" },
        if f.is_async { " async" } else { "" }
    )
}

/// Computes the declaration fingerprint of a checked module (§8.2).
///
/// Covered: classes (kind, field names, types, and order), enum member
/// values, Q32 string-alias member spellings and order, module-level
/// variable names and types, and every user function signature — free
/// functions, constructors, and methods, including parameter escape facts.
/// Synthesized helpers do not enter the hash (compiler.md §119).
/// Lambda parameter escape facts also enter the hash (compiler.md §118).
/// The ordered source module run identities also enter the hash (compiler.md §137 rule 3c).
/// Other body expressions, statements, and default values do not enter it.
#[must_use]
pub fn declaration_hash(m: &hir::Module) -> DeclarationHash {
    let label = |kind: &str, name: &str, pos: &subscript_compiler::Pos, ambiguous: bool| {
        format!("{kind} {}", hir::declaration_label(name, pos, ambiguous))
    };
    let mut entries: Vec<(String, u64)> = Vec::new();
    let mut push = |name: String, text: &str| entries.push((name, fnv1a(text.as_bytes())));

    push(
        format!("module run order/set {:?}", m.initializer_modules),
        &format!("{:?}", m.initializer_modules),
    );

    for entry in &m.host_entries {
        push(
            format!("host entry {}", entry.name),
            &format!("{:?}", entry.signature),
        );
    }

    for c in &m.classes {
        let ambiguous = m
            .classes
            .iter()
            .filter(|other| other.name == c.name)
            .count()
            > 1;
        let fields: Vec<String> = c
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{i}:{}:{:?}", f.name, f.ty))
            .collect();
        push(
            label("class", &c.name, &c.pos, ambiguous),
            &format!(
                "{}|{}|{:?}|{}",
                c.pos.file,
                c.name,
                crate::layout::class_shape(c),
                fields.join(";")
            ),
        );
        if let Some(ctor) = &c.ctor {
            push(
                label("constructor", &c.name, &c.pos, ambiguous),
                &signature_text(ctor),
            );
        }
        for method in &c.methods {
            push(
                label(
                    "method",
                    &format!("{}.{}", c.name, method.name),
                    &c.pos,
                    ambiguous,
                ),
                &signature_text(method),
            );
        }
    }
    for e in &m.enums {
        let members: Vec<String> = e
            .members
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        push(
            label(
                "enum",
                &e.name,
                &e.pos,
                m.enums.iter().filter(|other| other.name == e.name).count() > 1,
            ),
            &format!("{}|{}|{}", e.pos.file, e.name, members.join(";")),
        );
    }
    for alias in &m.string_aliases {
        let wires = alias
            .wire_values
            .as_ref()
            .map_or_else(String::new, |values| {
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            });
        push(
            label(
                "string alias",
                &alias.name,
                &alias.pos,
                m.string_aliases
                    .iter()
                    .filter(|other| other.name == alias.name)
                    .count()
                    > 1,
            ),
            &format!(
                "{}|{}|{}|{wires}",
                alias.pos.file,
                alias.name,
                alias.members.join(";")
            ),
        );
    }
    for g in &m.globals {
        push(
            label(
                "variable",
                &g.name,
                &g.pos,
                m.globals
                    .iter()
                    .filter(|other| other.name == g.name)
                    .count()
                    > 1,
            ),
            &format!("{}:{:?}", g.symbol.full_text(), g.ty),
        );
    }
    for f in m.functions.iter().filter(|f| !f.synthesized_helper) {
        push(
            label(
                "function",
                &f.name,
                &f.pos,
                m.functions
                    .iter()
                    .filter(|other| other.name == f.name)
                    .count()
                    > 1,
            ),
            &format!("{}|{}", f.symbol.full_text(), signature_text(f)),
        );
    }

    let mut lambdas = Vec::new();
    fn lambda_facts(m: &hir::Module, child: hir::HirChild<'_>, facts: &mut Vec<String>) {
        match child {
            hir::HirChild::Expr(expr) => {
                if let hir::ExprKind::Lambda { params, .. } = &expr.kind {
                    if params.iter().any(|p| m.carries_capture(&p.ty)) {
                        facts.push(
                            params
                                .iter()
                                .map(|p| format!("{:?}", p.escape_kind))
                                .collect::<Vec<_>>()
                                .join(","),
                        );
                    }
                }
                for child in expr.children() {
                    lambda_facts(m, child, facts);
                }
            }
            hir::HirChild::Stmt(stmt) => {
                for child in stmt.children() {
                    lambda_facts(m, child, facts);
                }
            }
        }
    }
    for owner in m.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(expr) => {
                lambda_facts(m, hir::HirChild::Expr(expr), &mut lambdas)
            }
            hir::ExpressionOwner::Body { statements, .. } => {
                for stmt in statements {
                    lambda_facts(m, hir::HirChild::Stmt(stmt), &mut lambdas);
                }
            }
        }
    }
    if !lambdas.is_empty() {
        push("lambda parameter escapes".to_owned(), &lambdas.join(";"));
    }

    let mut value: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, h) in &entries {
        value = fnv1a(
            &[
                value.to_le_bytes(),
                fnv1a(name.as_bytes()).to_le_bytes(),
                h.to_le_bytes(),
            ]
            .concat(),
        );
    }
    DeclarationHash { entries, value }
}

// ----- session -----

/// Why a hot reload was refused.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReloadError {
    /// The new sources do not check.
    Rejected(Vec<Diagnostic>),
    /// A declaration changed, so the swap is not eligible: the running
    /// program keeps its current code and its Context untouched.
    DeclarationChanged {
        /// The first declaration that differs, by name.
        declaration: String,
    },
    /// Script code was on the stack (a swap may only happen between
    /// host calls into script).
    ScriptOnStack,
    /// At least one worker may still execute code from the current
    /// generation, so the swap must wait until every worker is joined.
    LiveWorkers,
    /// A called foreign C symbol was absent from every native library
    /// supplied when the session was created.
    UnresolvedForeignSymbol(String),
    /// An internal lowering/backend failure (a bug, not a user error).
    Internal(String),
}

impl std::fmt::Display for ReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReloadError::Rejected(diags) => {
                write!(f, "reload rejected with {} diagnostic(s)", diags.len())?;
                for d in diags {
                    write!(f, "\n  {d}")?;
                }
                Ok(())
            }
            ReloadError::DeclarationChanged { declaration } => write!(
                f,
                "reload refused: declaration `{declaration}` changed; \
                 only function bodies can be hot-swapped"
            ),
            ReloadError::ScriptOnStack => write!(
                f,
                "reload refused: script code is on the stack; swaps happen \
                 only between host calls into script"
            ),
            ReloadError::LiveWorkers => write!(
                f,
                "reload refused: the Context has live workers; join them before swapping code"
            ),
            ReloadError::UnresolvedForeignSymbol(name) => write!(
                f,
                "unresolved foreign symbol `{name}`: no supplied native library registers it"
            ),
            ReloadError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ReloadError {}

/// Host-owned block backing the module-level variables of a session.
///
/// Kept out of module data so its address, its contents, and the
/// collection roots registered over it survive every swap.
struct GlobalBlock {
    ptr: *mut u8,
    layout: Option<Layout>,
    size: u32,
    align: u32,
}

impl GlobalBlock {
    fn new(size: u32, align: u32) -> Result<GlobalBlock, String> {
        if size == 0 {
            return Ok(GlobalBlock {
                ptr: std::ptr::null_mut(),
                layout: None,
                size,
                align,
            });
        }
        let layout = Layout::from_size_align(size as usize, align.max(1) as usize)
            .map_err(|e| internal(format!("global block layout: {e}")))?;
        // SAFETY: `layout` has non-zero size.
        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(internal("global block allocation failed"));
        }
        Ok(GlobalBlock {
            ptr,
            layout: Some(layout),
            size,
            align,
        })
    }
}

impl Drop for GlobalBlock {
    fn drop(&mut self) {
        if let Some(layout) = self.layout {
            // SAFETY: `ptr`/`layout` came from `alloc_zeroed` above and
            // are freed exactly once, here.
            unsafe { dealloc(self.ptr, layout) };
        }
    }
}

/// One argument passed from a host to a host-callable script export.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum EntryArg {
    /// An opaque handle borrowed for the duration of the call.
    Handle(*mut c_void),
    /// An 8-bit signed integer.
    I8(i8),
    /// An 8-bit unsigned integer.
    U8(u8),
    /// A 16-bit signed integer.
    I16(i16),
    /// A 16-bit unsigned integer.
    U16(u16),
    /// A 32-bit signed integer.
    I32(i32),
    /// A 32-bit unsigned integer.
    U32(u32),
    /// A 64-bit signed integer.
    I64(i64),
    /// A 64-bit unsigned integer.
    U64(u64),
    /// Raw IEEE 754 binary16 bits.
    F16(u16),
    /// A 32-bit IEEE float.
    F32(f32),
    /// A 64-bit IEEE float.
    F64(f64),
    /// A boolean.
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryParamKind {
    Handle,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F16,
    F32,
    F64,
    Bool,
}

impl EntryParamKind {
    fn name(self) -> &'static str {
        match self {
            Self::Handle => "opaque handle",
            Self::I8 => "i8",
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F16 => "f16",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "boolean",
        }
    }
}

impl EntryArg {
    fn kind(self) -> EntryParamKind {
        match self {
            Self::Handle(_) => EntryParamKind::Handle,
            Self::I8(_) => EntryParamKind::I8,
            Self::U8(_) => EntryParamKind::U8,
            Self::I16(_) => EntryParamKind::I16,
            Self::U16(_) => EntryParamKind::U16,
            Self::I32(_) => EntryParamKind::I32,
            Self::U32(_) => EntryParamKind::U32,
            Self::I64(_) => EntryParamKind::I64,
            Self::U64(_) => EntryParamKind::U64,
            Self::F16(_) => EntryParamKind::F16,
            Self::F32(_) => EntryParamKind::F32,
            Self::F64(_) => EntryParamKind::F64,
            Self::Bool(_) => EntryParamKind::Bool,
        }
    }

    fn value(self) -> EntryValue {
        match self {
            Self::Handle(value) => EntryValue { handle: value },
            Self::I8(value) => EntryValue { i8_: value },
            Self::U8(value) => EntryValue { u8_: value },
            Self::I16(value) => EntryValue { i16_: value },
            Self::U16(value) | Self::F16(value) => EntryValue { u16_: value },
            Self::I32(value) => EntryValue { i32_: value },
            Self::U32(value) => EntryValue { u32_: value },
            Self::I64(value) => EntryValue { i64_: value },
            Self::U64(value) => EntryValue { u64_: value },
            Self::F32(value) => EntryValue { f32_: value },
            Self::F64(value) => EntryValue { f64_: value },
            Self::Bool(value) => EntryValue {
                bool_: u8::from(value),
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
union EntryValue {
    handle: *mut c_void,
    i8_: i8,
    u8_: u8,
    i16_: i16,
    u16_: u16,
    i32_: i32,
    u32_: u32,
    i64_: i64,
    u64_: u64,
    f32_: f32,
    f64_: f64,
    bool_: u8,
}

#[derive(Clone)]
struct SessionEntry {
    code: *const u8,
    params: Vec<EntryParamKind>,
    uses_argument_adapter: bool,
}

/// A live dev-tier program that can have its function bodies swapped.
///
/// Create it from sources, call exported entries from the host, and
/// [`ReloadSession::reload`] between calls. Accepted swaps keep the
/// Context — globals, live allocations, and the stdout sink — and
/// execution continues; refused swaps leave everything untouched.
#[must_use = "a session owns the running program; dropping it ends the run"]
pub struct ReloadSession {
    // Field order is load-bearing: Context::drop closes and joins every
    // worker before any JIT module containing worker code is released.
    ctx: Box<Context>,
    // compiler.md §272 rule 1: retain the initializer failure across host calls.
    initializer_trap: Option<TrapReport>,
    output_prefix: Vec<u8>,
    file_provider: Option<subscript_runtime::ffi::FileProvider>,
    modules: Vec<JITModule>,
    table: Vec<*const u8>,
    globals: GlobalBlock,
    entries: HashMap<String, SessionEntry>,
    async_runner: *const u8,
    positions: PositionTable,
    decls: DeclarationHash,
    runner_error: Option<subscript_compiler::Diagnostic>,
    native_libraries: Vec<NativeLibrary>,
    check_options: subscript_compiler::CheckOptions,
}

/// One compiled generation: the module plus what the driver needs from
/// it.
struct Generation {
    module: JITModule,
    table: Vec<*const u8>,
    entries: HashMap<String, SessionEntry>,
    async_runner: *const u8,
    init_slot: Option<usize>,
    positions: PositionTable,
    globals_size: u32,
    globals_align: u32,
}

impl Generation {
    /// Releases a generation that never became part of a session.
    ///
    /// Only for generations no code address escaped from: a generation
    /// the session accepted is retained for the session's lifetime,
    /// because pre-swap function values point into it.
    fn discard(self) {
        // SAFETY: this generation was never installed — no Context saw
        // its table, nothing ran from it, and no pointer into its code
        // or data escaped this function's caller.
        unsafe { self.module.free_memory() };
    }
}

fn entry_param_kind(module: &hir::Module, ty: &Type) -> Result<EntryParamKind, String> {
    Ok(match ty {
        Type::I8 => EntryParamKind::I8,
        Type::U8 => EntryParamKind::U8,
        Type::I16 => EntryParamKind::I16,
        Type::U16 => EntryParamKind::U16,
        Type::I32 => EntryParamKind::I32,
        Type::U32 => EntryParamKind::U32,
        Type::I64 => EntryParamKind::I64,
        Type::U64 => EntryParamKind::U64,
        Type::F16 => EntryParamKind::F16,
        Type::F32 => EntryParamKind::F32,
        Type::F64 => EntryParamKind::F64,
        Type::Bool => EntryParamKind::Bool,
        other if is_opaque_handle(module, other) => EntryParamKind::Handle,
        other => {
            return Err(internal(format!(
                "host entry has unsupported parameter type {other:?}"
            )))
        }
    })
}

fn is_opaque_handle(module: &hir::Module, ty: &Type) -> bool {
    let Type::Class(id) = ty else {
        return false;
    };
    module.classes.get(id.0).is_some_and(|class| {
        !class.is_value
            && !class.is_descriptor
            && !class.is_boundary
            && class.fields.is_empty()
            && class.ctor.is_none()
            && class.methods.is_empty()
            && class.index_signature.is_none()
    })
}

/// Compiles `hir` in reload mode into a fresh JIT module and resolves
/// every slot to a finalized code address.
fn compile(
    hirm: &hir::Module,
    libraries: &[NativeLibrary],
    positions: PositionTable,
) -> Result<Generation, RunError> {
    let flags = dev_flags().map_err(RunError::Internal)?;
    let isa = cranelift_native::builder()
        .map_err(|e| RunError::Internal(internal(format!("host ISA: {e}"))))
        .and_then(|b| {
            b.finish(flags)
                .map_err(|e| RunError::Internal(internal(format!("ISA flags: {e}"))))
        })?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    // §110 rule 1: each generation is one module, and each module gets
    // one reservation of its own.
    install_reservation(&mut builder, hirm.source_bytes).map_err(RunError::Internal)?;
    register_runtime(&mut builder);
    register_symbols(&mut builder, libraries);
    let mut module = JITModule::new(builder);

    // A failure past this point must release the module's code pages:
    // a dropped `JITModule` frees nothing by itself.
    let lowered = match lower_module_with_positions(
        &mut module,
        hirm,
        LowerOptions {
            reload: true,
            require_main: false,
        },
        positions,
    ) {
        Ok(l) => l,
        Err(e) => {
            // SAFETY: nothing ran and no pointer into this module
            // escaped; the partially built module is unreachable.
            unsafe { module.free_memory() };
            return Err(RunError::Internal(e));
        }
    };
    if let Some(name) = missing_symbol(&lowered.foreign_symbols, libraries) {
        // SAFETY: lowering has not finalized or executed the module and no
        // code or data pointer escaped.
        unsafe { module.free_memory() };
        return Err(RunError::UnresolvedForeignSymbol(name.to_string()));
    }
    if let Err(e) = module.finalize_definitions() {
        // SAFETY: finalization failed before any generated entry was
        // exposed or executed.
        unsafe { module.free_memory() };
        return Err(RunError::Internal(internal(format!("finalize: {e}"))));
    }

    let slot_index =
        |id: FuncId, slots: &[Option<FuncId>]| slots.iter().position(|s| *s == Some(id));
    let init_slot = slot_index(lowered.init, &lowered.slots);
    let mut entries = HashMap::new();
    for e in &lowered.entries {
        let id = e.reload_adapter.unwrap_or(e.id);
        let params = e
            .params
            .iter()
            .map(|parameter| entry_param_kind(hirm, parameter))
            .collect::<Result<Vec<_>, _>>()
            .map_err(RunError::Internal)?;
        entries.insert(
            e.name.clone(),
            SessionEntry {
                code: module.get_finalized_function(id),
                params,
                uses_argument_adapter: e.reload_adapter.is_some(),
            },
        );
    }
    let table: Vec<*const u8> = lowered
        .slots
        .iter()
        .map(|s| match s {
            Some(id) => module.get_finalized_function(*id),
            None => std::ptr::null(),
        })
        .collect();

    let async_runner = module.get_finalized_function(lowered.async_runner);
    Ok(Generation {
        async_runner,
        module,
        table,
        entries,
        init_slot,
        positions: lowered.positions,
        globals_size: lowered.globals_size,
        globals_align: lowered.globals_align,
    })
}

/// Calls a `(ctx) -> void` entry through the indirection table.
fn call_slot(ctx: &mut Context, table: &[*const u8], slot: usize) -> Result<(), String> {
    let code = *table
        .get(slot)
        .ok_or_else(|| internal("entry slot out of range"))?;
    if code.is_null() {
        return Err(internal("entry slot holds no code"));
    }
    call_code(ctx, code)
}

fn prepare_call(ctx: &mut Context) {
    // The host boundary clears the trap record (§8.2): a trap ends the
    // call, not the session. Context state stays unchanged.
    ctx.clear_trap();
    ctx.enter_script();
}

fn call_code(ctx: &mut Context, code: *const u8) -> Result<(), String> {
    if code.is_null() {
        return Err(internal("entry holds no code"));
    }
    type Entry = unsafe extern "C" fn(*mut Context);
    prepare_call(ctx);
    // SAFETY: `code` is finalized JIT code for a function the lowering
    // built with exactly this signature (`(ctx) -> void`, host C
    // calling convention); its module is retained by the session for
    // the session's whole life; `ctx` is a live exclusive Context.
    // Generated code never unwinds — traps return through the emitted
    // flag-check paths — so no panic crosses this boundary.
    unsafe {
        let f: Entry = std::mem::transmute(code);
        f(&mut *ctx);
    }
    ctx.exit_script();
    Ok(())
}

fn call_code_with(ctx: &mut Context, code: *const u8, values: &[EntryValue]) -> Result<(), String> {
    if code.is_null() {
        return Err(internal("entry holds no code"));
    }
    type Entry = unsafe extern "C" fn(*mut Context, *const EntryValue);
    prepare_call(ctx);
    // SAFETY: `code` is a finalized reload adapter with the declared
    // `(ctx, values) -> void` signature. The values match its recorded
    // parameter signature, and the session retains the adapter's module.
    unsafe {
        let function: Entry = std::mem::transmute(code);
        function(&mut *ctx, values.as_ptr());
    }
    ctx.exit_script();
    Ok(())
}

impl ReloadSession {
    /// Compiles `files` in reload mode, runs the module-global
    /// initializer, and returns the live session.
    ///
    /// # Errors
    ///
    /// [`RunError::Rejected`] when the checker rejects the program,
    /// [`RunError::Trap`] when the initializer trapped,
    /// [`RunError::UnresolvedForeignSymbol`] when the program calls a
    /// symbol but no native library was supplied,
    /// [`RunError::Internal`] on backend failures.
    pub fn new(files: &[SourceFile]) -> Result<ReloadSession, RunError> {
        Self::new_with_native_libraries(files, &[])
    }

    /// Compiles `files`, runs the module-global initializer, and returns the
    /// live session together with any trap raised by that initializer.
    ///
    /// Unlike [`ReloadSession::new`], an initializer trap does not discard the
    /// session. This is intended for reload-capable hosts, which must report a
    /// trapped call while keeping the Context available for a later edit.
    ///
    /// # Errors
    ///
    /// [`RunError::Rejected`] when the checker rejects the program,
    /// [`RunError::UnresolvedForeignSymbol`] when the program calls a symbol
    /// without supplying a native library, and [`RunError::Internal`] on
    /// backend failures. Initializer traps are returned in the tuple.
    pub fn new_capturing_initializer_trap(
        files: &[SourceFile],
    ) -> Result<(ReloadSession, Option<TrapReport>), RunError> {
        Self::build(files, &[])
    }

    /// Compiles `files` in reload mode with caller-supplied native
    /// libraries, runs the module-global initializer, and returns the live
    /// session. The same libraries remain available to every accepted
    /// reload.
    ///
    /// # Errors
    ///
    /// Returns the same [`RunError`] variants as [`ReloadSession::new`],
    /// including [`RunError::UnresolvedForeignSymbol`] when a called
    /// foreign symbol is absent from `libraries`.
    pub fn new_with_native_libraries(
        files: &[SourceFile],
        libraries: &[NativeLibrary],
    ) -> Result<ReloadSession, RunError> {
        let (session, trap) = Self::build(files, libraries)?;
        match trap {
            Some(trap) => Err(RunError::Trap(trap)),
            None => Ok(session),
        }
    }

    fn build(
        files: &[SourceFile],
        libraries: &[NativeLibrary],
    ) -> Result<(ReloadSession, Option<TrapReport>), RunError> {
        Self::build_configured(
            files,
            crate::RunConfig {
                native_libraries: libraries,
                ..crate::RunConfig::default()
            },
        )
    }

    /// Builds a session with enabled modules and a provider installed before initialization.
    ///
    /// # Errors
    /// Returns the same failures as new_with_native_libraries.
    /// [`RunError::Internal`] when `config` sets an option other than
    /// `enabled_modules`, `native_libraries`, and `file_provider`.
    pub fn new_configured(
        files: &[SourceFile],
        config: crate::RunConfig<'_>,
    ) -> Result<Self, RunError> {
        let (session, trap) = Self::build_configured(files, config)?;
        match trap {
            Some(trap) => Err(RunError::Trap(trap)),
            None => Ok(session),
        }
    }

    /// Captures an initializer trap with explicit standard-module configuration.
    ///
    /// # Errors
    /// Returns the same build failures as new_capturing_initializer_trap.
    pub fn new_capturing_initializer_trap_configured(
        files: &[SourceFile],
        config: crate::RunConfig<'_>,
    ) -> Result<(Self, Option<TrapReport>), RunError> {
        Self::build_configured(files, config)
    }

    fn build_configured(
        files: &[SourceFile],
        config: crate::RunConfig<'_>,
    ) -> Result<(Self, Option<TrapReport>), RunError> {
        if config.pre_init_hook.is_some()
            || config.pre_entry_hook.is_some()
            || config.post_run_hook.is_some()
            || config.fail_alloc_after.is_some()
            || config.freed_handle_diagnostics
            || config.memory_accounting
        {
            return Err(RunError::Internal(internal(
                "a reload session applies only enabled modules, native libraries, \
                 and a file provider",
            )));
        }
        let libraries = config.native_libraries;
        let check_options = config.check_options();
        let hirm = subscript_compiler::check_program_with(files, &check_options)
            .map_err(RunError::Rejected)?;
        let decls = declaration_hash(&hirm);
        let gen = compile(&hirm, libraries, PositionTable::new())?;
        let globals = match GlobalBlock::new(gen.globals_size, gen.globals_align) {
            Ok(g) => g,
            Err(e) => {
                gen.discard();
                return Err(RunError::Internal(e));
            }
        };

        let mut session = ReloadSession {
            ctx: Context::new(),
            initializer_trap: None,
            output_prefix: Vec::new(),
            file_provider: config.file_provider,
            modules: vec![gen.module],
            table: gen.table,
            globals,
            entries: gen.entries,
            async_runner: gen.async_runner,
            positions: gen.positions,
            decls,
            runner_error: hirm.runner_main().err(),
            native_libraries: libraries.to_vec(),
            check_options,
        };
        // SAFETY: with_file_provider requires host state and callbacks to outlive this session (section 263).
        unsafe {
            session.ctx.set_file_provider(config.file_provider);
        }
        unsafe {
            // SAFETY: ReloadSession retains the table and JIT modules until after Context drop (section 263).
            session.ctx.set_fn_table(session.table.as_ptr())
        };
        unsafe {
            // SAFETY: ReloadSession retains its globals block until after Context drop (section 263).
            session.ctx.set_globals(session.globals.ptr)
        };
        let init_slot = gen
            .init_slot
            .ok_or_else(|| RunError::Internal(internal("no initializer slot")))?;
        call_slot(&mut session.ctx, &session.table, init_slot).map_err(RunError::Internal)?;
        let trap = match session.check_trap() {
            Ok(()) => None,
            Err(RunError::Trap(trap)) => Some(trap),
            Err(error) => return Err(error),
        };
        session.initializer_trap = trap.clone();
        Ok((session, trap))
    }

    /// The current declaration fingerprint.
    #[must_use]
    pub fn declaration_hash(&self) -> &DeclarationHash {
        &self.decls
    }

    /// Returns the public host names in sorted order.
    #[must_use]
    pub fn host_entry_names(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.entries.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Calls the exported `main(): void` when the session has one.
    /// Session creation does not require this entry.
    ///
    /// # Errors
    ///
    /// As [`ReloadSession::call_export`].
    pub fn call_main(&mut self) -> Result<(), RunError> {
        self.check_initializer()?;
        if let Some(diagnostic) = &self.runner_error {
            return Err(RunError::Rejected(vec![diagnostic.clone()]));
        }
        self.call_export("main")
    }

    /// Runs main, kicks the shared async root list, and pumps pending jobs.
    ///
    /// # Errors
    /// Returns the runner diagnostic, a backend error, or the first script trap.
    pub fn run_main(&mut self) -> Result<(), RunError> {
        self.call_main()?;
        call_code(&mut self.ctx, self.async_runner).map_err(RunError::Internal)?;
        self.check_trap()?;
        while self.async_pending() != 0 {
            self.async_step()?;
        }
        Ok(())
    }

    /// Calls the exported zero-argument `void` function `name`. This
    /// is a host call into script: it is the only place script code
    /// runs, and a reload may only happen between two such calls.
    ///
    /// The trap record is cleared on entry, so a trap ends the call,
    /// not the session (§8.2): the next call runs normally, over the
    /// same Context state. Clearing is reporting-only — a stale
    /// coroutine stays stale and traps again on the next resume, and a
    /// deleted allocation stays deleted.
    ///
    /// # Errors
    ///
    /// [`RunError::Trap`] when *this* call trapped (the host process is
    /// never killed), [`RunError::Internal`] when no such entry
    /// exists.
    pub fn call_export(&mut self, name: &str) -> Result<(), RunError> {
        self.check_initializer()?;
        let code = self
            .entries
            .get(name)
            .filter(|entry| entry.params.is_empty())
            .map(|entry| entry.code)
            .ok_or_else(|| {
                RunError::Internal(internal(format!(
                    "`{name}` is not an exported zero-argument void function"
                )))
            })?;
        call_code(&mut self.ctx, code).map_err(RunError::Internal)?;
        self.check_trap()
    }

    /// Calls the host-callable export `name` with `args`.
    ///
    /// The function validates the name, arity, and every argument kind
    /// before it runs script code.
    ///
    /// # Errors
    ///
    /// [`RunError::Trap`] when the call traps. [`RunError::Internal`] when
    /// the export does not exist or an argument does not match its signature.
    pub fn call_export_with(&mut self, name: &str, args: &[EntryArg]) -> Result<(), RunError> {
        self.check_initializer()?;
        let entry = self.entries.get(name).ok_or_else(|| {
            RunError::Internal(internal(format!("`{name}` is not a host-callable export")))
        })?;
        if entry.params.len() != args.len() {
            return Err(RunError::Internal(internal(format!(
                "`{name}` expects {} argument(s), got {}",
                entry.params.len(),
                args.len()
            ))));
        }
        for (index, (expected, actual)) in entry.params.iter().zip(args).enumerate() {
            let actual_kind = actual.kind();
            if *expected != actual_kind {
                return Err(RunError::Internal(internal(format!(
                    "`{name}` argument {index} expects {}, got {}",
                    expected.name(),
                    actual_kind.name()
                ))));
            }
        }
        let code = entry.code;
        let uses_argument_adapter = entry.uses_argument_adapter;
        let values = args
            .iter()
            .copied()
            .map(EntryArg::value)
            .collect::<Vec<_>>();
        if uses_argument_adapter {
            call_code_with(&mut self.ctx, code, &values).map_err(RunError::Internal)?;
        } else {
            call_code(&mut self.ctx, code).map_err(RunError::Internal)?;
        }
        self.check_trap()
    }

    /// Number of live Context allocations, for host inspection.
    #[must_use]
    pub fn live_allocations(&self) -> usize {
        self.ctx.live_count()
    }

    /// Returns the live boundary scratch allocation count of this Context (compiler.md section 264).
    #[must_use]
    pub fn live_boundary_scratch_allocations(&self) -> usize {
        self.ctx.boundary_scratch_mark()
    }

    /// Returns the number of ready jobs plus parked registrations (§94.2).
    /// The count includes a trapping job that the Context still holds trapped.
    /// It excludes blocked continuations, stopped frames, and completed frames
    /// retained only by handles. This observer executes no script.
    #[must_use]
    pub fn async_pending(&self) -> usize {
        self.ctx.async_pending()
    }

    /// The number of registered async invocations without a cached
    /// completion (`compiler.md` §94.2). A reload host reads it beside
    /// [`ReloadSession::async_pending`] to tell quiescence from blocked
    /// work. It executes no script.
    #[must_use]
    pub fn async_unfinished(&self) -> usize {
        self.ctx.async_unfinished()
    }

    /// Reads registered tasks in id order, with resolved positions beside their ids (§169).
    /// This read runs no script and changes no state.
    #[must_use]
    pub fn async_tasks(&self) -> Vec<crate::AsyncTaskInfo> {
        let mut records: Vec<subscript_runtime::AsyncTaskInfo> = Vec::new();
        // SAFETY: the callback only copies records into its matching vector.
        unsafe {
            self.ctx.visit_async_tasks(
                Some(crate::async_inspection::collect),
                (&mut records as *mut Vec<subscript_runtime::AsyncTaskInfo>).cast(),
            );
        }
        records
            .into_iter()
            .map(|info| crate::AsyncTaskInfo {
                task_id: info.task_id,
                awaited_task_id: info.awaited_task_id,
                state: info.state,
                kind: info.kind,
                function_pos_id: info.function_pos_id,
                await_pos_id: info.await_pos_id,
                create_pos_id: info.create_pos_id,
                reserved: info.reserved,
                create_pos: self.positions.report_position(info.create_pos_id),
                function_pos: self.positions.report_position(info.function_pos_id),
                await_pos: self.positions.report_position(info.await_pos_id),
            })
            .collect()
    }

    /// Runs one host checkpoint (`compiler.md` §94.1) and returns the work
    /// still pending.
    ///
    /// This does not clear a pending trap: the runtime's trapped-Context
    /// no-op rule remains observable. Old JIT generations are retained by
    /// the session, so callbacks queued before a reload remain callable and
    /// report the normal stale-coroutine trap.
    pub fn async_step(&mut self) -> Result<usize, RunError> {
        self.check_initializer()?;
        // SAFETY: the session retains every JIT module that can have placed
        // a callback in this Context's pending queue.
        let remaining = unsafe { self.ctx.async_step() };
        self.check_trap()?;
        Ok(remaining)
    }

    /// Starts at most `max_dispatches` jobs and returns checkpoint counts (§168).
    /// A dispatch runs to suspension, completion, or a trap; this is no time limit.
    ///
    /// # Errors
    /// Returns the Context trap, if one exists after the checkpoint.
    pub fn async_step_budget(
        &mut self,
        max_dispatches: u64,
    ) -> Result<subscript_runtime::AsyncStepReport, RunError> {
        self.check_initializer()?;
        // SAFETY: the session retains every queued JIT callback's module.
        let report = unsafe { self.ctx.async_step_budget(max_dispatches) };
        self.check_trap()?;
        Ok(report)
    }

    /// Takes the stdout bytes produced since the last take.
    ///
    /// The session installs no print observer, so the Context sink is
    /// its capture.
    #[must_use]
    pub fn take_output(&mut self) -> Vec<u8> {
        let mut output = std::mem::take(&mut self.output_prefix);
        output.extend(self.ctx.take_stdout());
        output
    }

    /// Attempts to swap in the function bodies of `files`.
    ///
    /// If the initializer failed, rebuilds the session and runs the initializer again (§272 rule 3).
    /// Otherwise, accepted when the declaration fingerprint is unchanged: the
    /// indirection table is repointed at the newly compiled bodies,
    /// Context state (globals, live allocations, sink) survives, and
    /// execution continues at the next host call. Coroutines suspended
    /// across the swap are invalidated and trap on resume; a trap does
    /// not end the session, so a swap after one is applied normally.
    ///
    /// A module-level variable's **initializer** is a body-position
    /// expression and is outside the declaration hash, so editing one
    /// is *accepted and has no effect*: initializers run in `subscript_init`,
    /// which executes once when initialization succeeds and is never re-run
    /// by a swap — re-running it would overwrite exactly the state the
    /// swap is required to preserve. Changing a variable's name or
    /// type does change the hash and is refused.
    ///
    /// # Errors
    ///
    /// [`ReloadError::Rejected`] when the new sources do not check,
    /// [`ReloadError::DeclarationChanged`] naming the first differing
    /// declaration, [`ReloadError::ScriptOnStack`] when script code is
    /// running, [`ReloadError::UnresolvedForeignSymbol`] when an edited
    /// body first calls an unsupplied symbol, and [`ReloadError::Internal`]
    /// on backend failures. In every failure case the running program is
    /// untouched.
    pub fn reload(&mut self, files: &[SourceFile]) -> Result<(), ReloadError> {
        if self.ctx.script_depth() != 0 {
            return Err(ReloadError::ScriptOnStack);
        }
        // compiler.md §272 rule 3: a failed initializer needs a fresh global block.
        if self.initializer_trap.is_some() {
            let modules: Vec<&str> = self
                .check_options
                .enabled_modules
                .iter()
                .map(String::as_str)
                .collect();
            let (mut replacement, _) = Self::build_configured(
                files,
                crate::RunConfig {
                    enabled_modules: &modules,
                    native_libraries: &self.native_libraries,
                    file_provider: self.file_provider,
                    ..crate::RunConfig::default()
                },
            )
            .map_err(|error| match error {
                RunError::Rejected(diagnostics) => ReloadError::Rejected(diagnostics),
                RunError::UnresolvedForeignSymbol(name) => {
                    ReloadError::UnresolvedForeignSymbol(name)
                }
                other => ReloadError::Internal(other.to_string()),
            })?;
            // compiler.md §272 rule 3: retain unread output before the new initializer output.
            replacement.output_prefix = self.take_output();
            *self = replacement;
            return Ok(());
        }
        if self.ctx.has_live_workers() {
            return Err(ReloadError::LiveWorkers);
        }
        let hirm = subscript_compiler::check_program_with(files, &self.check_options)
            .map_err(ReloadError::Rejected)?;
        let decls = declaration_hash(&hirm);
        if decls != self.decls {
            return Err(ReloadError::DeclarationChanged {
                declaration: self
                    .decls
                    .first_difference(&decls)
                    .unwrap_or_else(|| "<unknown>".to_string()),
            });
        }
        let gen =
            compile(&hirm, &self.native_libraries, self.positions.clone()).map_err(|error| {
                match error {
                    RunError::UnresolvedForeignSymbol(name) => {
                        ReloadError::UnresolvedForeignSymbol(name)
                    }
                    other => ReloadError::Internal(other.to_string()),
                }
            })?;
        // Both follow from the unchanged declaration hash; checked
        // rather than assumed, because getting either wrong would
        // corrupt a live Context instead of failing loudly.
        if gen.table.len() != self.table.len() {
            gen.discard();
            return Err(ReloadError::Internal(internal(
                "recompiled module has a different slot count",
            )));
        }
        if gen.globals_size != self.globals.size || gen.globals_align != self.globals.align {
            gen.discard();
            return Err(ReloadError::Internal(internal(
                "recompiled module has a different module-global layout",
            )));
        }
        // Everything that can fail has happened; from here the swap is
        // total. The old module stays alive so pre-swap code addresses
        // remain valid.
        self.modules.push(gen.module);
        self.table = gen.table;
        self.entries = gen.entries;
        self.async_runner = gen.async_runner;
        self.runner_error = hirm.runner_main().err();
        self.positions = gen.positions;
        unsafe {
            // SAFETY: ReloadSession retains the table and JIT modules until after Context drop (section 263).
            self.ctx.set_fn_table(self.table.as_ptr())
        };
        self.ctx.bump_reload_epoch();
        Ok(())
    }

    // compiler.md §272 rule 2: refuse entries before any script code runs.
    fn check_initializer(&self) -> Result<(), RunError> {
        if let Some(trap) = &self.initializer_trap {
            let mut trap = trap.clone();
            trap.message = format!("initializer trap: {}", trap.message);
            trap.stdout = [self.output_prefix.as_slice(), self.ctx.stdout_bytes()].concat();
            return Err(RunError::Trap(trap));
        }
        Ok(())
    }

    /// Turns a pending Context trap into a [`RunError::Trap`] with its
    /// TS position resolved through the current position table and the
    /// current stdout sink captured without draining the session.
    fn check_trap(&self) -> Result<(), RunError> {
        match self.ctx.trap_record() {
            None => Ok(()),
            Some(r) => Err(RunError::Trap(TrapReport {
                rule: r.kind,
                message: r.message.clone(),
                // §112 rule 1: the recorded id resolves through this
                // session's own table, and id 0 is its reserved entry.
                pos: self.positions.report_position(r.pos_id),
                stdout: [self.output_prefix.as_slice(), self.ctx.stdout_bytes()].concat(),
            })),
        }
    }
}

impl std::fmt::Debug for ReloadSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReloadSession")
            .field("generations", &self.modules.len())
            .field("slots", &self.table.len())
            .field("declaration_hash", &self.decls.value())
            .finish_non_exhaustive()
    }
}

impl Drop for ReloadSession {
    fn drop(&mut self) {
        // The Context must die before the code and data it may point
        // into: interned string literals live in module data.
        drop(std::mem::replace(&mut self.ctx, Context::new()));
        for module in std::mem::take(&mut self.modules) {
            // SAFETY: every execution has returned (a session is
            // dropped only between host calls) and the Context that
            // could hold pointers into this module's data is already
            // dropped.
            unsafe { module.free_memory() };
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod async_review_tests {
    use super::*;
    use subscript_runtime::TrapKind;
    mod programs {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/async_review.rs"
        ));
    }

    #[test]
    fn cleared_continuations_never_replay_or_leak() {
        for (source, expected, unfinished, line) in [
            (programs::BODY, "m1\nm2\n", 1, 6),
            (programs::CALLEE, "m1\nm2\nboom:start\n", 2, 4),
            (programs::SETTLED, "m1\nm2\n", 1, 7),
        ] {
            let mut session =
                ReloadSession::new(&[SourceFile::new("replay.ts", source)]).expect("session");
            session.call_main().expect("kick");
            let Err(RunError::Trap(trap)) = session.async_step() else {
                panic!("expected bounds trap")
            };
            assert_eq!(trap.rule, TrapKind::IndexOutOfBounds);
            assert_eq!(trap.message, "index 5 out of bounds for array length 0");
            assert_eq!((trap.pos.line, trap.pos.col), (line, 14));
            assert_eq!(session.take_output(), expected.as_bytes());
            assert_eq!(session.async_pending(), 1);
            assert_eq!(session.async_unfinished(), unfinished);
            let allocations = session.ctx.live_count();
            eprintln!("replay.ts:{line}:14 allocations={allocations} unfinished={unfinished}");
            for _ in 0..5 {
                let ctx = &mut *session.ctx as *mut Context;
                // SAFETY: the host owns the Context; no script is active.
                assert_eq!(
                    unsafe { subscript_runtime::ffi::subscript_rt_ctx_clear_trap(ctx) },
                    1
                );
                assert_eq!(session.async_pending(), 0);
                assert_eq!(session.async_step().expect("cleared checkpoint"), 0);
                assert_eq!(session.async_unfinished(), unfinished);
                assert_eq!(session.ctx.live_count(), allocations);
                assert_eq!(session.take_output(), b"");
            }
        }
    }

    #[test]
    fn clearance_preserves_blocked_waiters_and_advances_other_work() {
        let source = "async function faulty(): Promise<i32> {
  await Context.suspend();
  unreachable();
  return 1;
}
export async function main(): Promise<void> {
  const value: i32 = await faulty();
  print(\"blocked\");
}
export async function peer(): Promise<void> {
  await Context.suspend();
  Context.collect();
  print(\"peer\");
}
";
        let mut session =
            ReloadSession::new(&[SourceFile::new("blocked.ts", source)]).expect("session");
        session.call_main().expect("kick main");
        session.call_export("peer").expect("kick peer");
        assert!(matches!(session.async_step(), Err(RunError::Trap(_))));
        assert_eq!(
            (session.async_pending(), session.async_unfinished()),
            (2, 3)
        );
        session.ctx.clear_trap();
        assert_eq!(
            (session.async_pending(), session.async_unfinished()),
            (1, 3)
        );
        assert_eq!(session.async_step().expect("peer advances and collects"), 0);
        assert_eq!(session.async_unfinished(), 2);
        assert_eq!(session.take_output(), b"peer\n");
        assert_eq!(
            session.async_step().expect("blocked waiter stays blocked"),
            0
        );
        assert_eq!(session.async_unfinished(), 2);
    }

    #[test]
    fn cleared_checkpoint_control_advances_the_same_shape() {
        let mut session = ReloadSession::new(&[SourceFile::new("control.ts", programs::CONTROL)])
            .expect("session");
        session.call_main().expect("kick");
        assert_eq!(session.take_output(), b"m1\n");
        assert_eq!(session.async_step().expect("first step"), 1);
        assert_eq!(session.take_output(), b"m2\n");
        session.ctx.clear_trap();
        assert_eq!(session.async_step().expect("control resumes"), 0);
        assert_eq!(session.take_output(), b"m3\n");
        assert_eq!(session.async_unfinished(), 0);
    }
}

#[cfg(test)]
mod inspection_tests;
