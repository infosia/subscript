//! Typed-HIR-to-C emitter — the **ship tier** (`specs/blocks/compiler.md`
//! §11, plan §8 Rev 2).
//!
//! P4 measured Cranelift ship-AOT at ~23× a hand-written C baseline and
//! attributed the bulk of the gap to Cranelift's scalar output; P4.2
//! emitted C from the same typed HIR and measured it at ~1.05×. This
//! module is the P4.2 spike extended from a22's subset to the full run
//! set a01–a24 and made the ship tier: the dev tier stays Cranelift JIT
//! with hot reload, the ship tier is HIR→C→`clang -O2`.
//!
//! # Reuse of the runtime
//!
//! The emitted translation unit **links the existing runtime static
//! library** rather than re-implementing runtime logic in C. Every
//! array, string, formatting, allocation, and trap operation is the
//! same `subscript_rt_*` C-ABI entry point the CLIF lowering calls, so array
//! growth, string content, Q14 shortest-round-trip formatting, and trap
//! reporting are byte-for-byte identical to the dev-JIT tier by
//! construction rather than by replication. (The P4.2 spike was
//! self-contained purely for measurement isolation.) The emitted unit
//! exports the same host-entry surface the AOT object does — `subscript_init`
//! and `subscript_export_<name>` taking the Context — so it is a drop-in
//! subject for the standing gate and for the device-triple link, linked
//! with the same [`crate::AOT_ENTRY_C`] host entry.
//!
//! # Semantic faithfulness
//!
//! The emitter mirrors the CLIF lowering's semantics, not a hand
//! optimization; where the emitted C and the CLIF path could differ the
//! CLIF path (and the runtime) is the reference:
//!
//! - **C2 value-class copy semantics.** A `@CStruct class` is a C
//!   `struct`, passed and returned by value and copied on assignment —
//!   C's own struct-value semantics reproduce copy-on-assign/pass/return
//!   without any explicit copy. `FixedArray<T, N>` is a `struct { T a[N];
//!   }` wrapper so it, too, has value semantics; its C-ABI layout is
//!   identical to the bare array (design invariant 1).
//! - **Reference classes** are Context allocations (`subscript_rt_alloc`);
//!   their handle is the payload pointer, and fields are read/written
//!   through a `struct` view of the payload (the same C-ABI layout the
//!   runtime allocates). `Context.free` is `subscript_rt_delete`, `Context.collect()`
//!   is `subscript_rt_collect`.
//! - **Checked growable `T[]`** is the runtime's array: `subscript_rt_array_*`
//!   for `new`/`push`/`pop`/`length`/indexing, so bounds checks, push
//!   growth, and OOB traps match the runtime exactly.
//! - **`FixedArray` in-place with the P4.1 proof-based bounds-check
//!   elimination.** An index proven in `[0, N)` by the same interval /
//!   induction analysis the CLIF path uses is a plain unchecked `a[i]`;
//!   an unproven index keeps a checked access that traps.
//! - **f32 stays f32.** Float locals/expressions are `float`; f32
//!   literals carry the `f` suffix and are printed in shortest *f32*
//!   form so the C constant round-trips with a single rounding. Compiled
//!   `-ffp-contract=off` to match the language, which never contracts a
//!   multiply-add.
//! - **Q14 formatting** is the runtime's (`subscript_rt_fmt_*`), so an f32
//!   checksum prints the same bytes both tiers.
//! - **Trap model.** Fault-capable calls are followed by an inline read
//!   of the Context trap flag; a set flag unwinds generated C frames and
//!   is reported by the host entry without aborting the host, matching
//!   the dev tier.
//!
//! # Scope
//!
//! The emitter handles every construct the run set a01–a24 uses:
//! reference and value classes, methods and constructors, `Nullable`
//! and null narrowing, non-capturing function values and non-escaping
//! capturing lambdas, generators (CPS state machine), enums, growable
//! and fixed arrays, strings (length / slice / concat / compare /
//! interpolation), `while` / `for` / `switch` / `if` / ternary, and
//! default parameters. A construct outside the run set is reported as a
//! clean `Err` until a corpus entry needs it (§11).
//!
//! # A note on the GC root discipline
//!
//! The CLIF path registers module-global roots and per-call shadow
//! frames so `Context.collect()` can see live handles. The emitted C does not
//! replicate that discipline: no run-set entry observably depends on it
//! (the only `Context.collect()` in the corpus, a16, collects an allocation that
//! is already dead, and interned string literals are rooted by the
//! runtime itself), and the standing gate — byte-identity across all 24
//! entries on both tiers — is the oracle. Should a future entry make
//! rooting observable, this is where it is added.

use std::collections::HashSet;
use std::fmt::Write as _;

use subscript_compiler::hir;
use subscript_compiler::types::{ClassId, FuncType, Type, MAX_AGGREGATE_BYTES};
use subscript_compiler::Pos;
use subscript_runtime::context as rtc;
use subscript_runtime::TrapKind;

use crate::layout::{is_managed, managed_words, Layouts};
use crate::lower::is_host_callable_export;
use crate::trap_sites::{lower_trap_sites, TrapSiteConsumer};

fn checked_shadow_words(left: u32, right: u32) -> Result<u32, String> {
    let words = left
        .checked_add(right)
        .ok_or_else(|| "C emitter shadow-frame word count overflows u32".to_string())?;
    let maximum = MAX_AGGREGATE_BYTES / 8;
    if words <= maximum {
        Ok(words)
    } else {
        Err(format!(
            "C emitter shadow frame exceeds the supported aggregate limit of \
             {MAX_AGGREGATE_BYTES} bytes"
        ))
    }
}

/// An emitted C translation unit plus the trap position table its
/// `pos_id` arguments index (mirrors [`crate::AotObject::positions`]),
/// so a trap the linked program reports can be resolved back to a TS
/// position by the driver.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CProgram {
    /// The C source text.
    pub source: String,
    /// Trap position table: `pos_id` -> TS position.
    pub positions: Vec<Pos>,
    /// Generated C declarations for the allocation class-id and
    /// source-position tables defined by [`CProgram::source`].
    pub allocation_metadata_header: String,
    /// The generated table-definition section embedded byte-for-byte in
    /// [`CProgram::source`].
    pub allocation_metadata_source: String,
    pub(crate) foreign_symbols: Vec<String>,
}

/// Emits a C translation unit for a checked HIR module (§11).
///
/// The unit exports `subscript_init(subscript_rt_context* ctx)` and one
/// `subscript_export_<name>` wrapper for each host-callable export, imports the
/// runtime's `subscript_rt_*` entry points, and is linked with the runtime
/// static library and [`crate::AOT_ENTRY_C`].
///
/// # Errors
///
/// Returns an error string when the module uses an HIR construct outside
/// the run set's scope, or has no exported `main(): void`.
pub fn emit_c(module: &hir::Module) -> Result<CProgram, String> {
    Emitter::new(module)?.emit(true)
}

/// Emits a C translation unit for a host-owned entry program.
///
/// This has the same output as [`emit_c`], but permits a module with no
/// exported `main(): void`. The translation unit still defines `subscript_init`
/// and every host-callable export as `subscript_export_<name>`; the caller
/// supplies `main` and chooses which exports to drive.
///
/// # Errors
///
/// Returns an error string when the module uses an HIR construct outside
/// the run set's scope.
pub fn emit_c_without_main(module: &hir::Module) -> Result<CProgram, String> {
    Emitter::new(module)?.emit(false)
}

// ----- generator frame planning -----

/// Collects the types of every `let` reachable in `stmts`, in the
/// pre-order the emitter descends them, so a generator frame lays its
/// locals out in the same order the emission consumes them.
fn walk_lets<'h>(stmts: &'h [hir::Stmt], out: &mut Vec<(&'h str, &'h Type)>) {
    for s in stmts {
        match s {
            hir::Stmt::Let { name, ty, .. } => out.push((name, ty)),
            hir::Stmt::If { then, els, .. } => {
                walk_lets(then, out);
                if let Some(e) = els {
                    walk_lets(e, out);
                }
            }
            hir::Stmt::While { body, .. } => walk_lets(body, out),
            hir::Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    walk_lets(std::slice::from_ref(&**i), out);
                }
                walk_lets(body, out);
            }
            hir::Stmt::ForOf { name, ty, body, .. } => {
                out.push((name, ty));
                walk_lets(body, out);
            }
            hir::Stmt::Switch { cases, .. } => {
                for c in cases {
                    walk_lets(&c.body, out);
                }
            }
            hir::Stmt::Block(b) => walk_lets(b, out),
            _ => {}
        }
    }
}

// ----- the emitter -----

/// C `this` context of the current function body.
#[derive(Clone, Copy)]
enum ThisCtx {
    /// Not in a constructor or method.
    None,
    /// Value-class constructor: `_this` is a `Sub` struct lvalue that is
    /// built and returned by value.
    ValueLValue,
    /// Value-class method: `_this` is a `Sub*` pointing at the receiver's
    /// storage (C2 — a mutating value method mutates the receiver, so the
    /// receiver is passed by pointer exactly as the CLIF path does).
    ValuePtr,
    /// Reference-class constructor/method: `_this` is a `void*` handle.
    Reference,
}

impl ThisCtx {
    /// The C expression denoting `this` as a value in this context.
    fn this_expr(self) -> Result<&'static str, String> {
        Ok(match self {
            ThisCtx::None => return Err("`this` outside a constructor or method".to_string()),
            ThisCtx::ValueLValue | ThisCtx::Reference => "_this",
            ThisCtx::ValuePtr => "(*_this)",
        })
    }
}

/// State of the generator being lowered (CPS state machine).
struct GenState {
    /// Whether this frame is a generator or an async function.
    kind: FrameKind,
    /// Yield sites seen so far (resume-label counter).
    yields: u32,
    /// Cursor into the frame's `let` fields, consumed in emission order.
    let_cursor: usize,
    /// Frame field name for each `let`, in emission order.
    let_fields: Vec<String>,
    /// C type of the yielded value.
    yield_ct: String,
    /// Cursor into child-frame pointer fields used by direct async calls.
    child_cursor: usize,
    /// Frame field name for each direct async call, in emission order.
    child_fields: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Generator,
    Async,
}

/// C temporaries already materialized for one HIR trap site.
enum TrapOperand {
    Pending,
    Value(String),
    Condition(String),
    DynamicIndex { handle: String, index: String },
    WireValue { wire: String, valid: String },
}

/// Post-call copy-back for a pointer-passed string-field boundary struct.
/// The two names are C temporaries emitted while marshaling the argument.
struct BoundaryPtrWriteback {
    cid: ClassId,
    source: String,
    scratch: String,
}

struct Emitter<'m> {
    module: &'m hir::Module,
    /// C-ABI layouts, shared with the CLIF path: used for the exact same
    /// managed-word counts so the shadow frame the collector scans has
    /// the identical shape (M1).
    layouts: Layouts,
    /// `this` context of the current function.
    this: ThisCtx,
    /// Temporary receiver while evaluating a Q33 descriptor member
    /// default at its construction site.
    descriptor_this: Option<String>,
    /// Generator lowering state, when inside a generator resume body.
    gen: Option<GenState>,
    /// Map from a source local name to a generator frame-field access
    /// expression (`f->g0_x`), innermost scope last.
    gen_locals: Vec<(String, String)>,
    /// Declared type of each in-scope local/parameter of the current
    /// function, innermost last, so a lambda can emit the exact C type
    /// of each captured local (C2).
    local_types: Vec<(String, Type)>,
    /// Shadow-frame access expression of each rooted (managed, or
    /// managed-interior aggregate) local or parameter of the current
    /// function, innermost last (M1: the collector scans the frame, so a
    /// live handle held in one survives `Context.collect()`).
    managed_scope: Vec<(String, String)>,
    /// Next managed-`let` shadow slot to assign in emission order.
    shadow_cursor: u32,
    /// True when the current function pushed a shadow frame (so its exits
    /// pop it).
    has_shadow: bool,
    /// Source-level return type of the current emitted function.
    current_ret: Type,
    /// Trap position table.
    positions: Vec<Pos>,
    /// Fresh-temporary counter.
    tmp: u32,
    /// Fresh-label counter.
    label: u32,
    /// Lambda counter.
    lambda: u32,
    /// Prototype lines emitted ahead of every definition.
    protos: String,
    /// Helper definitions (lambdas, wrappers) emitted before the bodies.
    helpers: String,
    /// Names of function-reference wrappers already emitted.
    wrappers: HashSet<String>,
    /// Aggregate typedefs already emitted, by C name.
    emitted_types: HashSet<String>,
    /// Break/continue targets of the enclosing loops and switches, as
    /// (break label, optional continue label) pairs.
    loops: Vec<(String, Option<String>)>,
    /// Map/Set traversals active at the current emission point.
    assoc_iters: Vec<String>,
    /// Foreign C symbols called by the emitted program, in first-use
    /// order.
    foreign_symbols: Vec<String>,
}

impl<'m> Emitter<'m> {
    fn new(module: &'m hir::Module) -> Result<Emitter<'m>, String> {
        Ok(Emitter {
            module,
            layouts: Layouts::build(module)?,
            this: ThisCtx::None,
            descriptor_this: None,
            gen: None,
            gen_locals: Vec::new(),
            local_types: Vec::new(),
            managed_scope: Vec::new(),
            shadow_cursor: 0,
            has_shadow: false,
            current_ret: Type::Void,
            positions: Vec::new(),
            tmp: 0,
            label: 0,
            lambda: 0,
            protos: String::new(),
            helpers: String::new(),
            wrappers: HashSet::new(),
            emitted_types: HashSet::new(),
            loops: Vec::new(),
            assoc_iters: Vec::new(),
            foreign_symbols: Vec::new(),
        })
    }

    fn pos_id(&mut self, pos: &Pos) -> u32 {
        self.positions.push(pos.clone());
        (self.positions.len() - 1) as u32
    }

    fn fresh_tmp(&mut self) -> String {
        let n = self.tmp;
        self.tmp += 1;
        format!("_t{n}")
    }

    fn fresh_label(&mut self) -> String {
        let n = self.label;
        self.label += 1;
        format!("_L{n}")
    }

    fn current_this_expr(&self) -> Result<String, String> {
        self.descriptor_this
            .clone()
            .map_or_else(|| self.this.this_expr().map(str::to_string), Ok)
    }

    // ----- class / type naming -----

    fn class(&self, id: ClassId) -> Result<&'m hir::ClassDef, String> {
        self.module
            .classes
            .get(id.0)
            .ok_or_else(|| format!("class id {} out of range", id.0))
    }

    fn class_name(&self, id: ClassId) -> Result<String, String> {
        let c = self.class(id)?;
        Ok(format!("Sub_{}_{}", id.0, sanitize(&c.name)))
    }

    fn is_value_class(&self, id: ClassId) -> Result<bool, String> {
        Ok(self.class(id)?.is_value)
    }

    /// C type for a value of `ty` (as a variable, parameter, field, or
    /// return). Aggregates (value classes, `FixedArray`, `IterResult`,
    /// function values) get their own named struct types.
    fn ctype(&self, ty: &Type) -> Result<String, String> {
        Ok(match ty {
            Type::I8 => "int8_t".to_string(),
            Type::U8 => "uint8_t".to_string(),
            Type::I16 => "int16_t".to_string(),
            Type::U16 | Type::F16 => "uint16_t".to_string(),
            Type::I32 => "int32_t".to_string(),
            Type::U32 => "uint32_t".to_string(),
            Type::I64 => "int64_t".to_string(),
            Type::U64 => "uint64_t".to_string(),
            // A Date is its i64 epoch-millisecond value (stdlib.md §3).
            Type::Date => "int64_t".to_string(),
            Type::F32 => "float".to_string(),
            Type::F64 => "double".to_string(),
            Type::Bool => "int32_t".to_string(),
            Type::Enum(_) | Type::StringAlias(_) => "int32_t".to_string(),
            Type::Void => "void".to_string(),
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Worker(..)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Generator(_)
            | Type::Nullable(_)
            | Type::Null => "void*".to_string(),
            Type::Func(_) => "SubFn".to_string(),
            Type::Class(id) => {
                if self.is_value_class(*id)? {
                    self.class_name(*id)?
                } else {
                    "void*".to_string()
                }
            }
            Type::FixedArray(elem, n) => self.fixed_array_name(elem, *n)?,
            Type::IterResult(v) => self.iter_result_name(v)?,
            other => return Err(format!("type {other:?} is outside the run set's scope")),
        })
    }

    fn fixed_array_name(&self, elem: &Type, n: u32) -> Result<String, String> {
        Ok(format!("FA_{}_{n}", self.type_tag(elem)?))
    }

    fn iter_result_name(&self, value: &Type) -> Result<String, String> {
        Ok(format!("IR_{}", self.type_tag(value)?))
    }

    /// A short identifier fragment uniquely naming a type, for building
    /// aggregate typedef names.
    fn type_tag(&self, ty: &Type) -> Result<String, String> {
        Ok(match ty {
            Type::I8 => "i8".to_string(),
            Type::U8 => "u8".to_string(),
            Type::I16 => "i16".to_string(),
            Type::U16 => "u16".to_string(),
            Type::F16 => "f16".to_string(),
            Type::I32 => "i32".to_string(),
            Type::U32 => "u32".to_string(),
            Type::I64 => "i64".to_string(),
            Type::U64 => "u64".to_string(),
            Type::Date => "date".to_string(),
            Type::F32 => "f32".to_string(),
            Type::F64 => "f64".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Enum(id) => format!("enum{}", id.0),
            Type::StringAlias(id) => format!("stralias{}", id.0),
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Worker(..)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Generator(_)
            | Type::Nullable(_)
            | Type::Null => "ptr".to_string(),
            Type::Func(_) => "fn".to_string(),
            Type::Class(id) => {
                if self.is_value_class(*id)? {
                    format!("c{}", id.0)
                } else {
                    "ptr".to_string()
                }
            }
            Type::FixedArray(elem, n) => format!("FA{}x{n}", self.type_tag(elem)?),
            Type::IterResult(v) => format!("IR{}", self.type_tag(v)?),
            other => return Err(format!("type tag for {other:?}")),
        })
    }

    // ----- top-level -----

    fn emit(&mut self, require_main: bool) -> Result<CProgram, String> {
        // Validate the entry point exists (mirrors lower_module_with).
        let has_main = self.module.functions.iter().any(|f| {
            f.name == "main"
                && f.exported
                && !f.is_generator
                && f.params.is_empty()
                && f.ret == Type::Void
        });
        if require_main && !has_main {
            return Err("no exported `main(): void` entry point".to_string());
        }

        // Aggregate type definitions, in dependency order.
        let mut typedefs = String::new();
        self.emit_type_definitions(&mut typedefs)?;

        // Module state. The one layout-fixed block is allocated per
        // Context by subscript_init; only immutable lookup data remains
        // process-wide.
        let mut globals = String::new();
        globals.push_str("typedef struct SubscriptModuleGlobals {\n");
        if self.module.globals.is_empty() {
            globals.push_str("    unsigned char subscript_empty;\n");
        } else {
            for g in &self.module.globals {
                let _ = writeln!(
                    globals,
                    "    {} g_{};",
                    self.ctype(&g.ty)?,
                    sanitize(&g.name)
                );
            }
        }
        globals.push_str("} SubscriptModuleGlobals;\n");
        let globals_offset = rtc::Context::globals_offset();
        let _ = writeln!(
            globals,
            "static inline SubscriptModuleGlobals* subscript_globals(void* ctx) {{\n\
             \x20   SubscriptModuleGlobals* globals;\n\
             \x20   memcpy(&globals, (const unsigned char*)ctx + {globals_offset}u, sizeof globals);\n\
             \x20   return globals;\n\
             }}"
        );
        if !self.module.string_aliases.is_empty() {
            globals.push_str(
                "typedef struct {\n\
                 \x20   const unsigned char* data;\n\
                 \x20   uint64_t len;\n\
                 } SubStringAliasMember;\n",
            );
            for (alias_index, alias) in self.module.string_aliases.iter().enumerate() {
                let _ = writeln!(
                    globals,
                    "static const SubStringAliasMember subscript_string_alias_{alias_index}[] = {{"
                );
                for member in &alias.members {
                    let _ = writeln!(
                        globals,
                        "    {{ (const unsigned char*){}, {}ull }},",
                        c_string_literal(member.as_bytes()),
                        member.len()
                    );
                }
                globals.push_str("};\n");
            }
        }
        // Bodies (which append prototypes and helper definitions as they
        // discover lambdas and function-reference wrappers).
        let mut bodies = String::new();

        // Prototypes for every constructor, method, and free function.
        for (ci, c) in self.module.classes.iter().enumerate() {
            if c.ctor.is_some() {
                let proto = self.ctor_signature(ci, c)?;
                let _ = writeln!(self.protos, "{proto};");
            }
            for m in &c.methods {
                if m.is_async {
                    let creator = self.async_method_creator_signature(ci, m)?;
                    let resume = self.async_method_resume_signature(ci, m);
                    let _ = writeln!(self.protos, "{creator};");
                    let _ = writeln!(self.protos, "{resume};");
                } else {
                    let proto = self.method_signature(ci, m)?;
                    let _ = writeln!(self.protos, "{proto};");
                }
            }
        }
        for f in &self.module.functions {
            if f.is_generator || f.is_async {
                let cp = self.gen_creator_signature(f)?;
                let rp = self.gen_resume_signature(f)?;
                let _ = writeln!(self.protos, "{cp};");
                let _ = writeln!(self.protos, "{rp};");
            } else {
                let proto = self.fn_signature(f)?;
                let _ = writeln!(self.protos, "{proto};");
            }
        }
        let _ = writeln!(
            self.protos,
            "void subscript_init(subscript_rt_context* ctx);"
        );
        for index in 0..self.module.worker_entries.len() {
            self.emit_worker_entry_adapter(index)?;
        }

        // Definitions.
        for (ci, c) in self.module.classes.iter().enumerate() {
            if c.ctor.is_some() {
                self.emit_constructor(&mut bodies, ci, c)?;
            }
            for m in &c.methods {
                if m.is_async {
                    self.emit_async_method(&mut bodies, ci, m)?;
                } else {
                    self.emit_method(&mut bodies, ci, m)?;
                }
            }
        }
        for f in &self.module.functions {
            if f.is_generator {
                self.emit_generator(&mut bodies, f)?;
            } else if f.is_async {
                self.emit_async(&mut bodies, f)?;
            } else {
                self.emit_function(&mut bodies, f)?;
            }
        }

        // Module initializer and exported entry surface.
        self.emit_init(&mut bodies)?;
        self.emit_exports(&mut bodies)?;

        let mut out = String::new();
        out.push_str(PREAMBLE);
        // Each bound mirror supplies its own C include spelling (§23.4).
        // The HIR preserves ingestion order; no header is inferred from a
        // foreign symbol or language type.
        if !self.module.foreign_fns.is_empty() && self.module.foreign_mirrors.is_empty() {
            return Err(
                "internal error at foreign C preamble: foreign functions have no mirror provenance"
                    .to_string(),
            );
        }
        for foreign in &self.module.foreign_fns {
            if self.module.foreign_mirrors.get(foreign.mirror.0).is_none() {
                return Err(format!(
                    "internal error at foreign function `{}`: mirror provenance index {} is missing",
                    foreign.name, foreign.mirror.0
                ));
            }
        }
        for mirror in &self.module.foreign_mirrors {
            if mirror.include.contains('"') {
                return Err(format!(
                    "internal error at mirror `{}`: header provenance cannot be written as a C include",
                    mirror.source_name
                ));
            }
            let _ = writeln!(out, "#include \"{}\"", mirror.include);
        }
        if !self.module.foreign_mirrors.is_empty() {
            out.push_str(
                "/* The runtime callback view and every bound string view have the same\n\
                 \x20* C ABI layout. That layout identity makes the later function-pointer\n\
                 \x20* cast to a header callback typedef sound. */\n\
                 typedef struct subscript_callback_string_view {\n\
                 \x20   const uint8_t* data;\n\
                 \x20   size_t len;\n\
                 } subscript_callback_string_view;\n\
                 extern void subscript_rt_cb_trampoline(subscript_callback_string_view message, void* userdata1, void* userdata2);\n\n",
            );
        }
        out.push_str(&typedefs);
        out.push('\n');
        out.push_str(&globals);
        out.push('\n');
        out.push_str(&self.protos);
        out.push('\n');
        out.push_str(&self.helpers);
        out.push_str(&bodies);

        let positions = std::mem::take(&mut self.positions);
        let allocation_metadata_source =
            render_allocation_metadata_definitions(self.module, &positions);
        out.push_str(&allocation_metadata_source);
        let allocation_metadata_header = render_allocation_metadata_header();
        let foreign_symbols = std::mem::take(&mut self.foreign_symbols);

        Ok(CProgram {
            source: out,
            positions,
            allocation_metadata_header,
            allocation_metadata_source,
            foreign_symbols,
        })
    }

    // ----- aggregate type definitions -----

    fn emit_type_definitions(&mut self, out: &mut String) -> Result<(), String> {
        // Collect every aggregate type mentioned anywhere in the module.
        let mut set: Vec<Type> = Vec::new();
        let mut ordered: Vec<Type> = Vec::new();
        let mut seen: Vec<Type> = Vec::new();
        self.collect_aggregates(&mut set)?;
        // Deterministic order: iterate the sorted set, DFS each so that
        // contained aggregates are defined first.
        for ty in &set {
            self.order_aggregate(ty, &mut seen, &mut ordered)?;
        }
        for ty in &ordered {
            self.emit_one_typedef(out, ty)?;
        }
        Ok(())
    }

    fn collect_aggregates(&self, set: &mut Vec<Type>) -> Result<(), String> {
        for c in &self.module.classes {
            for f in &c.fields {
                collect_aggr_ty(&f.ty, set);
            }
            if let Some(ctor) = &c.ctor {
                self.collect_fn_aggr(ctor, set);
            }
            for m in &c.methods {
                self.collect_fn_aggr(m, set);
            }
        }
        for g in &self.module.globals {
            collect_aggr_ty(&g.ty, set);
            collect_aggr_expr(&g.init, set);
        }
        for f in &self.module.functions {
            self.collect_fn_aggr(f, set);
        }
        for f in &self.module.foreign_fns {
            for p in &f.params {
                collect_aggr_ty(&p.ty, set);
            }
            collect_aggr_ty(&f.ret, set);
        }
        Ok(())
    }

    fn collect_fn_aggr(&self, f: &hir::Function, set: &mut Vec<Type>) {
        for p in &f.params {
            collect_aggr_ty(&p.ty, set);
            if let Some(d) = &p.default {
                collect_aggr_expr(d, set);
            }
        }
        collect_aggr_ty(&f.ret, set);
        collect_aggr_stmts(&f.body, set);
    }

    /// Depth-first post-order so a struct that embeds another aggregate
    /// is emitted after it.
    fn order_aggregate(
        &self,
        ty: &Type,
        seen: &mut Vec<Type>,
        out: &mut Vec<Type>,
    ) -> Result<(), String> {
        if !is_aggregate(ty) || seen.contains(ty) {
            return Ok(());
        }
        seen.push(ty.clone());
        // Dependencies: the element/value/field types stored by value.
        match ty {
            Type::FixedArray(elem, _) => self.order_aggregate(elem, seen, out)?,
            Type::IterResult(v) => self.order_aggregate(v, seen, out)?,
            Type::Class(id) if self.is_value_class(*id)? => {
                for f in &self.class(*id)?.fields {
                    self.order_aggregate(&f.ty, seen, out)?;
                }
            }
            _ => {}
        }
        out.push(ty.clone());
        Ok(())
    }

    fn emit_one_typedef(&mut self, out: &mut String, ty: &Type) -> Result<(), String> {
        // A reference class's `ctype` is `void*`; its struct name is the
        // `Sub_*` layout view, so name Class types by `class_name`.
        let name = match ty {
            Type::Class(id) => self.class_name(*id)?,
            _ => self.ctype(ty)?,
        };
        if !self.emitted_types.insert(name.clone()) {
            return Ok(());
        }
        match ty {
            Type::FixedArray(elem, n) => {
                let _ = writeln!(
                    out,
                    "typedef struct {{ {} a[{n}]; }} {name};",
                    self.ctype(elem)?
                );
            }
            Type::IterResult(v) => {
                let _ = writeln!(
                    out,
                    "typedef struct {{ int32_t done; {} value; }} {name};",
                    self.ctype(v)?
                );
            }
            Type::Class(id) => {
                let _ = writeln!(out, "typedef struct {name} {{");
                let class = self.class(*id)?;
                if class.fields.is_empty() {
                    let _ = writeln!(out, "    char subscript_opaque;");
                }
                for field in &class.fields {
                    let _ = writeln!(out, "    {};", self.field_decl(&field.name, &field.ty)?);
                }
                let _ = writeln!(out, "}} {name};");
            }
            other => return Err(format!("typedef for {other:?}")),
        }
        Ok(())
    }

    /// A `"<type> <name>"` declaration fragment (arrays wrap in their
    /// `FA` struct type, so this is uniform).
    fn field_decl(&self, name: &str, ty: &Type) -> Result<String, String> {
        Ok(format!("{} {}", self.ctype(ty)?, sanitize(name)))
    }

    // ----- signatures -----

    fn fn_c_name(f: &hir::Function) -> String {
        format!("subscript_fn_{}", sanitize(&f.name))
    }

    /// Parameter list for a plain function/method (aggregates by value).
    fn param_list(&self, params: &[hir::Param]) -> Result<String, String> {
        let mut parts = Vec::with_capacity(params.len());
        for p in params {
            parts.push(format!("{} {}", self.ctype(&p.ty)?, sanitize(&p.name)));
        }
        Ok(parts.join(", "))
    }

    fn fn_signature(&self, f: &hir::Function) -> Result<String, String> {
        let ret = self.ctype(&f.ret)?;
        let name = Emitter::fn_c_name(f);
        let params = self.param_list(&f.params)?;
        if params.is_empty() {
            Ok(format!("static {ret} {name}(void* ctx)"))
        } else {
            Ok(format!("static {ret} {name}(void* ctx, {params})"))
        }
    }

    fn ctor_signature(&self, ci: usize, c: &hir::ClassDef) -> Result<String, String> {
        let ctor = c.ctor.as_ref().ok_or("constructor missing")?;
        let cname = self.class_name(ClassId(ci))?;
        let params = self.param_list(&ctor.params)?;
        let sep = if params.is_empty() { "" } else { ", " };
        if c.is_value {
            Ok(format!(
                "static {cname} subscript_ctor{ci}(void* ctx{sep}{params})"
            ))
        } else {
            Ok(format!(
                "static void subscript_ctor{ci}(void* ctx, void* _this{}{params})",
                if params.is_empty() { "" } else { ", " }
            ))
        }
    }

    fn method_signature(&self, ci: usize, m: &hir::Function) -> Result<String, String> {
        let ret = self.ctype(&m.ret)?;
        let params = self.param_list(&m.params)?;
        let sep = if params.is_empty() { "" } else { ", " };
        // C2: a value-class receiver is a pointer to the receiver's
        // storage (so a mutating method mutates it), exactly as the CLIF
        // path passes value-method receivers.
        let recv = if self.class(ClassId(ci))?.is_value {
            format!("{}*", self.class_name(ClassId(ci))?)
        } else {
            "void*".to_string()
        };
        Ok(format!(
            "static {ret} subscript_m{ci}_{}(void* ctx, {recv} _this{sep}{params})",
            sanitize(&m.name)
        ))
    }

    fn async_method_creator_signature(
        &self,
        ci: usize,
        method: &hir::Function,
    ) -> Result<String, String> {
        let params = self.param_list(&method.params)?;
        let separator = if params.is_empty() { "" } else { ", " };
        Ok(format!(
            "static void* subscript_m{ci}_{}(void* ctx, void* _this{separator}{params})",
            sanitize(&method.name)
        ))
    }

    fn async_method_resume_signature(&self, ci: usize, method: &hir::Function) -> String {
        format!(
            "static uint8_t subscript_m{ci}_{}_resume(void* ctx, void* _frame, void* _out)",
            sanitize(&method.name)
        )
    }

    fn gen_creator_signature(&self, f: &hir::Function) -> Result<String, String> {
        let params = self.param_list(&f.params)?;
        if params.is_empty() {
            Ok(format!(
                "static void* subscript_fn_{}(void* ctx)",
                sanitize(&f.name)
            ))
        } else {
            Ok(format!(
                "static void* subscript_fn_{}(void* ctx, {params})",
                sanitize(&f.name)
            ))
        }
    }

    fn gen_resume_signature(&self, f: &hir::Function) -> Result<String, String> {
        let ret = if f.is_async { "uint8_t" } else { "int32_t" };
        Ok(format!(
            "static {ret} subscript_resume_{}(void* ctx, void* _frame, void* _out)",
            sanitize(&f.name)
        ))
    }

    // ----- constructors -----

