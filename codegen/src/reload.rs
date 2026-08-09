//! Dev-tier hot reload (`specs/blocks/compiler.md` §1, §8.2).
//!
//! # Reload eligibility
//!
//! A swap is accepted only when the module's **declaration hash** is
//! unchanged. The hash covers every class (field names, types, and
//! order), every enum member value, every `FixedArray` shape (as part
//! of the type spelling it appears in), every module-level variable's
//! name and type, and every function signature. Function *bodies* are
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

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::FuncId;
use subscript_compiler::types::display_type;
use subscript_compiler::{
    check_program, hir, ClassId, Diagnostic, EnumId, Pos, SourceFile, StringAliasId, Type,
};
use subscript_runtime::Context;

use crate::jit::{register_runtime, RunError, TrapReport};
use crate::lower::{dev_flags, internal, lower_module_with, LowerOptions};
use crate::native::{missing_symbol, register_symbols};
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

/// Renders a type with the module's nominal names, so that a class or
/// enum rename, or a `FixedArray` length change, changes the hash.
fn ty_name(m: &hir::Module, ty: &Type) -> String {
    let class = |id: ClassId| {
        m.classes
            .get(id.0)
            .map_or_else(|| format!("<class #{}>", id.0), |c| c.name.clone())
    };
    let enum_ = |id: EnumId| {
        m.enums
            .get(id.0)
            .map_or_else(|| format!("<enum #{}>", id.0), |e| e.name.clone())
    };
    let string_alias = |id: StringAliasId| {
        m.string_aliases.get(id.0).map_or_else(
            || format!("<string alias #{}>", id.0),
            |alias| alias.name.clone(),
        )
    };
    display_type(ty, &class, &enum_, &string_alias)
}

/// Spells a function signature: name, parameter types in order, return
/// type, and the two shape bits that change the entry surface.
fn signature_text(m: &hir::Module, f: &hir::Function) -> String {
    let params: Vec<String> = f.params.iter().map(|p| ty_name(m, &p.ty)).collect();
    format!(
        "{}({}) -> {}{}{}{}",
        f.name,
        params.join(","),
        ty_name(m, &f.ret),
        if f.exported { " export" } else { "" },
        if f.is_generator { " generator" } else { "" },
        if f.is_async { " async" } else { "" }
    )
}