    fn emit_constructor(
        &mut self,
        out: &mut String,
        ci: usize,
        c: &hir::ClassDef,
    ) -> Result<(), String> {
        let ctor = c.ctor.as_ref().ok_or("constructor missing")?;
        let sig = self.ctor_signature(ci, c)?;
        let cname = self.class_name(ClassId(ci))?;
        let _ = writeln!(out, "{sig} {{");
        self.begin_fn(
            if c.is_value {
                ThisCtx::ValueLValue
            } else {
                ThisCtx::Reference
            },
            if c.is_value {
                Type::Class(ClassId(ci))
            } else {
                Type::Void
            },
        );
        if c.is_value {
            let _ = writeln!(out, "    {cname} _this;");
            let _ = writeln!(out, "    memset(&_this, 0, sizeof _this);");
        }
        self.emit_prologue(out, &ctor.params, &ctor.body, 1)?;
        // Field initializers, then the constructor body.
        for field in &c.fields {
            if let Some(init) = &field.init {
                let v = self.eval(init, out, 1)?;
                if c.is_value {
                    let _ = writeln!(out, "    _this.{} = {v};", sanitize(&field.name));
                } else {
                    let _ = writeln!(
                        out,
                        "    (({cname}*)_this)->{} = {v};",
                        sanitize(&field.name)
                    );
                }
            }
        }
        self.emit_block(out, &ctor.body, 1)?;
        self.emit_shadow_pop(out, 1);
        if c.is_value {
            let _ = writeln!(out, "    return _this;");
        }
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    fn emit_method(
        &mut self,
        out: &mut String,
        ci: usize,
        m: &hir::Function,
    ) -> Result<(), String> {
        let sig = self.method_signature(ci, m)?;
        let is_value = self.class(ClassId(ci))?.is_value;
        let _ = writeln!(out, "{sig} {{");
        self.begin_fn(
            if is_value {
                ThisCtx::ValuePtr
            } else {
                ThisCtx::Reference
            },
            m.ret.clone(),
        );
        self.emit_prologue(out, &m.params, &m.body, 1)?;
        self.emit_block(out, &m.body, 1)?;
        self.emit_exit(out, &m.ret, 1)?;
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    fn emit_function(&mut self, out: &mut String, f: &hir::Function) -> Result<(), String> {
        let sig = self.fn_signature(f)?;
        let _ = writeln!(out, "{sig} {{");
        self.begin_fn(ThisCtx::None, f.ret.clone());
        self.emit_prologue(out, &f.params, &f.body, 1)?;
        self.emit_block(out, &f.body, 1)?;
        self.emit_exit(out, &f.ret, 1)?;
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    /// Resets per-function emitter state.
    fn begin_fn(&mut self, this: ThisCtx, ret: Type) {
        self.this = this;
        self.descriptor_this = None;
        self.gen = None;
        self.gen_locals.clear();
        self.local_types.clear();
        self.managed_scope.clear();
        self.assoc_iters.clear();
        self.shadow_cursor = 0;
        self.has_shadow = false;
        self.current_ret = ret;
    }

    /// Emits the shadow-frame prologue and records parameter types (M1,
    /// C2). Every parameter or local that is a Context allocation, or an
    /// aggregate whose interior holds Context handles (a `FixedArray` of
    /// references/strings, an `IterResult` of a managed type), lives in a
    /// per-call shadow frame the collector conservatively word-scans, so
    /// a live handle held in one survives `Context.collect()`; the frame is
    /// pushed here and popped at every exit, exactly as the CLIF path's
    /// `shadow_push`/`shadow_pop` do (the P2 M1 fix, on the CLIF side).
    fn emit_prologue(
        &mut self,
        out: &mut String,
        params: &[hir::Param],
        body: &[hir::Stmt],
        depth: usize,
    ) -> Result<(), String> {
        for p in params {
            self.local_types.push((p.name.clone(), p.ty.clone()));
        }
        let n = self.shadow_words(params, body)?;
        if n == 0 {
            return Ok(());
        }
        let ind = indent(depth);
        let _ = writeln!(
            out,
            "{ind}void* _ssroots[{n}]; memset(_ssroots, 0, sizeof _ssroots);"
        );
        let _ = writeln!(out, "{ind}subscript_rt_shadow_push(ctx, _ssroots, {n}ull);");
        self.has_shadow = true;
        let mut slot = 0u32;
        for p in params {
            let w = managed_words(&self.layouts, &p.ty)?;
            if w == 0 {
                continue;
            }
            let access = self.root_slot_store(out, &p.ty, slot, &sanitize(&p.name), depth)?;
            self.managed_scope.push((p.name.clone(), access));
            slot = checked_shadow_words(slot, w)?;
        }
        self.shadow_cursor = slot;
        Ok(())
    }

    /// Stores `value` (a managed scalar or a managed-interior aggregate,
    /// of type `ty`) into shadow slot `slot`, and returns the C access
    /// expression for that slot. A managed scalar is one `void*` slot; a
    /// managed-interior aggregate occupies `managed_words` consecutive
    /// slots holding its bytes (its interior handles land on
    /// word-aligned offsets the conservative scan reads).
    fn root_slot_store(
        &mut self,
        out: &mut String,
        ty: &Type,
        slot: u32,
        value: &str,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        if is_managed(&self.layouts, ty)? {
            let _ = writeln!(out, "{ind}_ssroots[{slot}] = {value};");
            Ok(format!("_ssroots[{slot}]"))
        } else {
            let cty = self.ctype(ty)?;
            let _ = writeln!(out, "{ind}*({cty}*)&_ssroots[{slot}] = {value};");
            Ok(format!("(*({cty}*)&_ssroots[{slot}])"))
        }
    }

    /// Pops the shadow frame if one was pushed.
    fn emit_shadow_pop(&mut self, out: &mut String, depth: usize) {
        if self.has_shadow {
            let _ = writeln!(out, "{}subscript_rt_shadow_pop(ctx);", indent(depth));
        }
    }

    /// Returns from the current emitted function after a trap, matching
    /// the dev tier's per-frame unwind. The caller checks the Context
    /// trap flag after every fault-capable call and continues unwinding.
    fn emit_trap_return(&mut self, out: &mut String, depth: usize) -> Result<(), String> {
        self.emit_assoc_iter_ends(out, depth);
        self.emit_shadow_pop(out, depth);
        if let Some(gen) = &self.gen {
            let done = if gen.kind == FrameKind::Async { 0 } else { 1 };
            let _ = writeln!(out, "{}return {done};", indent(depth));
        } else if self.current_ret == Type::Void {
            let _ = writeln!(out, "{}return;", indent(depth));
        } else {
            let zero = self.zero_value(&self.current_ret.clone())?;
            let _ = writeln!(out, "{}return {zero};", indent(depth));
        }
        Ok(())
    }

    /// Emits the pending-trap check and current-frame unwind.
    ///
    /// P19 deliberately makes emitted C depend on one private Context
    /// layout fact outside the generated host header: `trap_flag` is the
    /// first `u32` (`Context::trap_flag_offset() == 0`). Keep the layout
    /// assumption in this one emitter method; every ship-tier check goes
    /// through it so the assumption cannot spread silently.
    fn emit_trap_check(&mut self, out: &mut String, depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        let _ = writeln!(out, "{ind}if (*(const uint32_t*)ctx != 0u) {{");
        self.emit_trap_return(out, depth + 1)?;
        let _ = writeln!(out, "{ind}}}");
        Ok(())
    }

    /// Lowers one explicit HIR trap site.
    ///
    /// This match is intentionally exhaustive: a new site is a build error
    /// until the C lowering states how it is handled.
    fn emit_trap_site(
        &mut self,
        site: &hir::TrapSite,
        operand: TrapOperand,
        out: &mut String,
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        match site {
            hir::TrapSite::Allocation { .. } | hir::TrapSite::Call { .. } => {
                if !matches!(operand, TrapOperand::Pending) {
                    return Err("pending-trap site received a value".to_string());
                }
                self.emit_trap_check(out, depth)
            }
            hir::TrapSite::Unreachable { pos } => {
                if !matches!(operand, TrapOperand::Pending) {
                    return Err("unreachable trap site received a value".to_string());
                }
                let pos_id = self.pos_id(pos);
                let _ = writeln!(
                    out,
                    "{ind}subscript_rt_trap(ctx, {}u, {pos_id}u);",
                    TrapKind::UnreachableReached as u32
                );
                self.emit_trap_return(out, depth)
            }
            hir::TrapSite::DivisionByZero { pos } => {
                let TrapOperand::Value(divisor) = operand else {
                    return Err("division trap site has no divisor".to_string());
                };
                let pos_id = self.pos_id(pos);
                let _ = writeln!(out, "{ind}if ({divisor} == 0) {{");
                let _ = writeln!(
                    out,
                    "{}subscript_rt_trap(ctx, {}u, {pos_id}u);",
                    indent(depth + 1),
                    TrapKind::DivisionByZero as u32
                );
                self.emit_trap_return(out, depth + 1)?;
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            hir::TrapSite::IndexRead { pos } | hir::TrapSite::IndexWrite { pos } => {
                let pos_id = self.pos_id(pos);
                match operand {
                    TrapOperand::Condition(condition) => {
                        let _ = writeln!(out, "{ind}if (!({condition})) {{");
                        let _ = writeln!(
                            out,
                            "{}subscript_rt_trap(ctx, {}u, {pos_id}u);",
                            indent(depth + 1),
                            TrapKind::IndexOutOfBounds as u32
                        );
                        self.emit_trap_return(out, depth + 1)?;
                        let _ = writeln!(out, "{ind}}}");
                        Ok(())
                    }
                    TrapOperand::DynamicIndex { handle, index } => {
                        let header = self.fresh_tmp();
                        let _ = writeln!(
                            out,
                            "{ind}SsArrayHeader* {header} = (SsArrayHeader*){handle};"
                        );
                        let _ = writeln!(
                            out,
                            "{ind}if ({index} < 0 || (uint64_t){index} >= {header}->len) {{"
                        );
                        let _ = writeln!(
                            out,
                            "{}(void)subscript_rt_array_ptr(ctx, {handle}, {index}, {pos_id}u);",
                            indent(depth + 1)
                        );
                        self.emit_trap_return(out, depth + 1)?;
                        let _ = writeln!(out, "{ind}}}");
                        Ok(())
                    }
                    _ => Err("index trap site has no materialized index".to_string()),
                }
            }
            hir::TrapSite::JsonResultValue { pos } => {
                let TrapOperand::Condition(condition) = operand else {
                    return Err("JsonResult.value trap site has no `ok` value".to_string());
                };
                let pos_id = self.pos_id(pos);
                let _ = writeln!(out, "{ind}if (!({condition})) {{");
                let _ = writeln!(
                    out,
                    "{}subscript_rt_trap(ctx, {}u, {pos_id}u);",
                    indent(depth + 1),
                    TrapKind::JsonResultValue as u32
                );
                self.emit_trap_return(out, depth + 1)?;
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            hir::TrapSite::NullNarrowing { pos } => {
                let TrapOperand::Value(pointer) = operand else {
                    return Err("null-narrowing site has no pointer".to_string());
                };
                let pos_id = self.pos_id(pos);
                let _ = writeln!(out, "{ind}if ({pointer} == 0) {{");
                let _ = writeln!(
                    out,
                    "{}subscript_rt_trap(ctx, {}u, {pos_id}u);",
                    indent(depth + 1),
                    TrapKind::NullNarrowing as u32
                );
                self.emit_trap_return(out, depth + 1)?;
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            hir::TrapSite::ClassMismatch { class, pos } => {
                let TrapOperand::Value(pointer) = operand else {
                    return Err("class-mismatch site has no pointer".to_string());
                };
                let pos_id = self.pos_id(pos);
                let class_offset = rtc::CLASS_ID_OFFSET;
                let _ = writeln!(
                    out,
                    "{ind}if (*(const uint32_t*)((const unsigned char*){pointer} + ({class_offset})) != {}u) {{",
                    class.0
                );
                let _ = writeln!(
                    out,
                    "{}subscript_rt_trap(ctx, {}u, {pos_id}u);",
                    indent(depth + 1),
                    TrapKind::ClassMismatch as u32
                );
                self.emit_trap_return(out, depth + 1)?;
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            hir::TrapSite::DevOnlyLifetime { .. }
            | hir::TrapSite::DevReloadOnlyStaleCoroutine { .. } => {
                // Explicitly one-tier sites: §8.1b leaves releasing-C
                // lifetime behavior unspecified, and C has no reload mode.
                Ok(())
            }
            hir::TrapSite::WireEnumValue { alias, pos } => {
                let TrapOperand::WireValue { wire, valid } = operand else {
                    return Err("wire-enum trap site has no wire value".to_string());
                };
                let definition = self
                    .module
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| "wire-enum alias id is out of range".to_string())?;
                let pos_id = self.pos_id(pos);
                let _ = writeln!(out, "{ind}if (!({valid})) {{");
                let _ = writeln!(
                    out,
                    "{}subscript_rt_trap_wire_enum(ctx, (const unsigned char*){}, {}ull, {wire}, {pos_id}u);",
                    indent(depth + 1),
                    c_string_literal(definition.name.as_bytes()),
                    definition.name.len(),
                );
                self.emit_trap_return(out, depth + 1)?;
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
        }
    }

    /// Function exit on the fall-through path: pop the shadow frame, then
    /// (for a non-`void` return) a zeroed return keeps the C well-formed
    /// (the checker proves all paths return).
    fn emit_exit(&mut self, out: &mut String, ret: &Type, depth: usize) -> Result<(), String> {
        self.emit_shadow_pop(out, depth);
        if *ret == Type::Void {
            return Ok(());
        }
        let ind = indent(depth);
        let _ = writeln!(out, "{ind}return {};", self.zero_value(ret)?);
        Ok(())
    }

    /// Number of shadow-frame words a function needs: `managed_words` per
    /// parameter and per `let` (walk order), summing managed scalars (one
    /// word) and managed-interior aggregates (their word-rounded size),
    /// exactly as the CLIF path's `shadow_words`.
    fn shadow_words(&self, params: &[hir::Param], body: &[hir::Stmt]) -> Result<u32, String> {
        let mut n = 0u32;
        for p in params {
            n = checked_shadow_words(n, managed_words(&self.layouts, &p.ty)?)?;
        }
        let mut lets: Vec<(&str, &Type)> = Vec::new();
        walk_lets(body, &mut lets);
        for (_, ty) in lets {
            n = checked_shadow_words(n, managed_words(&self.layouts, ty)?)?;
        }
        Ok(n)
    }

    /// True when a value of `ty` is a Context allocation held directly
    /// (a scalar collection root), i.e. `managed_words` is nonzero and it
    /// is not merely a managed-interior aggregate. Used to decide whether
    /// a local needs shadow-frame storage at all.
    fn needs_rooting(&self, ty: &Type) -> Result<bool, String> {
        Ok(managed_words(&self.layouts, ty)? > 0)
    }

    fn zero_value(&self, ty: &Type) -> Result<String, String> {
        Ok(match ty {
            Type::Void => String::new(),
            Type::F32 => "0.0f".to_string(),
            Type::F64 => "0.0".to_string(),
            Type::I8
            | Type::U8
            | Type::I16
            | Type::U16
            | Type::F16
            | Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::Bool
            | Type::Enum(_)
            | Type::StringAlias(_) => "0".to_string(),
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Worker(..)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Generator(_)
            | Type::Nullable(_)
            | Type::Null => "0".to_string(),
            Type::Class(id) if !self.is_value_class(*id)? => "0".to_string(),
            _ => format!("({}){{0}}", self.ctype(ty)?),
        })
    }

    // ----- init and exports -----

    fn emit_init(&mut self, out: &mut String) -> Result<(), String> {
        let _ = writeln!(out, "void subscript_init(subscript_rt_context* ctx) {{");
        self.begin_fn(ThisCtx::None, Type::Void);
        let _ = writeln!(
            out,
            "    if (subscript_rt_globals_init(ctx, sizeof(SubscriptModuleGlobals), _Alignof(SubscriptModuleGlobals)) == (void*)0) return;"
        );
        let globals: Vec<hir::Global> = self.module.globals.to_vec();
        for g in &globals {
            let v = self.eval(&g.init, out, 1)?;
            let slot = self.global_ref(&g.name);
            let _ = writeln!(out, "    {slot} = {v};");
            // A managed global (or managed-interior aggregate global) is
            // a permanent collection root (M1): `managed_words` words,
            // as in the CLIF path's `root_add`.
            let words = managed_words(&self.layouts, &g.ty)?;
            if words > 0 {
                let _ = writeln!(
                    out,
                    "    subscript_rt_root_add(ctx, &({slot}), {words}ull);"
                );
            }
        }
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    fn emit_exports(&mut self, out: &mut String) -> Result<(), String> {
        let functions = self.module.functions.clone();
        for f in &functions {
            if is_host_callable_export(self.module, f) {
                let export = format!("subscript_export_{}", sanitize(&f.name));
                if f.is_async {
                    let creator = Emitter::fn_c_name(f);
                    let resume = format!("subscript_resume_{}", sanitize(&f.name));
                    let _ = writeln!(out, "void {export}(subscript_rt_context* ctx) {{");
                    let _ = writeln!(out, "    void* _frame = {creator}(ctx);");
                    let _ = writeln!(out, "    if (*(const uint32_t*)ctx != 0u) return;");
                    let _ = writeln!(out, "    subscript_rt_async_kick(ctx, _frame, {resume});");
                    let _ = writeln!(out, "}}");
                } else {
                    let cn = Emitter::fn_c_name(f);
                    let params = self.param_list(&f.params)?;
                    let separator = if params.is_empty() { "" } else { ", " };
                    let arguments = f
                        .params
                        .iter()
                        .map(|parameter| sanitize(&parameter.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let argument_separator = if arguments.is_empty() { "" } else { ", " };
                    if f.params
                        .iter()
                        .any(|parameter| matches!(parameter.ty, Type::StringAlias(_)))
                    {
                        let _ = writeln!(
                            out,
                            "void {export}(subscript_rt_context* ctx{separator}{params}) {{"
                        );
                        self.begin_fn(ThisCtx::None, Type::Void);
                        for parameter in &f.params {
                            if let Type::StringAlias(alias) = parameter.ty {
                                let site = hir::TrapSite::WireEnumValue {
                                    alias,
                                    pos: parameter.pos.clone(),
                                };
                                self.validate_wire_alias(
                                    alias,
                                    &sanitize(&parameter.name),
                                    &site,
                                    out,
                                    1,
                                )?;
                            }
                        }
                        let _ = writeln!(out, "    {cn}(ctx{argument_separator}{arguments});");
                        let _ = writeln!(out, "}}");
                    } else {
                        let _ = writeln!(
                            out,
                            "void {export}(subscript_rt_context* ctx{separator}{params}) {{ {cn}(ctx{argument_separator}{arguments}); }}"
                        );
                    }
                }
            }
        }
        let _ = writeln!(
            out,
            "void subscript_kick_async_exports(subscript_rt_context* ctx) {{"
        );
        for f in &self.module.functions {
            if f.exported
                && f.is_async
                && f.name != "main"
                && f.params.is_empty()
                && f.ret == Type::Void
            {
                let _ = writeln!(out, "    subscript_export_{}(ctx);", sanitize(&f.name));
                let _ = writeln!(out, "    if (*(const uint32_t*)ctx != 0u) return;");
            }
        }
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    // ----- statements -----

    fn emit_block(
        &mut self,
        out: &mut String,
        stmts: &[hir::Stmt],
        depth: usize,
    ) -> Result<(), String> {
        for s in stmts {
            self.emit_stmt(out, s, depth)?;
        }
        Ok(())
    }

    fn emit_stmt(&mut self, out: &mut String, s: &hir::Stmt, depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        match s {
            hir::Stmt::Let { name, ty, init, .. } => self.emit_let(out, name, ty, init, depth),
            hir::Stmt::Expr(e) => self.emit_expr_stmt(out, e, depth),
            hir::Stmt::Return { value, .. } => self.emit_return(out, value.as_ref(), depth),
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                let c = self.eval(cond, out, depth)?;
                let _ = writeln!(out, "{ind}if ({c}) {{");
                self.emit_block(out, then, depth + 1)?;
                if let Some(e) = els {
                    let _ = writeln!(out, "{ind}}} else {{");
                    self.emit_block(out, e, depth + 1)?;
                }
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            hir::Stmt::While { cond, body, .. } => self.emit_while(out, cond, body, depth),
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => self.emit_for(
                out,
                init.as_deref(),
                cond.as_ref(),
                step.as_ref(),
                body,
                depth,
            ),
            hir::Stmt::ForOf { .. } => self.emit_for_of(out, s, depth),
            hir::Stmt::Switch { disc, cases, .. } => self.emit_switch(out, disc, cases, depth),
            hir::Stmt::Break(_) => {
                let brk = self.cur_break()?;
                let _ = writeln!(out, "{ind}goto {brk};");
                Ok(())
            }
            hir::Stmt::Continue(_) => {
                let cont = self.cur_continue()?;
                let _ = writeln!(out, "{ind}goto {cont};");
                Ok(())
            }
            hir::Stmt::Block(b) => {
                let _ = writeln!(out, "{ind}{{");
                self.emit_block(out, b, depth + 1)?;
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            other => Err(format!(
                "statement {other:?} is outside the run set's scope"
            )),
        }
    }

    fn emit_let(
        &mut self,
        out: &mut String,
        name: &str,
        ty: &Type,
        init: &hir::Expr,
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        // Generator locals live in the frame, not as C variables.
        if self.gen.is_some() {
            let field = self.gen_next_let_field(name)?;
            let v = self.eval(init, out, depth)?;
            let _ = writeln!(out, "{ind}_f->{field} = {v};");
            return Ok(());
        }
        // A managed local — or an aggregate whose interior holds managed
        // handles — lives in the shadow frame so `Context.collect()` sees its
        // handle(s) (M1); its storage is shadow slot(s), not a C var.
        if self.needs_rooting(ty)? {
            let w = managed_words(&self.layouts, ty)?;
            let slot = self.shadow_cursor;
            self.shadow_cursor = checked_shadow_words(self.shadow_cursor, w)?;
            self.local_types.push((name.to_string(), ty.clone()));
            let v = self.eval(init, out, depth)?;
            let access = self.root_slot_store(out, ty, slot, &v, depth)?;
            self.managed_scope.push((name.to_string(), access));
            return Ok(());
        }
        self.local_types.push((name.to_string(), ty.clone()));
        let cname = sanitize(name);
        match ty {
            Type::FixedArray(..) if matches!(init.kind, hir::ExprKind::ArrayLit(_)) => {
                let cty = self.ctype(ty)?;
                let elems = match &init.kind {
                    hir::ExprKind::ArrayLit(e) => e,
                    _ => unreachable!(),
                };
                let vals = self.eval_list(elems, out, depth)?;
                let _ = writeln!(out, "{ind}{cty} {cname} = {{ {{ {vals} }} }};");
                Ok(())
            }
            _ => {
                let cty = self.ctype(ty)?;
                let v = self.eval(init, out, depth)?;
                let _ = writeln!(out, "{ind}{cty} {cname} = {v};");
                Ok(())
            }
        }
    }

    fn emit_return(
        &mut self,
        out: &mut String,
        value: Option<&hir::Expr>,
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        // Inside a coroutine resume body, `return` completes the frame.
        if let Some(kind) = self.gen.as_ref().map(|g| g.kind) {
            if kind == FrameKind::Async {
                if let Some(value) = value {
                    let text = self.eval(value, out, depth)?;
                    let ret = self.ctype(&value.ty)?;
                    let _ = writeln!(out, "{ind}*({ret}*)_out = {text};");
                }
            }
            self.emit_assoc_iter_ends(out, depth);
            let _ = writeln!(out, "{ind}_f->_state = {GEN_DONE}; return 1;");
            return Ok(());
        }
        match value {
            None => {
                self.emit_assoc_iter_ends(out, depth);
                self.emit_shadow_pop(out, depth);
                let _ = writeln!(out, "{ind}return;");
            }
            Some(v) => {
                // The return value is computed before the frame is
                // popped; no collection runs between the pop and the
                // return, and the shadow array's memory outlives the pop
                // (it is unregistered, not freed), so reading it is safe.
                let text = self.eval(v, out, depth)?;
                if let Type::Class(cid) = self.current_ret.clone() {
                    if self.is_value_class(cid)?
                        && self.boundary_struct_contains_pointer_member(cid, &mut HashSet::new())?
                    {
                        let ctype = self.class_name(cid)?;
                        let stable = self.fresh_tmp();
                        let _ = writeln!(out, "{ind}{ctype} {stable} = {text};");
                        self.emit_stabilize_boundary_return_value(
                            cid,
                            &format!("&{stable}"),
                            &v.pos,
                            out,
                            depth,
                            &mut HashSet::new(),
                        )?;
                        self.emit_assoc_iter_ends(out, depth);
                        self.emit_shadow_pop(out, depth);
                        let _ = writeln!(out, "{ind}return {stable};");
                        return Ok(());
                    }
                }
                self.emit_assoc_iter_ends(out, depth);
                self.emit_shadow_pop(out, depth);
                let _ = writeln!(out, "{ind}return {text};");
            }
        }
        Ok(())
    }

    fn emit_assoc_iter_ends(&self, out: &mut String, depth: usize) {
        let ind = indent(depth);
        for handle in self.assoc_iters.iter().rev() {
            let _ = writeln!(out, "{ind}subscript_rt_assoc_iter_end(ctx, {handle});");
        }
    }

    fn emit_while(
        &mut self,
        out: &mut String,
        cond: &hir::Expr,
        body: &[hir::Stmt],
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        let ind1 = indent(depth + 1);
        let top = self.fresh_label();
        let brk = self.fresh_label();
        let _ = writeln!(out, "{ind}{top}: ;");
        let c = self.eval(cond, out, depth)?;
        let _ = writeln!(out, "{ind}if (!({c})) goto {brk};");
        let _ = writeln!(out, "{ind}{{");
        self.loops_push(brk.clone(), top.clone());
        self.emit_block(out, body, depth + 1)?;
        self.loops_pop();
        let _ = writeln!(out, "{ind1}goto {top};");
        let _ = writeln!(out, "{ind}}}");
        let _ = writeln!(out, "{ind}{brk}: ;");
        Ok(())
    }

    fn emit_for(
        &mut self,
        out: &mut String,
        init: Option<&hir::Stmt>,
        cond: Option<&hir::Expr>,
        step: Option<&hir::Expr>,
        body: &[hir::Stmt],
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        let ind1 = indent(depth + 1);
        let top = self.fresh_label();
        let cont = self.fresh_label();
        let brk = self.fresh_label();
        let _ = writeln!(out, "{ind}{{");
        if let Some(i) = init {
            self.emit_stmt(out, i, depth + 1)?;
        }
        let _ = writeln!(out, "{ind1}{top}: ;");
        if let Some(c) = cond {
            let cv = self.eval(c, out, depth + 1)?;
            let _ = writeln!(out, "{ind1}if (!({cv})) goto {brk};");
        }
        let _ = writeln!(out, "{ind1}{{");
        self.loops_push(brk.clone(), cont.clone());
        self.emit_block(out, body, depth + 2)?;
        self.loops_pop();
        let _ = writeln!(out, "{}{cont}: ;", indent(depth + 2));
        if let Some(s) = step {
            self.emit_expr_stmt(out, s, depth + 2)?;
        }
        let _ = writeln!(out, "{}goto {top};", indent(depth + 2));
        let _ = writeln!(out, "{ind1}}}");
        let _ = writeln!(out, "{ind1}{brk}: ;");
        let _ = writeln!(out, "{ind}}}");
        Ok(())
    }

    fn emit_for_of_binding(
        &mut self,
        out: &mut String,
        name: &str,
        ty: &Type,
        depth: usize,
    ) -> Result<String, String> {
        self.local_types.push((name.to_string(), ty.clone()));
        if self.gen.is_some() {
            let field = self.gen_next_let_field(name)?;
            return Ok(format!("_f->{field}"));
        }
        let zero = self.zero_value(ty)?;
        if self.needs_rooting(ty)? {
            let words = managed_words(&self.layouts, ty)?;
            let slot = self.shadow_cursor;
            self.shadow_cursor = checked_shadow_words(self.shadow_cursor, words)?;
            let access = self.root_slot_store(out, ty, slot, &zero, depth)?;
            self.managed_scope.push((name.to_string(), access.clone()));
            return Ok(access);
        }
        let cty = self.ctype(ty)?;
        let cname = sanitize(name);
        let _ = writeln!(out, "{}{cty} {cname} = {zero};", indent(depth));
        // Record even an unrooted binding so it masks a same-named
        // rooted binding from an earlier lexical block.
        self.managed_scope.push((name.to_string(), cname.clone()));
        Ok(cname)
    }

    fn emit_for_of(
        &mut self,
        out: &mut String,
        stmt: &hir::Stmt,
        depth: usize,
    ) -> Result<(), String> {
        use hir::ForOfKind as K;
        let hir::Stmt::ForOf {
            name,
            ty,
            subject,
            kind,
            body,
            pos,
        } = stmt
        else {
            return Err("non-for-of statement passed to emit_for_of".to_string());
        };
        let kind = *kind;

        let ind = indent(depth);
        let ind1 = indent(depth + 1);
        let top = self.fresh_label();
        let cont = self.fresh_label();
        let brk = self.fresh_label();
        let subject_value = self.eval(subject, out, depth)?;
        let subject_ty = self.ctype(&subject.ty)?;
        let subject_tmp = self.fresh_tmp();
        let index = self.fresh_tmp();
        let bound = self.fresh_tmp();
        let pid = self.pos_id(pos);
        let managed_base = self.managed_scope.len();
        let local_types_base = self.local_types.len();
        let gen_locals_base = self.gen_locals.len();

        let _ = writeln!(out, "{ind}{{");
        let _ = writeln!(out, "{ind1}{subject_ty} {subject_tmp} = {subject_value};");
        let binding = self.emit_for_of_binding(out, name, ty, depth + 1)?;
        let _ = writeln!(out, "{ind1}uint64_t {index} = 0;");
        match kind {
            K::ArrayValues | K::ArrayKeys => {
                let _ = writeln!(
                    out,
                    "{ind1}uint64_t {bound} = (uint64_t)subscript_rt_array_len(ctx, {subject_tmp});"
                );
            }
            K::FixedArrayValues => {
                let Type::FixedArray(_, count) = &subject.ty else {
                    return Err("fixed-array for-of subject type".to_string());
                };
                let _ = writeln!(out, "{ind1}uint64_t {bound} = {count}ull;");
            }
            K::MapKeys | K::MapValues | K::SetValues => {
                let _ = writeln!(
                    out,
                    "{ind1}uint64_t {bound} = subscript_rt_assoc_iter_begin(ctx, {subject_tmp}, {pid}u);"
                );
                self.emit_trap_check(out, depth + 1)?;
                self.assoc_iters.push(subject_tmp.clone());
            }
            K::StringCodePoints => {
                let _ = writeln!(
                    out,
                    "{ind1}uint64_t {bound} = (uint64_t)subscript_rt_str_len(ctx, {subject_tmp});"
                );
            }
            other => return Err(format!("unknown ForOfKind {other:?}")),
        }
        let _ = writeln!(out, "{ind1}{top}: ;");
        let condition = if matches!(kind, K::ArrayValues | K::ArrayKeys) {
            format!(
                "{index} < {bound} && {index} < (uint64_t)subscript_rt_array_len(ctx, {subject_tmp})"
            )
        } else {
            format!("{index} < {bound}")
        };
        let _ = writeln!(out, "{ind1}if (!({condition})) goto {brk};");

        match kind {
            K::ArrayKeys => {
                let _ = writeln!(out, "{ind1}{binding} = (int32_t){index};");
            }
            K::ArrayValues => {
                let ect = self.ctype(ty)?;
                let _ = writeln!(
                    out,
                    "{ind1}{binding} = ((const {ect}*)subscript_rt_array_data(ctx, {subject_tmp}))[{index}];"
                );
            }
            K::FixedArrayValues => {
                let _ = writeln!(out, "{ind1}{binding} = ({subject_tmp}).a[{index}];");
            }
            K::MapKeys | K::MapValues | K::SetValues => {
                let cty = self.ctype(ty)?;
                let value = self.fresh_tmp();
                let select = u32::from(kind == K::MapValues);
                let _ = writeln!(out, "{ind1}{cty} {value};");
                let _ = writeln!(
                    out,
                    "{ind1}if (!subscript_rt_assoc_iter_copy(ctx, {subject_tmp}, {index}, \
                     {select}u, &{value}, {pid}u)) goto {cont};"
                );
                self.emit_trap_check(out, depth + 1)?;
                let _ = writeln!(out, "{ind1}{binding} = {value};");
            }
            K::StringCodePoints => {
                let next = self.fresh_tmp();
                let value = self.fresh_tmp();
                let _ = writeln!(out, "{ind1}int32_t {next} = (int32_t){index};");
                let _ = writeln!(
                    out,
                    "{ind1}void* {value} = subscript_rt_str_iter_code_point(ctx, {subject_tmp}, \
                     (int32_t){index}, &{next}, {pid}u);"
                );
                self.emit_trap_check(out, depth + 1)?;
                let _ = writeln!(out, "{ind1}{binding} = {value};");
                let _ = writeln!(out, "{ind1}{index} = (uint64_t){next};");
            }
            other => return Err(format!("unknown ForOfKind {other:?}")),
        }

        self.loops_push(brk.clone(), cont.clone());
        self.emit_block(out, body, depth + 1)?;
        self.loops_pop();
        if matches!(kind, K::MapKeys | K::MapValues | K::SetValues) {
            self.assoc_iters.pop();
        }
        let _ = writeln!(out, "{ind1}{cont}: ;");
        if kind != K::StringCodePoints {
            let _ = writeln!(out, "{ind1}{index}++;");
        }
        let _ = writeln!(out, "{ind1}goto {top};");
        let _ = writeln!(out, "{ind1}{brk}: ;");
        if matches!(kind, K::MapKeys | K::MapValues | K::SetValues) {
            let _ = writeln!(
                out,
                "{ind1}subscript_rt_assoc_iter_end(ctx, {subject_tmp});"
            );
        }
        let _ = writeln!(out, "{ind}}}");
        self.managed_scope.truncate(managed_base);
        self.local_types.truncate(local_types_base);
        self.gen_locals.truncate(gen_locals_base);
        Ok(())
    }

    fn emit_switch(
        &mut self,
        out: &mut String,
        disc: &hir::Expr,
        cases: &[hir::SwitchCase],
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        let ind1 = indent(depth + 1);
        let dv = self.eval(disc, out, depth)?;
        let dty = self.ctype(&disc.ty)?;
        let brk = self.fresh_label();
        let labels: Vec<String> = cases.iter().map(|_| self.fresh_label()).collect();
        let default_idx = cases.iter().position(|c| c.test.is_none());
        let _ = writeln!(out, "{ind}{{");
        let _ = writeln!(out, "{ind1}{dty} _disc = {dv};");
        if matches!(disc.ty, Type::StringAlias(_)) {
            // Q32/R14 case labels are checker-proven alias members. Plain
            // aliases carry declaration-order discriminants; wire aliases
            // carry wire values (§52.1). Both remain integer-only switches.
            let ind2 = indent(depth + 2);
            let _ = writeln!(out, "{ind1}switch (_disc) {{");
            for (i, case) in cases.iter().enumerate() {
                if let Some(test) = &case.test {
                    let hir::ExprKind::Int(value) = test.kind else {
                        return Err(
                            "string-literal union switch case is not an integer discriminant"
                                .to_string(),
                        );
                    };
                    let _ = writeln!(out, "{ind2}case {value}: goto {};", labels[i]);
                }
            }
            match default_idx {
                Some(i) => {
                    let _ = writeln!(out, "{ind2}default: goto {};", labels[i]);
                }
                None => {
                    let _ = writeln!(out, "{ind2}default: goto {brk};");
                }
            }
            let _ = writeln!(out, "{ind1}}}");
        } else {
            for (i, case) in cases.iter().enumerate() {
                if let Some(test) = &case.test {
                    let t = self.eval(test, out, depth + 1)?;
                    let _ = writeln!(out, "{ind1}if (_disc == {t}) goto {};", labels[i]);
                }
            }
            match default_idx {
                Some(i) => {
                    let _ = writeln!(out, "{ind1}goto {};", labels[i]);
                }
                None => {
                    let _ = writeln!(out, "{ind1}goto {brk};");
                }
            }
        }
        // Bodies fall through to the next arm unless they break.
        self.loops_push_switch(brk.clone());
        for (i, case) in cases.iter().enumerate() {
            let _ = writeln!(out, "{ind1}{}: ;", labels[i]);
            self.emit_block(out, &case.body, depth + 1)?;
        }
        self.loops_pop();
        let _ = writeln!(out, "{ind1}{brk}: ;");
        let _ = writeln!(out, "{ind}}}");
        Ok(())
    }

    // ----- loop/switch context for break/continue targets -----

    fn loops_push(&mut self, brk: String, cont: String) {
        self.loop_stack_mut().push((brk, Some(cont)));
    }

    fn loops_push_switch(&mut self, brk: String) {
        self.loop_stack_mut().push((brk, None));
    }

    fn loops_pop(&mut self) {
        self.loop_stack_mut().pop();
    }

    fn cur_break(&mut self) -> Result<String, String> {
        self.loop_stack_mut()
            .last()
            .map(|(b, _)| b.clone())
            .ok_or_else(|| "break outside a loop or switch".to_string())
    }

    fn cur_continue(&mut self) -> Result<String, String> {
        for (_, c) in self.loop_stack_mut().iter().rev() {
            if let Some(c) = c {
                return Ok(c.clone());
            }
        }
        Err("continue outside a loop".to_string())
    }

    fn loop_stack_mut(&mut self) -> &mut Vec<(String, Option<String>)> {
        &mut self.loops
    }

    // ----- expression statements -----

    fn emit_expr_stmt(
        &mut self,
        out: &mut String,
        e: &hir::Expr,
        depth: usize,
    ) -> Result<(), String> {
        use hir::ExprKind as K;
        let ind = indent(depth);
        match &e.kind {
            K::Assign { op, target, value } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "assignment", |sites| {
                    self.emit_assign(out, *op, target, value, sites, depth)
                })
            }
            K::Call {
                callee: hir::Callee::Ambient(a),
                args,
            } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "ambient call", |sites| {
                    self.emit_ambient(out, *a, args, &e.pos, sites, depth)?;
                    if let Some(site) =
                        sites.take(|site| matches!(site, hir::TrapSite::Call { .. }))
                    {
                        self.emit_trap_site(site, TrapOperand::Pending, out, depth)?;
                    }
                    Ok(())
                })
            }
            K::Call {
                callee: hir::Callee::Method { recv, name },
                args,
            } if is_array_mutator(&recv.ty, name) => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "array mutator call", |sites| {
                    self.emit_array_mutator(out, recv, name, args, &e.pos, sites, depth)?;
                    if let Some(site) =
                        sites.take(|site| matches!(site, hir::TrapSite::Call { .. }))
                    {
                        self.emit_trap_site(site, TrapOperand::Pending, out, depth)?;
                    }
                    Ok(())
                })
            }
            _ => {
                let text = self.eval(e, out, depth)?;
                if !text.is_empty() {
                    let _ = writeln!(out, "{ind}{text};");
                }
                Ok(())
            }
        }
    }

    fn emit_ambient(
        &mut self,
        out: &mut String,
        a: hir::AmbientFn,
        args: &[hir::Expr],
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        match a {
            hir::AmbientFn::Print => {
                let arg = args.first().ok_or("print arity")?;
                let h = self.eval(arg, out, depth)?;
                let _ = writeln!(out, "{ind}subscript_rt_print(ctx, {h});");
            }
            hir::AmbientFn::Unreachable => {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Unreachable { .. }),
                    "unreachable() has no HIR trap site".to_string(),
                )?;
                self.emit_trap_site(site, TrapOperand::Pending, out, depth)?;
            }
            hir::AmbientFn::Collect => {
                let _ = writeln!(out, "{ind}subscript_rt_collect(ctx);");
            }
            hir::AmbientFn::UnsafeDelete => {
                let arg = args.first().ok_or("Context.free arity")?;
                let p = self.eval_pinned(arg, out, depth)?;
                while let Some(site) =
                    sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                {
                    self.emit_trap_site(site, TrapOperand::Value(p.clone()), out, depth)?;
                }
                let pid = self.pos_id(pos);
                let _ = writeln!(out, "{ind}subscript_rt_delete(ctx, {p}, {pid}u);");
            }
            _ => return Err("unknown ambient function".to_string()),
        }
        Ok(())
    }

    // The explicit HIR trap-site input is kept separate from call operands
    // so evaluation order and full-site consumption remain reviewable.
    #[allow(clippy::too_many_arguments)]
    fn emit_array_mutator(
        &mut self,
        out: &mut String,
        recv: &hir::Expr,
        name: &str,
        args: &[hir::Expr],
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        let elem = match &recv.ty {
            Type::Array(e) => (**e).clone(),
            other => return Err(format!("array method on {other:?}")),
        };
        // Receiver before argument (the element temp is a statement).
        let h = self.eval_pinned(recv, out, depth)?;
        while let Some(site) =
            sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
        {
            self.emit_trap_site(site, TrapOperand::Value(h.clone()), out, depth)?;
        }
        let ect = self.ctype(&elem)?;
        let pid = self.pos_id(pos);
        match name {
            "push" => {
                let arg = args.first().ok_or("push arity")?;
                let v = self.eval(arg, out, depth)?;
                let _ = writeln!(
                    out,
                    "{ind}{{ {ect} _e = {v}; subscript_rt_array_push(ctx, {h}, &_e, {pid}u); }}"
                );
            }
            "pop" => {
                let _ = writeln!(
                    out,
                    "{ind}{{ {ect} _d; subscript_rt_array_pop(ctx, {h}, &_d, {pid}u); }}"
                );
            }
            other => return Err(format!("array mutator `{other}`")),
        }
        Ok(())
    }

    /// Assignment as a statement, carrying C2 copy semantics and
    /// growth-safe dynamic-array element stores (N3).
    fn emit_assign(
        &mut self,
        out: &mut String,
        op: Option<hir::BinOp>,
        target: &hir::Expr,
        value: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
        depth: usize,
    ) -> Result<(), String> {
        let ind = indent(depth);
        // Dynamic-array element store. A plain assignment evaluates the
        // RHS before resolving the checked write address. A compound
        // assignment resolves and reads first (so an invalid target traps
        // before the RHS), then resolves again after the RHS for a
        // growth-safe write.
        if matches!(
            &target.kind,
            hir::ExprKind::Index { obj, .. } if matches!(obj.ty, Type::Array(_))
        ) {
            self.eval_dynamic_array_assign(op, target, value, sites, out, depth)?;
            return Ok(());
        }
        let place = self.place(target, sites, out, depth)?;
        match op {
            None => {
                // Chain-slot address-of (Q13): a value struct assigned into
                // a `Struct | null` boundary pointer slot stores the
                // address of the struct's storage.
                if self.is_boundary_struct_ptr(&target.ty)? {
                    if let Type::Class(cid) = value.ty {
                        if self.is_value_class(cid)? {
                            let p = self.value_recv_ptr(value, cid, out, depth)?;
                            let _ = writeln!(out, "{ind}{place} = {p};");
                            return Ok(());
                        }
                    }
                }
                let v = self.eval(value, out, depth)?;
                let _ = writeln!(out, "{ind}{place} = {v};");
            }
            Some(bin) => {
                if target.ty == Type::Str && bin == hir::BinOp::Add {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        "string compound assignment has no HIR allocation site",
                    )?;
                    let v = self.eval(value, out, depth)?;
                    let pid = self.pos_id(&target.pos);
                    let call = format!("subscript_rt_str_concat(ctx, {place}, {v}, {pid}u)");
                    let result = self.eval_site_checked_call(call, &Type::Str, site, out, depth)?;
                    let _ = writeln!(out, "{ind}{place} = {result};");
                } else if target.ty.is_integer() && matches!(bin, hir::BinOp::Div | hir::BinOp::Rem)
                {
                    let v = self.eval(value, out, depth)?;
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                        "integer compound assignment has no HIR trap site",
                    )?;
                    let result = self.eval_checked_divrem(
                        &target.ty,
                        bin == hir::BinOp::Div,
                        &place,
                        &v,
                        site,
                        out,
                        depth,
                    )?;
                    let _ = writeln!(out, "{ind}{place} = {result};");
                } else if matches!(bin, hir::BinOp::Shl | hir::BinOp::Shr | hir::BinOp::UShr) {
                    let v = self.eval(value, out, depth)?;
                    let shifted = shift_expr(bin, &target.ty, &place, &v)?;
                    let _ = writeln!(out, "{ind}{place} = {shifted};");
                } else {
                    let sym = binop_sym(bin)?;
                    let v = self.eval(value, out, depth)?;
                    let _ = writeln!(out, "{ind}{place} = {place} {sym} {v};");
                }
            }
        }
        Ok(())
    }

    /// Dynamic-array assignment shared by statement and expression
    /// positions. The returned temporary is the assigned value.
    fn eval_dynamic_array_assign(
        &mut self,
        op: Option<hir::BinOp>,
        target: &hir::Expr,
        value: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let hir::ExprKind::Index { obj, index, .. } = &target.kind else {
            return Err("dynamic-array assignment target is not an index".to_string());
        };
        let Type::Array(elem) = &obj.ty else {
            return Err("dynamic-array assignment target is not an Array".to_string());
        };
        let ind = indent(depth);
        let ect = self.ctype(elem)?;
        let handle = self.eval_pinned(obj, out, depth)?;
        while let Some(site) =
            sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
        {
            self.emit_trap_site(site, TrapOperand::Value(handle.clone()), out, depth)?;
        }
        let index = self.eval_pinned(index, out, depth)?;
        let read_site = sites.take(|site| matches!(site, hir::TrapSite::IndexRead { .. }));
        let write_site = sites.take_required(
            |site| matches!(site, hir::TrapSite::IndexWrite { .. }),
            "dynamic array assignment has no HIR write site",
        )?;

        let current = if op.is_some() {
            let read_site = read_site.ok_or("compound array assignment has no HIR read site")?;
            let pointer =
                self.emit_dynamic_index_addr(&handle, &index, elem, read_site, out, depth)?;
            let current = self.fresh_tmp();
            let _ = writeln!(out, "{ind}{ect} {current} = *{pointer};");
            Some(current)
        } else {
            None
        };
        let rhs = self.eval(value, out, depth)?;
        let rhs_tmp = self.fresh_tmp();
        let _ = writeln!(out, "{ind}{ect} {rhs_tmp} = {rhs};");

        let assigned = match (op, current) {
            (None, None) => rhs_tmp,
            (Some(hir::BinOp::Add), Some(current)) if **elem == Type::Str => {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Allocation { .. }),
                    "string array compound assignment has no HIR allocation site",
                )?;
                let pos_id = self.pos_id(&target.pos);
                let call = format!("subscript_rt_str_concat(ctx, {current}, {rhs_tmp}, {pos_id}u)");
                self.eval_site_checked_call(call, elem, site, out, depth)?
            }
            (Some(bin @ (hir::BinOp::Div | hir::BinOp::Rem)), Some(current))
                if elem.is_integer() =>
            {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                    "integer array compound assignment has no HIR trap site",
                )?;
                self.eval_checked_divrem(
                    elem,
                    bin == hir::BinOp::Div,
                    &current,
                    &rhs_tmp,
                    site,
                    out,
                    depth,
                )?
            }
            (Some(bin @ (hir::BinOp::Shl | hir::BinOp::Shr | hir::BinOp::UShr)), Some(current)) => {
                let expression = shift_expr(bin, elem, &current, &rhs_tmp)?;
                let combined = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{ect} {combined} = {expression};");
                combined
            }
            (Some(bin), Some(current)) => {
                let symbol = binop_sym(bin)?;
                let combined = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{ect} {combined} = {current} {symbol} {rhs_tmp};");
                combined
            }
            _ => return Err("invalid dynamic-array assignment shape".to_string()),
        };

        // Resolve only after the RHS and compound operation: either may
        // fault or reallocate before the growth-safe write.
        let pointer =
            self.emit_dynamic_index_addr(&handle, &index, elem, write_site, out, depth)?;
        let _ = writeln!(out, "{ind}*{pointer} = {assigned};");
        Ok(assigned)
    }

    /// A C lvalue for an assignable place. A top-level dynamic-array
    /// element store still uses `emit_assign` directly; nested value-class
    /// field places resolve their dynamic-array element here.
    ///
    /// The returned lvalue embeds no user-visible evaluation: every
    /// sub-expression it needs (field bases, indices) is bound to a
    /// temporary first. So the target is evaluated before the assigned
    /// value, as the dev tier does (`lower/func.rs`, `eval_assign` calls
    /// `place` first), and a compound assignment may spell the place
    /// twice without running its base twice.
    fn place(
        &mut self,
        e: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Local(name) => Ok(self.local_ref(name)),
            K::Global(name) => Ok(self.global_ref(name)),
            K::This => self.current_this_expr(),
            K::Field { obj, name } => {
                let (base, arrow) = self.field_base(obj, sites, out, depth)?;
                Ok(format!("{base}{arrow}{}", sanitize(name)))
            }
            K::Index {
                obj,
                index,
                checked,
            } => match &obj.ty {
                Type::FixedArray(_, n) => {
                    let base = self.place_with_own_sites(obj, out, depth)?;
                    let idx = self.eval_pinned(index, out, depth)?;
                    if let Some(site) = sites.take(|site| {
                        matches!(
                            site,
                            hir::TrapSite::IndexRead { .. } | hir::TrapSite::IndexWrite { .. }
                        )
                    }) {
                        let elem = match &obj.ty {
                            Type::FixedArray(el, _) => (**el).clone(),
                            _ => unreachable!(),
                        };
                        let p = self.emit_fixed_index_addr(
                            &format!("{base}.a"),
                            *n,
                            &idx,
                            &elem,
                            site,
                            out,
                            depth,
                        )?;
                        Ok(format!("(*{p})"))
                    } else if !*checked {
                        Ok(format!("{base}.a[{idx}]"))
                    } else {
                        Err("checked fixed-array place has no HIR index site".to_string())
                    }
                }
                Type::Array(elem) => {
                    let handle = self.eval_pinned(obj, out, depth)?;
                    while let Some(site) =
                        sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                    {
                        self.emit_trap_site(site, TrapOperand::Value(handle.clone()), out, depth)?;
                    }
                    let index = self.eval_pinned(index, out, depth)?;
                    let site = sites.take_required(
                        |site| {
                            matches!(
                                site,
                                hir::TrapSite::IndexRead { .. } | hir::TrapSite::IndexWrite { .. }
                            )
                        },
                        "checked dynamic-array place has no HIR index site",
                    )?;
                    let pointer =
                        self.emit_dynamic_index_addr(&handle, &index, elem, site, out, depth)?;
                    Ok(format!("(*{pointer})"))
                }
                other => Err(format!("assignment target index on {other:?}")),
            },
            other => Err(format!("assignment target {other:?}")),
        }
    }

    /// The base expression and member operator (`.`/`->`) for a field
    /// access in a **place** ([`Self::place`]): the base is bound to a
    /// temporary, so the place carries no evaluation of its own.
    fn field_base(
        &mut self,
        obj: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<(String, &'static str), String> {
        match &obj.ty {
            Type::Class(id) => {
                if self.is_value_class(*id)? {
                    Ok((self.place_or_eval(obj, out, depth)?, "."))
                } else {
                    let cname = self.class_name(*id)?;
                    let o = self.eval_pinned(obj, out, depth)?;
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }),
                        "reference field place has no HIR lifetime site",
                    )?;
                    self.emit_trap_site(site, TrapOperand::Value(o.clone()), out, depth)?;
                    Ok((format!("(({cname}*)({o}))"), "->"))
                }
            }
            Type::IterResult(_) => Ok((self.place_or_eval(obj, out, depth)?, ".")),
            other => Err(format!("field access on {other:?}")),
        }
    }

    /// For an assignable value-class receiver, an lvalue; otherwise the
    /// value bound to a temporary, which is both a legal `.field` base in
    /// C (an rvalue struct is not) and the dev tier's behaviour: the
    /// write lands in a temporary and is correctly lost (C2).
    fn place_or_eval(
        &mut self,
        obj: &hir::Expr,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        use hir::ExprKind as K;
        match &obj.kind {
            K::Local(_) | K::Global(_) | K::Field { .. } | K::Index { .. } | K::This => {
                self.place_with_own_sites(obj, out, depth)
            }
            _ => {
                let v = self.eval_pinned(obj, out, depth)?;
                Ok(format!("({v})"))
            }
        }
    }

    fn place_with_own_sites(
        &mut self,
        obj: &hir::Expr,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let sites = obj.trap_sites(self.module);
        lower_trap_sites(&sites, "nested place", |sites| {
            self.place(obj, sites, out, depth)
        })
    }

    fn local_ref(&self, name: &str) -> String {
        if self.gen.is_some() {
            for (n, access) in self.gen_locals.iter().rev() {
                if n == name {
                    return access.clone();
                }
            }
        }
        // A rooted local/param is its shadow-frame access (M1).
        for (n, access) in self.managed_scope.iter().rev() {
            if n == name {
                return access.clone();
            }
        }
        sanitize(name)
    }

    fn global_ref(&self, name: &str) -> String {
        format!("subscript_globals(ctx)->g_{}", sanitize(name))
    }

    // ----- expressions -----

    /// Evaluates `e` to a C expression, emitting any preceding
    /// statements (temporaries, hoisted chains) into `out` at `depth`.
    fn eval(&mut self, e: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Int(v) => Ok(int_literal(*v, &e.ty)),
            K::Float(v) => {
                if e.ty == Type::F16 {
                    Ok(format!(
                        "subscript_rt_f16_from_f64({})",
                        float_literal(*v, &Type::F64)
                    ))
                } else {
                    Ok(float_literal(*v, &e.ty))
                }
            }
            K::Bool(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
            K::Str(s) => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "string literal", |sites| {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        "string literal has no HIR allocation site",
                    )?;
                    self.string_literal(s.as_bytes(), site, out, depth)
                })
            }
            K::Null => Ok("((void*)0)".to_string()),
            K::This => self.current_this_expr(),
            K::Local(name) => Ok(self.local_ref(name)),
            K::Global(name) => Ok(self.global_ref(name)),
            K::FuncRef(name) => self.func_ref_value(name),
            K::EnumMember { value, .. } => Ok(value.to_string()),
            K::Unary { op, operand } => {
                let v = self.eval(operand, out, depth)?;
                let expr = match op {
                    hir::UnOp::Neg => format!("(-({v}))"),
                    hir::UnOp::Not => format!("(!({v}))"),
                    hir::UnOp::BitNot => format!("(~({v}))"),
                    _ => return Err("unknown unary operator".to_string()),
                };
                if is_narrow_integer(&operand.ty)
                    && matches!(op, hir::UnOp::Neg | hir::UnOp::BitNot)
                {
                    Ok(format!("(({})({expr}))", self.ctype(&operand.ty)?))
                } else {
                    Ok(expr)
                }
            }
            K::Binary { op, left, right } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "binary expression", |sites| {
                    self.eval_binary(*op, left, right, &e.pos, sites, out, depth)
                })
            }
            K::Assign { op, target, value } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "assignment expression", |sites| {
                    self.eval_assign_expr(*op, target, value, sites, out, depth)
                })
            }
            K::Cast(inner) => {
                let reference_narrowing = matches!(e.ty, Type::Class(_))
                    && (matches!(inner.ty, Type::Object)
                        || matches!(&inner.ty, Type::Nullable(ty) if **ty == Type::Object));
                let v = if reference_narrowing {
                    self.eval_pinned(inner, out, depth)?
                } else {
                    self.eval(inner, out, depth)?
                };
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "cast", |sites| {
                    self.eval_cast(&v, &inner.ty, &e.ty, sites, out, depth)
                })
            }
            K::Call { callee, args } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "call", |sites| {
                    self.eval_call(callee, args, &e.ty, &e.pos, sites, out, depth)
                })
            }
            K::New { class, args } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "new expression", |sites| {
                    self.eval_new(*class, args, sites, out, depth)
                })
            }
            K::DescriptorLit { class, fields } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "descriptor literal", |sites| {
                    self.eval_descriptor_lit(*class, fields, sites, out, depth)
                })
            }
            K::Zero => self.zero_value(&e.ty),
            K::RawNew { class } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "RawNew", |sites| {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        "RawNew has no HIR allocation site",
                    )?;
                    let hir::TrapSite::Allocation { pos } = site else {
                        return Err("RawNew has a non-allocation HIR site".to_string());
                    };
                    if self.is_value_class(*class)? {
                        return Err("raw allocation requested for a value class".to_string());
                    }
                    let cname = self.class_name(*class)?;
                    let pid = self.pos_id(pos);
                    let call = format!(
                        "subscript_rt_alloc(ctx, sizeof({cname}), {}u, {pid}u)",
                        class.0
                    );
                    self.eval_site_checked_call(call, &e.ty, site, out, depth)
                })
            }
            K::Field { obj, name } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "field read", |sites| {
                    let value = self.eval_field(obj, name, sites, out, depth)?;
                    if let Type::StringAlias(alias) = &e.ty {
                        if let Some(site) = sites.take(|site| {
                            matches!(site, hir::TrapSite::WireEnumValue { alias: site_alias, .. } if site_alias == alias)
                        }) {
                            let wire = self.fresh_tmp();
                            let _ = writeln!(out, "{}int32_t {wire} = {value};", indent(depth));
                            return self.validate_wire_alias(*alias, &wire, site, out, depth);
                        }
                    }
                    Ok(value)
                })
            }
            K::JsonResultValue(obj) => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "JsonResult.value", |sites| {
                    self.eval_json_result_value(obj, out, depth, sites)
                })
            }
            K::Length(obj) => match &obj.ty {
                Type::Array(_) => {
                    let h = self.eval(obj, out, depth)?;
                    Ok(format!("subscript_rt_array_len(ctx, {h})"))
                }
                Type::Str => {
                    let h = self.eval(obj, out, depth)?;
                    Ok(format!("subscript_rt_str_len(ctx, {h})"))
                }
                Type::FixedArray(_, n) => Ok(n.to_string()),
                other => Err(format!("length of {other:?}")),
            },
            K::Index { obj, index, .. } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "index read", |sites| {
                    self.eval_index(obj, index, sites, out, depth)
                })
            }
            K::ArrayLit(elems) => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "array literal", |sites| {
                    self.eval_array_lit(&e.ty, elems, sites, out, depth)
                })
            }
            K::ArraySpreadLit(elems) => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "array spread literal", |sites| {
                    self.eval_array_spread_lit(&e.ty, elems, sites, out, depth)
                })
            }
            K::Template(parts) => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "template", |sites| {
                    self.eval_template(parts, sites, out, depth)
                })
            }
            K::Lambda {
                params,
                ret,
                body,
                captures,
            } => self.eval_lambda(params, ret, body, captures, out, depth),
            K::Yield(arg) => self.eval_yield(arg.as_deref(), out, depth),
            K::AsyncSuspend => self.eval_async_suspend(out, depth),
            K::AsyncCall { callee, args } => {
                let sites = e.trap_sites(self.module);
                lower_trap_sites(&sites, "async call", |sites| {
                    self.eval_async_call(callee, args, &e.ty, sites, out, depth)
                })
            }
            K::Cond { cond, then, els } => self.eval_cond(cond, then, els, &e.ty, out, depth),
            other => Err(format!(
                "expression {other:?} is outside the run set's scope"
            )),
        }
    }

    /// Evaluates an operand list left to right (see [`Self::eval_operands`])
    /// and joins the results into a C argument list.
    fn eval_list(
        &mut self,
        elems: &[hir::Expr],
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let refs: Vec<&hir::Expr> = elems.iter().collect();
        Ok(self.eval_operands(&refs, out, depth)?.join(", "))
    }

    /// Evaluates the operands of one C expression **left to right**, the
    /// order the language and the dev tier (`lower/func.rs`) both use.
    ///
    /// [`Self::eval`] returns an expression string and emits its
    /// statements separately, so a later operand's statements would
    /// otherwise run before an earlier operand's expression; C's own
    /// operand order is unspecified and cannot carry the guarantee. Each
    /// operand is therefore lowered into its own statement buffer, and an
    /// operand is bound to a temporary exactly when a **later** operand
    /// lowered to statements — so a list whose operands are plain
    /// expressions emits exactly what it did before, with no temporaries.
    fn eval_operands(
        &mut self,
        exprs: &[&hir::Expr],
        out: &mut String,
        depth: usize,
    ) -> Result<Vec<String>, String> {
        let n = exprs.len();
        let mut bufs: Vec<String> = Vec::with_capacity(n);
        let mut vals: Vec<String> = Vec::with_capacity(n);
        for e in exprs {
            let mut buf = String::new();
            let v = self.eval(e, &mut buf, depth)?;
            bufs.push(buf);
            vals.push(v);
        }
        // `pin[i]`: some operand after `i` emitted statements.
        let mut pin = vec![false; n];
        let mut later = false;
        for i in (0..n).rev() {
            pin[i] = later;
            later = later || !bufs[i].is_empty();
        }
        for i in 0..n {
            out.push_str(&bufs[i]);
            if pin[i] {
                vals[i] = self.bind_tmp(&vals[i], exprs[i], out, depth)?;
            }
        }
        Ok(vals)
    }

    /// Binds the already-emitted value `v` of `e` to a fresh temporary,
    /// so it is computed here rather than where the expression string
    /// lands. Constants are returned unchanged: no statement can change
    /// them.
    fn bind_tmp(
        &mut self,
        v: &str,
        e: &hir::Expr,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        use hir::ExprKind as K;
        if matches!(
            e.kind,
            K::Int(_) | K::Float(_) | K::Bool(_) | K::Null | K::EnumMember { .. }
        ) || e.ty == Type::Void
        {
            return Ok(v.to_string());
        }
        let ct = self.ctype(&e.ty)?;
        let t = self.fresh_tmp();
        let _ = writeln!(out, "{}{ct} {t} = {v};", indent(depth));
        Ok(t)
    }

    /// Evaluates `e` and binds the value to a temporary, so it is
    /// computed **where the statement is emitted** rather than where the
    /// returned expression string finally lands.
    ///
    /// [`Self::eval`] returns an expression string; any statement emitted
    /// afterwards therefore runs *before* that expression. A method call
    /// evaluates its receiver before its arguments (TS/JS; the dev tier
    /// does so by construction), so every operand an argument's
    /// statements could overtake is pinned here first. C's own
    /// argument-evaluation order is unspecified and cannot carry the
    /// property.
    ///
    /// Constants are returned unchanged: no statement can change them.
    fn eval_pinned(
        &mut self,
        e: &hir::Expr,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let v = self.eval(e, out, depth)?;
        self.bind_tmp(&v, e, out, depth)
    }

    fn string_literal(
        &mut self,
        bytes: &[u8],
        site: &hir::TrapSite,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let hir::TrapSite::Allocation { pos } = site else {
            return Err("string literal has a non-allocation HIR site".to_string());
        };
        let pid = self.pos_id(pos);
        let call = format!(
            "subscript_rt_str_lit(ctx, (const unsigned char*){}, {}ull, {pid}u)",
            c_string_literal(bytes),
            bytes.len()
        );
        self.eval_site_checked_call(call, &Type::Str, site, out, depth)
    }

    // The explicit HIR trap-site input is kept separate from expression
    // operands so each guard stays tied to its materialized values.
    #[allow(clippy::too_many_arguments)]
    fn eval_binary(
        &mut self,
        op: hir::BinOp,
        left: &hir::Expr,
        right: &hir::Expr,
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        use hir::BinOp as B;
        let operand_ty = if left.ty == Type::Null {
            right.ty.clone()
        } else {
            left.ty.clone()
        };

        // `&&`/`||` short-circuit: the right operand may lower to
        // statements, which C's `&&` cannot guard — emit the branch, as
        // the dev tier does (`lower/func.rs`, `eval_binary`).
        if matches!(op, B::And | B::Or) {
            let ind = indent(depth);
            let l = self.eval(left, out, depth)?;
            let res = self.fresh_tmp();
            let _ = writeln!(out, "{ind}int32_t {res} = ({l}) != 0;");
            let guard = if op == B::And {
                res.clone()
            } else {
                format!("!{res}")
            };
            let _ = writeln!(out, "{ind}if ({guard}) {{");
            let r = self.eval(right, out, depth + 1)?;
            let _ = writeln!(out, "{}{res} = ({r}) != 0;", indent(depth + 1));
            let _ = writeln!(out, "{ind}}}");
            return Ok(res);
        }

        if operand_ty == Type::Str {
            let ops = self.eval_operands(&[left, right], out, depth)?;
            let (l, r) = (&ops[0], &ops[1]);
            return match op {
                B::Add => {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        "string addition has no HIR allocation site",
                    )?;
                    let pid = self.pos_id(pos);
                    let call = format!("subscript_rt_str_concat(ctx, {l}, {r}, {pid}u)");
                    self.eval_site_checked_call(call, &Type::Str, site, out, depth)
                }
                B::Eq => Ok(format!("(subscript_rt_str_eq(ctx, {l}, {r}) != 0)")),
                B::Ne => Ok(format!("(subscript_rt_str_eq(ctx, {l}, {r}) == 0)")),
                _ => Err("string operator outside the run set's scope".to_string()),
            };
        }

        let ops = self.eval_operands(&[left, right], out, depth)?;
        let (l, r) = (&ops[0], &ops[1]);
        if operand_ty == Type::F16 {
            let sym = binop_sym(op)?;
            return Ok(format!(
                "(subscript_rt_f16_to_f64({l}) {sym} subscript_rt_f16_to_f64({r}))"
            ));
        }
        let float = operand_ty.is_float();
        match op {
            B::Div if !float => {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                    "integer division has no HIR trap site",
                )?;
                return self.eval_checked_divrem(&operand_ty, true, l, r, site, out, depth);
            }
            B::Rem => {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                    "integer remainder has no HIR trap site",
                )?;
                return self.eval_checked_divrem(&operand_ty, false, l, r, site, out, depth);
            }
            _ => {}
        }
        let sym = binop_sym(op)?;
        let expr = if matches!(op, B::Shl | B::Shr | B::UShr) {
            shift_expr(op, &operand_ty, l, r)?
        } else {
            format!("({l} {sym} {r})")
        };
        if is_narrow_integer(&operand_ty)
            && matches!(
                op,
                B::Add
                    | B::Sub
                    | B::Mul
                    | B::BitAnd
                    | B::BitOr
                    | B::BitXor
                    | B::Shl
                    | B::Shr
                    | B::UShr
            )
        {
            Ok(format!("(({})({expr}))", self.ctype(&operand_ty)?))
        } else {
            Ok(expr)
        }
    }

    /// Expands the zero-divisor guard in the caller before invoking the
    /// integer div/rem helper, so a fault returns before the result can be
    /// consumed by the enclosing expression or store.
    fn eval_checked_divrem(
        &mut self,
        ty: &Type,
        div: bool,
        left: &str,
        right: &str,
        site: &hir::TrapSite,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let helper = divrem_helper(ty, div)?;
        let pid = self.pos_id(site.pos());
        let ind = indent(depth);
        let cty = self.ctype(ty)?;
        let left_tmp = self.fresh_tmp();
        let right_tmp = self.fresh_tmp();
        let _ = writeln!(out, "{ind}{cty} {left_tmp} = {left};");
        let _ = writeln!(out, "{ind}{cty} {right_tmp} = {right};");
        self.emit_trap_site(site, TrapOperand::Value(right_tmp.clone()), out, depth)?;
        let call = format!("{helper}(ctx, {left_tmp}, {right_tmp}, {pid}u)");
        let result = self.fresh_tmp();
        let _ = writeln!(out, "{ind}{cty} {result} = {call};");
        Ok(result)
    }

    /// Expands a growable-array bounds check in the caller, so the fault
    /// branch can unwind before any load or store uses the address.
    fn emit_dynamic_index_addr(
        &mut self,
        handle: &str,
        index: &str,
        elem: &Type,
        site: &hir::TrapSite,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let h = self.fresh_tmp();
        let i = self.fresh_tmp();
        let p = self.fresh_tmp();
        let ect = self.ctype(elem)?;
        let _ = writeln!(out, "{ind}void* {h} = {handle};");
        let _ = writeln!(out, "{ind}int32_t {i} = {index};");
        self.emit_trap_site(
            site,
            TrapOperand::DynamicIndex {
                handle: h.clone(),
                index: i.clone(),
            },
            out,
            depth,
        )?;
        let header = self.fresh_tmp();
        let _ = writeln!(out, "{ind}SsArrayHeader* {header} = (SsArrayHeader*){h};");
        let _ = writeln!(
            out,
            "{ind}{ect}* {p} = ({ect}*)({header}->data + (int64_t){i} * (int64_t){header}->elem_size);"
        );
        Ok(p)
    }

    /// Expands a FixedArray bounds check in the caller for the same
    /// immediate-unwind reason as [`Self::emit_dynamic_index_addr`].
    fn emit_fixed_index_addr(
        &mut self,
        base: &str,
        len: u32,
        index: &str,
        elem: &Type,
        site: &hir::TrapSite,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let b = self.fresh_tmp();
        let i = self.fresh_tmp();
        let p = self.fresh_tmp();
        let ect = self.ctype(elem)?;
        let _ = writeln!(out, "{ind}void* {b} = (void*)({base});");
        let _ = writeln!(out, "{ind}int32_t {i} = {index};");
        self.emit_trap_site(
            site,
            TrapOperand::Condition(format!("(uint32_t){i} < {len}u")),
            out,
            depth,
        )?;
        let _ = writeln!(
            out,
            "{ind}{ect}* {p} = ({ect}*)((unsigned char*){b} + (int64_t){i} * (int64_t)sizeof({ect}));"
        );
        Ok(p)
    }

    fn eval_assign_expr(
        &mut self,
        op: Option<hir::BinOp>,
        target: &hir::Expr,
        value: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        if matches!(
            &target.kind,
            hir::ExprKind::Index { obj, .. } if matches!(obj.ty, Type::Array(_))
        ) {
            return self.eval_dynamic_array_assign(op, target, value, sites, out, depth);
        }
        // Assignment used as an expression (including loop steps).
        let place = self.place(target, sites, out, depth)?;
        let v = self.eval(value, out, depth)?;
        match op {
            None => Ok(format!("({place} = {v})")),
            Some(bin) => {
                if target.ty == Type::Str && bin == hir::BinOp::Add {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        "string compound expression has no HIR allocation site",
                    )?;
                    let pid = self.pos_id(&target.pos);
                    let call = format!("subscript_rt_str_concat(ctx, {place}, {v}, {pid}u)");
                    let result = self.eval_site_checked_call(call, &Type::Str, site, out, depth)?;
                    Ok(format!("({place} = {result})"))
                } else if target.ty.is_integer() && matches!(bin, hir::BinOp::Div | hir::BinOp::Rem)
                {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                        "integer compound expression has no HIR trap site",
                    )?;
                    let result = self.eval_checked_divrem(
                        &target.ty,
                        bin == hir::BinOp::Div,
                        &place,
                        &v,
                        site,
                        out,
                        depth,
                    )?;
                    Ok(format!("({place} = {result})"))
                } else if matches!(bin, hir::BinOp::Shl | hir::BinOp::Shr | hir::BinOp::UShr) {
                    let shifted = shift_expr(bin, &target.ty, &place, &v)?;
                    Ok(format!("({place} = {shifted})"))
                } else {
                    let sym = binop_sym(bin)?;
                    Ok(format!("({place} = {place} {sym} {v})"))
                }
            }
        }
    }

    fn eval_cast(
        &mut self,
        v: &str,
        from: &Type,
        to: &Type,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        if matches!(to, Type::Class(_))
            && (matches!(from, Type::Object)
                || matches!(from, Type::Nullable(inner) if **inner == Type::Object))
        {
            while let Some(site) = sites.take(|_| true) {
                self.emit_trap_site(site, TrapOperand::Value(v.to_string()), out, depth)?;
            }
            return Ok(v.to_string());
        }
        // Every non-reference cast is a C cast, except that enum sources
        // behave as i32.
        let from = if matches!(from, Type::Enum(_)) {
            Type::I32
        } else {
            from.clone()
        };
        if from == *to {
            return Ok(format!("({v})"));
        }
        if *to == Type::F16 {
            if matches!(from, Type::F32 | Type::F64) {
                return Ok(format!("subscript_rt_f16_from_f64((double)({v}))"));
            }
            return Err(format!("cast {from:?} -> f16"));
        }
        if from == Type::F16 {
            return match to {
                Type::F32 => Ok(format!("((float)subscript_rt_f16_to_f64({v}))")),
                Type::F64 => Ok(format!("subscript_rt_f16_to_f64({v})")),
                other => Err(format!("cast f16 -> {other:?}")),
            };
        }
        // float -> integer: saturate to match the CLIF `fcvt_*_sat`.
        if from.is_float() && to.is_integer() {
            let helper = float_to_int_helper(to)?;
            return Ok(format!("{helper}({v})"));
        }
        let ct = self.ctype(to)?;
        Ok(format!("(({ct})({v}))"))
    }

    /// Field access as a value (read); the base is evaluated by value so
    /// a field on an array-element value class or any other rvalue base
    /// works.
    fn eval_field(
        &mut self,
        obj: &hir::Expr,
        name: &str,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        match &obj.ty {
            Type::Class(id) if self.is_value_class(*id)? => {
                let base = self.eval(obj, out, depth)?;
                Ok(format!("({base}).{}", sanitize(name)))
            }
            Type::Class(id) => {
                let cname = self.class_name(*id)?;
                let base = self.eval_pinned(obj, out, depth)?;
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }),
                    "reference field has no HIR lifetime site",
                )?;
                self.emit_trap_site(site, TrapOperand::Value(base.clone()), out, depth)?;
                Ok(format!("(({cname}*)({base}))->{}", sanitize(name)))
            }
            Type::IterResult(_) => {
                let base = self.eval(obj, out, depth)?;
                Ok(format!("({base}).{}", sanitize(name)))
            }
            other => Err(format!("field access on {other:?}")),
        }
    }

    /// Emits the guarded read of a `JsonResult<T>` payload. A failed
    /// guard returns immediately from the current C function so no
    /// zeroed reference payload can be dereferenced after the trap.
    fn eval_json_result_value(
        &mut self,
        obj: &hir::Expr,
        out: &mut String,
        depth: usize,
        sites: &mut TrapSiteConsumer<'_>,
    ) -> Result<String, String> {
        let Type::Class(id) = &obj.ty else {
            return Err("JsonResult value receiver is not a class".to_string());
        };
        let class = self.class(*id)?;
        if class.is_value
            || class.fields.len() != 2
            || class.fields[0].name != "ok"
            || class.fields[0].ty != Type::Bool
            || class.fields[1].name != "value"
        {
            return Err("JsonResult value receiver has an invalid layout".to_string());
        }
        let cname = self.class_name(*id)?;
        let base = self.eval_pinned(obj, out, depth)?;
        while let Some(site) = sites.take(|site| {
            matches!(
                site,
                hir::TrapSite::DevOnlyLifetime { .. } | hir::TrapSite::JsonResultValue { .. }
            )
        }) {
            let operand = if matches!(site, hir::TrapSite::DevOnlyLifetime { .. }) {
                TrapOperand::Value(base.clone())
            } else {
                TrapOperand::Condition(format!("(({cname}*)({base}))->ok"))
            };
            self.emit_trap_site(site, operand, out, depth)?;
        }
        Ok(format!("(({cname}*)({base}))->value"))
    }

    fn eval_index(
        &mut self,
        obj: &hir::Expr,
        index: &hir::Expr,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        match &obj.ty {
            Type::FixedArray(elem, n) => {
                let ops = self.eval_operands(&[obj, index], out, depth)?;
                let (base, idx) = (&ops[0], &ops[1]);
                if let Some(site) =
                    sites.take(|site| matches!(site, hir::TrapSite::IndexRead { .. }))
                {
                    let p = self.emit_fixed_index_addr(
                        &format!("({base}).a"),
                        *n,
                        idx,
                        elem,
                        site,
                        out,
                        depth,
                    );
                    let p = p?;
                    Ok(format!("(*{p})"))
                } else {
                    Ok(format!("({base}).a[{idx}]"))
                }
            }
            Type::Array(elem) => {
                let ops = self.eval_operands(&[obj, index], out, depth)?;
                let (h, idx) = (&ops[0], &ops[1]);
                while let Some(site) =
                    sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                {
                    self.emit_trap_site(site, TrapOperand::Value(h.clone()), out, depth)?;
                }
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::IndexRead { .. }),
                    "dynamic index has no HIR read site",
                )?;
                let p = self.emit_dynamic_index_addr(h, idx, elem, site, out, depth)?;
                Ok(format!("(*{p})"))
            }
            other => Err(format!("index on {other:?}")),
        }
    }

    fn eval_array_lit(
        &mut self,
        ty: &Type,
        elems: &[hir::Expr],
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        match ty {
            Type::FixedArray(_, _) => {
                let cty = self.ctype(ty)?;
                let vals = self.eval_list(elems, out, depth)?;
                Ok(format!("(({cty}){{ {{ {vals} }} }})"))
            }
            Type::Array(elem) => {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Allocation { .. }),
                    "array literal has no HIR allocation site",
                )?;
                let hir::TrapSite::Allocation { pos } = site else {
                    return Err("array literal has a non-allocation HIR site".to_string());
                };
                let ect = self.ctype(elem)?;
                let pid = self.pos_id(pos);
                let call = format!("subscript_rt_array_new(ctx, sizeof({ect}), {pid}u)");
                let h = self.eval_site_checked_call(call, ty, site, out, depth)?;
                for e in elems {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        "array literal element has no HIR allocation site",
                    )?;
                    let hir::TrapSite::Allocation { pos } = site else {
                        return Err(
                            "array literal element has a non-allocation HIR site".to_string()
                        );
                    };
                    let v = self.eval(e, out, depth)?;
                    let epid = self.pos_id(pos);
                    let _ = writeln!(
                        out,
                        "{ind}{{ {ect} _e = {v}; subscript_rt_array_push(ctx, {h}, &_e, {epid}u); }}"
                    );
                    self.emit_trap_site(site, TrapOperand::Pending, out, depth)?;
                }
                Ok(h)
            }
            other => Err(format!("array literal of {other:?}")),
        }
    }

    fn eval_array_spread_lit(
        &mut self,
        ty: &Type,
        elems: &[hir::ArrayLitElem],
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let Type::Array(elem_ty) = ty else {
            return Err("spread literal is not a dynamic array".to_string());
        };
        let ind = indent(depth);
        let initial = sites.take_required(
            |site| matches!(site, hir::TrapSite::Allocation { .. }),
            "array spread literal has no allocation site",
        )?;
        let hir::TrapSite::Allocation { pos } = initial else {
            return Err("array spread literal allocation site kind".to_string());
        };
        let ect = self.ctype(elem_ty)?;
        let pid = self.pos_id(pos);
        let call = format!("subscript_rt_array_new(ctx, sizeof({ect}), {pid}u)");
        let handle = self.eval_site_checked_call(call, ty, initial, out, depth)?;

        for elem in elems {
            let site = sites.take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                "array spread element has no allocation site",
            )?;
            let hir::TrapSite::Allocation { pos } = site else {
                return Err("array spread element site kind".to_string());
            };
            let pid = self.pos_id(pos);
            match elem.spread {
                None => {
                    let value = self.eval(&elem.expr, out, depth)?;
                    let _ = writeln!(
                        out,
                        "{ind}{{ {ect} _e = {value}; \
                         subscript_rt_array_push(ctx, {handle}, &_e, {pid}u); }}"
                    );
                }
                Some(hir::SpreadKind::Array) => {
                    let source = self.eval_pinned(&elem.expr, out, depth)?;
                    let _ = writeln!(
                        out,
                        "{ind}subscript_rt_array_spread_array(ctx, {handle}, {source}, {pid}u);"
                    );
                }
                Some(hir::SpreadKind::FixedArray) => {
                    let source = self.eval_pinned(&elem.expr, out, depth)?;
                    let Type::FixedArray(_, count) = &elem.expr.ty else {
                        return Err("fixed spread source type".to_string());
                    };
                    let _ = writeln!(
                        out,
                        "{ind}subscript_rt_array_spread_fixed(ctx, {handle}, \
                         ({source}).a, {count}ull, {pid}u);"
                    );
                }
                Some(hir::SpreadKind::MapKeys | hir::SpreadKind::SetValues) => {
                    let source = self.eval_pinned(&elem.expr, out, depth)?;
                    let _ = writeln!(
                        out,
                        "{ind}subscript_rt_array_spread_assoc(ctx, {handle}, {source}, {pid}u);"
                    );
                }
                Some(hir::SpreadKind::StringCodePoints) => {
                    let source = self.eval_pinned(&elem.expr, out, depth)?;
                    let _ = writeln!(
                        out,
                        "{ind}subscript_rt_array_spread_string(ctx, {handle}, {source}, {pid}u);"
                    );
                }
                Some(other) => return Err(format!("unknown SpreadKind {other:?}")),
            }
            self.emit_trap_site(site, TrapOperand::Pending, out, depth)?;
        }
        Ok(handle)
    }

    fn eval_template(
        &mut self,
        parts: &[hir::TplPart],
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let Some((first, rest)) = parts.split_first() else {
            let site = sites.take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                "empty template has no HIR allocation site",
            )?;
            let result = self.string_literal(b"", site, out, depth)?;
            return Ok(result);
        };
        let eval_part = |this: &mut Self,
                         part: &hir::TplPart,
                         sites: &mut TrapSiteConsumer<'_>,
                         out: &mut String|
         -> Result<String, String> {
            match part {
                hir::TplPart::Text(t) => {
                    let site = sites.take_required(
                        |site| matches!(site, hir::TrapSite::Allocation { .. }),
                        "template text has no HIR allocation site",
                    )?;
                    this.string_literal(t.as_bytes(), site, out, depth)
                }
                hir::TplPart::Expr(e) => {
                    let v = this.eval(e, out, depth)?;
                    let site = if e.ty == Type::Str {
                        None
                    } else {
                        Some(sites.take_required(
                            |site| matches!(site, hir::TrapSite::Allocation { .. }),
                            "template formatting has no HIR allocation site",
                        )?)
                    };
                    this.format_value(&v, &e.ty, site, out, depth)
                }
                other => Err(format!("template part {other:?}")),
            }
        };
        let first = eval_part(self, first, sites, out)?;
        let mut acc = self.fresh_tmp();
        let _ = writeln!(out, "{ind}void* {acc} = {first};");
        for part in rest {
            let piece = eval_part(self, part, sites, out)?;
            let site = sites.take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                "template concat has no HIR allocation site",
            )?;
            let hir::TrapSite::Allocation { pos } = site else {
                return Err("template concat has a non-allocation HIR site".to_string());
            };
            let pid = self.pos_id(pos);
            let call = format!("subscript_rt_str_concat(ctx, {acc}, {piece}, {pid}u)");
            acc = self.eval_site_checked_call(call, &Type::Str, site, out, depth)?;
        }
        Ok(acc)
    }

    fn format_value(
        &mut self,
        v: &str,
        ty: &Type,
        site: Option<&hir::TrapSite>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        if *ty == Type::Str {
            if site.is_some() {
                return Err("string interpolation has an allocation site".to_string());
            }
            return Ok(v.to_string());
        }
        let site = site.ok_or("formatting has no HIR allocation site")?;
        let hir::TrapSite::Allocation { pos } = site else {
            return Err("formatting has a non-allocation HIR site".to_string());
        };
        let pid = self.pos_id(pos);
        if let Type::StringAlias(id) = ty {
            let definition = self
                .module
                .string_aliases
                .get(id.0)
                .ok_or_else(|| "string alias id is out of range".to_string())?;
            let index = self.fresh_tmp();
            if let Some(wire_values) = &definition.wire_values {
                let wire = self.fresh_tmp();
                let _ = writeln!(out, "{}int32_t {wire} = {v};", indent(depth));
                let _ = writeln!(out, "{}int32_t {index} = 0;", indent(depth));
                for (member_index, wire_value) in wire_values.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "{}if ({wire} == {wire_value}) {{ {index} = {member_index}; }}",
                        indent(depth)
                    );
                }
            } else {
                let _ = writeln!(out, "{}int32_t {index} = {v};", indent(depth));
            }
            let table = format!("subscript_string_alias_{}", id.0);
            let call = format!(
                "subscript_rt_str_lit(ctx, {table}[{index}].data, \
                 {table}[{index}].len, {pid}u)"
            );
            return self.eval_site_checked_call(call, &Type::Str, site, out, depth);
        }
        let call = match ty {
            Type::I8 | Type::I16 => {
                format!("subscript_rt_fmt_i32(ctx, (int32_t)({v}), {pid}u)")
            }
            Type::U8 | Type::U16 => {
                format!("subscript_rt_fmt_u32(ctx, (uint32_t)({v}), {pid}u)")
            }
            Type::I32 | Type::Enum(_) => {
                format!("subscript_rt_fmt_i32(ctx, {v}, {pid}u)")
            }
            Type::U32 => format!("subscript_rt_fmt_u32(ctx, {v}, {pid}u)"),
            Type::I64 => format!("subscript_rt_fmt_i64(ctx, {v}, {pid}u)"),
            Type::U64 => format!("subscript_rt_fmt_u64(ctx, {v}, {pid}u)"),
            Type::F32 => format!("subscript_rt_fmt_f32(ctx, {v}, {pid}u)"),
            Type::F64 => format!("subscript_rt_fmt_f64(ctx, {v}, {pid}u)"),
            Type::F16 => {
                format!("subscript_rt_fmt_f64(ctx, subscript_rt_f16_to_f64({v}), {pid}u)")
            }
            Type::Bool => format!("subscript_rt_fmt_bool(ctx, {v}, {pid}u)"),
            other => return Err(format!("interpolation of {other:?}")),
        };
        self.eval_site_checked_call(call, &Type::Str, site, out, depth)
    }

    fn eval_cond(
        &mut self,
        cond: &hir::Expr,
        then: &hir::Expr,
        els: &hir::Expr,
        ty: &Type,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        // Evaluate the arms into a shared temporary via if/else so each
        // arm's side effects run only on its branch.
        let c = self.eval(cond, out, depth)?;
        let cty = self.ctype(ty)?;
        let res = self.fresh_tmp();
        let _ = writeln!(out, "{ind}{cty} {res};");
        // A contextual `Struct | null` conditional has pointer
        // representation even when one arm is the by-value struct. Keep
        // that arm in storage outside the branch block so the pointer
        // remains live for the surrounding boundary construction/call.
        let boundary_storage = if let Some(cid) = self.boundary_struct_ptr_id(ty)? {
            let storage = self.fresh_tmp();
            let storage_type = self.class_name(cid)?;
            let _ = writeln!(out, "{ind}{storage_type} {storage};");
            Some((cid, storage))
        } else {
            None
        };
        let _ = writeln!(out, "{ind}if ({c}) {{");
        let tv = self.eval(then, out, depth + 1)?;
        if let Some((cid, storage)) = &boundary_storage {
            if then.ty == Type::Class(*cid) {
                let _ = writeln!(out, "{}{storage} = {tv};", indent(depth + 1));
                let _ = writeln!(out, "{}{res} = &{storage};", indent(depth + 1));
            } else {
                let _ = writeln!(out, "{}{res} = {tv};", indent(depth + 1));
            }
        } else {
            let _ = writeln!(out, "{}{res} = {tv};", indent(depth + 1));
        }
        let _ = writeln!(out, "{ind}}} else {{");
        let ev = self.eval(els, out, depth + 1)?;
        if let Some((cid, storage)) = &boundary_storage {
            if els.ty == Type::Class(*cid) {
                let _ = writeln!(out, "{}{storage} = {ev};", indent(depth + 1));
                let _ = writeln!(out, "{}{res} = &{storage};", indent(depth + 1));
            } else {
                let _ = writeln!(out, "{}{res} = {ev};", indent(depth + 1));
            }
        } else {
            let _ = writeln!(out, "{}{res} = {ev};", indent(depth + 1));
        }
        let _ = writeln!(out, "{ind}}}");
        Ok(res)
    }

    // ----- calls -----

    // The explicit HIR trap-site input is kept separate from call operands
    // so backend call policy cannot be reconstructed from the callee.
    #[allow(clippy::too_many_arguments)]
    fn eval_call(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let trap_site = sites.take(|site| matches!(site, hir::TrapSite::Call { .. }));
        let checked = trap_site.is_some();
        match callee {
            hir::Callee::Func(name) => {
                let f = self.hir_fn(name)?;
                let argv = self.call_args(&f.params.clone(), args, out, depth)?;
                let sep = if argv.is_empty() { "" } else { ", " };
                let call = format!("subscript_fn_{}(ctx{sep}{argv})", sanitize(name));
                if let Some(site) = trap_site {
                    self.eval_site_checked_call(call, ret_ty, site, out, depth)
                } else {
                    Ok(call)
                }
            }
            hir::Callee::Value(v) => {
                let ft = match &v.ty {
                    Type::Func(ft) => (**ft).clone(),
                    other => return Err(format!("call of {other:?}")),
                };
                // The callee value is evaluated first (its `SubFn` temp is
                // that binding), then the arguments left to right.
                let fv = self.eval(v, out, depth)?;
                let fvt = self.fresh_tmp();
                let _ = writeln!(out, "{}SubFn {fvt} = {fv};", indent(depth));
                let cast = self.fn_ptr_cast(&ft)?;
                let mut parts = vec![format!("({fvt}).env")];
                let argv: Vec<&hir::Expr> = ft.params.iter().zip(args).map(|(_, a)| a).collect();
                parts.extend(self.eval_operands(&argv, out, depth)?);
                let call = format!("(({cast})({fvt}).code)(ctx, {})", parts.join(", "));
                if let Some(site) = trap_site {
                    self.eval_site_checked_call(call, ret_ty, site, out, depth)
                } else {
                    Ok(call)
                }
            }
            hir::Callee::Method { recv, name } => {
                self.eval_method(recv, name, args, ret_ty, pos, sites, out, depth, checked)
            }
            hir::Callee::Foreign(name) => {
                self.eval_foreign_call(name, args, ret_ty, pos, sites, out, depth, checked)
            }
            // A Math intrinsic (stdlib.md §1) calls its opaque runtime
            // symbol — never a bare libm call, which clang would
            // constant-fold at -O2 (stdlib.md §0.2). Constants never
            // reach here: they folded to Float literals at check time.
            hir::Callee::Math(f) => {
                let argv = self.eval_list(args, out, depth)?;
                let sep = if argv.is_empty() { "" } else { ", " };
                let call = format!("{}(ctx{sep}{argv})", f.symbol());
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            // Q25/Q26 Number and parsing intrinsics all call the shared
            // opaque runtime. Trap-capable entries carry the source
            // position assigned by this emitter.
            hir::Callee::Num(f) => {
                let argv = self.eval_list(args, out, depth)?;
                let call = if f.takes_pos_id() {
                    let pid = self.pos_id(pos);
                    format!("{}(ctx, {argv}, {pid}u)", f.symbol())
                } else {
                    format!("{}(ctx, {argv})", f.symbol())
                };
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(if f.returns_bool() {
                    format!("({result} != 0)")
                } else {
                    result
                })
            }
            // A Date intrinsic (stdlib.md §3) calls its opaque runtime
            // symbol; the value is its int64_t millisecond form. The
            // trapping entries carry a position id; `getTime` never
            // reaches here (folded at check time).
            hir::Callee::Date(f) => {
                use subscript_compiler::hir::DateFn as D;
                let call = match f {
                    D::New => {
                        let ms = self.eval(args.first().ok_or("Date arity")?, out, depth)?;
                        let pid = self.pos_id(pos);
                        format!("subscript_rt_date_new(ctx, {ms}, {pid}u)")
                    }
                    D::Utc => {
                        if args.len() != 7 {
                            return Err("Date.UTC arity (checker normalizes to 7)".to_string());
                        }
                        let argv = self.eval_list(args, out, depth)?;
                        let pid = self.pos_id(pos);
                        format!("subscript_rt_date_utc(ctx, {argv}, {pid}u)")
                    }
                    D::Now => "subscript_rt_date_now(ctx)".to_string(),
                    D::ToIso => {
                        let ms =
                            self.eval(args.first().ok_or("toISOString receiver")?, out, depth)?;
                        let pid = self.pos_id(pos);
                        format!("subscript_rt_date_to_iso(ctx, {ms}, {pid}u)")
                    }
                    accessor => {
                        let code = accessor
                            .field_code()
                            .ok_or_else(|| format!("Date intrinsic {accessor:?}"))?;
                        let ms =
                            self.eval(args.first().ok_or("Date accessor receiver")?, out, depth)?;
                        format!("subscript_rt_date_get(ctx, {ms}, {code}u)")
                    }
                };
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            // Leaves of the checker-generated, call-site-monomorphized
            // JSON serializer graph. Traversal is ordinary HIR; escaping,
            // number formatting, building, and cycle state live once in
            // the shared runtime.
            hir::Callee::Json(f) => {
                let argv = self.eval_list(args, out, depth)?;
                let pid = self.pos_id(pos);
                let call = if argv.is_empty() {
                    format!("{}(ctx, {pid}u)", f.symbol())
                } else {
                    format!("{}(ctx, {argv}, {pid}u)", f.symbol())
                };
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(if f.returns_bool() {
                    format!("({result} != 0)")
                } else {
                    result
                })
            }
            // A String method intrinsic (stdlib.md §8) calls its opaque
            // runtime symbol: `(ctx, recv, params…[, pos_id])`, the
            // receiver being the first HIR argument. A boolean result
            // arrives as int32_t 0/1 and is narrowed here.
            hir::Callee::Str(f) => {
                use subscript_compiler::hir::StrRet;
                if args.len() != 1 + f.params().len() {
                    return Err(format!("{} arity (checker normalizes)", f.name()));
                }
                // Receiver first, then the parameters, left to right.
                let argv = self.eval_list(args, out, depth)?;
                let call = if f.takes_pos_id() {
                    let pid = self.pos_id(pos);
                    format!("{}(ctx, {argv}, {pid}u)", f.symbol())
                } else {
                    format!("{}(ctx, {argv})", f.symbol())
                };
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(match f.ret() {
                    StrRet::Bool => format!("({result} != 0)"),
                    _ => result,
                })
            }
            hir::Callee::Regex(function) => {
                use subscript_compiler::hir::RegexFn as R;
                let expected = match function {
                    R::New | R::Test | R::Search | R::Split => 2,
                    R::Source | R::Flags => 1,
                    R::Replace | R::ReplaceAll => 3,
                    R::MatchStart | R::MatchEnd => 2,
                    other => return Err(format!("unknown RegexFn {other:?}")),
                };
                if args.len() != expected {
                    return Err(format!(
                        "{} arity (expected {expected}, got {})",
                        function.symbol(),
                        args.len()
                    ));
                }
                let argv = self.eval_list(args, out, depth)?;
                let call = if function.can_trap() {
                    let pid = self.pos_id(pos);
                    format!("{}(ctx, {argv}, {pid}u)", function.symbol())
                } else {
                    format!("{}(ctx, {argv})", function.symbol())
                };
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(if *function == R::Test {
                    format!("({result} != 0)")
                } else {
                    result
                })
            }
            // An Array method intrinsic (stdlib.md §9) calls its opaque
            // runtime symbol. The receiver is the first HIR argument;
            // element values the runtime receives are materialized into
            // temporaries and passed by pointer; a callback passes its
            // SubFn (code, env) halves; kind tags come from the shared
            // compiler mapping so the tiers cannot disagree.
            hir::Callee::Arr(f) => self.eval_arr_call(*f, args, ret_ty, pos, out, depth, checked),
            // Map/Set use the same opaque Context runtime in both tiers.
            // The concrete monomorphized key/value widths and key-kind
            // tag cross that boundary with construction.
            hir::Callee::Map(f) => self.eval_map_call(*f, args, ret_ty, pos, out, depth, checked),
            hir::Callee::Set(f) => self.eval_set_call(*f, args, ret_ty, pos, out, depth, checked),
            hir::Callee::Worker(function) => {
                self.eval_worker_call(*function, args, ret_ty, out, depth, checked)
            }
            other => Err(format!("callee {other:?} is outside the run set's scope")),
        }
    }

    /// Emits one Q35 call through the runtime's fixed worker/channel C API.
    fn eval_worker_call(
        &mut self,
        function: hir::WorkerFn,
        args: &[hir::Expr],
        ret_ty: &Type,
        out: &mut String,
        depth: usize,
        checked: bool,
    ) -> Result<String, String> {
        use hir::WorkerFn as W;
        let call = match function {
            W::Spawn(index) => {
                if !args.is_empty() {
                    return Err("Worker.spawn retained source arguments".to_string());
                }
                let entry = self
                    .module
                    .worker_entries
                    .get(index)
                    .ok_or_else(|| format!("worker entry index {index} out of range"))?;
                let input = self.class_name(entry.input)?;
                let output = self.class_name(entry.output)?;
                format!(
                    "subscript_rt_worker_spawn(ctx, subscript_init, subscript_worker_entry{index}, sizeof({input}), sizeof({output}))"
                )
            }
            W::Post
            | W::Poll
            | W::Close
            | W::Join
            | W::InboxWait
            | W::InboxPoll
            | W::OutboxPost => {
                let expected = if matches!(function, W::Post | W::OutboxPost) {
                    2
                } else {
                    1
                };
                if args.len() != expected {
                    return Err(format!(
                        "Worker intrinsic {function:?} has {} argument(s), expected {expected}",
                        args.len()
                    ));
                }
                let operands = self.eval_list(args, out, depth)?;
                let symbol = match function {
                    W::Post => "subscript_rt_worker_post",
                    W::Poll => "subscript_rt_worker_poll",
                    W::Close => "subscript_rt_worker_close",
                    W::Join => "subscript_rt_worker_join",
                    W::InboxWait => "subscript_rt_worker_inbox_wait",
                    W::InboxPoll => "subscript_rt_worker_inbox_poll",
                    W::OutboxPost => "subscript_rt_worker_outbox_post",
                    W::Spawn(_) => unreachable!("spawn handled above"),
                    _ => return Err(format!("unknown WorkerFn {function:?}")),
                };
                format!("{symbol}(ctx, {operands})")
            }
            _ => return Err(format!("unknown WorkerFn {function:?}")),
        };
        self.eval_call_with_policy(call, ret_ty, checked, out, depth)
    }

    /// Materializes one script-call result, checks the Context trap flag,
    /// and unwinds the current C frame before the result can be consumed.
    fn eval_checked_call(
        &mut self,
        call: String,
        ret_ty: &Type,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let result = if *ret_ty == Type::Void {
            let _ = writeln!(out, "{ind}{call};");
            String::new()
        } else {
            let temp = self.fresh_tmp();
            let cty = self.ctype(ret_ty)?;
            let _ = writeln!(out, "{ind}{cty} {temp} = {call};");
            temp
        };
        self.emit_trap_check(out, depth)?;
        Ok(result)
    }

    fn eval_site_checked_call(
        &mut self,
        call: String,
        ret_ty: &Type,
        site: &hir::TrapSite,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let result = if *ret_ty == Type::Void {
            let _ = writeln!(out, "{ind}{call};");
            String::new()
        } else {
            let temp = self.fresh_tmp();
            let cty = self.ctype(ret_ty)?;
            let _ = writeln!(out, "{ind}{cty} {temp} = {call};");
            temp
        };
        self.emit_trap_site(site, TrapOperand::Pending, out, depth)?;
        Ok(result)
    }

    fn eval_call_with_policy(
        &mut self,
        call: String,
        ret_ty: &Type,
        checked: bool,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        if checked {
            self.eval_checked_call(call, ret_ty, out, depth)
        } else {
            Ok(call)
        }
    }

    /// Emits an `Array` method intrinsic call (stdlib.md §9, Q22). The
    /// receiver is pinned to a temporary before any argument statement is
    /// emitted, so both tiers evaluate receiver-then-arguments
    /// ([`Self::eval_pinned`]); the in-place methods (`fill`, `reverse`,
    /// `sort`, `copyWithin`) then yield that same temporary as the
    /// expression's value.
    fn eval_arr_call(
        &mut self,
        f: hir::ArrFn,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
        out: &mut String,
        depth: usize,
        checked: bool,
    ) -> Result<String, String> {
        use hir::ArrFn as A;
        let ind = indent(depth);
        let recv = args.first().ok_or("array method receiver")?;
        let (elem, fixed_len) = match &recv.ty {
            Type::Array(e) => ((**e).clone(), None),
            Type::FixedArray(e, n) => ((**e).clone(), Some(*n)),
            other => return Err(format!("array method on {other:?}")),
        };
        let h = if fixed_len.is_some() {
            use hir::ExprKind as K;
            let addressable = matches!(
                recv.kind,
                K::Local(_) | K::Global(_) | K::Field { .. } | K::Index { .. } | K::This
            );
            let base = if addressable {
                self.eval(recv, out, depth)?
            } else {
                self.eval_pinned(recv, out, depth)?
            };
            let data = self.fresh_tmp();
            let _ = writeln!(out, "{ind}const void* {data} = (const void*)({base}).a;");
            data
        } else {
            self.eval_pinned(recv, out, depth)?
        };
        let arg_at = |args: &[hir::Expr], i: usize| -> Result<hir::Expr, String> {
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("{} arity (checker normalizes)", f.name()))
        };
        let callback_indexed = |callback: &hir::Expr| -> Result<u32, String> {
            let indexed_arity = f
                .callback_index_arity()
                .ok_or_else(|| format!("{} has no indexed callback shape", f.name()))?;
            let Type::Func(ft) = &callback.ty else {
                return Err(format!("{} callback is not a function", f.name()));
            };
            match ft.params.len() {
                arity if arity + 1 == indexed_arity => Ok(0),
                arity if arity == indexed_arity => Ok(1),
                arity => Err(format!(
                    "{} callback arity {arity} escaped the checker",
                    f.name()
                )),
            }
        };
        // Materializes one element value into an addressable temporary.
        let sym = if fixed_len.is_some() {
            f.fixed_symbol()
                .ok_or_else(|| format!("{} is not a FixedArray method", f.name()))?
        } else {
            f.symbol()
        };
        match f {
            A::IndexOf | A::LastIndexOf | A::Includes => {
                let kind = crate::layout::arr_elem_kind(self.module, &elem)?.code();
                let ect = self.ctype(&elem)?;
                let x = self.eval(&arg_at(args, 1)?, out, depth)?;
                let t = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{ect} {t} = {x};");
                let call = format!("{sym}(ctx, {h}, &{t}, {kind}u)");
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(if f == A::Includes {
                    format!("({result} != 0)")
                } else {
                    result
                })
            }
            A::Join => {
                let kind = crate::layout::arr_fmt_kind(&elem)?.code();
                let sep = self.eval(&arg_at(args, 1)?, out, depth)?;
                let pid = self.pos_id(pos);
                let call = format!("{sym}(ctx, {h}, {sep}, {kind}u, {pid}u)");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            A::Slice => {
                let start = self.eval_pinned(&arg_at(args, 1)?, out, depth)?;
                let end = self.eval(&arg_at(args, 2)?, out, depth)?;
                let pid = self.pos_id(pos);
                let call = format!("{sym}(ctx, {h}, {start}, {end}, {pid}u)");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            A::Fill => {
                // The receiver is already pinned and is also the
                // expression's value (in place, §9).
                let ect = self.ctype(&elem)?;
                let x = self.eval_pinned(&arg_at(args, 1)?, out, depth)?;
                let start = self.eval_pinned(&arg_at(args, 2)?, out, depth)?;
                let end = self.eval(&arg_at(args, 3)?, out, depth)?;
                let _ = writeln!(
                    out,
                    "{ind}{{ {ect} _e = {x}; {sym}(ctx, {h}, &_e, {start}, {end}); }}"
                );
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(h)
            }
            A::Reverse => {
                let _ = writeln!(out, "{ind}{sym}(ctx, {h});");
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(h)
            }
            A::Concat => {
                let other = self.eval(&arg_at(args, 1)?, out, depth)?;
                let pid = self.pos_id(pos);
                let call = format!("{sym}(ctx, {h}, {other}, {pid}u)");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            A::Splice => {
                let start = self.eval_pinned(&arg_at(args, 1)?, out, depth)?;
                let delete_count = self.eval(&arg_at(args, 2)?, out, depth)?;
                let pid = self.pos_id(pos);
                let call = format!("{sym}(ctx, {h}, {start}, {delete_count}, {pid}u)");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            A::Shift => {
                let ect = self.ctype(&elem)?;
                let value = self.fresh_tmp();
                let pid = self.pos_id(pos);
                let _ = writeln!(
                    out,
                    "{ind}{ect} {value}; {sym}(ctx, {h}, &{value}, {pid}u);"
                );
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(value)
            }
            A::Unshift => {
                let ect = self.ctype(&elem)?;
                let x = self.eval(&arg_at(args, 1)?, out, depth)?;
                let value = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{ect} {value} = {x};");
                let pid = self.pos_id(pos);
                let call = format!("{sym}(ctx, {h}, &{value}, {pid}u)");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            A::CopyWithin => {
                let target = self.eval_pinned(&arg_at(args, 1)?, out, depth)?;
                let start = self.eval_pinned(&arg_at(args, 2)?, out, depth)?;
                let end = self.eval(&arg_at(args, 3)?, out, depth)?;
                let _ = writeln!(out, "{ind}{sym}(ctx, {h}, {target}, {start}, {end});");
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(h)
            }
            A::ForEach | A::Filter | A::Some | A::Every | A::FindIndex => {
                let kind = crate::layout::arr_elem_kind(self.module, &elem)?.code();
                let callback = arg_at(args, 1)?;
                let indexed = callback_indexed(&callback)?;
                let fv = self.eval(&callback, out, depth)?;
                let tf = self.fresh_tmp();
                let _ = writeln!(out, "{ind}SubFn {tf} = {fv};");
                let fixed_shape = if let Some(n) = fixed_len {
                    format!(", {n}ull, sizeof({})", self.ctype(&elem)?)
                } else {
                    String::new()
                };
                let call = match (f, fixed_len) {
                    (A::Filter, Some(_)) => {
                        let pid = self.pos_id(pos);
                        format!(
                            "{sym}(ctx, {h}{fixed_shape}, {tf}.code, {tf}.env, {kind}u, {pid}u, {indexed}u)"
                        )
                    }
                    (A::Filter, None) => {
                        let pid = self.pos_id(pos);
                        format!("{sym}(ctx, {h}, {tf}.code, {tf}.env, {kind}u, {pid}u, {indexed}u)")
                    }
                    (_, Some(_)) => format!(
                        "{sym}(ctx, {h}{fixed_shape}, {tf}.code, {tf}.env, {kind}u, {indexed}u)"
                    ),
                    (_, None) => {
                        format!("{sym}(ctx, {h}, {tf}.code, {tf}.env, {kind}u, {indexed}u)")
                    }
                };
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(match f {
                    A::Some | A::Every => format!("({result} != 0)"),
                    _ => result,
                })
            }
            A::Sort => {
                let kind = crate::layout::arr_elem_kind(self.module, &elem)?.code();
                let fv = self.eval(&arg_at(args, 1)?, out, depth)?;
                let tf = self.fresh_tmp();
                let _ = writeln!(out, "{ind}SubFn {tf} = {fv};");
                let _ = writeln!(out, "{ind}{sym}(ctx, {h}, {tf}.code, {tf}.env, {kind}u);");
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(h)
            }
            A::Map => {
                let elem_kind = crate::layout::arr_elem_kind(self.module, &elem)?.code();
                let ret_elem = match ret_ty {
                    Type::Array(u) => (**u).clone(),
                    other => return Err(format!("map result {other:?}")),
                };
                let ret_kind = crate::layout::arr_elem_kind(self.module, &ret_elem)?.code();
                let rect = self.ctype(&ret_elem)?;
                let callback = arg_at(args, 1)?;
                let indexed = callback_indexed(&callback)?;
                let fv = self.eval(&callback, out, depth)?;
                let tf = self.fresh_tmp();
                let _ = writeln!(out, "{ind}SubFn {tf} = {fv};");
                let pid = self.pos_id(pos);
                let call = if let Some(n) = fixed_len {
                    let ect = self.ctype(&elem)?;
                    format!(
                        "{sym}(ctx, {h}, {n}ull, sizeof({ect}), {tf}.code, {tf}.env, {elem_kind}u, {ret_kind}u, sizeof({rect}), {pid}u, {indexed}u)"
                    )
                } else {
                    format!(
                        "{sym}(ctx, {h}, {tf}.code, {tf}.env, {elem_kind}u, {ret_kind}u, sizeof({rect}), {pid}u, {indexed}u)"
                    )
                };
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            A::Reduce | A::ReduceRight => {
                let elem_kind = crate::layout::arr_elem_kind(self.module, &elem)?.code();
                let acc_kind = crate::layout::arr_elem_kind(self.module, ret_ty)?.code();
                let acct = self.ctype(ret_ty)?;
                let callback = arg_at(args, 1)?;
                let indexed = callback_indexed(&callback)?;
                let fv = self.eval(&callback, out, depth)?;
                let tf = self.fresh_tmp();
                let _ = writeln!(out, "{ind}SubFn {tf} = {fv};");
                let init = self.eval(&arg_at(args, 2)?, out, depth)?;
                let acc = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{acct} {acc} = {init};");
                if let Some(n) = fixed_len {
                    let ect = self.ctype(&elem)?;
                    let _ = writeln!(
                        out,
                        "{ind}{sym}(ctx, {h}, {n}ull, sizeof({ect}), {tf}.code, {tf}.env, {elem_kind}u, {acc_kind}u, sizeof({acc}), &{acc}, {indexed}u);"
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "{ind}{sym}(ctx, {h}, {tf}.code, {tf}.env, {elem_kind}u, {acc_kind}u, sizeof({acc}), &{acc}, {indexed}u);"
                    );
                }
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(acc)
            }
            other => Err(format!("unknown ArrFn {other:?}")),
        }
    }

    /// Emits one monomorphized `Map<K, V>` intrinsic (stdlib.md §10).
    fn eval_map_call(
        &mut self,
        f: hir::MapFn,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
        out: &mut String,
        depth: usize,
        checked: bool,
    ) -> Result<String, String> {
        use hir::MapFn as M;
        let ind = indent(depth);
        if f == M::GroupBy {
            let (key, elem) = match (ret_ty, args.first().map(|arg| &arg.ty)) {
                (Type::Map(key, value), Some(Type::Array(elem))) => match &**value {
                    Type::Array(group_elem) if **group_elem == **elem => {
                        ((**key).clone(), (**elem).clone())
                    }
                    other => return Err(format!("Map.groupBy result value {other:?}")),
                },
                other => return Err(format!("Map.groupBy shape {other:?}")),
            };
            let items = self.eval_pinned(args.first().ok_or("Map.groupBy items")?, out, depth)?;
            let callback = self.eval(args.get(1).ok_or("Map.groupBy callback")?, out, depth)?;
            let ft = self.fresh_tmp();
            let _ = writeln!(out, "{ind}SubFn {ft} = {callback};");
            let bridge = self.emit_group_bridge(&elem, &key)?;
            let key_ct = self.ctype(&key)?;
            let key_kind = crate::layout::assoc_key_kind(self.module, &key)?.code();
            let pid = self.pos_id(pos);
            let call = format!(
                "subscript_rt_map_group_by(ctx, {items}, {ft}.code, {ft}.env, (const void*)&{bridge}, sizeof({key_ct}), {key_kind}u, {pid}u)"
            );
            return self.eval_call_with_policy(call, ret_ty, checked, out, depth);
        }
        let (key, value) = match f {
            M::New => match ret_ty {
                Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                other => return Err(format!("Map constructor result {other:?}")),
            },
            _ => match args.first().map(|a| &a.ty) {
                Some(Type::Map(k, v)) => ((**k).clone(), (**v).clone()),
                other => return Err(format!("Map method receiver {other:?}")),
            },
        };
        let key_ct = self.ctype(&key)?;
        let value_ct = self.ctype(&value)?;
        let key_kind = crate::layout::assoc_key_kind(self.module, &key)?.code();
        let arg_at = |i: usize| -> Result<&hir::Expr, String> {
            args.get(i)
                .ok_or_else(|| format!("{} arity (checker normalizes)", f.name()))
        };
        if f == M::New {
            let pid = self.pos_id(pos);
            let call = format!(
                "subscript_rt_map_new(ctx, sizeof({key_ct}), sizeof({value_ct}), {key_kind}u, {pid}u)"
            );
            return self.eval_call_with_policy(call, ret_ty, checked, out, depth);
        }

        let h = self.eval_pinned(arg_at(0)?, out, depth)?;
        match f {
            M::Size => {
                let call = format!("subscript_rt_map_size(ctx, {h})");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            M::Get => {
                let key_expr = self.eval(arg_at(1)?, out, depth)?;
                let kt = self.fresh_tmp();
                let result = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{key_ct} {kt} = {key_expr};");
                let _ = writeln!(out, "{ind}{value_ct} {result} = {{0}};");
                let _ = writeln!(
                    out,
                    "{ind}(void)subscript_rt_map_get(ctx, {h}, &{kt}, &{result});"
                );
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(result)
            }
            M::GetOr => {
                let key_expr = self.eval(arg_at(1)?, out, depth)?;
                let kt = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{key_ct} {kt} = {key_expr};");
                let fallback_expr = self.eval(arg_at(2)?, out, depth)?;
                let fallback = self.fresh_tmp();
                let result = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{value_ct} {fallback} = {fallback_expr};");
                let _ = writeln!(out, "{ind}{value_ct} {result} = {{0}};");
                let _ = writeln!(
                    out,
                    "{ind}subscript_rt_map_get_or(ctx, {h}, &{kt}, &{fallback}, &{result});"
                );
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(result)
            }
            M::Set => {
                let key_expr = self.eval(arg_at(1)?, out, depth)?;
                let kt = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{key_ct} {kt} = {key_expr};");
                let value_expr = self.eval(arg_at(2)?, out, depth)?;
                let vt = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{value_ct} {vt} = {value_expr};");
                let pid = self.pos_id(pos);
                let _ = writeln!(
                    out,
                    "{ind}subscript_rt_map_set(ctx, {h}, &{kt}, &{vt}, {pid}u);"
                );
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(h)
            }
            M::Has | M::Delete => {
                let key_expr = self.eval(arg_at(1)?, out, depth)?;
                let kt = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{key_ct} {kt} = {key_expr};");
                let call = format!("{}(ctx, {h}, &{kt})", f.symbol());
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(format!("({result} != 0)"))
            }
            M::Clear => {
                let call = format!("subscript_rt_map_clear(ctx, {h})");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            M::ForEach => {
                let callback = self.eval(arg_at(1)?, out, depth)?;
                let ft = self.fresh_tmp();
                let _ = writeln!(out, "{ind}SubFn {ft} = {callback};");
                let bridge = self.emit_assoc_bridge(&key, Some(&value))?;
                let call = format!(
                    "subscript_rt_map_for_each(ctx, {h}, {ft}.code, {ft}.env, (const void*)&{bridge})"
                );
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            M::GroupBy => Err("Map.GroupBy reached receiver lowering".to_string()),
            other => Err(format!("unknown MapFn {other:?}")),
        }
    }

    /// Emits one monomorphized `Set<K>` intrinsic (stdlib.md §10).
    fn eval_set_call(
        &mut self,
        f: hir::SetFn,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
        out: &mut String,
        depth: usize,
        checked: bool,
    ) -> Result<String, String> {
        use hir::SetFn as S;
        let ind = indent(depth);
        let key = match f {
            S::New => match ret_ty {
                Type::Set(k) => (**k).clone(),
                other => return Err(format!("Set constructor result {other:?}")),
            },
            _ => match args.first().map(|a| &a.ty) {
                Some(Type::Set(k)) => (**k).clone(),
                other => return Err(format!("Set method receiver {other:?}")),
            },
        };
        let key_ct = self.ctype(&key)?;
        let key_kind = crate::layout::assoc_key_kind(self.module, &key)?.code();
        let arg_at = |i: usize| -> Result<&hir::Expr, String> {
            args.get(i)
                .ok_or_else(|| format!("{} arity (checker normalizes)", f.name()))
        };
        if f == S::New {
            let pid = self.pos_id(pos);
            let call = format!("subscript_rt_set_new(ctx, sizeof({key_ct}), {key_kind}u, {pid}u)");
            return self.eval_call_with_policy(call, ret_ty, checked, out, depth);
        }

        let h = self.eval_pinned(arg_at(0)?, out, depth)?;
        match f {
            S::Size => {
                let call = format!("subscript_rt_set_size(ctx, {h})");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            S::Add => {
                let key_expr = self.eval(arg_at(1)?, out, depth)?;
                let kt = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{key_ct} {kt} = {key_expr};");
                let pid = self.pos_id(pos);
                let _ = writeln!(out, "{ind}subscript_rt_set_add(ctx, {h}, &{kt}, {pid}u);");
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(h)
            }
            S::Has | S::Delete => {
                let key_expr = self.eval(arg_at(1)?, out, depth)?;
                let kt = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{key_ct} {kt} = {key_expr};");
                let call = format!("{}(ctx, {h}, &{kt})", f.symbol());
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(format!("({result} != 0)"))
            }
            S::Clear => {
                let call = format!("subscript_rt_set_clear(ctx, {h})");
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            S::ForEach => {
                let callback = self.eval(arg_at(1)?, out, depth)?;
                let ft = self.fresh_tmp();
                let _ = writeln!(out, "{ind}SubFn {ft} = {callback};");
                let bridge = self.emit_assoc_bridge(&key, None)?;
                let call = format!(
                    "subscript_rt_set_for_each(ctx, {h}, {ft}.code, {ft}.env, (const void*)&{bridge})"
                );
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            S::Union | S::Intersection | S::Difference | S::SymmetricDifference => {
                let other = self.eval_pinned(arg_at(1)?, out, depth)?;
                let pid = self.pos_id(pos);
                let call = format!("{}(ctx, {h}, {other}, {pid}u)", f.symbol());
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            S::IsSubsetOf | S::IsSupersetOf | S::IsDisjointFrom => {
                let other = self.eval_pinned(arg_at(1)?, out, depth)?;
                let call = format!("{}(ctx, {h}, {other})", f.symbol());
                let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
                Ok(format!("({result} != 0)"))
            }
            other => Err(format!("unknown SetFn {other:?}")),
        }
    }

    /// Defines the typed C callback adapter expected by the opaque
    /// association runtime. Map callbacks receive `(value, key)` and Set
    /// callbacks receive `(key)`; the runtime performs the post-return
    /// trap check.
    fn emit_assoc_bridge(&mut self, key: &Type, value: Option<&Type>) -> Result<String, String> {
        let n = self.lambda;
        self.lambda += 1;
        let name = format!("subscript_assoc_bridge{n}");
        let key_ct = self.ctype(key)?;
        let (sig, call) = if let Some(value) = value {
            let value_ct = self.ctype(value)?;
            (
                format!(
                    "static void {name}(void* ctx, const void* code, const void* env, const void* value, const void* key)"
                ),
                format!(
                    "((void(*)(void*, void*, {value_ct}, {key_ct}))code)(ctx, (void*)env, *((const {value_ct}*)value), *((const {key_ct}*)key))"
                ),
            )
        } else {
            (
                format!(
                    "static void {name}(void* ctx, const void* code, const void* env, const void* key)"
                ),
                format!(
                    "((void(*)(void*, void*, {key_ct}))code)(ctx, (void*)env, *((const {key_ct}*)key))"
                ),
            )
        };
        let _ = writeln!(self.protos, "{sig};");
        let _ = writeln!(self.helpers, "{sig} {{ {call}; }}");
        Ok(name)
    }

    /// Defines the exact runtime worker-entry ABI adapter for one checked
    /// directly named module function.
    fn emit_worker_entry_adapter(&mut self, index: usize) -> Result<(), String> {
        let name = format!("subscript_worker_entry{index}");
        if !self.wrappers.insert(name.clone()) {
            return Ok(());
        }
        let entry = self
            .module
            .worker_entries
            .get(index)
            .ok_or_else(|| format!("worker entry index {index} out of range"))?;
        let target = self.hir_fn(&entry.function)?;
        if target.is_async
            || target.is_generator
            || target.ret != Type::Void
            || target.params.len() != 2
        {
            return Err(format!(
                "worker entry `{}` lost its checked shape",
                entry.function
            ));
        }
        let signature = format!(
            "static void {name}(subscript_rt_context* ctx, subscript_rt_worker_inbox* inbox, subscript_rt_worker_outbox* outbox)"
        );
        let target_name = Emitter::fn_c_name(target);
        let _ = writeln!(self.protos, "{signature};");
        let _ = writeln!(
            self.helpers,
            "{signature} {{ {target_name}(ctx, inbox, outbox); }}"
        );
        Ok(())
    }

    /// Defines the typed C callback adapter used by `Map.groupBy`.
    /// The runtime supplies an owned element copy and a key result slot.
    fn emit_group_bridge(&mut self, elem: &Type, key: &Type) -> Result<String, String> {
        let n = self.lambda;
        self.lambda += 1;
        let name = format!("subscript_group_bridge{n}");
        let elem_ct = self.ctype(elem)?;
        let key_ct = self.ctype(key)?;
        let sig = format!(
            "static void {name}(void* ctx, const void* code, const void* env, const void* element, void* key_out)"
        );
        let call = format!(
            "*(({key_ct}*)key_out) = (({key_ct}(*)(void*, void*, {elem_ct}))code)(ctx, (void*)env, *((const {elem_ct}*)element))"
        );
        let _ = writeln!(self.protos, "{sig};");
        let _ = writeln!(self.helpers, "{sig} {{ {call}; }}");
        Ok(name)
    }

    /// A `Struct | null` boundary pointer slot (a nullable value class):
    /// Q13's single implicit address-of position.
    fn is_boundary_struct_ptr(&self, ty: &Type) -> Result<bool, String> {
        if let Type::Nullable(inner) = ty {
            if let Type::Class(id) = **inner {
                return self.is_value_class(id);
            }
        }
        Ok(false)
    }

    fn boundary_struct_ptr_id(&self, ty: &Type) -> Result<Option<ClassId>, String> {
        let Type::Nullable(inner) = ty else {
            return Ok(None);
        };
        let Type::Class(id) = &**inner else {
            return Ok(None);
        };
        Ok(self.is_value_class(*id)?.then_some(*id))
    }

    /// Rehomes borrowed pointers reachable from a returned boundary value
    /// before its function frame dies. These language-layout copies live in
    /// the active foreign-call scratch scope, or until Context teardown when
    /// the boundary value is returned outside such a scope.
    fn emit_stabilize_boundary_return_value(
        &mut self,
        cid: ClassId,
        address: &str,
        pos: &Pos,
        out: &mut String,
        depth: usize,
        visiting: &mut HashSet<ClassId>,
    ) -> Result<(), String> {
        if !visiting.insert(cid) {
            return Ok(());
        }
        let class = self.class(cid)?.clone();
        let ind = indent(depth);
        for field in &class.fields {
            let name = sanitize(&field.name);
            if let Some(target_cid) = self.boundary_struct_ptr_id(&field.ty)? {
                let target_type = self.class_name(target_cid)?;
                let cast = self
                    .boundary_ptr_cast(&field.ty)?
                    .ok_or_else(|| "boundary return pointer lacks a header cast".to_string())?;
                let stable = self.fresh_tmp();
                let _ = writeln!(out, "{ind}if (({address})->{name} != NULL) {{");
                let pos_id = self.pos_id(pos);
                let _ = writeln!(
                    out,
                    "{}{target_type}* {stable} = ({target_type}*)subscript_rt_boundary_scratch_alloc(ctx, (uint64_t)sizeof({target_type}), {pos_id}u);",
                    indent(depth + 1),
                );
                self.emit_trap_check(out, depth + 1)?;
                let _ = writeln!(
                    out,
                    "{}memcpy({stable}, ({address})->{name}, sizeof *{stable});",
                    indent(depth + 1),
                );
                let _ = writeln!(
                    out,
                    "{}({address})->{name} = ({cast}){stable};",
                    indent(depth + 1),
                );
                self.emit_stabilize_boundary_return_value(
                    target_cid,
                    &stable,
                    pos,
                    out,
                    depth + 1,
                    visiting,
                )?;
                let _ = writeln!(out, "{ind}}}");
                continue;
            }
            if let Type::Class(inner) = &field.ty {
                if self.is_value_class(*inner)? {
                    self.emit_stabilize_boundary_return_value(
                        *inner,
                        &format!("&({address})->{name}"),
                        pos,
                        out,
                        depth,
                        visiting,
                    )?;
                    continue;
                }
            }
            if let Type::Array(element) = &field.ty {
                let Type::Class(element_cid) = &**element else {
                    continue;
                };
                if !self.is_value_class(*element_cid)? {
                    continue;
                }
                let element_type = self.class_name(*element_cid)?;
                let count = self.fresh_tmp();
                let data = self.fresh_tmp();
                let index = self.fresh_tmp();
                let _ = writeln!(
                    out,
                    "{ind}int32_t {count} = subscript_rt_array_len(ctx, ({address})->{name});"
                );
                let _ = writeln!(
                    out,
                    "{ind}{element_type}* {data} = ({element_type}*)subscript_rt_array_data(ctx, ({address})->{name});"
                );
                let _ = writeln!(
                    out,
                    "{ind}for (int32_t {index} = 0; {index} < {count}; {index}++) {{"
                );
                self.emit_stabilize_boundary_return_value(
                    *element_cid,
                    &format!("&{data}[{index}]"),
                    pos,
                    out,
                    depth + 1,
                    visiting,
                )?;
                let _ = writeln!(out, "{ind}}}");
            }
        }
        visiting.remove(&cid);
        Ok(())
    }

    fn boundary_struct_needs_scratch(&self, cid: ClassId) -> Result<bool, String> {
        self.boundary_struct_needs_scratch_inner(cid, &mut HashSet::new())
    }

    fn boundary_struct_needs_scratch_inner(
        &self,
        cid: ClassId,
        visiting: &mut HashSet<ClassId>,
    ) -> Result<bool, String> {
        if !visiting.insert(cid) {
            return Ok(false);
        }
        let class = self.class(cid)?;
        if !class.is_boundary {
            visiting.remove(&cid);
            return Ok(false);
        }
        for field in &class.fields {
            let lowered = match &field.ty {
                Type::Str | Type::Array(_) => true,
                Type::Class(inner) if self.is_value_class(*inner)? => {
                    self.boundary_struct_needs_scratch_inner(*inner, visiting)?
                }
                Type::Nullable(inner) => match &**inner {
                    // A pointer member activates scratch when its target has
                    // an absorbed lowering. Once its parent is in scratch,
                    // §33 also rebuilds plain targets beside that lowering.
                    Type::Class(inner) if self.is_value_class(*inner)? => {
                        self.boundary_struct_needs_scratch_inner(*inner, visiting)?
                    }
                    _ => false,
                },
                _ => false,
            };
            if lowered {
                visiting.remove(&cid);
                return Ok(true);
            }
        }
        visiting.remove(&cid);
        Ok(false)
    }

    /// True when an aggregate already being rebuilt must recurse rather
    /// than copy its language bytes. Unlike the root scratch predicate, a
    /// plain struct-pointer member is sufficient here because §33 must
    /// redirect it to child scratch once construction is active.
    fn boundary_struct_requires_recursive_build(&self, cid: ClassId) -> Result<bool, String> {
        Ok(self.boundary_struct_needs_scratch(cid)?
            || self.boundary_struct_contains_pointer_member(cid, &mut HashSet::new())?)
    }

    fn boundary_struct_contains_pointer_member(
        &self,
        cid: ClassId,
        visiting: &mut HashSet<ClassId>,
    ) -> Result<bool, String> {
        if !visiting.insert(cid) {
            return Ok(false);
        }
        for field in &self.class(cid)?.fields {
            let contains = match &field.ty {
                Type::Nullable(inner) => match &**inner {
                    Type::Class(inner) if self.is_value_class(*inner)? => true,
                    _ => false,
                },
                Type::Class(inner) if self.is_value_class(*inner)? => {
                    self.boundary_struct_contains_pointer_member(*inner, visiting)?
                }
                Type::Array(inner) => match &**inner {
                    Type::Class(inner) if self.is_value_class(*inner)? => {
                        self.boundary_struct_contains_pointer_member(*inner, visiting)?
                    }
                    _ => false,
                },
                _ => false,
            };
            if contains {
                visiting.remove(&cid);
                return Ok(true);
            }
        }
        visiting.remove(&cid);
        Ok(false)
    }

    fn boundary_struct_needs_scratch_array(&self, cid: ClassId) -> Result<bool, String> {
        self.boundary_struct_needs_scratch_array_inner(cid, &mut HashSet::new())
    }

    fn boundary_struct_needs_scratch_array_inner(
        &self,
        cid: ClassId,
        visiting: &mut HashSet<ClassId>,
    ) -> Result<bool, String> {
        if !visiting.insert(cid) {
            return Ok(false);
        }
        for field in &self.class(cid)?.fields {
            let uses = match &field.ty {
                Type::Array(element) => match &**element {
                    Type::Class(element_cid) if self.is_value_class(*element_cid)? => {
                        self.boundary_struct_requires_recursive_build(*element_cid)?
                    }
                    _ => false,
                },
                Type::Class(inner) if self.is_value_class(*inner)? => {
                    self.boundary_struct_needs_scratch_array_inner(*inner, visiting)?
                }
                // A recursively lowered pointer target is allocated from
                // the same re-entrant-safe call-duration scope as §32's
                // scratch arrays.
                Type::Nullable(inner) => match &**inner {
                    Type::Class(inner) if self.is_value_class(*inner)? => true,
                    _ => false,
                },
                _ => false,
            };
            if uses {
                visiting.remove(&cid);
                return Ok(true);
            }
        }
        visiting.remove(&cid);
        Ok(false)
    }

    fn boundary_type_needs_scratch_array(&self, ty: &Type) -> Result<bool, String> {
        match ty {
            Type::Nullable(inner) => match &**inner {
                Type::Class(cid) if self.is_value_class(*cid)? => {
                    if self.boundary_struct_needs_scratch(*cid)? {
                        self.boundary_struct_needs_scratch_array(*cid)
                    } else {
                        Ok(false)
                    }
                }
                _ => self.boundary_type_needs_scratch_array(inner),
            },
            Type::Class(cid) if self.is_value_class(*cid)? => {
                self.boundary_struct_needs_scratch_array(*cid)
            }
            Type::Array(element) => match &**element {
                Type::Class(cid) if self.is_value_class(*cid)? => {
                    self.boundary_struct_requires_recursive_build(*cid)
                }
                _ => Ok(false),
            },
            _ => Ok(false),
        }
    }

    /// For a boundary struct-pointer target (`Struct | null`), the foreign
    /// header pointer type an emitted pointer expression is cast to
    /// (`HeaderStruct*`) — the header struct name, not the language name.
    /// `None` when `ty` is not a boundary struct pointer.
    fn boundary_ptr_cast(&self, ty: &Type) -> Result<Option<String>, String> {
        if let Type::Nullable(inner) = ty {
            if let Type::Class(cid) = **inner {
                if self.is_value_class(cid)? {
                    return Ok(Some(format!("{}*", self.class(cid)?.name)));
                }
            }
        }
        Ok(None)
    }

    /// The pointer expression for a boundary struct-pointer target, before
    /// the header-type cast: a value struct's storage address (chain-slot
    /// address-of), or an existing pointer (`null`, or a `Struct | null`
    /// value).
    fn boundary_struct_ptr_expr(
        &mut self,
        arg: &hir::Expr,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        if let Type::Class(cid) = arg.ty {
            if self.is_value_class(cid)? {
                return self.value_recv_ptr(arg, cid, out, depth);
            }
        }
        self.eval(arg, out, depth)
    }

    /// Emits a foreign C-ABI call (`Callee::Foreign`, P5.2b): a direct
    /// call of the header symbol with each argument marshaled per Q13. The
    /// C compiler resolves the ABI; the host supplies the linked symbol.
    fn eval_foreign_call(
        &mut self,
        name: &str,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
        checked: bool,
    ) -> Result<String, String> {
        if !self.foreign_symbols.iter().any(|symbol| symbol == name) {
            self.foreign_symbols.push(name.to_string());
        }
        let ff = self
            .module
            .foreign_fns
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("unknown foreign function `{name}`"))?
            .clone();
        let mut uses_scratch_allocations = false;
        for parameter in &ff.params {
            uses_scratch_allocations |= self.boundary_type_needs_scratch_array(&parameter.ty)?;
        }
        let scratch_mark = if uses_scratch_allocations {
            let mark = self.fresh_tmp();
            let _ = writeln!(
                out,
                "{}uint64_t {mark} = subscript_rt_boundary_scratch_mark(ctx);",
                indent(depth)
            );
            Some(mark)
        } else {
            None
        };
        // Arguments are marshaled left to right (the dev tier marshals in
        // argument order). Each lands in its own statement buffer; an
        // argument whose marshaled form is not already a read of bound
        // temporaries is bound when a later argument emitted statements.
        let mut bufs: Vec<String> = Vec::new();
        let mut part_groups: Vec<Vec<String>> = Vec::new();
        let mut pin_cts: Vec<Option<String>> = Vec::new();
        let mut boundary_writebacks = Vec::new();
        for (p, a) in ff.params.iter().zip(args) {
            let mut buf = String::new();
            let (expr, pin_ct) = self.marshal_foreign_c_arg(
                &ff.name,
                p,
                a,
                &mut buf,
                depth,
                &mut boundary_writebacks,
                scratch_mark.as_deref(),
                pos,
            )?;
            bufs.push(buf);
            part_groups.push(expr);
            pin_cts.push(pin_ct);
        }
        let mut later = false;
        let mut pin = vec![false; part_groups.len()];
        for i in (0..part_groups.len()).rev() {
            pin[i] = later;
            later = later || !bufs[i].is_empty();
        }
        for i in 0..part_groups.len() {
            out.push_str(&bufs[i]);
            if let (true, Some(ct)) = (pin[i], pin_cts[i].clone()) {
                if part_groups[i].len() != 1 {
                    return Err(format!(
                        "internal error: foreign argument group for `{}` cannot be pinned as one {ct}",
                        ff.params[i].name
                    ));
                }
                let t = self.fresh_tmp();
                let _ = writeln!(out, "{}{ct} {t} = {};", indent(depth), part_groups[i][0]);
                part_groups[i][0] = t;
            }
        }
        let parts: Vec<String> = part_groups.into_iter().flatten().collect();
        let call = format!("{name}({})", parts.join(", "));
        if !boundary_writebacks.is_empty() {
            let result = if let Type::Class(cid) = &ff.ret {
                if self.is_value_class(*cid)? {
                    let ind = indent(depth);
                    let header_ty = self.class(*cid)?.name.clone();
                    let lang_ty = self.class_name(*cid)?;
                    let header_result = self.fresh_tmp();
                    let language_result = self.fresh_tmp();
                    let _ = writeln!(out, "{ind}{header_ty} {header_result} = {call};");
                    if let Some(mark) = &scratch_mark {
                        let _ = writeln!(
                            out,
                            "{}subscript_rt_boundary_scratch_release(ctx, {mark});",
                            indent(depth)
                        );
                    }
                    if checked {
                        self.emit_trap_check(out, depth)?;
                    }
                    let _ = writeln!(
                        out,
                        "{ind}{lang_ty} {language_result}; memcpy(&{language_result}, &{header_result}, sizeof {language_result});"
                    );
                    language_result
                } else {
                    let result = self.emit_foreign_call_result(
                        call,
                        ret_ty,
                        checked && scratch_mark.is_none(),
                        out,
                        depth,
                    )?;
                    if let Some(mark) = &scratch_mark {
                        let _ = writeln!(
                            out,
                            "{}subscript_rt_boundary_scratch_release(ctx, {mark});",
                            indent(depth)
                        );
                        if checked {
                            self.emit_trap_check(out, depth)?;
                        }
                    }
                    result
                }
            } else {
                let result = self.emit_foreign_call_result(
                    call,
                    ret_ty,
                    checked && scratch_mark.is_none(),
                    out,
                    depth,
                )?;
                if let Some(mark) = &scratch_mark {
                    let _ = writeln!(
                        out,
                        "{}subscript_rt_boundary_scratch_release(ctx, {mark});",
                        indent(depth)
                    );
                    if checked {
                        self.emit_trap_check(out, depth)?;
                    }
                }
                result
            };
            for writeback in boundary_writebacks {
                self.emit_string_field_boundary_writeback(writeback, pos, out, depth)?;
            }
            return Ok(result);
        }
        // A by-value boundary-struct return (§14.2): the C compiler performs
        // the struct-return ABI; the returned header struct is copied into a
        // language value class of identical layout (invariant 1), so callers
        // see an ordinary in-language value they can read fields from.
        if let Type::Class(cid) = &ff.ret {
            if self.is_value_class(*cid)? {
                let ind = indent(depth);
                let header_ty = self.class(*cid)?.name.clone();
                let lang_ty = self.class_name(*cid)?;
                let h = self.fresh_tmp();
                let t = self.fresh_tmp();
                let _ = writeln!(out, "{ind}{header_ty} {h} = {call};");
                if let Some(mark) = &scratch_mark {
                    let _ = writeln!(
                        out,
                        "{}subscript_rt_boundary_scratch_release(ctx, {mark});",
                        indent(depth)
                    );
                }
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                let _ = writeln!(out, "{ind}{lang_ty} {t}; memcpy(&{t}, &{h}, sizeof {t});");
                return Ok(t);
            }
        }
        if let Some(mark) = &scratch_mark {
            let result = self.emit_foreign_call_result(call, ret_ty, false, out, depth)?;
            let _ = writeln!(
                out,
                "{}subscript_rt_boundary_scratch_release(ctx, {mark});",
                indent(depth)
            );
            if checked {
                self.emit_trap_check(out, depth)?;
            }
            Ok(result)
        } else {
            let result = self.eval_call_with_policy(call, ret_ty, checked, out, depth)?;
            if let Type::StringAlias(alias) = ff.ret {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::WireEnumValue { alias: site_alias, .. } if *site_alias == alias),
                    "wire-enum foreign return has no HIR trap site",
                )?;
                self.validate_wire_alias(alias, &result, site, out, depth)
            } else {
                Ok(result)
            }
        }
    }

    /// Validates one C-entered wire result and preserves its identity
    /// representation (§52.1/§52.3).
    fn validate_wire_alias(
        &mut self,
        alias: subscript_compiler::StringAliasId,
        wire: &str,
        site: &hir::TrapSite,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let values = self
            .module
            .string_aliases
            .get(alias.0)
            .ok_or_else(|| "wire-enum alias id is out of range".to_string())?
            .wire_values
            .as_ref()
            .ok_or_else(|| "foreign string alias return has no wire mapping".to_string())?;
        let valid = values
            .iter()
            .map(|value| format!("{wire} == {value}"))
            .collect::<Vec<_>>()
            .join(" || ");
        self.emit_trap_site(
            site,
            TrapOperand::WireValue {
                wire: wire.to_string(),
                valid,
            },
            out,
            depth,
        )?;
        Ok(wire.to_string())
    }

    /// Materializes a foreign-call result before pointer scratch copy-back.
    /// Foreign calls normally carry a HIR call site, but keeping the unchecked
    /// form well-formed makes this helper total for hand-built HIR tests.
    fn emit_foreign_call_result(
        &mut self,
        call: String,
        ret_ty: &Type,
        checked: bool,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        if checked {
            return self.eval_checked_call(call, ret_ty, out, depth);
        }
        let ind = indent(depth);
        if *ret_ty == Type::Void {
            let _ = writeln!(out, "{ind}{call};");
            Ok(String::new())
        } else {
            let result = self.fresh_tmp();
            let ctype = self.ctype(ret_ty)?;
            let _ = writeln!(out, "{ind}{ctype} {result} = {call};");
            Ok(result)
        }
    }

    /// Marshals one argument of a foreign call to a C expression (Q13),
    /// emitting any needed temporaries into `out`.
    ///
    /// Returns one or more C argument expressions and, when the sole
    /// expression still evaluates something of its own, the C type it can
    /// be bound to so the caller can fix its evaluation order. A §27 scalar
    /// pair returns two expressions (count then pointer), both reading
    /// temporaries bound here. `None` means the expression(s) only read
    /// temporaries this function already bound (or immutable state), so
    /// where they are evaluated cannot be observed.
    fn marshal_foreign_c_arg(
        &mut self,
        function_name: &str,
        parameter: &hir::Param,
        arg: &hir::Expr,
        out: &mut String,
        depth: usize,
        boundary_writebacks: &mut Vec<BoundaryPtrWriteback>,
        scratch_mark: Option<&str>,
        call_pos: &Pos,
    ) -> Result<(Vec<String>, Option<String>), String> {
        let ind = indent(depth);
        match &parameter.ty {
            Type::StringAlias(alias) => {
                let definition = self
                    .module
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| "wire-enum alias id is out of range".to_string())?;
                if definition.wire_values.is_none() {
                    return Err(format!(
                        "internal error at foreign function `{function_name}` parameter `{}`: plain string alias reached the boundary",
                        parameter.name
                    ));
                }
                let value = self.eval(arg, out, depth)?;
                Ok((vec![value], Some("int32_t".to_string())))
            }
            Type::Str => {
                let aggregate = match &parameter.foreign_provenance {
                    Some(hir::ForeignTypeProvenance::StringView { aggregate }) => aggregate.clone(),
                    None => {
                        return Err(format!(
                            "internal error at foreign function `{function_name}` parameter `{}`: missing string-view provenance",
                            parameter.name
                        ));
                    }
                    Some(other) => {
                        return Err(format!(
                            "internal error at foreign function `{function_name}` parameter `{}`: expected string-view provenance, found {other:?}",
                            parameter.name
                        ));
                    }
                };
                // Strings are immutable, so the data/length reads below
                // are stable wherever they land.
                let h = self.eval(arg, out, depth)?;
                let t = self.fresh_tmp();
                let _ = writeln!(out, "{ind}void* {t} = {h};");
                Ok((
                    vec![format!(
                        "(({aggregate}){{ (const char*)subscript_rt_str_data(ctx, {t}), (size_t)subscript_rt_str_len(ctx, {t}) }})"
                    )],
                    None,
                ))
            }
            Type::Array(element_ty) => {
                let provenance = parameter.foreign_provenance.clone().ok_or_else(|| {
                    format!(
                        "internal error at foreign function `{function_name}` parameter `{}`: missing array boundary provenance",
                        parameter.name
                    )
                })?;
                let h = self.eval(arg, out, depth)?;
                let t = self.fresh_tmp();
                let _ = writeln!(out, "{ind}void* {t} = {h};");
                // The data pointer and count are read here, not at the
                // call: a later argument may grow the array and move its
                // storage, and the dev tier reads them at this point. A
                // recursively lowered struct element uses a call-duration
                // C-layout scratch array (§32).
                let lowered_element = match &**element_ty {
                    Type::Class(cid)
                        if self.is_value_class(*cid)?
                            && self.boundary_struct_requires_recursive_build(*cid)? =>
                    {
                        Some(*cid)
                    }
                    _ => None,
                };
                let (d, n) = if let Some(element_cid) = lowered_element {
                    self.marshal_boundary_array_c(
                        element_cid,
                        &t,
                        out,
                        depth,
                        scratch_mark.ok_or_else(|| {
                            "recursive boundary array lowering lacks a scratch scope".to_string()
                        })?,
                        call_pos,
                    )?
                } else {
                    let d = self.fresh_tmp();
                    let n = self.fresh_tmp();
                    let _ = writeln!(
                        out,
                        "{ind}const void* {d} = subscript_rt_array_data(ctx, {t});"
                    );
                    let _ = writeln!(
                        out,
                        "{ind}size_t {n} = (size_t)subscript_rt_array_len(ctx, {t});"
                    );
                    (d, n)
                };
                match provenance {
                    hir::ForeignTypeProvenance::Descriptor {
                        aggregate,
                        element,
                        element_const,
                    } => {
                        // A (pointer, count) descriptor is passed BY VALUE,
                        // so the compound literal names the exact header
                        // aggregate type. Mutable pointers cast away the
                        // runtime accessor's const-qualified view.
                        let elem_cast = if element_const {
                            String::new()
                        } else {
                            format!("({element}*)")
                        };
                        Ok((
                            vec![format!("(({aggregate}){{ {elem_cast}{d}, {n} }})")],
                            None,
                        ))
                    }
                    hir::ForeignTypeProvenance::ScalarPair {
                        element,
                        element_const,
                    } => {
                        // §27 reconstructs the original two C parameters,
                        // count first. The pointer addresses the language
                        // array's own backing store, so mutable callee writes
                        // are immediately visible after the call.
                        let pointer = if element_const {
                            format!("(const {element}*){d}")
                        } else {
                            format!("({element}*){d}")
                        };
                        Ok((vec![n, pointer], None))
                    }
                    other => Err(format!(
                        "internal error at foreign function `{function_name}` parameter `{}`: expected array boundary provenance, found {other:?}",
                        parameter.name
                    )),
                }
            }
            Type::Class(id) if self.is_value_class(*id)? => {
                let header = self.class(*id)?.name.clone();
                let expr = self.marshal_boundary_c_struct(*id, arg, out, depth)?;
                Ok((vec![expr], Some(header)))
            }
            _ if self.is_boundary_struct_ptr(&parameter.ty)? => {
                // Struct | null pointer: address of a value struct's
                // storage (chain-slot address-of), or an existing pointer
                // (`null`, or a `Struct | null` value). Cast to the foreign
                // header pointer type: the language struct is layout-
                // identical (invariant 1) so the pointer is ABI-safe, but
                // nominally distinct, and the cast documents that intent
                // and compiles clean on any clang.
                let cid = self
                    .boundary_struct_ptr_id(&parameter.ty)?
                    .ok_or_else(|| "boundary struct ptr lacks a class id".to_string())?;
                if self.boundary_struct_needs_scratch(cid)? {
                    let (expr, writeback) = self.marshal_string_field_boundary_c_ptr(
                        cid,
                        arg,
                        out,
                        depth,
                        scratch_mark,
                        call_pos,
                    )?;
                    boundary_writebacks.push(writeback);
                    Ok((vec![expr], None))
                } else {
                    let cast = self
                        .boundary_ptr_cast(&parameter.ty)?
                        .ok_or_else(|| "boundary struct ptr lacks a header type".to_string())?;
                    let expr = self.boundary_struct_ptr_expr(arg, out, depth)?;
                    Ok((vec![format!("({cast})({expr})")], Some(cast)))
                }
            }
            _ => {
                let v = self.eval(arg, out, depth)?;
                let ct = self.ctype(&parameter.ty)?;
                Ok((vec![v], Some(ct)))
            }
        }
    }

    /// Builds one actual-header-layout scratch record for a pointer-passed
    /// boundary struct containing direct string-view fields. The string data
    /// pointers borrow the language string bytes for this call only.
    fn marshal_string_field_boundary_c_ptr(
        &mut self,
        cid: ClassId,
        arg: &hir::Expr,
        out: &mut String,
        depth: usize,
        scratch_mark: Option<&str>,
        call_pos: &Pos,
    ) -> Result<(String, BoundaryPtrWriteback), String> {
        let ind = indent(depth);
        let class = self.class(cid)?.clone();
        let language_type = self.class_name(cid)?;
        let header_type = class.name.clone();
        let pointer_expr = self.boundary_struct_ptr_expr(arg, out, depth)?;
        let source = self.fresh_tmp();
        let scratch = self.fresh_tmp();
        let c_pointer = self.fresh_tmp();
        let _ = writeln!(
            out,
            "{ind}{language_type}* {source} = ({language_type}*)({pointer_expr});"
        );
        let _ = writeln!(out, "{ind}{header_type} {scratch} = ({header_type}){{0}};");
        let _ = writeln!(out, "{ind}{header_type}* {c_pointer} = NULL;");
        let _ = writeln!(out, "{ind}if ({source} != NULL) {{");
        self.emit_boundary_scratch_c_value(
            cid,
            &format!("*{source}"),
            &scratch,
            out,
            depth + 1,
            scratch_mark,
            call_pos,
        )?;
        let _ = writeln!(out, "{}{c_pointer} = &{scratch};", indent(depth + 1));
        let _ = writeln!(out, "{ind}}}");
        Ok((
            c_pointer,
            BoundaryPtrWriteback {
                cid,
                source,
                scratch,
            },
        ))
    }

    /// Recursively assigns one language-layout boundary value into an
    /// actual-header-layout C lvalue (§32). Positional initialization is
    /// intentional: collapsed pair count fields do not exist in HIR, while
    /// the header still contains both adjacent C members.
    fn emit_boundary_scratch_c_value(
        &mut self,
        cid: ClassId,
        source: &str,
        destination: &str,
        out: &mut String,
        depth: usize,
        scratch_mark: Option<&str>,
        call_pos: &Pos,
    ) -> Result<(), String> {
        let class = self.class(cid)?.clone();
        let mut components = Vec::new();
        let mut aggregate_copies = Vec::new();
        for field in &class.fields {
            let field_name = sanitize(&field.name);
            let source_field = format!("({source}).{field_name}");
            match &field.ty {
                Type::Str => components.push(format!(
                    "{{ (const char*)subscript_rt_str_data(ctx, {source_field}), (size_t)subscript_rt_str_len(ctx, {source_field}) }}"
                )),
                Type::Array(element) => {
                    let lowered_element = match &**element {
                        Type::Class(element_cid) if self.is_value_class(*element_cid)?
                            && self.boundary_struct_requires_recursive_build(*element_cid)? => {
                            Some(*element_cid)
                        }
                        _ => None,
                    };
                    if let Some(element_cid) = lowered_element {
                        let (data, count) = self.marshal_boundary_array_c(
                            element_cid,
                            &source_field,
                            out,
                            depth,
                            scratch_mark.ok_or_else(|| {
                                "recursive boundary array lowering lacks a scratch scope"
                                    .to_string()
                            })?,
                            call_pos,
                        )?;
                        components.push(count);
                        components.push(data);
                    } else {
                        components.push(format!(
                            "(size_t)subscript_rt_array_len(ctx, {source_field})"
                        ));
                        components.push(format!(
                            "(void*)subscript_rt_array_data(ctx, {source_field})"
                        ));
                    }
                }
                Type::Class(inner) if self.is_value_class(*inner)? => {
                    if self.boundary_struct_requires_recursive_build(*inner)? {
                        let nested = self.fresh_tmp();
                        let header = self.class(*inner)?.name.clone();
                        let _ = writeln!(
                            out,
                            "{}{header} {nested} = ({header}){{0}};",
                            indent(depth)
                        );
                        self.emit_boundary_scratch_c_value(
                            *inner,
                            &source_field,
                            &nested,
                            out,
                            depth,
                            scratch_mark,
                            call_pos,
                        )?;
                        components.push(nested);
                    } else {
                        components.push("{0}".to_string());
                        aggregate_copies.push((field_name, source_field));
                    }
                }
                Type::Nullable(inner) => {
                    if let Type::Class(pointer_cid) = &**inner {
                        if self.is_value_class(*pointer_cid)? {
                            let pointer = self.marshal_boundary_pointer_member_c(
                                *pointer_cid,
                                &source_field,
                                out,
                                depth,
                                scratch_mark.ok_or_else(|| {
                                    "recursive boundary pointer lowering lacks a scratch scope"
                                        .to_string()
                                })?,
                                call_pos,
                            )?;
                            components.push(pointer);
                            continue;
                        }
                    }
                    components.push(source_field);
                }
                Type::I8
                | Type::U8
                | Type::I16
                | Type::U16
                | Type::F16
                | Type::I32
                | Type::U32
                | Type::I64
                | Type::U64
                | Type::F32
                | Type::F64
                | Type::Bool
                | Type::Enum(_)
                | Type::StringAlias(_)
                | Type::Object => components.push(source_field),
                Type::Class(inner) if !self.is_value_class(*inner)? => {
                    components.push(source_field)
                }
                other => {
                    return Err(format!(
                        "boundary struct `{}` field `{}` has unsupported recursive scratch type {other:?}",
                        class.name, field.name
                    ));
                }
            }
        }
        let _ = writeln!(
            out,
            "{}{destination} = ({}){{ {} }};",
            indent(depth),
            class.name,
            components.join(", ")
        );
        for (field_name, source_field) in aggregate_copies {
            let _ = writeln!(
                out,
                "{}memcpy(&({destination}).{field_name}, &{source_field}, sizeof ({destination}).{field_name});",
                indent(depth)
            );
        }
        Ok(())
    }

    /// Rebuilds one non-null boundary-struct pointer member into storage
    /// owned by the current call-duration scratch scope (§33). The source
    /// pointer addresses language-layout storage; the returned pointer names
    /// actual-header-layout storage, or remains `NULL` for a null source.
    fn marshal_boundary_pointer_member_c(
        &mut self,
        target_cid: ClassId,
        source: &str,
        out: &mut String,
        depth: usize,
        _scratch_mark: &str,
        call_pos: &Pos,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let language_target = self.class_name(target_cid)?;
        let header_target = self.class(target_cid)?.name.clone();
        let language_pointer = self.fresh_tmp();
        let scratch_pointer = self.fresh_tmp();
        let _ = writeln!(
            out,
            "{ind}const {language_target}* {language_pointer} = (const {language_target}*)({source});"
        );
        let _ = writeln!(out, "{ind}{header_target}* {scratch_pointer} = NULL;");
        let _ = writeln!(out, "{ind}if ({language_pointer} != NULL) {{");
        let pos_id = self.pos_id(call_pos);
        let _ = writeln!(
            out,
            "{}{scratch_pointer} = ({header_target}*)subscript_rt_boundary_scratch_alloc(ctx, (uint64_t)sizeof({header_target}), {pos_id}u);",
            indent(depth + 1)
        );
        self.emit_trap_check(out, depth + 1)?;
        self.emit_boundary_scratch_c_value(
            target_cid,
            &format!("*{language_pointer}"),
            &format!("*{scratch_pointer}"),
            out,
            depth + 1,
            Some(_scratch_mark),
            call_pos,
        )?;
        let _ = writeln!(out, "{ind}}}");
        Ok(scratch_pointer)
    }

    /// Builds a call-duration C-layout array for a collapsed pair whose
    /// value-struct elements themselves need recursive lowering.
    fn marshal_boundary_array_c(
        &mut self,
        element_cid: ClassId,
        handle: &str,
        out: &mut String,
        depth: usize,
        _scratch_mark: &str,
        call_pos: &Pos,
    ) -> Result<(String, String), String> {
        let ind = indent(depth);
        let language_element = self.class_name(element_cid)?;
        let header_element = self.class(element_cid)?.name.clone();
        let source = self.fresh_tmp();
        let count = self.fresh_tmp();
        let scratch = self.fresh_tmp();
        let index = self.fresh_tmp();
        let _ = writeln!(
            out,
            "{ind}size_t {count} = (size_t)subscript_rt_array_len(ctx, {handle});"
        );
        let _ = writeln!(out, "{ind}const {language_element}* {source} = (const {language_element}*)subscript_rt_array_data(ctx, {handle});");
        let pos_id = self.pos_id(call_pos);
        let _ = writeln!(
            out,
            "{ind}{header_element}* {scratch} = ({header_element}*)subscript_rt_boundary_scratch_alloc(ctx, (uint64_t)({count} * sizeof({header_element})), {pos_id}u);"
        );
        self.emit_trap_check(out, depth)?;
        let _ = writeln!(
            out,
            "{ind}for (size_t {index} = 0; {index} < {count}; {index}++) {{"
        );
        self.emit_boundary_scratch_c_value(
            element_cid,
            &format!("{source}[{index}]"),
            &format!("{scratch}[{index}]"),
            out,
            depth + 1,
            Some(_scratch_mark),
            call_pos,
        )?;
        let _ = writeln!(out, "{ind}}}");
        Ok((scratch, count))
    }

    /// Copies one C-filled scratch record back to language layout. Direct
    /// string-view fields allocate Context-owned language strings; the
    /// generic view type avoids depending on the registered view's field
    /// names while preserving its `{pointer,length}` C layout.
    fn emit_string_field_boundary_writeback(
        &mut self,
        writeback: BoundaryPtrWriteback,
        pos: &Pos,
        out: &mut String,
        depth: usize,
    ) -> Result<(), String> {
        let class = self.class(writeback.cid)?.clone();
        let ind = indent(depth);
        let _ = writeln!(out, "{ind}if ({} != NULL) {{", writeback.source);
        for field in &class.fields {
            let name = sanitize(&field.name);
            if field.ty == Type::Str {
                let view = self.fresh_tmp();
                let _ = writeln!(
                    out,
                    "{}subscript_callback_string_view {view}; memcpy(&{view}, &{}.{name}, sizeof {view});",
                    indent(depth + 1),
                    writeback.scratch
                );
                let pos_id = self.pos_id(pos);
                let _ = writeln!(
                    out,
                    "{}{}->{name} = subscript_rt_str_from_view(ctx, {view}.data, (uint64_t){view}.len, {pos_id}u);",
                    indent(depth + 1),
                    writeback.source
                );
                self.emit_trap_check(out, depth + 1)?;
            } else if matches!(&field.ty, Type::Array(_)) {
                // Writes through a mutable element pointer already land in
                // the language array. Do not replace its handle from the C
                // scratch's transient pointer/count pair.
                continue;
            } else if self.is_boundary_struct_ptr(&field.ty)? {
                // §33 pointer-member scratch is input-only. Never copy its
                // transient child pointer back into language storage.
                continue;
            } else if let Type::Class(id) = &field.ty {
                if self.is_value_class(*id)? {
                    if self.boundary_struct_requires_recursive_build(*id)? {
                        // §32 admits this position in the script→C direction
                        // only. Do not interpret its lowered C bytes as the
                        // narrower language object representation.
                        continue;
                    }
                    let _ = writeln!(
                        out,
                        "{}memcpy(&{}->{name}, &{}.{name}, sizeof {}->{name});",
                        indent(depth + 1),
                        writeback.source,
                        writeback.scratch,
                        writeback.source
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "{}{}->{name} = {}.{name};",
                        indent(depth + 1),
                        writeback.source,
                        writeback.scratch
                    );
                }
            } else {
                let _ = writeln!(
                    out,
                    "{}{}->{name} = {}.{name};",
                    indent(depth + 1),
                    writeback.source,
                    writeback.scratch
                );
            }
        }
        let _ = writeln!(out, "{ind}}}");
        Ok(())
    }

    /// Marshals a by-value boundary struct to the corresponding C header
    /// struct: pointer/scalar fields pass through; a function-pointer
    /// field becomes the generic trampoline plus a binding built from the
    /// following userdata slot (the callback-info idiom), so the C API
    /// sees `(fnptr, void* userdata)`.
    fn marshal_boundary_c_struct(
        &mut self,
        cid: ClassId,
        arg: &hir::Expr,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let lang_ty = self.class_name(cid)?;
        let v = self.eval(arg, out, depth)?;
        let t = self.fresh_tmp();
        let _ = writeln!(out, "{ind}{lang_ty} {t} = {v};");
        let fields = self.class(cid)?.fields.clone();
        let header_name = self.class(cid)?.name.clone();
        let mut parts = Vec::new();
        let mut i = 0;
        while i < fields.len() {
            let f = &fields[i];
            match &f.ty {
                Type::Func(_) => {
                    let callback_typedef = match &f.foreign_provenance {
                        Some(hir::ForeignTypeProvenance::Callback { typedef_name }) => {
                            typedef_name.clone()
                        }
                        None => {
                            return Err(format!(
                                "internal error at boundary struct `{header_name}` field `{}`: missing callback typedef provenance",
                                f.name
                            ));
                        }
                        Some(other) => {
                            return Err(format!(
                                "internal error at boundary struct `{header_name}` field `{}`: expected callback typedef provenance, found {other:?}",
                                f.name
                            ));
                        }
                    };
                    // The callback field is followed by one or two userdata
                    // slots (§14.4). Both are bound into one binding the
                    // trampoline reads; the C struct's first userdata slot
                    // carries the binding, any second slot carries null (the
                    // binding is authoritative for both language userdata).
                    let ud1 = fields.get(i + 1).ok_or_else(|| {
                        "a callback field needs a following userdata slot".to_string()
                    })?;
                    let has_ud2 = fields
                        .get(i + 2)
                        .map(|f| is_userdata_slot(&f.ty))
                        .unwrap_or(false);
                    let ud2_expr = if has_ud2 {
                        format!("{t}.{}", sanitize(&fields[i + 2].name))
                    } else {
                        "NULL".to_string()
                    };
                    parts.push(format!("({callback_typedef})&subscript_rt_cb_trampoline"));
                    parts.push(format!(
                        "subscript_rt_cb_bind(ctx, {t}.{}.code, {t}.{}.env, {t}.{}, {})",
                        sanitize(&f.name),
                        sanitize(&f.name),
                        sanitize(&ud1.name),
                        ud2_expr
                    ));
                    if has_ud2 {
                        // Second userdata C slot → null.
                        parts.push("NULL".to_string());
                        i += 3;
                    } else {
                        i += 2;
                    }
                }
                Type::Array(_) => {
                    // Descriptor-embedded `(count, pointer)` array field
                    // (§13.2): the language struct carries one `T[]`; the C
                    // struct declares the pair `size_t <n>Count; const T*
                    // <n>;` (count-first), so the positional compound literal
                    // fills count then pointer, both from the array's own
                    // backing store (zero-copy). The element pointer type is
                    // the C struct field's, so no element-specific cast is
                    // needed (unlike the standalone descriptor).
                    let fld = sanitize(&f.name);
                    parts.push(format!("(size_t)subscript_rt_array_len(ctx, {t}.{fld})"));
                    parts.push(format!("(void*)subscript_rt_array_data(ctx, {t}.{fld})"));
                    i += 1;
                }
                _ => {
                    parts.push(format!("{t}.{}", sanitize(&f.name)));
                    i += 1;
                }
            }
        }
        Ok(format!("(({header_name}){{ {} }})", parts.join(", ")))
    }

    /// A boundary-struct field initializer: for a `Struct | null` field
    /// receiving a value struct, the address of that struct's storage
    /// (chain-slot address-of); otherwise the plain value.
    fn boundary_field_init(
        &mut self,
        fty: &Type,
        arg: &hir::Expr,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        if let Some(cast) = self.boundary_ptr_cast(fty)? {
            // Same header-pointer cast as at a direct foreign-call argument
            // (see `marshal_foreign_c_arg`): layout-identical, ABI-safe,
            // nominally distinct.
            let expr = self.boundary_struct_ptr_expr(arg, out, depth)?;
            return Ok(format!("({cast})({expr})"));
        }
        self.eval(arg, out, depth)
    }

    /// The argument list of a script call: the declared arguments, with
    /// each omitted optional filled by its default, evaluated left to
    /// right ([`Self::eval_operands`]).
    fn call_args(
        &mut self,
        params: &[hir::Param],
        args: &[hir::Expr],
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let mut exprs: Vec<&hir::Expr> = Vec::with_capacity(params.len());
        for (i, p) in params.iter().enumerate() {
            exprs.push(match args.get(i) {
                Some(a) => a,
                None => p
                    .default
                    .as_ref()
                    .ok_or_else(|| format!("missing argument `{}`", p.name))?,
            });
        }
        Ok(self.eval_operands(&exprs, out, depth)?.join(", "))
    }

    fn eval_method(
        &mut self,
        recv: &hir::Expr,
        name: &str,
        args: &[hir::Expr],
        ret_ty: &Type,
        pos: &Pos,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
        checked: bool,
    ) -> Result<String, String> {
        match recv.ty.clone() {
            Type::Str => Err(format!("string method `{name}`")),
            Type::Array(elem) => {
                // `pop` used as a value (mutators-as-statements are
                // handled by emit_array_mutator).
                if name != "pop" {
                    return Err(format!("array method `{name}` in value position"));
                }
                let h = self.eval_pinned(recv, out, depth)?;
                while let Some(site) =
                    sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                {
                    self.emit_trap_site(site, TrapOperand::Value(h.clone()), out, depth)?;
                }
                let ect = self.ctype(&elem)?;
                let pid = self.pos_id(pos);
                let d = self.fresh_tmp();
                let _ = writeln!(
                    out,
                    "{}{ect} {d}; subscript_rt_array_pop(ctx, {h}, &{d}, {pid}u);",
                    indent(depth)
                );
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(d)
            }
            Type::Generator(y) => {
                if name != "next" {
                    return Err(format!("generator method `{name}`"));
                }
                let g = self.eval_pinned(recv, out, depth)?;
                while let Some(site) = sites.take(|site| {
                    matches!(
                        site,
                        hir::TrapSite::DevOnlyLifetime { .. }
                            | hir::TrapSite::DevReloadOnlyStaleCoroutine { .. }
                    )
                }) {
                    self.emit_trap_site(site, TrapOperand::Value(g.clone()), out, depth)?;
                }
                let ir = self.iter_result_name(&y)?;
                let creator = self.generator_of(recv)?;
                let step = self.fresh_tmp();
                let ind = indent(depth);
                let _ = writeln!(out, "{ind}{ir} {step}; memset(&{step}, 0, sizeof {step});");
                let _ = writeln!(
                    out,
                    "{ind}{step}.done = subscript_resume_{}(ctx, {g}, &{step}.value);",
                    sanitize(&creator)
                );
                if checked {
                    self.emit_trap_check(out, depth)?;
                }
                Ok(step)
            }
            Type::Class(cid) => {
                let m = self.hir_method(cid.0, name)?;
                // C2: a value receiver is passed by pointer to its
                // storage (so a mutating method mutates the receiver); a
                // reference receiver passes its handle. The receiver is
                // evaluated before the arguments (the dev tier evaluates
                // it first, then `push_args`), so it is bound to a
                // temporary whenever an argument lowers to statements.
                let mut rbuf = String::new();
                let mut recv_c = if self.is_value_class(cid)? {
                    self.value_recv_ptr(recv, cid, &mut rbuf, depth)?
                } else {
                    let recv_c = self.eval_pinned(recv, &mut rbuf, depth)?;
                    while let Some(site) =
                        sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
                    {
                        self.emit_trap_site(
                            site,
                            TrapOperand::Value(recv_c.clone()),
                            &mut rbuf,
                            depth,
                        )?;
                    }
                    recv_c
                };
                let mut abuf = String::new();
                let argv = self.call_args(&m.params.clone(), args, &mut abuf, depth)?;
                out.push_str(&rbuf);
                if !abuf.is_empty() {
                    let ct = if self.is_value_class(cid)? {
                        format!("{}*", self.class_name(cid)?)
                    } else {
                        "void*".to_string()
                    };
                    let t = self.fresh_tmp();
                    let _ = writeln!(out, "{}{ct} {t} = {recv_c};", indent(depth));
                    recv_c = t;
                }
                out.push_str(&abuf);
                let sep = if argv.is_empty() { "" } else { ", " };
                let call = format!(
                    "subscript_m{}_{}(ctx, {recv_c}{sep}{argv})",
                    cid.0,
                    sanitize(name)
                );
                self.eval_call_with_policy(call, ret_ty, checked, out, depth)
            }
            other => Err(format!("method on {other:?}")),
        }
    }

    /// A `Sub*` pointing at a value-class receiver's storage (C2). When
    /// the receiver is an lvalue its address is taken so a mutating
    /// method mutates it; an rvalue is materialized into a temporary
    /// first, so a mutation of the temporary is correctly lost, matching
    /// the CLIF path (whose rvalue receiver is a temp too).
    fn value_recv_ptr(
        &mut self,
        recv: &hir::Expr,
        cid: ClassId,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        use hir::ExprKind as K;
        let addressable = matches!(
            recv.kind,
            K::Local(_) | K::Global(_) | K::Field { .. } | K::Index { .. } | K::This
        );
        if addressable {
            // `eval` of a value-class lvalue expression yields a C lvalue
            // (a named local, a field access, an array-element deref, or
            // `(*_this)`), so its address is the receiver's storage.
            let lv = self.eval(recv, out, depth)?;
            Ok(format!("&({lv})"))
        } else {
            let cname = self.class_name(cid)?;
            let v = self.eval(recv, out, depth)?;
            let t = self.fresh_tmp();
            let _ = writeln!(out, "{}{cname} {t} = {v};", indent(depth));
            Ok(format!("&{t}"))
        }
    }

    fn eval_new(
        &mut self,
        class: ClassId,
        args: &[hir::Expr],
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let c = self.class(class)?.clone();
        // A mirror boundary struct has no in-language constructor body: its
        // `new` is a struct literal filled positionally from the arguments
        // (arg `i` → field `i`), each through the boundary coercion
        // (chain-slot address-of for a `Struct | null` field).
        if c.is_boundary {
            let cname = self.class_name(class)?;
            let fields = c.fields.clone();
            if args.len() != fields.len() {
                return Err(format!(
                    "boundary struct `{}` expects {} field arguments, got {}",
                    c.name,
                    fields.len(),
                    args.len()
                ));
            }
            // Field initializers run left to right, like any other
            // operand list: each into its own buffer, binding an earlier
            // one when a later one lowered to statements.
            let mut bufs: Vec<String> = Vec::new();
            let mut parts: Vec<String> = Vec::new();
            for (i, field) in fields.iter().enumerate() {
                let mut buf = String::new();
                parts.push(self.boundary_field_init(&field.ty, &args[i], &mut buf, depth)?);
                bufs.push(buf);
            }
            let mut pin = vec![false; parts.len()];
            let mut later = false;
            for i in (0..parts.len()).rev() {
                pin[i] = later;
                later = later || !bufs[i].is_empty();
            }
            for i in 0..parts.len() {
                out.push_str(&bufs[i]);
                if pin[i] {
                    let ct = match self.boundary_ptr_cast(&fields[i].ty)? {
                        Some(cast) => cast,
                        None => self.ctype(&fields[i].ty)?,
                    };
                    let t = self.fresh_tmp();
                    let _ = writeln!(out, "{}{ct} {t} = {};", indent(depth), parts[i]);
                    parts[i] = t;
                }
            }
            return Ok(format!("(({cname}){{ {} }})", parts.join(", ")));
        }
        if self.is_value_class(class)? {
            if let Some(ctor) = &c.ctor {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Call { .. }),
                    "value constructor has no HIR call site",
                )?;
                let argv = self.call_args(&ctor.params, args, out, depth)?;
                let sep = if argv.is_empty() { "" } else { ", " };
                let call = format!("subscript_ctor{}(ctx{sep}{argv})", class.0);
                self.eval_site_checked_call(call, &Type::Class(class), site, out, depth)
            } else {
                let cname = self.class_name(class)?;
                if c.fields.iter().any(|field| field.init.is_some()) {
                    let this = self.fresh_tmp();
                    let _ = writeln!(out, "{}{cname} {this} = ({cname}){{0}};", indent(depth));
                    for field in &c.fields {
                        if let Some(init) = &field.init {
                            let value = self.eval(init, out, depth)?;
                            let _ = writeln!(
                                out,
                                "{}{this}.{} = {value};",
                                indent(depth),
                                sanitize(&field.name)
                            );
                        }
                    }
                    Ok(this)
                } else {
                    Ok(format!("({cname}){{0}}"))
                }
            }
        } else {
            let allocation = sites.take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                "reference new has no HIR allocation site",
            )?;
            let hir::TrapSite::Allocation { pos } = allocation else {
                unreachable!()
            };
            let cname = self.class_name(class)?;
            let pid = self.pos_id(pos);
            let call = format!(
                "subscript_rt_alloc(ctx, sizeof({cname}), {}u, {pid}u)",
                class.0
            );
            let this =
                self.eval_site_checked_call(call, &Type::Class(class), allocation, out, depth)?;
            if let Some(ctor) = &c.ctor {
                let site = sites.take_required(
                    |site| matches!(site, hir::TrapSite::Call { .. }),
                    "reference constructor has no HIR call site",
                )?;
                let argv = self.call_args(&ctor.params, args, out, depth)?;
                let sep = if argv.is_empty() { "" } else { ", " };
                let call = format!("subscript_ctor{}(ctx, {this}{sep}{argv})", class.0);
                self.eval_site_checked_call(call, &Type::Void, site, out, depth)?;
            } else {
                for field in &c.fields {
                    if let Some(init) = &field.init {
                        let value = self.eval(init, out, depth)?;
                        let _ = writeln!(
                            out,
                            "{}(({cname}*){this})->{} = {value};",
                            indent(depth),
                            sanitize(&field.name)
                        );
                    }
                }
            }
            Ok(this)
        }
    }

    /// Q33 descriptor sugar: an ordinary reference allocation followed by
    /// declaration-ordered member stores. Omitted members evaluate their
    /// checked defaults once for this construction.
    fn eval_descriptor_lit(
        &mut self,
        class: ClassId,
        fields: &[Option<hir::Expr>],
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let descriptor = self.class(class)?.clone();
        if !descriptor.is_descriptor || descriptor.is_value {
            return Err("DescriptorLit does not name a descriptor reference class".to_string());
        }
        if fields.len() != descriptor.fields.len() {
            return Err(format!(
                "descriptor `{}` has {} fields but its literal has {} slots",
                descriptor.name,
                descriptor.fields.len(),
                fields.len()
            ));
        }
        let allocation = sites.take_required(
            |site| matches!(site, hir::TrapSite::Allocation { .. }),
            "descriptor literal has no HIR allocation site",
        )?;
        let hir::TrapSite::Allocation { pos } = allocation else {
            unreachable!()
        };
        let cname = self.class_name(class)?;
        let pid = self.pos_id(pos);
        let call = format!(
            "subscript_rt_alloc(ctx, sizeof({cname}), {}u, {pid}u)",
            class.0
        );
        let this =
            self.eval_site_checked_call(call, &Type::Class(class), allocation, out, depth)?;

        for (slot, field) in fields.iter().zip(&descriptor.fields) {
            let value = match slot {
                Some(value) => self.eval(value, out, depth)?,
                None => {
                    if !field.is_defaulted {
                        return Err(format!(
                            "required descriptor member `{}` has no literal value",
                            field.name
                        ));
                    }
                    let default = field.init.as_ref().ok_or_else(|| {
                        format!(
                            "defaulted descriptor member `{}` has no checked default",
                            field.name
                        )
                    })?;
                    let saved_this = self.descriptor_this.replace(this.clone());
                    let evaluated = self.eval(default, out, depth);
                    self.descriptor_this = saved_this;
                    evaluated?
                }
            };
            let _ = writeln!(
                out,
                "{}(({cname}*)({this}))->{} = {value};",
                indent(depth),
                sanitize(&field.name)
            );
        }
        Ok(this)
    }

    // ----- function values and lambdas -----

    fn func_ref_value(&mut self, name: &str) -> Result<String, String> {
        let wrap = format!("subscript_wrap_{}", sanitize(name));
        if self.wrappers.insert(wrap.clone()) {
            self.emit_func_wrapper(name, &wrap)?;
        }
        Ok(format!("((SubFn){{ (void*)&{wrap}, ((void*)0) }})"))
    }

    fn emit_func_wrapper(&mut self, name: &str, wrap: &str) -> Result<(), String> {
        let f = self.hir_fn(name)?.clone();
        let ret = self.ctype(&f.ret)?;
        let params = self.param_list(&f.params)?;
        let sep = if params.is_empty() { "" } else { ", " };
        let sig = format!("static {ret} {wrap}(void* ctx, void* _env{sep}{params})");
        let _ = writeln!(self.protos, "{sig};");
        let argv: Vec<String> = f.params.iter().map(|p| sanitize(&p.name)).collect();
        let asep = if argv.is_empty() { "" } else { ", " };
        let call = format!(
            "subscript_fn_{}(ctx{asep}{})",
            sanitize(name),
            argv.join(", ")
        );
        let _ = writeln!(
            self.helpers,
            "{sig} {{ (void)_env; {}{call}; }}",
            if f.ret == Type::Void { "" } else { "return " }
        );
        Ok(())
    }

    fn fn_ptr_cast(&self, ft: &FuncType) -> Result<String, String> {
        let ret = self.ctype(&ft.ret)?;
        let mut parts = vec!["void*".to_string(), "void*".to_string()];
        for p in &ft.params {
            parts.push(self.ctype(p)?);
        }
        Ok(format!("{ret}(*)({})", parts.join(", ")))
    }

    fn eval_lambda(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[hir::Capture],
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let n = self.lambda;
        self.lambda += 1;
        let name = format!("subscript_lambda{n}");
        let env_ty = format!("EnvL{n}");
        let ind = indent(depth);

        // Environment: captured values by value (C5), non-escaping so it
        // may live in the creating frame.
        let cap_tys: Vec<(String, Type)> = captures
            .iter()
            .map(|capture| (capture.name.clone(), capture.ty.clone()))
            .collect();
        let env_expr = if captures.is_empty() {
            "((void*)0)".to_string()
        } else {
            // The environment is a named struct of the captured values by
            // value (C5), built into a fresh temp in the creating frame
            // (non-escaping, so stack lifetime suffices). Each field
            // carries the capture's *actual* C type (C2).
            let mut fields = String::new();
            for (cn, t) in &cap_tys {
                let _ = write!(fields, "{} {}; ", self.ctype(t)?, sanitize(cn));
            }
            let _ = writeln!(self.protos, "typedef struct {{ {fields}}} {env_ty};");
            let etmp = self.fresh_tmp();
            let _ = writeln!(out, "{ind}{env_ty} {etmp};");
            for (cn, _) in &cap_tys {
                let _ = writeln!(
                    out,
                    "{ind}{etmp}.{} = {};",
                    sanitize(cn),
                    self.local_ref(cn)
                );
            }
            format!("(void*)&{etmp}")
        };

        // Emit the lambda function into helpers.
        self.emit_lambda_fn(&name, &env_ty, params, ret, body, &cap_tys)?;
        Ok(format!("((SubFn){{ (void*)&{name}, {env_expr} }})"))
    }

    fn emit_lambda_fn(
        &mut self,
        name: &str,
        env_ty: &str,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        caps: &[(String, Type)],
    ) -> Result<(), String> {
        let retc = self.ctype(ret)?;
        let params_c = self.param_list(params)?;
        let sep = if params_c.is_empty() { "" } else { ", " };
        let sig = format!("static {retc} {name}(void* ctx, void* _env{sep}{params_c})");
        let _ = writeln!(self.protos, "{sig};");

        // The lambda is a distinct function: save and reset the enclosing
        // function's per-function state, restore it afterward.
        let saved_this = self.this;
        let saved_gen = std::mem::take(&mut self.gen);
        let saved_gl = std::mem::take(&mut self.gen_locals);
        let saved_lt = std::mem::take(&mut self.local_types);
        let saved_ms = std::mem::take(&mut self.managed_scope);
        let saved_cursor = self.shadow_cursor;
        let saved_has = self.has_shadow;
        let saved_ret = self.current_ret.clone();
        self.this = ThisCtx::None;
        self.shadow_cursor = 0;
        self.has_shadow = false;
        self.current_ret = ret.clone();

        let mut fbody = String::new();
        if !caps.is_empty() {
            let _ = writeln!(fbody, "    {env_ty}* _e = ({env_ty}*)_env;");
            let _ = writeln!(fbody, "    (void)_e;");
        } else {
            let _ = writeln!(fbody, "    (void)_env;");
        }
        self.emit_prologue(&mut fbody, params, body, 1)?;
        // Captures become local const copies read from the env, with
        // their actual types (C2).
        for (cn, ct) in caps {
            let _ = writeln!(
                fbody,
                "    {} {} = _e->{};",
                self.ctype(ct)?,
                sanitize(cn),
                sanitize(cn)
            );
            self.local_types.push((cn.clone(), ct.clone()));
        }
        self.emit_block(&mut fbody, body, 1)?;
        self.emit_exit(&mut fbody, ret, 1)?;

        let _ = writeln!(self.helpers, "{sig} {{\n{fbody}}}\n");

        self.this = saved_this;
        self.gen = saved_gen;
        self.gen_locals = saved_gl;
        self.local_types = saved_lt;
        self.managed_scope = saved_ms;
        self.shadow_cursor = saved_cursor;
        self.has_shadow = saved_has;
        self.current_ret = saved_ret;
        Ok(())
    }

    // ----- generators -----

    fn generator_of(&self, recv: &hir::Expr) -> Result<String, String> {
        // The generator handle came from a creator call; recover the
        // creator name from the receiver when it is a direct call, else
        // from a local bound to such a call. The run set binds the
        // generator to a local, so track it via the receiver's origin.
        // For the common `g.next()` where `g = creator(...)`, we record
        // the creator on the receiver's type is not possible; instead we
        // find the single generator whose yield type matches.
        match &recv.ty {
            Type::Generator(y) => {
                let mut found = None;
                for f in &self.module.functions {
                    if f.is_generator {
                        if let Type::Generator(fy) = &f.ret {
                            if fy == y {
                                if found.is_some() {
                                    return Err("ambiguous generator resume target".to_string());
                                }
                                found = Some(f.name.clone());
                            }
                        }
                    }
                }
                found.ok_or_else(|| "no generator matches the receiver".to_string())
            }
            other => Err(format!("generator receiver {other:?}")),
        }
    }

    fn gen_next_let_field(&mut self, name: &str) -> Result<String, String> {
        let g = self
            .gen
            .as_mut()
            .ok_or("generator let outside a generator")?;
        let field = g
            .let_fields
            .get(g.let_cursor)
            .cloned()
            .ok_or("generator frame let cursor exhausted")?;
        g.let_cursor += 1;
        self.gen_locals
            .push((name.to_string(), format!("_f->{field}")));
        Ok(field)
    }

    fn gen_next_child_field(&mut self) -> Result<String, String> {
        let g = self
            .gen
            .as_mut()
            .ok_or("async child frame outside a coroutine")?;
        let field = g
            .child_fields
            .get(g.child_cursor)
            .cloned()
            .ok_or("async child-frame cursor exhausted")?;
        g.child_cursor += 1;
        Ok(field)
    }

    fn emit_generator(&mut self, out: &mut String, f: &hir::Function) -> Result<(), String> {
        let yield_ty = match &f.ret {
            Type::Generator(y) => (**y).clone(),
            other => return Err(format!("generator return {other:?}")),
        };
        let gen_struct = format!("Gen_{}", sanitize(&f.name));

        // Frame layout: state word, params, then lets in emission order.
        let mut lets: Vec<(&str, &Type)> = Vec::new();
        walk_lets(&f.body, &mut lets);
        let mut let_fields = Vec::with_capacity(lets.len());
        let mut struct_body = String::from("    int32_t _state;\n");
        for p in &f.params {
            let _ = writeln!(
                struct_body,
                "    {} {};",
                self.ctype(&p.ty)?,
                sanitize(&p.name)
            );
        }
        for (i, (_, ty)) in lets.iter().enumerate() {
            let field = format!("g{i}");
            let _ = writeln!(struct_body, "    {} {};", self.ctype(ty)?, field);
            let_fields.push(field);
        }
        let _ = writeln!(
            out,
            "typedef struct {gen_struct} {{\n{struct_body}}} {gen_struct};"
        );

        // Creator.
        let creator_sig = self.gen_creator_signature(f)?;
        let _ = writeln!(out, "{creator_sig} {{");
        self.begin_fn(ThisCtx::None, f.ret.clone());
        let sites = f.trap_sites();
        lower_trap_sites(&sites, "generator frame creation", |sites| {
            let site = sites.take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                "generator has no HIR allocation site",
            )?;
            let hir::TrapSite::Allocation { pos } = site else {
                return Err("generator has a non-allocation HIR site".to_string());
            };
            let pid = self.pos_id(pos);
            let _ = writeln!(
                out,
                "    void* _frame = subscript_rt_alloc(ctx, sizeof({gen_struct}), {}u, {pid}u);",
                rtc::CLASS_GENERATOR
            );
            self.emit_trap_site(site, TrapOperand::Pending, out, 1)
        })?;
        let _ = writeln!(out, "    {gen_struct}* _f = ({gen_struct}*)_frame;");
        let _ = writeln!(out, "    _f->_state = 0;");
        for p in &f.params {
            let _ = writeln!(out, "    _f->{0} = {0};", sanitize(&p.name));
        }
        let _ = writeln!(out, "    return _frame;");
        let _ = writeln!(out, "}}\n");

        // Resume state machine.
        let resume_sig = self.gen_resume_signature(f)?;
        let n_yields = count_yields(&f.body);
        let _ = writeln!(out, "{resume_sig} {{");
        let _ = writeln!(out, "    {gen_struct}* _f = ({gen_struct}*)_frame;");
        let _ = writeln!(out, "    (void)_out;");
        // Dispatch on the state word.
        let _ = writeln!(out, "    switch (_f->_state) {{");
        let _ = writeln!(out, "        case 0: goto _gstart;");
        for i in 0..n_yields {
            let _ = writeln!(out, "        case {}: goto _gresume{i};", i + 1);
        }
        let _ = writeln!(out, "        default: return 1;");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "    _gstart: ;");

        self.begin_fn(ThisCtx::None, Type::I32);
        self.gen = Some(GenState {
            kind: FrameKind::Generator,
            yields: 0,
            let_cursor: 0,
            let_fields,
            yield_ct: self.ctype(&yield_ty)?,
            child_cursor: 0,
            child_fields: Vec::new(),
        });
        for p in &f.params {
            self.gen_locals
                .push((p.name.clone(), format!("_f->{}", sanitize(&p.name))));
        }
        self.emit_block(out, &f.body, 1)?;
        // Fell off the end: done.
        let _ = writeln!(out, "    _f->_state = {GEN_DONE}; return 1;");
        let _ = writeln!(out, "}}\n");
        self.gen = None;
        self.gen_locals.clear();
        Ok(())
    }

    fn emit_async(&mut self, out: &mut String, f: &hir::Function) -> Result<(), String> {
        self.emit_async_with(out, f, None)
    }

    fn emit_async_method(
        &mut self,
        out: &mut String,
        class: usize,
        method: &hir::Function,
    ) -> Result<(), String> {
        if self.class(ClassId(class))?.is_value {
            return Err("async method lowering received a value class".to_string());
        }
        self.emit_async_with(out, method, Some(class))
    }

    fn emit_async_with(
        &mut self,
        out: &mut String,
        f: &hir::Function,
        class: Option<usize>,
    ) -> Result<(), String> {
        let frame_struct = match class {
            Some(class) => format!("Async_m{class}_{}", sanitize(&f.name)),
            None => format!("Async_{}", sanitize(&f.name)),
        };

        // Async frames use the same Context-owned allocation class and
        // conservative frame scan as generators. Child-frame pointers are
        // explicit fields so a suspended await chain remains live.
        let mut lets: Vec<(&str, &Type)> = Vec::new();
        walk_lets(&f.body, &mut lets);
        let mut let_fields = Vec::with_capacity(lets.len());
        let mut struct_body = String::from("    int32_t _state;\n");
        if class.is_some() {
            struct_body.push_str("    void* _this;\n");
        }
        for p in &f.params {
            let _ = writeln!(
                struct_body,
                "    {} {};",
                self.ctype(&p.ty)?,
                sanitize(&p.name)
            );
        }
        for (i, (_, ty)) in lets.iter().enumerate() {
            let field = format!("g{i}");
            let _ = writeln!(struct_body, "    {} {};", self.ctype(ty)?, field);
            let_fields.push(field);
        }
        let child_count = count_async_calls(&f.body);
        let mut child_fields = Vec::with_capacity(child_count as usize);
        for i in 0..child_count {
            let field = format!("child{i}");
            let _ = writeln!(struct_body, "    void* {field};");
            child_fields.push(field);
        }
        let _ = writeln!(
            out,
            "typedef struct {frame_struct} {{\n{struct_body}}} {frame_struct};"
        );

        let creator_sig = match class {
            Some(class) => self.async_method_creator_signature(class, f)?,
            None => self.gen_creator_signature(f)?,
        };
        let _ = writeln!(out, "{creator_sig} {{");
        self.begin_fn(ThisCtx::None, Type::Generator(Box::new(Type::Void)));
        let sites = f.trap_sites();
        lower_trap_sites(&sites, "async frame creation", |sites| {
            let site = sites.take_required(
                |site| matches!(site, hir::TrapSite::Allocation { .. }),
                "async function has no HIR allocation site",
            )?;
            let hir::TrapSite::Allocation { pos } = site else {
                return Err("async function has a non-allocation HIR site".to_string());
            };
            let pid = self.pos_id(pos);
            let _ = writeln!(
                out,
                "    void* _frame = subscript_rt_alloc(ctx, sizeof({frame_struct}), {}u, {pid}u);",
                rtc::CLASS_GENERATOR
            );
            self.emit_trap_site(site, TrapOperand::Pending, out, 1)
        })?;
        let _ = writeln!(out, "    {frame_struct}* _f = ({frame_struct}*)_frame;");
        let _ = writeln!(out, "    _f->_state = 0;");
        if class.is_some() {
            let _ = writeln!(out, "    _f->_this = _this;");
        }
        for p in &f.params {
            let _ = writeln!(out, "    _f->{0} = {0};", sanitize(&p.name));
        }
        let _ = writeln!(out, "    return _frame;");
        let _ = writeln!(out, "}}\n");

        let resume_sig = match class {
            Some(class) => self.async_method_resume_signature(class, f),
            None => self.gen_resume_signature(f)?,
        };
        let suspensions = count_yields(&f.body);
        let _ = writeln!(out, "{resume_sig} {{");
        let _ = writeln!(out, "    {frame_struct}* _f = ({frame_struct}*)_frame;");
        if class.is_some() {
            let _ = writeln!(out, "    void* _this = _f->_this;");
        }
        let _ = writeln!(out, "    switch (_f->_state) {{");
        let _ = writeln!(out, "        case 0: goto _gstart;");
        for i in 0..suspensions {
            let _ = writeln!(out, "        case {}: goto _gresume{i};", i + 1);
        }
        let _ = writeln!(out, "        default: return 1;");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "    _gstart: ;");

        self.begin_fn(
            if class.is_some() {
                ThisCtx::Reference
            } else {
                ThisCtx::None
            },
            Type::I32,
        );
        self.gen = Some(GenState {
            kind: FrameKind::Async,
            yields: 0,
            let_cursor: 0,
            let_fields,
            yield_ct: self.ctype(&f.ret)?,
            child_cursor: 0,
            child_fields,
        });
        for p in &f.params {
            self.gen_locals
                .push((p.name.clone(), format!("_f->{}", sanitize(&p.name))));
        }
        self.emit_block(out, &f.body, 1)?;
        let _ = writeln!(out, "    _f->_state = {GEN_DONE}; return 1;");
        let _ = writeln!(out, "}}\n");
        self.gen = None;
        self.gen_locals.clear();
        Ok(())
    }

    fn eval_yield(
        &mut self,
        arg: Option<&hir::Expr>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let ind = indent(depth);
        let (n, yct) = {
            let g = self.gen.as_ref().ok_or("yield outside a generator")?;
            (g.yields, g.yield_ct.clone())
        };
        if let Some(a) = arg {
            let v = self.eval(a, out, depth)?;
            let _ = writeln!(out, "{ind}*({yct}*)_out = {v};");
        }
        let _ = writeln!(out, "{ind}_f->_state = {}; return 0;", n + 1);
        let _ = writeln!(out, "{ind}_gresume{n}: ;");
        if let Some(g) = self.gen.as_mut() {
            g.yields += 1;
        }
        Ok(String::new())
    }

    fn eval_async_suspend(&mut self, out: &mut String, depth: usize) -> Result<String, String> {
        let ind = indent(depth);
        let n = {
            let g = self.gen.as_ref().ok_or("await outside a coroutine")?;
            if g.kind != FrameKind::Async {
                return Err("async suspension inside a generator".to_string());
            }
            g.yields
        };
        let _ = writeln!(out, "{ind}_f->_state = {}; return 0;", n + 1);
        let _ = writeln!(out, "{ind}_gresume{n}: ;");
        if let Some(g) = self.gen.as_mut() {
            g.yields += 1;
        }
        Ok(String::new())
    }

    fn eval_async_call(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        ret_ty: &Type,
        sites: &mut TrapSiteConsumer<'_>,
        out: &mut String,
        depth: usize,
    ) -> Result<String, String> {
        let (f, creator, resume, receiver) = match callee {
            hir::AsyncCallee::Function(function) => (
                self.hir_fn(function)?,
                format!("subscript_fn_{}", sanitize(function)),
                format!("subscript_resume_{}", sanitize(function)),
                None,
            ),
            hir::AsyncCallee::Method {
                class,
                receiver,
                name,
            } => (
                self.hir_method(class.0, name)?,
                format!("subscript_m{}_{}", class.0, sanitize(name)),
                format!("subscript_m{}_{}_resume", class.0, sanitize(name)),
                Some(receiver.as_ref()),
            ),
            _ => return Err("unknown async callee kind".to_string()),
        };
        if !f.is_async {
            return Err("async call targets a synchronous declaration".to_string());
        }
        let params = f.params.clone();
        let field = self.gen_next_child_field()?;
        let n = {
            let g = self.gen.as_ref().ok_or("async call outside a coroutine")?;
            if g.kind != FrameKind::Async {
                return Err("async call inside a generator".to_string());
            }
            g.yields
        };
        let ind = indent(depth);
        let receiver = if let Some(receiver) = receiver {
            let receiver_value = self.eval_pinned(receiver, out, depth)?;
            while let Some(site) =
                sites.take(|site| matches!(site, hir::TrapSite::DevOnlyLifetime { .. }))
            {
                self.emit_trap_site(site, TrapOperand::Value(receiver_value.clone()), out, depth)?;
            }
            // Keep the receiver live if an explicit argument collects before
            // the method creator installs it in the child frame.
            let _ = writeln!(out, "{ind}_f->{field} = {receiver_value};");
            Some(receiver_value)
        } else {
            None
        };
        let argv = self.call_args(&params, args, out, depth)?;
        let explicit = if argv.is_empty() {
            String::new()
        } else {
            format!(", {argv}")
        };
        let creator_args = match receiver {
            Some(receiver) => format!("ctx, {receiver}{explicit}"),
            None => format!("ctx{explicit}"),
        };
        let site = sites.take_required(
            |site| matches!(site, hir::TrapSite::Call { .. }),
            "async call has no HIR call site",
        )?;
        let _ = writeln!(out, "{ind}_f->{field} = {creator}({creator_args});");
        self.emit_trap_site(site, TrapOperand::Pending, out, depth)?;
        let _ = writeln!(out, "{ind}goto _aattempt{n};");
        let _ = writeln!(out, "{ind}_gresume{n}: ;");
        let _ = writeln!(out, "{ind}_aattempt{n}: ;");
        let result = self.fresh_tmp();
        let out_arg = if *ret_ty == Type::Void {
            "0".to_string()
        } else {
            let cty = self.ctype(ret_ty)?;
            let zero = self.zero_value(ret_ty)?;
            let _ = writeln!(out, "{ind}{cty} {result} = {zero};");
            format!("&{result}")
        };
        let _ = writeln!(
            out,
            "{ind}if (!{resume}(ctx, _f->{field}, {out_arg})) {{ _f->_state = {}; return 0; }}",
            n + 1
        );
        // The single HIR call site governs every dynamic resume of this
        // await. It was consumed above; subsequent polls still propagate
        // a callee trap through the same caller unwind.
        self.emit_trap_check(out, depth)?;
        if let Some(g) = self.gen.as_mut() {
            g.yields += 1;
        }
        Ok(if *ret_ty == Type::Void {
            String::new()
        } else {
            result
        })
    }

    // ----- HIR lookups -----

    fn hir_fn(&self, name: &str) -> Result<&'m hir::Function, String> {
        self.module
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("unknown function `{name}`"))
    }

    fn hir_method(&self, cid: usize, name: &str) -> Result<&'m hir::Function, String> {
        self.module
            .classes
            .get(cid)
            .and_then(|c| c.methods.iter().find(|m| m.name == name))
            .ok_or_else(|| format!("unknown method `{name}` on class {cid}"))
    }
}