/// Computes the declaration fingerprint of a checked module (§8.2).
///
/// Covered: classes (kind, field names, types, and order), enum member
/// values, Q32 string-alias member spellings and order, module-level
/// variable names and types, and every function signature — free
/// functions, constructors, and methods. Not covered: any function body,
/// and therefore any expression, statement, or default-argument value
/// inside one.
#[must_use]
pub fn declaration_hash(m: &hir::Module) -> DeclarationHash {
    let mut entries: Vec<(String, u64)> = Vec::new();
    let mut push = |name: String, text: &str| entries.push((name, fnv1a(text.as_bytes())));

    for c in &m.classes {
        let fields: Vec<String> = c
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{i}:{}:{}", f.name, ty_name(m, &f.ty)))
            .collect();
        push(
            format!("class {}", c.name),
            &format!(
                "{}|{}|{}",
                c.name,
                if c.is_value { "value" } else { "reference" },
                fields.join(";")
            ),
        );
        if let Some(ctor) = &c.ctor {
            push(format!("constructor {}", c.name), &signature_text(m, ctor));
        }
        for method in &c.methods {
            push(
                format!("method {}.{}", c.name, method.name),
                &signature_text(m, method),
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
            format!("enum {}", e.name),
            &format!("{}|{}", e.name, members.join(";")),
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
            format!("string alias {}", alias.name),
            &format!("{}|{}|{wires}", alias.name, alias.members.join(";")),
        );
    }
    for g in &m.globals {
        push(
            format!("variable {}", g.name),
            &format!("{}:{}", g.name, ty_name(m, &g.ty)),
        );
    }
    for f in &m.functions {
        push(format!("function {}", f.name), &signature_text(m, f));
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
    modules: Vec<JITModule>,
    table: Vec<*const u8>,
    globals: GlobalBlock,
    entries: HashMap<String, usize>,
    positions: Vec<Pos>,
    decls: DeclarationHash,
    native_libraries: Vec<NativeLibrary>,
}

/// One compiled generation: the module plus what the driver needs from
/// it.
struct Generation {
    module: JITModule,
    table: Vec<*const u8>,
    entries: HashMap<String, usize>,
    init_slot: Option<usize>,
    positions: Vec<Pos>,
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

/// Compiles `hir` in reload mode into a fresh JIT module and resolves
/// every slot to a finalized code address.
fn compile(hirm: &hir::Module, libraries: &[NativeLibrary]) -> Result<Generation, RunError> {
    let flags = dev_flags().map_err(RunError::Internal)?;
    let isa = cranelift_native::builder()
        .map_err(|e| RunError::Internal(internal(format!("host ISA: {e}"))))
        .and_then(|b| {
            b.finish(flags)
                .map_err(|e| RunError::Internal(internal(format!("ISA flags: {e}"))))
        })?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    register_runtime(&mut builder);
    register_symbols(&mut builder, libraries);
    let mut module = JITModule::new(builder);

    // A failure past this point must release the module's code pages:
    // a dropped `JITModule` frees nothing by itself.
    let lowered = match lower_module_with(
        &mut module,
        hirm,
        LowerOptions {
            reload: true,
            require_main: false,
        },
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
        if let Some(i) = slot_index(e.id, &lowered.slots) {
            entries.insert(e.name.clone(), i);
        }
    }
    let table: Vec<*const u8> = lowered
        .slots
        .iter()
        .map(|s| match s {
            Some(id) => module.get_finalized_function(*id),
            None => std::ptr::null(),
        })
        .collect();

    Ok(Generation {
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
    type Entry = unsafe extern "C" fn(*mut Context);
    // The host boundary clears the trap record (§8.2): a trap ends the
    // call, not the session. Context state is untouched by the clear,
    // so a stale coroutine stays stale and a deleted allocation stays
    // deleted. Cleared here, with no generated code on the stack, so
    // the offset-0 flag can only be raised again by this call.
    ctx.clear_trap();
    ctx.enter_script();
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
        let hirm = check_program(files).map_err(RunError::Rejected)?;
        let decls = declaration_hash(&hirm);
        let gen = compile(&hirm, libraries)?;
        let globals = match GlobalBlock::new(gen.globals_size, gen.globals_align) {
            Ok(g) => g,
            Err(e) => {
                gen.discard();
                return Err(RunError::Internal(e));
            }
        };

        let mut session = ReloadSession {
            ctx: Context::new(),
            modules: vec![gen.module],
            table: gen.table,
            globals,
            entries: gen.entries,
            positions: gen.positions,
            decls,
            native_libraries: libraries.to_vec(),
        };
        session.ctx.set_fn_table(session.table.as_ptr());
        session.ctx.set_globals(session.globals.ptr);
        let init_slot = gen
            .init_slot
            .ok_or_else(|| RunError::Internal(internal("no initializer slot")))?;
        call_slot(&mut session.ctx, &session.table, init_slot).map_err(RunError::Internal)?;
        let trap = match session.check_trap() {
            Ok(()) => None,
            Err(RunError::Trap(trap)) => Some(trap),
            Err(error) => return Err(error),
        };
        Ok((session, trap))
    }

    /// The current declaration fingerprint.
    #[must_use]
    pub fn declaration_hash(&self) -> &DeclarationHash {
        &self.decls
    }

    /// Calls the exported `main(): void` when the session has one.
    /// Session creation does not require this entry.
    ///
    /// # Errors
    ///
    /// As [`ReloadSession::call_export`].
    pub fn call_main(&mut self) -> Result<(), RunError> {
        self.call_export("main")
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
        let slot = *self.entries.get(name).ok_or_else(|| {
            RunError::Internal(internal(format!(
                "`{name}` is not an exported zero-argument void function"
            )))
        })?;
        call_slot(&mut self.ctx, &self.table, slot).map_err(RunError::Internal)?;
        self.check_trap()
    }

    /// Number of suspended async roots currently owned by the live
    /// Context. Calling an async export kicks a root but does not pump it;
    /// reload-capable hosts retain the same explicit polling control as C
    /// hosts using `subscript_rt_ctx_async_pending`.
    #[must_use]
    pub fn async_pending(&self) -> usize {
        self.ctx.async_pending()
    }

    /// Polls every async root pending at call entry once, in kick order,
    /// and returns the number still pending.
    ///
    /// This does not clear a pending trap: the runtime's trapped-Context
    /// no-op rule remains observable. Old JIT generations are retained by
    /// the session, so callbacks queued before a reload remain callable and
    /// report the normal stale-coroutine trap.
    pub fn async_step(&mut self) -> Result<usize, RunError> {
        // SAFETY: the session retains every JIT module that can have placed
        // a callback in this Context's pending queue.
        let remaining = unsafe { self.ctx.async_step() };
        self.check_trap()?;
        Ok(remaining)
    }

    /// Takes the stdout bytes produced since the last take.
    #[must_use]
    pub fn take_output(&mut self) -> Vec<u8> {
        self.ctx.take_stdout()
    }

    /// Attempts to swap in the function bodies of `files`.
    ///
    /// Accepted when the declaration fingerprint is unchanged: the
    /// indirection table is repointed at the newly compiled bodies,
    /// Context state (globals, live allocations, sink) survives, and
    /// execution continues at the next host call. Coroutines suspended
    /// across the swap are invalidated and trap on resume; a trap does
    /// not end the session, so a swap after one is applied normally.
    ///
    /// A module-level variable's **initializer** is a body-position
    /// expression and is outside the declaration hash, so editing one
    /// is *accepted and has no effect*: initializers run in `subscript_init`,
    /// which executes once when the session starts and is never re-run
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
        if self.ctx.has_live_workers() {
            return Err(ReloadError::LiveWorkers);
        }
        let hirm = check_program(files).map_err(ReloadError::Rejected)?;
        let decls = declaration_hash(&hirm);
        if decls != self.decls {
            return Err(ReloadError::DeclarationChanged {
                declaration: self
                    .decls
                    .first_difference(&decls)
                    .unwrap_or_else(|| "<unknown>".to_string()),
            });
        }
        let gen = compile(&hirm, &self.native_libraries).map_err(|error| match error {
            RunError::UnresolvedForeignSymbol(name) => ReloadError::UnresolvedForeignSymbol(name),
            other => ReloadError::Internal(other.to_string()),
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
        self.positions = gen.positions;
        self.ctx.set_fn_table(self.table.as_ptr());
        self.ctx.bump_reload_epoch();
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
                pos: self
                    .positions
                    .get(r.pos_id as usize)
                    .cloned()
                    .unwrap_or_else(|| Pos::new(String::new(), 0, 0)),
                stdout: self.ctx.stdout_bytes().to_vec(),
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
mod tests {
    use super::*;
    use subscript_runtime::TrapKind;

    fn src(text: &str) -> Vec<SourceFile> {
        vec![SourceFile::new("live.ts", text)]
    }

    fn hash_of(text: &str) -> DeclarationHash {
        let m = check_program(&src(text)).expect("checks");
        declaration_hash(&m)
    }

    #[test]
    fn declaration_hash_ignores_bodies() {
        let a = hash_of("export function main(): void {\n  print(\"a\");\n}\n");
        let b = hash_of("export function main(): void {\n  print(\"b\");\n  print(\"c\");\n}\n");
        assert_eq!(a, b);
        assert_eq!(a.first_difference(&b), None);
    }

    #[test]
    fn declaration_hash_is_stable_across_recompiles() {
        let text = "class C { x: i32; constructor() { this.x = 1; } }\nlet n: i32 = 0;\nexport function main(): void {\n  n += 1;\n}\n";
        assert_eq!(hash_of(text).value(), hash_of(text).value());
    }

    #[test]
    fn declaration_hash_changes_for_a_new_field() {
        let a = hash_of(
            "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {}\n",
        );
        let b = hash_of("class C { x: i32; y: i32; constructor() { this.x = 1; this.y = 2; } }\nexport function main(): void {}\n");
        assert_ne!(a, b);
        assert_eq!(a.first_difference(&b).as_deref(), Some("class C"));
    }

    #[test]
    fn declaration_hash_changes_for_field_reordering() {
        let a = hash_of("@CStruct\nclass V { x: i32; y: f32; constructor(x: i32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void {}\n");
        let b = hash_of("@CStruct\nclass V { y: f32; x: i32; constructor(x: i32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void {}\n");
        assert_ne!(a, b);
    }

    #[test]
    fn declaration_hash_changes_for_enum_values_and_fixed_array_shapes() {
        let a = hash_of("enum E { A = 1, B = 2 }\nexport function main(): void {}\n");
        let b = hash_of("enum E { A = 1, B = 3 }\nexport function main(): void {}\n");
        assert_ne!(a, b);
        assert_eq!(a.first_difference(&b).as_deref(), Some("enum E"));

        let c =
            hash_of("let g: FixedArray<i32, 4> = [0, 0, 0, 0];\nexport function main(): void {}\n");
        let d = hash_of(
            "let g: FixedArray<i32, 5> = [0, 0, 0, 0, 0];\nexport function main(): void {}\n",
        );
        assert_ne!(c, d);
        assert_eq!(c.first_difference(&d).as_deref(), Some("variable g"));
    }

    #[test]
    fn declaration_hash_changes_for_a_signature_edit() {
        let a =
            hash_of("function f(x: i32): i32 {\n  return x;\n}\nexport function main(): void {}\n");
        let b = hash_of(
            "function f(x: i64): i32 {\n  return x as i32;\n}\nexport function main(): void {}\n",
        );
        assert_ne!(a, b);
        assert_eq!(a.first_difference(&b).as_deref(), Some("function f"));
    }

    #[test]
    fn declaration_hash_includes_map_and_set_type_arguments() {
        let map_i32 = hash_of(
            "let values: Map<i32, i32> = new Map<i32, i32>();\n\
             export function main(): void { print(`${values.size}`); }\n",
        );
        let map_i64 = hash_of(
            "let values: Map<i64, i32> = new Map<i64, i32>();\n\
             export function main(): void { print(`${values.size}`); }\n",
        );
        assert_ne!(map_i32, map_i64);
        assert_eq!(
            map_i32.first_difference(&map_i64).as_deref(),
            Some("variable values")
        );

        let set_i32 = hash_of(
            "let values: Set<i32> = new Set<i32>();\n\
             export function main(): void { print(`${values.size}`); }\n",
        );
        let set_i64 = hash_of(
            "let values: Set<i64> = new Set<i64>();\n\
             export function main(): void { print(`${values.size}`); }\n",
        );
        assert_ne!(set_i32, set_i64);
    }

    #[test]
    fn declarations_are_listed_in_declaration_order() {
        let h = hash_of("class C { x: i32; constructor() { this.x = 0; } }\nlet g: i32 = 1;\nexport function main(): void {}\n");
        assert_eq!(
            h.declarations(),
            vec!["class C", "constructor C", "variable g", "function main"]
        );
    }

    #[test]
    fn session_runs_and_reports_output() {
        let mut s = ReloadSession::new(&src(
            "export function main(): void {\n  print(\"one\");\n}\n",
        ))
        .expect("session");
        s.call_main().expect("call");
        assert_eq!(s.take_output(), b"one\n");
        assert_eq!(s.take_output(), b"");
    }

    #[test]
    fn session_reports_a_trap_without_killing_the_host() {
        let mut s = ReloadSession::new(&src(
            "export function main(): void {\n  const xs: i32[] = [];\n  xs.pop();\n}\n",
        ))
        .expect("session");
        match s.call_main() {
            Err(RunError::Trap(t)) => assert_eq!(t.rule, TrapKind::EmptyPop),
            other => panic!("expected a trap, got {other:?}"),
        }
    }

    #[test]
    fn suspended_async_frame_is_stale_after_reload() {
        let before = "export async function main(): Promise<void> {\n\
                      \x20 print(\"before\");\n\
                      \x20 await Context.suspend();\n\
                      \x20 print(\"old continuation\");\n\
                      }\n";
        let after = "export async function main(): Promise<void> {\n\
                     \x20 print(\"before\");\n\
                     \x20 await Context.suspend();\n\
                     \x20 print(\"new continuation\");\n\
                     }\n";
        let mut session = ReloadSession::new(&src(before)).expect("session");
        session.call_main().expect("async kick");
        assert_eq!(session.take_output(), b"before\n");
        assert_eq!(session.ctx.async_pending(), 1);
        session.reload(&src(after)).expect("body-only reload");

        // SAFETY: the session retains every JIT generation, including the
        // callback queued by the pre-reload async wrapper.
        assert_eq!(unsafe { session.ctx.async_step() }, 1);
        match session.check_trap() {
            Err(RunError::Trap(trap)) => assert_eq!(trap.rule, TrapKind::StaleCoroutine),
            other => panic!("expected stale async-frame trap, got {other:?}"),
        }
    }

    #[test]
    fn reload_is_refused_while_the_context_has_a_live_worker() {
        let source = "class Message { value: i32 = 0; }\n\
                      function blocked(inbox: Inbox<Message>, outbox: Outbox<Message>): void {\n\
                      \x20 const message: Message | null = inbox.wait();\n\
                      }\n\
                      export function main(): void {\n\
                      \x20 const worker: Worker<Message, Message> = Worker.spawn(blocked);\n\
                      }\n";
        let mut session = ReloadSession::new(&src(source)).expect("worker reload session");
        session.call_main().expect("spawn live worker");
        assert!(matches!(
            session.reload(&src(source)),
            Err(ReloadError::LiveWorkers)
        ));
    }

    #[test]
    fn reload_mode_worker_echo_round_trip_completes_without_a_trap() {
        let source = "class Message {\n\
                      \x20 value: i32;\n\
                      \x20 constructor(value: i32) { this.value = value; }\n\
                      }\n\
                      function echo(inbox: Inbox<Message>, outbox: Outbox<Message>): void {\n\
                      \x20 const message: Message | null = inbox.wait();\n\
                      \x20 if (message !== null) { outbox.post(message); }\n\
                      }\n\
                      export function main(): void {\n\
                      \x20 const worker: Worker<Message, Message> = Worker.spawn(echo);\n\
                      \x20 worker.post(new Message(37));\n\
                      \x20 worker.close();\n\
                      \x20 worker.join();\n\
                      \x20 const reply: Message | null = worker.poll();\n\
                      \x20 if (reply !== null) { print(`echo=${reply.value}`); }\n\
                      }\n";
        let mut session = ReloadSession::new(&src(source)).expect("worker reload session");
        session.call_main().expect("reload-mode worker round trip");
        assert_eq!(session.take_output(), b"echo=37\n");
        assert!(session.ctx.trap_record().is_none());
        assert!(!session.ctx.has_live_workers());
    }

    #[test]
    fn reload_session_declares_context_before_jit_modules() {
        let source = include_str!("reload.rs");
        let fields = source
            .split_once("pub struct ReloadSession {")
            .expect("ReloadSession declaration")
            .1
            .split_once('}')
            .expect("ReloadSession field list")
            .0;
        let context = fields.find("ctx: Box<Context>").expect("Context field");
        let modules = fields
            .find("modules: Vec<JITModule>")
            .expect("JIT modules field");
        assert!(
            context < modules,
            "ReloadSession must drop its Context, which joins workers, before JIT modules"
        );
    }

    #[test]
    fn initializer_trap_can_be_captured_without_dropping_the_session() {
        let (mut s, trap) =
            ReloadSession::new_capturing_initializer_trap(&src("let xs: i32[] = [];\n\
             let value: i32 = xs.pop();\n\
             export function main(): void {\n\
             \x20 print(\"still live\");\n\
             }\n"))
            .expect("session");
        assert_eq!(trap.map(|report| report.rule), Some(TrapKind::EmptyPop));
        assert!(s.take_output().is_empty());

        s.call_main().expect("call after initializer trap");
        assert_eq!(s.take_output(), b"still live\n");
    }

    #[test]
    fn unknown_entry_is_an_internal_error() {
        let mut s = ReloadSession::new(&src("export function main(): void {}\n")).expect("session");
        assert!(matches!(s.call_export("nope"), Err(RunError::Internal(_))));
    }

    #[test]
    fn reload_of_unchecked_sources_is_rejected() {
        let mut s = ReloadSession::new(&src("export function main(): void {}\n")).expect("session");
        assert!(matches!(
            s.reload(&src("const x: number = 1;\n")),
            Err(ReloadError::Rejected(_))
        ));
    }

    #[test]
    fn errors_render_their_cause() {
        let e = ReloadError::DeclarationChanged {
            declaration: "class C".to_string(),
        };
        assert!(e.to_string().contains("class C"));
        assert!(ReloadError::ScriptOnStack.to_string().contains("stack"));
        assert!(ReloadError::LiveWorkers
            .to_string()
            .contains("live workers"));
    }

    #[test]
    fn debug_shows_generations_and_slots() {
        let s = ReloadSession::new(&src("export function main(): void {}\n")).expect("session");
        let text = format!("{s:?}");
        assert!(text.contains("generations"), "got {text}");
    }
}