// ----- free helpers -----

fn push_unique(set: &mut Vec<Type>, ty: &Type) {
    if !set.contains(ty) {
        set.push(ty.clone());
    }
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

fn is_aggregate(ty: &Type) -> bool {
    matches!(
        ty,
        Type::FixedArray(..) | Type::IterResult(_) | Type::Class(_)
    )
}

/// True for a callback-info userdata slot — the boundary `object | null`
/// form (`Type::Nullable(Object)`) or a bare `object` (§14.4): a callback
/// field is followed by one or two such slots.
fn is_userdata_slot(ty: &Type) -> bool {
    matches!(ty, Type::Object) || matches!(ty, Type::Nullable(inner) if **inner == Type::Object)
}

fn collect_aggr_ty(ty: &Type, set: &mut Vec<Type>) {
    match ty {
        Type::Class(_) => {
            push_unique(set, ty);
        }
        Type::FixedArray(elem, _) => {
            push_unique(set, ty);
            collect_aggr_ty(elem, set);
        }
        Type::IterResult(v) => {
            push_unique(set, ty);
            collect_aggr_ty(v, set);
        }
        Type::Array(e) | Type::Set(e) | Type::Nullable(e) | Type::Generator(e) => {
            collect_aggr_ty(e, set);
        }
        Type::Map(k, v) => {
            collect_aggr_ty(k, set);
            collect_aggr_ty(v, set);
        }
        Type::Worker(input, output) => {
            collect_aggr_ty(input, set);
            collect_aggr_ty(output, set);
        }
        Type::Inbox(message) | Type::Outbox(message) => collect_aggr_ty(message, set),
        Type::Func(ft) => {
            for p in &ft.params {
                collect_aggr_ty(p, set);
            }
            collect_aggr_ty(&ft.ret, set);
        }
        _ => {}
    }
}

fn collect_aggr_expr(e: &hir::Expr, set: &mut Vec<Type>) {
    use hir::ExprKind as K;
    collect_aggr_ty(&e.ty, set);
    match &e.kind {
        K::Unary { operand, .. } => collect_aggr_expr(operand, set),
        K::Binary { left, right, .. } => {
            collect_aggr_expr(left, set);
            collect_aggr_expr(right, set);
        }
        K::Assign { target, value, .. } => {
            collect_aggr_expr(target, set);
            collect_aggr_expr(value, set);
        }
        K::Cast(inner) => collect_aggr_expr(inner, set),
        K::Call { callee, args } => {
            match callee {
                hir::Callee::Value(v) => collect_aggr_expr(v, set),
                hir::Callee::Method { recv, .. } => collect_aggr_expr(recv, set),
                _ => {}
            }
            for a in args {
                collect_aggr_expr(a, set);
            }
        }
        K::New { args, .. } => {
            for a in args {
                collect_aggr_expr(a, set);
            }
        }
        K::DescriptorLit { fields, .. } => {
            for value in fields.iter().flatten() {
                collect_aggr_expr(value, set);
            }
        }
        K::Field { obj, .. } | K::JsonResultValue(obj) => collect_aggr_expr(obj, set),
        K::Length(obj) => collect_aggr_expr(obj, set),
        K::Index { obj, index, .. } => {
            collect_aggr_expr(obj, set);
            collect_aggr_expr(index, set);
        }
        K::ArrayLit(elems) => {
            for x in elems {
                collect_aggr_expr(x, set);
            }
        }
        K::ArraySpreadLit(elems) => {
            for elem in elems {
                collect_aggr_expr(&elem.expr, set);
            }
        }
        K::Template(parts) => {
            for p in parts {
                if let hir::TplPart::Expr(x) = p {
                    collect_aggr_expr(x, set);
                }
            }
        }
        K::Cond { cond, then, els } => {
            collect_aggr_expr(cond, set);
            collect_aggr_expr(then, set);
            collect_aggr_expr(els, set);
        }
        K::Yield(Some(a)) => collect_aggr_expr(a, set),
        K::AsyncCall { callee, args } => {
            if let Some(receiver) = callee.receiver() {
                collect_aggr_expr(receiver, set);
            }
            for arg in args {
                collect_aggr_expr(arg, set);
            }
        }
        K::Lambda {
            params, ret, body, ..
        } => {
            for p in params {
                collect_aggr_ty(&p.ty, set);
            }
            collect_aggr_ty(ret, set);
            collect_aggr_stmts(body, set);
        }
        _ => {}
    }
}

fn collect_aggr_stmts(stmts: &[hir::Stmt], set: &mut Vec<Type>) {
    for s in stmts {
        match s {
            hir::Stmt::Let { ty, init, .. } => {
                collect_aggr_ty(ty, set);
                collect_aggr_expr(init, set);
            }
            hir::Stmt::Expr(e) => collect_aggr_expr(e, set),
            hir::Stmt::Return { value: Some(v), .. } => collect_aggr_expr(v, set),
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                collect_aggr_expr(cond, set);
                collect_aggr_stmts(then, set);
                if let Some(e) = els {
                    collect_aggr_stmts(e, set);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                collect_aggr_expr(cond, set);
                collect_aggr_stmts(body, set);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(i) = init {
                    collect_aggr_stmts(std::slice::from_ref(&**i), set);
                }
                if let Some(c) = cond {
                    collect_aggr_expr(c, set);
                }
                if let Some(s) = step {
                    collect_aggr_expr(s, set);
                }
                collect_aggr_stmts(body, set);
            }
            hir::Stmt::ForOf {
                ty, subject, body, ..
            } => {
                collect_aggr_ty(ty, set);
                collect_aggr_expr(subject, set);
                collect_aggr_stmts(body, set);
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                collect_aggr_expr(disc, set);
                for c in cases {
                    if let Some(t) = &c.test {
                        collect_aggr_expr(t, set);
                    }
                    collect_aggr_stmts(&c.body, set);
                }
            }
            hir::Stmt::Block(b) => collect_aggr_stmts(b, set),
            _ => {}
        }
    }
}

fn count_yields(stmts: &[hir::Stmt]) -> u32 {
    let mut n = 0;
    for s in stmts {
        match s {
            hir::Stmt::Let { init, .. } => n += count_yields_expr(init),
            hir::Stmt::Expr(e) => n += count_yields_expr(e),
            hir::Stmt::Return { value, .. } => n += value.as_ref().map_or(0, count_yields_expr),
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                n += count_yields_expr(cond) + count_yields(then);
                if let Some(e) = els {
                    n += count_yields(e);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                n += count_yields_expr(cond) + count_yields(body)
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(i) = init {
                    n += count_yields(std::slice::from_ref(&**i));
                }
                n += cond.as_ref().map_or(0, count_yields_expr);
                n += step.as_ref().map_or(0, count_yields_expr);
                n += count_yields(body);
            }
            hir::Stmt::ForOf { subject, body, .. } => {
                n += count_yields_expr(subject) + count_yields(body);
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                n += count_yields_expr(disc);
                for c in cases {
                    n += c.test.as_ref().map_or(0, count_yields_expr) + count_yields(&c.body);
                }
            }
            hir::Stmt::Block(b) => n += count_yields(b),
            _ => {}
        }
    }
    n
}

fn count_yields_expr(e: &hir::Expr) -> u32 {
    use hir::ExprKind as K;
    match &e.kind {
        K::Yield(arg) => 1 + arg.as_deref().map_or(0, count_yields_expr),
        K::AsyncSuspend => 1,
        K::AsyncCall { callee, args } => {
            1 + callee.receiver().map_or(0, count_yields_expr)
                + args.iter().map(count_yields_expr).sum::<u32>()
        }
        K::Unary { operand, .. } => count_yields_expr(operand),
        K::Binary { left, right, .. } => count_yields_expr(left) + count_yields_expr(right),
        K::Assign { target, value, .. } => count_yields_expr(target) + count_yields_expr(value),
        K::Cast(inner) => count_yields_expr(inner),
        K::Call { callee, args } => {
            let mut n: u32 = args.iter().map(count_yields_expr).sum();
            match callee {
                hir::Callee::Value(v) => n += count_yields_expr(v),
                hir::Callee::Method { recv, .. } => n += count_yields_expr(recv),
                _ => {}
            }
            n
        }
        K::New { args, .. } => args.iter().map(count_yields_expr).sum(),
        K::DescriptorLit { fields, .. } => fields.iter().flatten().map(count_yields_expr).sum(),
        K::Field { obj, .. } | K::JsonResultValue(obj) => count_yields_expr(obj),
        K::Length(obj) => count_yields_expr(obj),
        K::Index { obj, index, .. } => count_yields_expr(obj) + count_yields_expr(index),
        K::ArrayLit(elems) => elems.iter().map(count_yields_expr).sum(),
        K::ArraySpreadLit(elems) => elems.iter().map(|elem| count_yields_expr(&elem.expr)).sum(),
        K::Template(parts) => parts
            .iter()
            .map(|p| match p {
                hir::TplPart::Expr(e) => count_yields_expr(e),
                _ => 0,
            })
            .sum(),
        K::Cond { cond, then, els } => {
            count_yields_expr(cond) + count_yields_expr(then) + count_yields_expr(els)
        }
        _ => 0,
    }
}

fn count_async_calls(stmts: &[hir::Stmt]) -> u32 {
    fn expr(e: &hir::Expr) -> u32 {
        use hir::ExprKind as K;
        match &e.kind {
            K::AsyncCall { callee, args } => {
                1 + callee.receiver().map_or(0, expr) + args.iter().map(expr).sum::<u32>()
            }
            K::Unary { operand, .. } | K::Cast(operand) => expr(operand),
            K::Binary { left, right, .. }
            | K::Assign {
                target: left,
                value: right,
                ..
            } => expr(left) + expr(right),
            K::Call { callee, args } => {
                let callee_count = match callee {
                    hir::Callee::Value(value) => expr(value),
                    hir::Callee::Method { recv, .. } => expr(recv),
                    _ => 0,
                };
                callee_count + args.iter().map(expr).sum::<u32>()
            }
            K::New { args, .. } => args.iter().map(expr).sum(),
            K::DescriptorLit { fields, .. } => fields.iter().flatten().map(expr).sum(),
            K::Field { obj, .. } | K::JsonResultValue(obj) | K::Length(obj) => expr(obj),
            K::Index { obj, index, .. } => expr(obj) + expr(index),
            K::ArrayLit(elems) => elems.iter().map(expr).sum(),
            K::ArraySpreadLit(elems) => elems.iter().map(|elem| expr(&elem.expr)).sum(),
            K::Template(parts) => parts
                .iter()
                .map(|part| match part {
                    hir::TplPart::Expr(value) => expr(value),
                    _ => 0,
                })
                .sum(),
            K::Cond { cond, then, els } => expr(cond) + expr(then) + expr(els),
            K::Yield(arg) => arg.as_deref().map_or(0, expr),
            K::Lambda { body, .. } => count_async_calls(body),
            _ => 0,
        }
    }

    let mut count = 0;
    for stmt in stmts {
        count += match stmt {
            hir::Stmt::Let { init, .. } | hir::Stmt::Expr(init) => expr(init),
            hir::Stmt::Return { value, .. } => value.as_ref().map_or(0, expr),
            hir::Stmt::If {
                cond, then, els, ..
            } => expr(cond) + count_async_calls(then) + els.as_deref().map_or(0, count_async_calls),
            hir::Stmt::While { cond, body, .. } => expr(cond) + count_async_calls(body),
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                init.as_deref()
                    .map_or(0, |stmt| count_async_calls(std::slice::from_ref(stmt)))
                    + cond.as_ref().map_or(0, expr)
                    + step.as_ref().map_or(0, expr)
                    + count_async_calls(body)
            }
            hir::Stmt::ForOf { subject, body, .. } => expr(subject) + count_async_calls(body),
            hir::Stmt::Switch { disc, cases, .. } => {
                expr(disc)
                    + cases
                        .iter()
                        .map(|case| {
                            case.test.as_ref().map_or(0, expr) + count_async_calls(&case.body)
                        })
                        .sum::<u32>()
            }
            hir::Stmt::Block(body) => count_async_calls(body),
            _ => 0,
        };
    }
    count
}

fn is_array_mutator(recv_ty: &Type, name: &str) -> bool {
    matches!(recv_ty, Type::Array(_)) && matches!(name, "push" | "pop")
}

fn binop_sym(op: hir::BinOp) -> Result<&'static str, String> {
    use hir::BinOp as B;
    Ok(match op {
        B::Add => "+",
        B::Sub => "-",
        B::Mul => "*",
        B::Div => "/",
        B::Eq => "==",
        B::Ne => "!=",
        B::Lt => "<",
        B::Le => "<=",
        B::Gt => ">",
        B::Ge => ">=",
        B::And => "&&",
        B::Or => "||",
        B::BitAnd => "&",
        B::BitOr => "|",
        B::BitXor => "^",
        B::Shl => "<<",
        B::Shr => ">>",
        B::UShr => ">>",
        _ => return Err("unknown binary operator".to_string()),
    })
}

fn divrem_helper(ty: &Type, is_div: bool) -> Result<&'static str, String> {
    Ok(match (ty, is_div) {
        (Type::I8, true) => "subscript_sdiv_i8",
        (Type::I8, false) => "subscript_srem_i8",
        (Type::U8, true) => "subscript_udiv_u8",
        (Type::U8, false) => "subscript_urem_u8",
        (Type::I16, true) => "subscript_sdiv_i16",
        (Type::I16, false) => "subscript_srem_i16",
        (Type::U16, true) => "subscript_udiv_u16",
        (Type::U16, false) => "subscript_urem_u16",
        (Type::I32, true) => "subscript_sdiv_i32",
        (Type::I32, false) => "subscript_srem_i32",
        (Type::U32, true) => "subscript_udiv_u32",
        (Type::U32, false) => "subscript_urem_u32",
        (Type::I64, true) => "subscript_sdiv_i64",
        (Type::I64, false) => "subscript_srem_i64",
        (Type::U64, true) => "subscript_udiv_u64",
        (Type::U64, false) => "subscript_urem_u64",
        (other, _) => return Err(format!("integer div/rem on {other:?}")),
    })
}

fn float_to_int_helper(to: &Type) -> Result<&'static str, String> {
    Ok(match to {
        Type::I8 => "subscript_f2i8",
        Type::U8 => "subscript_f2u8",
        Type::I16 => "subscript_f2i16",
        Type::U16 => "subscript_f2u16",
        Type::I32 => "subscript_f2i32",
        Type::U32 => "subscript_f2u32",
        Type::I64 => "subscript_f2i64",
        Type::U64 => "subscript_f2u64",
        other => return Err(format!("float to {other:?}")),
    })
}

fn is_narrow_integer(ty: &Type) -> bool {
    matches!(ty, Type::I8 | Type::U8 | Type::I16 | Type::U16)
}

fn unsigned_ctype(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => "uint8_t",
        Type::I16 | Type::U16 => "uint16_t",
        Type::I32 | Type::U32 => "uint32_t",
        Type::I64 | Type::U64 => "uint64_t",
        other => return Err(format!("unsigned carrier for {other:?}")),
    })
}

fn integer_ctype(ty: &Type) -> Result<&'static str, String> {
    Ok(match ty {
        Type::I8 => "int8_t",
        Type::U8 => "uint8_t",
        Type::I16 => "int16_t",
        Type::U16 => "uint16_t",
        Type::I32 => "int32_t",
        Type::U32 => "uint32_t",
        Type::I64 => "int64_t",
        Type::U64 => "uint64_t",
        other => return Err(format!("integer carrier for {other:?}")),
    })
}

fn integer_width(ty: &Type) -> Result<u32, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 => 32,
        Type::I64 | Type::U64 => 64,
        other => return Err(format!("shift width for {other:?}")),
    })
}

/// Emits Q18's total shift operation. The explicit amount mask is
/// required even for narrow types because C promotes them to `int`
/// before shifting. Left shift and logical right shift use an unsigned
/// carrier, avoiding signed-left-shift overflow as well as over-shift UB.
fn shift_expr(op: hir::BinOp, ty: &Type, left: &str, right: &str) -> Result<String, String> {
    let width = integer_width(ty)?;
    let amount = format!("(({right}) & {}u)", width - 1);
    let carrier = match op {
        hir::BinOp::Shl | hir::BinOp::UShr => unsigned_ctype(ty)?,
        hir::BinOp::Shr => integer_ctype(ty)?,
        other => return Err(format!("shift expression for {other:?}")),
    };
    let sym = if op == hir::BinOp::Shl { "<<" } else { ">>" };
    Ok(format!(
        "(({})((({carrier})({left})) {sym} {amount}))",
        integer_ctype(ty)?
    ))
}

/// Sanitizes an HIR identifier (which may carry `<...>` from
/// monomorphization) into a C identifier fragment.
fn sanitize(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            s.push(ch);
        } else {
            s.push('_');
        }
    }
    if is_c_keyword(&s) {
        s.push('_');
    }
    s
}

/// C keywords (and a few common reserved identifiers) a script name must
/// not collide with; a colliding name gets a trailing `_`, applied
/// uniformly so declarations and uses still agree.
fn is_c_keyword(s: &str) -> bool {
    matches!(
        s,
        "auto"
            | "break"
            | "case"
            | "char"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extern"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "register"
            | "restrict"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "typedef"
            | "union"
            | "unsigned"
            | "void"
            | "volatile"
            | "while"
            | "_Bool"
            | "_Complex"
            | "ctx"
    )
}

fn int_literal(v: i64, ty: &Type) -> String {
    match ty {
        Type::U8 => format!("((uint8_t){})", v as u8),
        Type::U16 => format!("((uint16_t){})", v as u16),
        Type::I8 => format!("((int8_t){v})"),
        Type::I16 => format!("((int16_t){v})"),
        Type::U32 => format!("{}u", v as u32),
        Type::U64 => format!("{}ull", v as u64),
        Type::I64 => {
            if v == i64::MIN {
                "(-9223372036854775807ll - 1)".to_string()
            } else {
                format!("{v}ll")
            }
        }
        _ => {
            if v == i64::from(i32::MIN) {
                "(-2147483647 - 1)".to_string()
            } else {
                v.to_string()
            }
        }
    }
}

fn float_literal(v: f64, ty: &Type) -> String {
    if *ty == Type::F32 {
        let f = v as f32;
        if f.is_nan() {
            return "((float)(0.0f/0.0f))".to_string();
        }
        if f.is_infinite() {
            return if f < 0.0 {
                "((float)(-1.0f/0.0f))".to_string()
            } else {
                "((float)(1.0f/0.0f))".to_string()
            };
        }
        let mut s = format!("{f:?}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            s.push_str(".0");
        }
        format!("{s}f")
    } else {
        if v.is_nan() {
            return "(0.0/0.0)".to_string();
        }
        if v.is_infinite() {
            return if v < 0.0 {
                "(-1.0/0.0)".to_string()
            } else {
                "(1.0/0.0)".to_string()
            };
        }
        let mut s = format!("{v:?}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            s.push_str(".0");
        }
        s
    }
}

fn render_allocation_metadata_header() -> String {
    r#"/* DO NOT EDIT. Generated by subscript-codegen from the checked program. */
#ifndef SUBSCRIPT_ALLOCATION_METADATA_H
#define SUBSCRIPT_ALLOCATION_METADATA_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint32_t class_id;
    const char *name;
} subscript_alloc_class_info;

typedef struct {
    const char *file;
    uint32_t line;
    uint32_t column;
} subscript_alloc_position_info;

extern const subscript_alloc_class_info subscript_alloc_classes[];
extern const uint64_t subscript_alloc_class_count;
extern const subscript_alloc_position_info subscript_alloc_positions[];
extern const uint64_t subscript_alloc_position_count;

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* SUBSCRIPT_ALLOCATION_METADATA_H */
"#
    .to_string()
}

fn render_allocation_metadata_definitions(module: &hir::Module, positions: &[Pos]) -> String {
    let mut out = String::from(
        "\n/* Allocation attribution tables. Generated from checked HIR and the\n\
         * exact pos_id sequence above; consume through the generated\n\
         * allocation metadata header. */\n\
         typedef struct { uint32_t class_id; const char *name; } subscript_alloc_class_info;\n\
         typedef struct { const char *file; uint32_t line; uint32_t column; } subscript_alloc_position_info;\n\n\
         const subscript_alloc_class_info subscript_alloc_classes[] = {\n",
    );
    for (class_id, name) in [
        (rtc::CLASS_STRING, "string"),
        (rtc::CLASS_ARRAY, "Array"),
        (rtc::CLASS_ARRAY_DATA, "ArrayData"),
        (rtc::CLASS_GENERATOR, "GeneratorFrame"),
        (rtc::CLASS_MAP, "Map"),
        (rtc::CLASS_SET, "Set"),
        (rtc::CLASS_MAP_DATA, "MapData"),
        (rtc::CLASS_MAP_INDEX, "MapIndex"),
    ] {
        let _ = writeln!(
            out,
            "    {{ {class_id}u, {} }},",
            c_string_literal(name.as_bytes())
        );
    }
    for (class_id, class) in module.classes.iter().enumerate() {
        let _ = writeln!(
            out,
            "    {{ {class_id}u, {} }},",
            c_string_literal(class.name.as_bytes())
        );
    }
    let _ = writeln!(
        out,
        "}};\nconst uint64_t subscript_alloc_class_count = {}u;\n",
        8 + module.classes.len()
    );

    out.push_str("const subscript_alloc_position_info subscript_alloc_positions[] = {\n");
    if positions.is_empty() {
        out.push_str("    { \"\", 0u, 0u }, /* placeholder; count is zero */\n");
    } else {
        for pos in positions {
            let _ = writeln!(
                out,
                "    {{ {}, {}u, {}u }},",
                c_string_literal(pos.file.as_bytes()),
                pos.line,
                pos.col
            );
        }
    }
    let _ = writeln!(
        out,
        "}};\nconst uint64_t subscript_alloc_position_count = {}u;",
        positions.len()
    );
    out
}

/// A C string literal with the exact bytes; non-printable bytes use
/// three-digit octal escapes (unambiguous, unlike `\x`).
fn c_string_literal(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(b as char),
            other => {
                let _ = write!(out, "\\{other:03o}");
            }
        }
    }
    out.push('"');
    out
}

/// The fixed prelude: runtime `extern` declarations, checked
/// integer-div/rem helpers, saturating float→int helpers, and the array
/// header view used by caller-expanded checked index access.
const PREAMBLE: &str = concat!(
    include_str!("../../runtime/include/subscript_runtime.h"),
    r#"

/* Generated by subscript's C emitter — the ship tier
 * (specs/blocks/compiler.md 11). Do not edit; fix the generator.
 * This translation unit carries the language's semantics and links the
 * runtime static library (subscript_rt_*), so arrays, strings, Q14 formatting,
 * and traps are identical to the dev-JIT tier. Compile -O2
 * -ffp-contract=off and link with the runtime archive and the host
 * entry (AOT_ENTRY_C). */

#include <stdint.h>
#include <string.h>

/* Runtime C-ABI boundary (runtime/src/ffi.rs). Handles are void*. */
extern void subscript_rt_print(void* ctx, const void* s);
extern void subscript_rt_collect(void* ctx);
extern void* subscript_rt_alloc(void* ctx, uint64_t size, uint32_t class_id, uint32_t pos_id);
extern void* subscript_rt_globals_init(void* ctx, uint64_t size, uint64_t align);
extern uint64_t subscript_rt_boundary_scratch_mark(void* ctx);
extern void* subscript_rt_boundary_scratch_alloc(void* ctx, uint64_t size, uint32_t pos_id);
extern void subscript_rt_boundary_scratch_release(void* ctx, uint64_t mark);
typedef uint8_t (*SubAsyncResume)(void* ctx, void* frame, void* out);
extern void subscript_rt_async_kick(void* ctx, void* frame, SubAsyncResume resume);
extern void subscript_rt_delete(void* ctx, void* payload, uint32_t pos_id);
extern void subscript_rt_trap(void* ctx, uint32_t kind, uint32_t pos_id);
extern void subscript_rt_trap_wire_enum(void* ctx, const unsigned char* alias, uint64_t alias_len, int32_t wire_value, uint32_t pos_id);
extern void subscript_rt_root_add(void* ctx, void* base, uint64_t words);
extern void subscript_rt_shadow_push(void* ctx, void* base, uint64_t slots);
extern void subscript_rt_shadow_pop(void* ctx);
extern void* subscript_rt_str_lit(void* ctx, const unsigned char* ptr, uint64_t len, uint32_t pos_id);
extern void* subscript_rt_str_from_view(void* ctx, const unsigned char* ptr, uint64_t len, uint32_t pos_id);
extern int32_t subscript_rt_str_len(void* ctx, const void* s);
extern void* subscript_rt_str_concat(void* ctx, const void* a, const void* b, uint32_t pos_id);
extern void* subscript_rt_str_slice(void* ctx, const void* s, int32_t start, int32_t end, uint32_t pos_id);
extern int32_t subscript_rt_str_eq(void* ctx, const void* a, const void* b);
extern void* subscript_rt_fmt_i32(void* ctx, int32_t v, uint32_t pos_id);
extern void* subscript_rt_fmt_u32(void* ctx, uint32_t v, uint32_t pos_id);
extern void* subscript_rt_fmt_i64(void* ctx, int64_t v, uint32_t pos_id);
extern void* subscript_rt_fmt_u64(void* ctx, uint64_t v, uint32_t pos_id);
extern void* subscript_rt_fmt_f32(void* ctx, float v, uint32_t pos_id);
extern void* subscript_rt_fmt_f64(void* ctx, double v, uint32_t pos_id);
extern void* subscript_rt_fmt_bool(void* ctx, uint32_t v, uint32_t pos_id);
/* P13 JSON.stringify builder leaves. The checker emits traversal helpers
 * for one exact static T; these runtime entries contain no RTTI. */
extern uint64_t subscript_rt_json_begin(void* ctx, uint32_t pos_id);
extern uint64_t subscript_rt_json_begin_tracked(void* ctx, uint32_t pos_id);
extern void* subscript_rt_json_finish(void* ctx, uint64_t builder, uint32_t pos_id);
extern void subscript_rt_json_raw(void* ctx, uint64_t builder, const void* value, uint32_t pos_id);
extern void subscript_rt_json_str(void* ctx, uint64_t builder, const void* value, uint32_t pos_id);
extern void subscript_rt_json_i32(void* ctx, uint64_t builder, int32_t value, uint32_t pos_id);
extern void subscript_rt_json_u32(void* ctx, uint64_t builder, uint32_t value, uint32_t pos_id);
extern void subscript_rt_json_i64(void* ctx, uint64_t builder, int64_t value, uint32_t pos_id);
extern void subscript_rt_json_u64(void* ctx, uint64_t builder, uint64_t value, uint32_t pos_id);
extern void subscript_rt_json_f32(void* ctx, uint64_t builder, float value, uint32_t pos_id);
extern void subscript_rt_json_f64(void* ctx, uint64_t builder, double value, uint32_t pos_id);
extern void subscript_rt_json_bool(void* ctx, uint64_t builder, uint8_t value, uint32_t pos_id);
extern void subscript_rt_json_date(void* ctx, uint64_t builder, int64_t value, uint32_t pos_id);
extern void subscript_rt_json_null(void* ctx, uint64_t builder, uint32_t pos_id);
extern int32_t subscript_rt_json_visit(void* ctx, uint64_t builder, const void* value, uint32_t pos_id);
extern void subscript_rt_json_leave(void* ctx, uint64_t builder, const void* value, uint32_t pos_id);
extern uint64_t subscript_rt_json_parse_begin(void* ctx, const void* text, uint32_t pos_id);
extern void subscript_rt_json_parse_end(void* ctx, uint64_t parser, uint32_t pos_id);
extern uint64_t subscript_rt_json_parse_root(void* ctx, uint64_t parser, uint32_t pos_id);
extern int32_t subscript_rt_json_parse_is_kind(void* ctx, uint64_t parser, uint64_t node, uint32_t kind, uint32_t pos_id);
extern int32_t subscript_rt_json_parse_number_fits(void* ctx, uint64_t parser, uint64_t node, uint32_t target, uint32_t pos_id);
extern double subscript_rt_json_parse_number(void* ctx, uint64_t parser, uint64_t node, uint32_t pos_id);
extern uint64_t subscript_rt_json_parse_integer(void* ctx, uint64_t parser, uint64_t node, uint32_t target, uint32_t pos_id);
extern int32_t subscript_rt_json_parse_bool(void* ctx, uint64_t parser, uint64_t node, uint32_t pos_id);
extern void* subscript_rt_json_parse_string(void* ctx, uint64_t parser, uint64_t node, uint32_t pos_id);
extern int32_t subscript_rt_json_parse_array_len(void* ctx, uint64_t parser, uint64_t node, uint32_t pos_id);
extern uint64_t subscript_rt_json_parse_array_get(void* ctx, uint64_t parser, uint64_t node, int32_t index, uint32_t pos_id);
extern uint64_t subscript_rt_json_parse_object_get(void* ctx, uint64_t parser, uint64_t node, const void* key, uint32_t pos_id);
/* Number and parsing intrinsics (stdlib.md 11, Q25/Q26).
 * Trap-capable entries carry source positions. */
extern int32_t subscript_rt_num_is_nan(void* ctx, double value);
extern int32_t subscript_rt_num_is_finite(void* ctx, double value);
extern int32_t subscript_rt_num_is_integer(void* ctx, double value);
extern int32_t subscript_rt_num_is_safe_integer(void* ctx, double value);
extern double subscript_rt_num_parse_int(void* ctx, const void* s, int32_t radix, uint32_t pos_id);
extern double subscript_rt_num_parse_float(void* ctx, const void* s, uint32_t pos_id);
extern void* subscript_rt_num_to_fixed(void* ctx, double value, int32_t digits, uint32_t pos_id);
extern void* subscript_rt_num_to_string_f32(void* ctx, float value, int32_t radix, uint32_t pos_id);
extern void* subscript_rt_num_to_string_f64(void* ctx, double value, int32_t radix, uint32_t pos_id);
extern void* subscript_rt_num_to_exponential(void* ctx, double value, int32_t digits, uint32_t pos_id);
extern void* subscript_rt_num_to_precision(void* ctx, double value, int32_t digits, uint32_t pos_id);
/* IEEE binary16 is raw uint16_t storage in emitted C. All conversion is
 * behind these opaque runtime symbols; no _Float16/__fp16 operation is
 * emitted (compiler.md 16.2). */
extern uint16_t subscript_rt_f16_from_f64(double v);
extern double subscript_rt_f16_to_f64(uint16_t bits);
extern void* subscript_rt_array_new(void* ctx, uint64_t elem_size, uint32_t pos_id);
extern int32_t subscript_rt_array_len(void* ctx, const void* a);
extern int32_t subscript_rt_array_push(void* ctx, void* a, const void* src, uint32_t pos_id);
extern void subscript_rt_array_pop(void* ctx, void* a, void* dst, uint32_t pos_id);
extern void* subscript_rt_array_ptr(void* ctx, void* a, int32_t idx, uint32_t pos_id);
extern uint64_t subscript_rt_assoc_iter_begin(void* ctx, void* value, uint32_t pos_id);
extern int32_t subscript_rt_assoc_iter_copy(void* ctx, void* value, uint64_t index, uint32_t select_value, void* out, uint32_t pos_id);
extern void subscript_rt_assoc_iter_end(void* ctx, void* value);
extern void* subscript_rt_str_iter_code_point(void* ctx, const void* s, int32_t index, int32_t* next, uint32_t pos_id);
extern void subscript_rt_array_spread_array(void* ctx, void* out, void* source, uint32_t pos_id);
extern void subscript_rt_array_spread_fixed(void* ctx, void* out, const void* data, uint64_t count, uint32_t pos_id);
extern void subscript_rt_array_spread_assoc(void* ctx, void* out, void* source, uint32_t pos_id);
extern void subscript_rt_array_spread_string(void* ctx, void* out, const void* source, uint32_t pos_id);
/* C-boundary marshaling (P5.2b): string/array data pointers and the
 * callback binding constructor. The generic trampoline declaration and
 * its local string-view layout follow the bound header includes. */
extern const void* subscript_rt_str_data(void* ctx, const void* s);
extern const void* subscript_rt_array_data(void* ctx, const void* a);
extern void* subscript_rt_cb_bind(void* ctx, const void* code, const void* env, void* userdata1, void* userdata2);

/* String method intrinsics (stdlib.md 8, Q21): byte measures over the
 * immutable UTF-8 string payloads; one opaque runtime symbol per
 * accepted method, shared with the dev tier. Fault-capable entries
 * carry a trailing pos_id; the pure search predicates take none. */
extern int32_t subscript_rt_str_index_of(void* ctx, const void* s, const void* needle, int32_t from);
extern int32_t subscript_rt_str_last_index_of(void* ctx, const void* s, const void* needle);
extern int32_t subscript_rt_str_includes(void* ctx, const void* s, const void* needle, int32_t from);
extern int32_t subscript_rt_str_starts_with(void* ctx, const void* s, const void* needle, int32_t position);
extern int32_t subscript_rt_str_ends_with(void* ctx, const void* s, const void* needle, int32_t end_position);
extern int32_t subscript_rt_str_char_code_at(void* ctx, const void* s, int32_t i, uint32_t pos_id);
extern void* subscript_rt_str_split(void* ctx, const void* s, const void* sep, uint32_t pos_id);
extern void* subscript_rt_str_trim(void* ctx, const void* s, uint32_t pos_id);
extern void* subscript_rt_str_trim_start(void* ctx, const void* s, uint32_t pos_id);
extern void* subscript_rt_str_trim_end(void* ctx, const void* s, uint32_t pos_id);
extern void* subscript_rt_str_repeat(void* ctx, const void* s, int32_t n, uint32_t pos_id);
extern void* subscript_rt_str_pad_start(void* ctx, const void* s, int32_t len, const void* pad, uint32_t pos_id);
extern void* subscript_rt_str_pad_end(void* ctx, const void* s, int32_t len, const void* pad, uint32_t pos_id);
extern void* subscript_rt_str_to_upper(void* ctx, const void* s, uint32_t pos_id);
extern void* subscript_rt_str_to_lower(void* ctx, const void* s, uint32_t pos_id);
extern void* subscript_rt_str_replace(void* ctx, const void* s, const void* pat, const void* repl, uint32_t pos_id);
extern void* subscript_rt_str_replace_all(void* ctx, const void* s, const void* pat, const void* repl, uint32_t pos_id);
extern void* subscript_rt_str_substring(void* ctx, const void* s, int32_t start, int32_t end, uint32_t pos_id);
extern void* subscript_rt_str_substr(void* ctx, const void* s, int32_t start, int32_t length, uint32_t pos_id);
extern void* subscript_rt_str_char_at(void* ctx, const void* s, int32_t i, uint32_t pos_id);
extern int32_t subscript_rt_str_code_point_at(void* ctx, const void* s, int32_t i, uint32_t pos_id);
extern void* subscript_rt_str_method_concat(void* ctx, const void* a, const void* b, uint32_t pos_id);

/* Regular-expression intrinsics (stdlib.md 15, Q31). */
extern void* subscript_rt_regex_new(void* ctx, const void* pattern, const void* flags, uint32_t pos_id);
extern int32_t subscript_rt_regex_test(void* ctx, const void* regex, const void* subject, uint32_t pos_id);
extern void* subscript_rt_regex_source(void* ctx, const void* regex, uint32_t pos_id);
extern void* subscript_rt_regex_flags(void* ctx, const void* regex, uint32_t pos_id);
extern int32_t subscript_rt_regex_search(void* ctx, const void* subject, const void* regex, uint32_t pos_id);
extern void* subscript_rt_regex_replace(void* ctx, const void* subject, const void* regex, const void* replacement, uint32_t pos_id);
extern void* subscript_rt_regex_replace_all(void* ctx, const void* subject, const void* regex, const void* replacement, uint32_t pos_id);
extern void* subscript_rt_regex_split(void* ctx, const void* subject, const void* regex, uint32_t pos_id);
extern int32_t subscript_rt_regex_match_start(void* ctx, const void* regex, int32_t group, uint32_t pos_id);
extern int32_t subscript_rt_regex_match_end(void* ctx, const void* regex, int32_t group, uint32_t pos_id);

/* Array method intrinsics (stdlib.md 9, Q22): one opaque runtime symbol
 * per accepted method, shared with the dev tier. Element values the
 * runtime receives travel by pointer; script callbacks travel as the
 * SubFn (code, env) halves; kind tags are the shared compiler mapping's
 * u32 codes; allocating entries carry a trailing pos_id. */
extern int32_t subscript_rt_arr_index_of(void* ctx, void* a, const void* x, uint32_t kind);
extern int32_t subscript_rt_arr_last_index_of(void* ctx, void* a, const void* x, uint32_t kind);
extern int32_t subscript_rt_arr_includes(void* ctx, void* a, const void* x, uint32_t kind);
extern void* subscript_rt_arr_join(void* ctx, void* a, const void* sep, uint32_t kind, uint32_t pos_id);
extern void* subscript_rt_arr_slice(void* ctx, void* a, int32_t start, int32_t end, uint32_t pos_id);
extern void subscript_rt_arr_fill(void* ctx, void* a, const void* x, int32_t start, int32_t end);
extern void subscript_rt_arr_reverse(void* ctx, void* a);
extern void* subscript_rt_arr_concat(void* ctx, void* a, void* b, uint32_t pos_id);
extern void subscript_rt_arr_for_each(void* ctx, void* a, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern void* subscript_rt_arr_map(void* ctx, void* a, const void* code, const void* env, uint32_t elem_kind, uint32_t ret_kind, uint64_t ret_size, uint32_t pos_id, uint32_t indexed);
extern void* subscript_rt_arr_filter(void* ctx, void* a, const void* code, const void* env, uint32_t kind, uint32_t pos_id, uint32_t indexed);
extern void subscript_rt_arr_reduce(void* ctx, void* a, const void* code, const void* env, uint32_t elem_kind, uint32_t acc_kind, uint64_t acc_size, void* acc, uint32_t indexed);
extern int32_t subscript_rt_arr_some(void* ctx, void* a, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern int32_t subscript_rt_arr_every(void* ctx, void* a, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern int32_t subscript_rt_arr_find_index(void* ctx, void* a, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern void subscript_rt_arr_sort(void* ctx, void* a, const void* code, const void* env, uint32_t kind);
extern void subscript_rt_arr_reduce_right(void* ctx, void* a, const void* code, const void* env, uint32_t elem_kind, uint32_t acc_kind, uint64_t acc_size, void* acc, uint32_t indexed);
extern void* subscript_rt_arr_splice(void* ctx, void* a, int32_t start, int32_t delete_count, uint32_t pos_id);
extern void subscript_rt_arr_shift(void* ctx, void* a, void* out, uint32_t pos_id);
extern int32_t subscript_rt_arr_unshift(void* ctx, void* a, const void* x, uint32_t pos_id);
extern void subscript_rt_arr_copy_within(void* ctx, void* a, int32_t target, int32_t start, int32_t end);
/* Q27 callback family on in-place FixedArray storage. These mirror the
 * dynamic-array callback ABI, with `(data, len, elem_size)` replacing
 * the growable-array handle. map/filter still return dynamic arrays. */
extern void subscript_rt_fixed_arr_for_each(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern void* subscript_rt_fixed_arr_map(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t elem_kind, uint32_t ret_kind, uint64_t ret_size, uint32_t pos_id, uint32_t indexed);
extern void* subscript_rt_fixed_arr_filter(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t kind, uint32_t pos_id, uint32_t indexed);
extern void subscript_rt_fixed_arr_reduce(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t elem_kind, uint32_t acc_kind, uint64_t acc_size, void* acc, uint32_t indexed);
extern int32_t subscript_rt_fixed_arr_some(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern int32_t subscript_rt_fixed_arr_every(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern int32_t subscript_rt_fixed_arr_find_index(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t kind, uint32_t indexed);
extern void subscript_rt_fixed_arr_reduce_right(void* ctx, const void* data, uint64_t len, uint64_t elem_size, const void* code, const void* env, uint32_t elem_kind, uint32_t acc_kind, uint64_t acc_size, void* acc, uint32_t indexed);

/* Map/Set intrinsics (stdlib.md 10, Q24): ordered entry storage and its
 * deterministic hash index live behind these shared runtime symbols.
 * Construction supplies monomorphized widths and the key-kind tag. */
extern void* subscript_rt_map_new(void* ctx, uint64_t key_size, uint64_t value_size, uint32_t key_kind, uint32_t pos_id);
extern void* subscript_rt_set_new(void* ctx, uint64_t key_size, uint32_t key_kind, uint32_t pos_id);
extern int32_t subscript_rt_map_size(void* ctx, void* map);
extern int32_t subscript_rt_set_size(void* ctx, void* set);
extern void* subscript_rt_map_set(void* ctx, void* map, const void* key, const void* value, uint32_t pos_id);
extern void* subscript_rt_set_add(void* ctx, void* set, const void* key, uint32_t pos_id);
extern int32_t subscript_rt_map_get(void* ctx, void* map, const void* key, void* out);
extern void subscript_rt_map_get_or(void* ctx, void* map, const void* key, const void* fallback, void* out);
extern int32_t subscript_rt_map_has(void* ctx, void* map, const void* key);
extern int32_t subscript_rt_set_has(void* ctx, void* set, const void* key);
extern int32_t subscript_rt_map_delete(void* ctx, void* map, const void* key);
extern int32_t subscript_rt_set_delete(void* ctx, void* set, const void* key);
extern void subscript_rt_map_clear(void* ctx, void* map);
extern void subscript_rt_set_clear(void* ctx, void* set);
extern void subscript_rt_map_for_each(void* ctx, void* map, const void* code, const void* env, const void* bridge);
extern void subscript_rt_set_for_each(void* ctx, void* set, const void* code, const void* env, const void* bridge);
extern void* subscript_rt_map_group_by(void* ctx, void* items, const void* code, const void* env, const void* bridge, uint64_t key_size, uint32_t key_kind, uint32_t pos_id);
extern void* subscript_rt_set_union(void* ctx, void* left, void* right, uint32_t pos_id);
extern void* subscript_rt_set_intersection(void* ctx, void* left, void* right, uint32_t pos_id);
extern void* subscript_rt_set_difference(void* ctx, void* left, void* right, uint32_t pos_id);
extern void* subscript_rt_set_symmetric_difference(void* ctx, void* left, void* right, uint32_t pos_id);
extern int32_t subscript_rt_set_is_subset_of(void* ctx, void* left, void* right);
extern int32_t subscript_rt_set_is_superset_of(void* ctx, void* left, void* right);
extern int32_t subscript_rt_set_is_disjoint_from(void* ctx, void* left, void* right);

/* Math intrinsics (stdlib.md 1): opaque runtime symbols, never bare
 * libm calls — clang constant-folds recognized libm calls at -O2, a
 * silent dev-JIT != ship-C divergence hazard (stdlib.md 0.2). */
extern double subscript_rt_math_abs(void* ctx, double x);
extern double subscript_rt_math_acos(void* ctx, double x);
extern double subscript_rt_math_acosh(void* ctx, double x);
extern double subscript_rt_math_asin(void* ctx, double x);
extern double subscript_rt_math_asinh(void* ctx, double x);
extern double subscript_rt_math_atan(void* ctx, double x);
extern double subscript_rt_math_atanh(void* ctx, double x);
extern double subscript_rt_math_cbrt(void* ctx, double x);
extern double subscript_rt_math_ceil(void* ctx, double x);
extern double subscript_rt_math_cos(void* ctx, double x);
extern double subscript_rt_math_cosh(void* ctx, double x);
extern double subscript_rt_math_exp(void* ctx, double x);
extern double subscript_rt_math_expm1(void* ctx, double x);
extern double subscript_rt_math_floor(void* ctx, double x);
extern double subscript_rt_math_log(void* ctx, double x);
extern double subscript_rt_math_log1p(void* ctx, double x);
extern double subscript_rt_math_log10(void* ctx, double x);
extern double subscript_rt_math_log2(void* ctx, double x);
extern double subscript_rt_math_round(void* ctx, double x);
extern double subscript_rt_math_sign(void* ctx, double x);
extern double subscript_rt_math_sin(void* ctx, double x);
extern double subscript_rt_math_sinh(void* ctx, double x);
extern double subscript_rt_math_sqrt(void* ctx, double x);
extern double subscript_rt_math_tan(void* ctx, double x);
extern double subscript_rt_math_tanh(void* ctx, double x);
extern double subscript_rt_math_trunc(void* ctx, double x);
extern double subscript_rt_math_atan2(void* ctx, double y, double x);
extern double subscript_rt_math_hypot(void* ctx, double a, double b);
extern double subscript_rt_math_pow(void* ctx, double base, double exp);
extern double subscript_rt_math_max(void* ctx, double a, double b);
extern double subscript_rt_math_min(void* ctx, double a, double b);
extern double subscript_rt_math_random(void* ctx);
extern int32_t subscript_rt_math_clz32(void* ctx, uint32_t x);
extern int32_t subscript_rt_math_imul(void* ctx, int32_t a, int32_t b);
extern double subscript_rt_math_fround(void* ctx, double x);
extern uint32_t subscript_rt_math_f32_to_bits(void* ctx, double x);
extern double subscript_rt_math_f32_from_bits(void* ctx, uint32_t bits);

/* Date intrinsics (stdlib.md 3): a Date value is its int64_t epoch
 * milliseconds; the calendar arithmetic lives in the runtime so both
 * tiers share one implementation. */
extern int64_t subscript_rt_date_utc(void* ctx, int32_t y, int32_t m0, int32_t d, int32_t h, int32_t min, int32_t s, int32_t ms, uint32_t pos_id);
extern int64_t subscript_rt_date_new(void* ctx, int64_t ms, uint32_t pos_id);
extern int64_t subscript_rt_date_now(void* ctx);
extern int32_t subscript_rt_date_get(void* ctx, int64_t ms, uint32_t field);
extern void* subscript_rt_date_to_iso(void* ctx, int64_t ms, uint32_t pos_id);

/* Trap kinds (runtime/src/trap.rs). */
enum { SS_TRAP_OOB = 1, SS_TRAP_DIV0 = 10 };

/* A non-capturing function value / capturing lambda: (code, env). */
typedef struct { void* code; void* env; } SubFn;

/* Mirror of the runtime ArrayHeader (runtime/src/context.rs, repr(C),
 * compiler.md invariant 1 / §10a). Generated index call sites expand
 * the bounds branch around this view; the out-of-bounds arm calls
 * subscript_rt_array_ptr, the sole producer of the trap and its exact message,
 * and returns from the current script frame before any load or store. */
typedef struct { uint64_t len; uint64_t cap; uint64_t elem_size; unsigned char* data; } SsArrayHeader;

/* Integer div/rem with the language's semantics: trap on a zero divisor;
 * two's-complement wrap for signed MIN / -1 and MIN % -1. */
static int8_t subscript_sdiv_i8(void* ctx, int8_t a, int8_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (int8_t)((int32_t)a / (int32_t)b);
}
static int8_t subscript_srem_i8(void* ctx, int8_t a, int8_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (int8_t)((int32_t)a % (int32_t)b);
}
static uint8_t subscript_udiv_u8(void* ctx, uint8_t a, uint8_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (uint8_t)((uint32_t)a / (uint32_t)b);
}
static uint8_t subscript_urem_u8(void* ctx, uint8_t a, uint8_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (uint8_t)((uint32_t)a % (uint32_t)b);
}
static int16_t subscript_sdiv_i16(void* ctx, int16_t a, int16_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (int16_t)((int32_t)a / (int32_t)b);
}
static int16_t subscript_srem_i16(void* ctx, int16_t a, int16_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (int16_t)((int32_t)a % (int32_t)b);
}
static uint16_t subscript_udiv_u16(void* ctx, uint16_t a, uint16_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (uint16_t)((uint32_t)a / (uint32_t)b);
}
static uint16_t subscript_urem_u16(void* ctx, uint16_t a, uint16_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return (uint16_t)((uint32_t)a % (uint32_t)b);
}
static int32_t subscript_sdiv_i32(void* ctx, int32_t a, int32_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return (int32_t)(0u - (uint32_t)a);
    return a / b;
}
static int32_t subscript_srem_i32(void* ctx, int32_t a, int32_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return 0;
    return a % b;
}
static uint32_t subscript_udiv_u32(void* ctx, uint32_t a, uint32_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a / b;
}
static uint32_t subscript_urem_u32(void* ctx, uint32_t a, uint32_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a % b;
}
static int64_t subscript_sdiv_i64(void* ctx, int64_t a, int64_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return (int64_t)(0ull - (uint64_t)a);
    return a / b;
}
static int64_t subscript_srem_i64(void* ctx, int64_t a, int64_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return 0;
    return a % b;
}
static uint64_t subscript_udiv_u64(void* ctx, uint64_t a, uint64_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a / b;
}
static uint64_t subscript_urem_u64(void* ctx, uint64_t a, uint64_t b, uint32_t pos) {
    if (b == 0) { subscript_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a % b;
}

/* Saturating float->int, matching the CLIF fcvt_to_*_sat choice. */
static int8_t subscript_f2i8(double v) {
    if (v != v) return 0;
    if (v <= -128.0) return (int8_t)-128;
    if (v >= 127.0) return (int8_t)127;
    return (int8_t)v;
}
static uint8_t subscript_f2u8(double v) {
    if (v != v || v <= 0.0) return 0;
    if (v >= 255.0) return (uint8_t)255;
    return (uint8_t)v;
}
static int16_t subscript_f2i16(double v) {
    if (v != v) return 0;
    if (v <= -32768.0) return (int16_t)-32768;
    if (v >= 32767.0) return (int16_t)32767;
    return (int16_t)v;
}
static uint16_t subscript_f2u16(double v) {
    if (v != v || v <= 0.0) return 0;
    if (v >= 65535.0) return (uint16_t)65535;
    return (uint16_t)v;
}
static int32_t subscript_f2i32(double v) {
    if (v != v) return 0;
    if (v <= -2147483648.0) return (int32_t)(-2147483647 - 1);
    if (v >= 2147483647.0) return 2147483647;
    return (int32_t)v;
}
static uint32_t subscript_f2u32(double v) {
    if (v != v) return 0;
    if (v <= 0.0) return 0;
    if (v >= 4294967295.0) return 4294967295u;
    return (uint32_t)v;
}
static int64_t subscript_f2i64(double v) {
    if (v != v) return 0;
    if (v <= -9223372036854775808.0) return (-9223372036854775807ll - 1);
    if (v >= 9223372036854775807.0) return 9223372036854775807ll;
    return (int64_t)v;
}
static uint64_t subscript_f2u64(double v) {
    if (v != v) return 0;
    if (v <= 0.0) return 0;
    if (v >= 18446744073709551615.0) return 18446744073709551615ull;
    return (uint64_t)v;
}

"#
);

/// Terminal state word of a coroutine frame (matches the CLIF lowering's
/// `GEN_DONE`).
const GEN_DONE: i64 = 0x7FFF_FFFF;

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    fn module_of(src: &str) -> hir::Module {
        check_program(&[SourceFile::new("t.ts", src)]).expect("clean check")
    }

    fn emit(src: &str) -> String {
        emit_c(&module_of(src)).expect("emit").source
    }

    #[test]
    fn shadow_frame_word_arithmetic_reports_overflow() {
        assert!(checked_shadow_words(MAX_AGGREGATE_BYTES / 8, 1).is_err());
        assert!(checked_shadow_words(u32::MAX, 1).is_err());
    }

    #[test]
    fn emits_the_host_entry_surface() {
        let c =
            emit("export function main(): void {\n  const x: f32 = 1.5;\n  print(`${x}`);\n}\n");
        assert!(c.contains("void subscript_init(subscript_rt_context* ctx)"));
        assert!(c.contains("void subscript_export_main(subscript_rt_context* ctx)"));
        assert!(c.contains("subscript_rt_fmt_f32"));
        assert!(c.contains("1.5f"));
        assert!(c.contains("if (*(const uint32_t*)ctx != 0u)"));
        assert!(!c.contains("subscript_rt_ctx_trap_kind(ctx)"));
    }

    #[test]
    fn i64_min_literal_uses_a_valid_c_spelling() {
        let c = emit(
            "export function main(): void {\n  const low: i64 = -9223372036854775808;\n  print(`${low}`);\n}\n",
        );
        assert!(c.contains("(-9223372036854775807ll - 1)"), "{c}");
    }

    #[test]
    fn emitted_c_has_no_mutable_static_storage_definitions() {
        assert_eq!(
            rtc::Context::globals_offset(),
            16,
            "ship C reads the fixed Context globals slot"
        );
        let c = emit(
            "type Label = \"worker\" | \"lambda\";\n\
             let moduleStateProbe: i32 = 7;\n\
             class Message {\n\
             \x20 value: i32 = 0;\n\
             }\n\
             function echo(inbox: Inbox<Message>, outbox: Outbox<Message>): void {\n\
             \x20 const message: Message | null = inbox.wait();\n\
             \x20 if (message !== null) {\n\
             \x20   outbox.post(message);\n\
             \x20 }\n\
             }\n\
             function recurse(value: i32): i32 {\n\
             \x20 const captured: i32 = value;\n\
             \x20 const read: () => i32 = (): i32 => captured;\n\
             \x20 if (value > 0) {\n\
             \x20   recurse(value - 1);\n\
             \x20 }\n\
             \x20 return read();\n\
             }\n\
             export function main(): void {\n\
             \x20 const label: Label = \"worker\";\n\
             \x20 moduleStateProbe += recurse(2);\n\
             \x20 print(`${label}:${moduleStateProbe}`);\n\
             \x20 const worker: Worker<Message, Message> = Worker.spawn(echo);\n\
             \x20 worker.close();\n\
             \x20 worker.join();\n\
             }\n",
        );
        assert!(c.contains("typedef struct SubscriptModuleGlobals {"));
        assert!(c.contains("int32_t g_moduleStateProbe;"));
        assert!(c.contains("subscript_rt_globals_init(ctx, sizeof(SubscriptModuleGlobals)"));
        assert!(c.contains("subscript_globals(ctx)->g_moduleStateProbe"));
        assert!(c.contains("static const SubStringAliasMember subscript_string_alias_0[] = {"));
        assert!(c.contains("EnvL0 _t"));

        fn is_static_function(declaration: &str) -> bool {
            let Some(open) = declaration.find('(') else {
                return false;
            };
            let before = &declaration[..open];
            if before.contains('=') || declaration[open + 1..].trim_start().starts_with('*') {
                return false;
            }
            before
                .split_ascii_whitespace()
                .next_back()
                .is_some_and(|name| {
                    !name.is_empty()
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                })
        }

        // Every line-start `static` that is not a function declaration or
        // definition declares storage. Function-pointer objects and call
        // initializers remain storage under this classifier, so only the
        // explicit immutable-table whitelist may survive.
        let static_storage: Vec<&str> = c
            .lines()
            .filter(|line| {
                line.trim_start()
                    .strip_prefix("static ")
                    .is_some_and(|declaration| !is_static_function(declaration))
            })
            .collect();
        assert!(
            static_storage.iter().all(|line| line
                .trim_start()
                .starts_with("static const SubStringAliasMember subscript_string_alias_")),
            "emitted C contains mutable or non-whitelisted static storage: {static_storage:?}"
        );
    }

    #[test]
    fn emits_host_owned_exports_without_a_script_main() {
        let module = module_of(
            "export function init(): void {}\n\
             export function update(): void {}\n\
             export function shutdown(): void {}\n",
        );
        let c = emit_c_without_main(&module)
            .expect("host-owned exports emit")
            .source;
        assert!(c.contains("void subscript_init(subscript_rt_context* ctx)"));
        assert!(c.contains("void subscript_export_init(subscript_rt_context* ctx)"));
        assert!(c.contains("void subscript_export_update(subscript_rt_context* ctx)"));
        assert!(c.contains("void subscript_export_shutdown(subscript_rt_context* ctx)"));
        assert!(!c.contains("void subscript_export_main(subscript_rt_context* ctx) {"));
        assert!(emit_c(&module).is_err());
    }

    #[test]
    fn regex_literals_construct_once_in_the_module_initializer() {
        let c = emit(
            "export function main(): void {\n\
             \x20 for (let i: i32 = 0; i < 3; i += 1) {\n\
             \x20   const regex: RegExp = /x/g;\n\
             \x20   print(`${regex.test(\"x\")} ${regex.source}`);\n\
             \x20 }\n\
             }\n",
        );
        let call = "subscript_rt_regex_new(ctx, ";
        assert_eq!(c.matches(call).count(), 1);
        let main = c
            .find("void subscript_export_main(subscript_rt_context* ctx) {")
            .expect("main definition");
        let init = c
            .find("void subscript_init(subscript_rt_context* ctx) {")
            .expect("init definition");
        assert!(init < main);
        assert!(c[init..main].contains(call));
        assert!(!c[main..].contains(call));
    }

    #[test]
    fn emitted_trap_flag_load_matches_the_runtime_context_layout() {
        assert_eq!(
            rtc::Context::trap_flag_offset(),
            0,
            "the emitted `*(const uint32_t*)ctx` load requires the trap flag at offset zero"
        );
        let c = emit(
            "export function main(): void {\n  const xs: i32[] = [];\n  print(`${xs[0]}`);\n}\n",
        );
        assert!(
            c.contains("if (*(const uint32_t*)ctx != 0u)"),
            "the ship-tier trap check must use the offset-zero load tied to Context::trap_flag_offset()"
        );
    }

    #[test]
    fn value_class_is_a_by_value_struct() {
        let c = emit("@CStruct\nclass V { x: f32; y: f32;\n constructor(x: f32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void {\n  const v: V = new V(1.0, 2.0);\n  print(`${v.x}`);\n}\n");
        assert!(c.contains("typedef struct Sub_0_V"));
        assert!(c.contains("subscript_ctor0(void* ctx"));
    }

    #[test]
    fn reference_class_uses_the_runtime_allocator() {
        let c = emit("class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  print(`${c.x}`);\n  Context.free(c);\n}\n");
        assert!(c.contains("subscript_rt_alloc"));
        assert!(c.contains("subscript_rt_delete"));
        assert!(c.contains("subscript_ctor0(ctx,"));
        assert!(!c.contains("subscript_new0(void* ctx"));
    }

    #[test]
    fn zero_field_opaque_handle_emits_a_valid_c_struct() {
        // A branded ambient interface is an opaque handle with no fields.
        // An empty C struct is rejected by MSVC C mode (C2016), so the
        // typedef must carry a placeholder member and stay a valid C11
        // struct. The member is never read or instantiated by value.
        let files = [
            SourceFile::ambient(
                "widget.generated.d.ts",
                "// @subscript-c-header include=\"widget.h\"\n\
                 interface Widget {\n\
                 \x20 readonly __sub_handle_Widget: never;\n\
                 }\n\
                 declare function widgetCreate(): Widget;\n\
                 declare function widgetDestroy(w: Widget): void;\n",
            ),
            SourceFile::new(
                "prog.ts",
                "export function main(): void {\n\
                 \x20 const w: Widget = widgetCreate();\n\
                 \x20 widgetDestroy(w);\n\
                 }\n",
            ),
        ];
        let module = check_program(&files).expect("clean check");
        let c = emit_c(&module).expect("emit").source;
        assert!(
            c.contains("char subscript_opaque;"),
            "zero-field opaque handle must emit a non-empty C11 struct:\n{c}"
        );
    }

    #[test]
    fn every_generated_class_type_reference_has_a_typedef_definition() {
        let files = [
            SourceFile::ambient(
                "fixture.generated.d.ts",
                "// @subscript-c-header include=\"fixture.h\"\n\
                 declare class BoundaryRecord {\n\
                 \x20 label: string;\n\
                 \x20 constructor(label: string);\n\
                 }\n\
                 declare function inspectBoundary(record: BoundaryRecord | null): u64;\n",
            ),
            SourceFile::new(
                "prog.ts",
                "export function main(): void {\n\
                 \x20 print(`${inspectBoundary(null)}`);\n\
                 }\n",
            ),
        ];
        let module = check_program(&files).expect("clean check");
        let c = emit_c(&module).expect("emit").source;
        let generated_names: HashSet<&str> = c
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .filter(|token| {
                let Some(rest) = token.strip_prefix("Sub_") else {
                    return false;
                };
                let Some((class_id, name)) = rest.split_once('_') else {
                    return false;
                };
                !class_id.is_empty()
                    && class_id.bytes().all(|byte| byte.is_ascii_digit())
                    && !name.is_empty()
            })
            .collect();
        let defined_names: HashSet<&str> = c
            .lines()
            .filter_map(|line| line.trim().strip_prefix("typedef struct "))
            .filter_map(|rest| rest.split_ascii_whitespace().next())
            .filter(|name| generated_names.contains(name))
            .collect();
        let missing: Vec<&str> = generated_names
            .difference(&defined_names)
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "generated class type references lack typedef definitions: {missing:?}\n{c}"
        );
    }

    #[test]
    fn fixed_array_proven_index_is_unchecked() {
        let c = emit("export function main(): void {\n  const xs: FixedArray<i32, 4> = [10, 20, 30, 40];\n  let sum: i32 = 0;\n  for (let i: i32 = 0; i < 4; i += 1) {\n    sum += xs[i];\n  }\n  print(`${sum}`);\n}\n");
        assert!(
            !c.contains("subscript_rt_trap(ctx, SS_TRAP_OOB"),
            "proven index must not be checked"
        );
        assert!(c.contains("(xs).a[i]"));
    }

    #[test]
    fn dynamic_array_index_is_checked() {
        let c = emit("export function main(): void {\n  const xs: i32[] = [];\n  xs.push(7);\n  print(`${xs[0]}`);\n}\n");
        assert!(c.contains("SsArrayHeader*"));
        assert!(c.contains("subscript_rt_array_ptr(ctx,"));
        assert!(c.contains("subscript_rt_array_push"));
    }

    #[test]
    fn string_methods_call_the_opaque_runtime_symbols() {
        // stdlib.md §8: one opaque symbol per method, receiver first,
        // pos_id only on the fault-capable entries, boolean results
        // narrowed from the runtime's int32_t.
        let c = emit(
            "export function main(): void {\n  const s: string = \"ab\";\n  print(`${s.indexOf(\"b\")}`);\n  print(`${s.includes(\"b\", 1)}`);\n  print(s.padStart(4));\n  print(s.trim());\n  print(`${s.split(\"a\").length}`);\n}\n",
        );
        // The checker-normalized defaults are visible in the emitted
        // call: indexOf's `from` 0 and padStart's " " pad literal.
        assert!(c.contains("subscript_rt_str_index_of(ctx, "), "{c}");
        assert!(c.contains(", 0)"), "normalized indexOf from: {c}");
        assert!(c.contains("(subscript_rt_str_includes(ctx, "), "{c}");
        assert!(c.contains(" != 0)"), "boolean narrowing: {c}");
        assert!(c.contains("subscript_rt_str_pad_start(ctx, "), "{c}");
        assert!(c.contains("subscript_rt_str_trim(ctx, "), "{c}");
        assert!(c.contains("subscript_rt_str_split(ctx, "), "{c}");
        // The pure predicates carry no pos_id (no `u)` suffix scan
        // needed: the signature in the preamble is the contract).
        for f in hir::StrFn::ALL {
            assert!(
                PREAMBLE.contains(&format!("{}(void* ctx", f.symbol())),
                "preamble lacks the {} declaration",
                f.symbol()
            );
        }
    }

    #[test]
    fn math_calls_use_the_opaque_runtime_symbol_never_libm() {
        // stdlib.md §0.2: a bare libm call would be constant-folded by
        // clang at -O2 — the emitted call must be the subscript_rt symbol.
        let c = emit("export function main(): void {\n  print(`${Math.floor(1.5)}`);\n  print(`${Math.pow(2.0, 10.0)}`);\n  print(`${Math.random()}`);\n  print(`${Math.clz32(0)}`);\n  print(`${Math.imul(2147483647, 2)}`);\n  print(`${Math.fround(1.1)}`);\n}\n");
        assert!(c.contains("subscript_rt_math_floor(ctx, 1.5)"));
        assert!(c.contains("subscript_rt_math_pow(ctx, 2.0, 10.0)"));
        assert!(c.contains("subscript_rt_math_random(ctx)"));
        assert!(c.contains("subscript_rt_math_clz32(ctx, 0u)"));
        assert!(c.contains("subscript_rt_math_imul(ctx, 2147483647, 2)"));
        assert!(c.contains("subscript_rt_math_fround(ctx, 1.1)"));
        assert!(!c.contains("__builtin_clz"));
        // Token-boundary scan: a bare `<name>(` whose preceding character
        // is not part of an identifier is a libm call regardless of the
        // surrounding punctuation (`=floor(`, `(pow(`, line-start, ...);
        // `subscript_rt_math_<name>(` never matches because `_` precedes the name.
        fn has_bare_call(c: &str, name: &str) -> bool {
            let needle = format!("{name}(");
            let mut from = 0;
            while let Some(pos) = c[from..].find(&needle) {
                let at = from + pos;
                let boundary = match c[..at].chars().next_back() {
                    None => true,
                    Some(prev) => !(prev.is_ascii_alphanumeric() || prev == '_'),
                };
                if boundary {
                    return true;
                }
                from = at + 1;
            }
            false
        }
        assert!(!has_bare_call(&c, "floor"), "bare libm floor call emitted");
        assert!(!has_bare_call(&c, "pow"), "bare libm pow call emitted");
        assert!(!has_bare_call(&c, "random"), "bare random call emitted");
    }

    #[test]
    fn number_calls_use_the_opaque_runtime_symbols() {
        let c = emit(
            "export function main(): void {\n\
               const parsed: f64 = parseInt(\"ff\", 16);\n\
               print(`${Number.isFinite(parsed)}`);\n\
               print(parseFloat(\"1.5x\").toFixed(2));\n\
               print(parsed.toString(16));\n\
               print(parsed.toExponential());\n\
               print(parsed.toPrecision(2));\n\
             }\n",
        );
        assert!(c.contains("subscript_rt_num_parse_int(ctx, "), "{c}");
        assert!(c.contains("(subscript_rt_num_is_finite(ctx, "), "{c}");
        assert!(c.contains("subscript_rt_num_parse_float(ctx, "), "{c}");
        assert!(c.contains("subscript_rt_num_to_fixed(ctx, "), "{c}");
        assert!(c.contains("subscript_rt_num_to_string_f64(ctx, "), "{c}");
        assert!(c.contains("subscript_rt_num_to_exponential(ctx, "), "{c}");
        assert!(c.contains("subscript_rt_num_to_precision(ctx, "), "{c}");
        for f in hir::NumFn::ALL {
            assert!(
                PREAMBLE.contains(&format!("{}(void* ctx", f.symbol())),
                "preamble lacks the {} declaration",
                f.symbol()
            );
        }
    }

    #[test]
    fn f16_conversion_uses_opaque_runtime_symbols_never_half_operations() {
        let c = emit(
            "export function main(): void {\n  const h: f16 = (1.0006 as f64) as f16;\n  print(`${h as f32}`);\n}\n",
        );
        assert!(
            c.contains("uint16_t h = subscript_rt_f16_from_f64((double)"),
            "{c}"
        );
        assert!(c.contains("subscript_rt_f16_to_f64("), "{c}");

        // The explanatory preamble comment names the forbidden C types;
        // no declaration or executable line may do so.
        let non_comment_lines: String = c
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !line.starts_with("/*") && !line.starts_with('*') && !line.starts_with("*/")
            })
            .collect();
        assert!(!non_comment_lines.contains("_Float16"), "{c}");
        assert!(!non_comment_lines.contains("__fp16"), "{c}");
    }

    #[test]
    fn shifts_emit_explicit_operand_width_masks() {
        let c = emit(
            "export function main(): void {\n\
               const narrow: u8 = 255;\n\
               const narrowAmount: u8 = 9;\n\
               const wide: i64 = -2;\n\
               const wideAmount: i64 = 65;\n\
               print(`${narrow << narrowAmount} ${wide << wideAmount} ${wide >> wideAmount}`);\n\
             }\n",
        );
        assert!(
            c.contains("((uint8_t)(narrow)) << ((narrowAmount) & 7u)"),
            "{c}"
        );
        assert!(
            c.contains("((uint64_t)(wide)) << ((wideAmount) & 63u)"),
            "{c}"
        );
        assert!(
            c.contains("((int64_t)(wide)) >> ((wideAmount) & 63u)"),
            "{c}"
        );
    }

    #[test]
    fn math_constants_fold_to_literals_not_symbols() {
        // stdlib.md §1: `Math.<CONST>` never reaches codegen as a
        // member read; the emitted C carries the f64 literal.
        let c = emit("export function main(): void {\n  print(`${Math.PI}`);\n  print(`${Math.SQRT1_2}`);\n}\n");
        assert!(c.contains("3.141592653589793"));
        assert!(c.contains("0.7071067811865476"));
        assert!(!c.contains("subscript_rt_math_PI"));
        // No math runtime call is emitted for a constant read (the
        // preamble's extern declarations spell `(void* ctx`, a call
        // spells `(ctx`).
        assert!(!c.contains("subscript_rt_math_pi"));
        for line in c.lines() {
            if line.contains("subscript_rt_math_") {
                assert!(
                    line.starts_with("extern "),
                    "unexpected math reference: {line}"
                );
            }
        }
    }
}
