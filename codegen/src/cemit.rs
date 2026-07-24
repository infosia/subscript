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
//! same `sub_rt_*` C-ABI entry point the CLIF lowering calls, so array
//! growth, string content, Q14 shortest-round-trip formatting, and trap
//! reporting are byte-for-byte identical to the dev-JIT tier by
//! construction rather than by replication. (The P4.2 spike was
//! self-contained purely for measurement isolation.) The emitted unit
//! exports the same host-entry surface the AOT object does — `ss_init`
//! and `ss_export_<name>` taking the Context — so it is a drop-in
//! subject for the standing gate and for the device-triple link, linked
//! with the same [`crate::AOT_ENTRY_C`] host entry.
//!
//! # Semantic faithfulness
//!
//! The emitter mirrors the CLIF lowering's semantics, not a hand
//! optimization; where the emitted C and the CLIF path could differ the
//! CLIF path (and the runtime) is the reference:
//!
//! - **C2 value-class copy semantics.** A `@value class` is a C
//!   `struct`, passed and returned by value and copied on assignment —
//!   C's own struct-value semantics reproduce copy-on-assign/pass/return
//!   without any explicit copy. `FixedArray<T, N>` is a `struct { T a[N];
//!   }` wrapper so it, too, has value semantics; its C-ABI layout is
//!   identical to the bare array (design invariant 1).
//! - **Reference classes** are Context allocations (`sub_rt_alloc`);
//!   their handle is the payload pointer, and fields are read/written
//!   through a `struct` view of the payload (the same C-ABI layout the
//!   runtime allocates). `unsafeDelete` is `sub_rt_delete`, `collect()`
//!   is `sub_rt_collect`.
//! - **Checked growable `T[]`** is the runtime's array: `sub_rt_array_*`
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
//! - **Q14 formatting** is the runtime's (`sub_rt_fmt_*`), so an f32
//!   checksum prints the same bytes both tiers.
//! - **Trap model.** Emitted checks call `sub_rt_trap`; a trap sets the
//!   Context flag and is reported by the host entry without aborting the
//!   host, matching the runtime.
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
//! frames so `collect()` can see live handles. The emitted C does not
//! replicate that discipline: no run-set entry observably depends on it
//! (the only `collect()` in the corpus, a16, collects an allocation that
//! is already dead, and interned string literals are rooted by the
//! runtime itself), and the standing gate — byte-identity across all 24
//! entries on both tiers — is the oracle. Should a future entry make
//! rooting observable, this is where it is added.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use subscript_compiler::hir;
use subscript_compiler::types::{ClassId, FuncType, Type};
use subscript_compiler::Pos;
use subscript_runtime::context as rtc;

use crate::layout::{is_managed, managed_words, Layouts};

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
}

/// Emits a C translation unit for a checked HIR module (§11).
///
/// The unit exports `ss_init(void* ctx)` and `ss_export_<name>(void*
/// ctx)` for each exported zero-argument `void` function, imports the
/// runtime's `sub_rt_*` entry points, and is linked with the runtime
/// static library and [`crate::AOT_ENTRY_C`].
///
/// # Errors
///
/// Returns an error string when the module uses an HIR construct outside
/// the run set's scope, or has no exported `main(): void`.
pub fn emit_c(module: &hir::Module) -> Result<CProgram, String> {
    Emitter::new(module)?.emit()
}

// ----- interval analysis (§10.1), carried from the P4.2 spike -----

/// An inclusive integer interval `[lo, hi]`, computed in `i64` with the
/// same widen-in-`i128`-then-narrow discipline as the CLIF lowering's
/// interval lattice, so it never wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Interval {
    lo: i64,
    hi: i64,
}

impl Interval {
    fn point(v: i64) -> Interval {
        Interval { lo: v, hi: v }
    }

    fn fit(lo: i128, hi: i128) -> Option<Interval> {
        let lo = i64::try_from(lo).ok()?;
        let hi = i64::try_from(hi).ok()?;
        if lo > hi {
            return None;
        }
        Some(Interval { lo, hi })
    }

    fn add(self, o: Interval) -> Option<Interval> {
        Interval::fit(
            i128::from(self.lo) + i128::from(o.lo),
            i128::from(self.hi) + i128::from(o.hi),
        )
    }

    fn sub(self, o: Interval) -> Option<Interval> {
        Interval::fit(
            i128::from(self.lo) - i128::from(o.hi),
            i128::from(self.hi) - i128::from(o.lo),
        )
    }

    fn mul(self, o: Interval) -> Option<Interval> {
        let corners = [
            i128::from(self.lo) * i128::from(o.lo),
            i128::from(self.lo) * i128::from(o.hi),
            i128::from(self.hi) * i128::from(o.lo),
            i128::from(self.hi) * i128::from(o.hi),
        ];
        let lo = corners.iter().copied().min()?;
        let hi = corners.iter().copied().max()?;
        Interval::fit(lo, hi)
    }
}

/// Inclusive representable range of an integer type (mirrors the CLIF
/// lowering; `u64`'s upper bound does not fit `i64`, so it is unproven).
fn int_type_range(ty: &Type) -> Option<Interval> {
    Some(match ty {
        Type::I32 => Interval {
            lo: i64::from(i32::MIN),
            hi: i64::from(i32::MAX),
        },
        Type::U32 => Interval {
            lo: 0,
            hi: i64::from(u32::MAX),
        },
        Type::I64 => Interval {
            lo: i64::MIN,
            hi: i64::MAX,
        },
        _ => return None,
    })
}

/// True when any statement assigns to a local named `name` (mirrors the
/// CLIF lowering's induction-counter reassignment guard).
fn stmts_assign_to(stmts: &[hir::Stmt], name: &str) -> bool {
    stmts.iter().any(|s| stmt_assigns_to(s, name))
}

fn stmt_assigns_to(s: &hir::Stmt, name: &str) -> bool {
    match s {
        hir::Stmt::Let { init, .. } => expr_assigns_to(init, name),
        hir::Stmt::Expr(e) => expr_assigns_to(e, name),
        hir::Stmt::Return { value, .. } => {
            value.as_ref().is_some_and(|e| expr_assigns_to(e, name))
        }
        hir::Stmt::If { cond, then, els, .. } => {
            expr_assigns_to(cond, name)
                || stmts_assign_to(then, name)
                || els.as_ref().is_some_and(|e| stmts_assign_to(e, name))
        }
        hir::Stmt::While { cond, body, .. } => {
            expr_assigns_to(cond, name) || stmts_assign_to(body, name)
        }
        hir::Stmt::For {
            init, cond, step, body, ..
        } => {
            init.as_deref().is_some_and(|i| stmt_assigns_to(i, name))
                || cond.as_ref().is_some_and(|e| expr_assigns_to(e, name))
                || step.as_ref().is_some_and(|e| expr_assigns_to(e, name))
                || stmts_assign_to(body, name)
        }
        hir::Stmt::Switch { disc, cases, .. } => {
            expr_assigns_to(disc, name)
                || cases.iter().any(|c| {
                    c.test.as_ref().is_some_and(|e| expr_assigns_to(e, name))
                        || stmts_assign_to(&c.body, name)
                })
        }
        hir::Stmt::Block(b) => stmts_assign_to(b, name),
        hir::Stmt::Break(_) | hir::Stmt::Continue(_) => false,
        // Conservative: a future variant is treated as possibly
        // assigning, which only declines a proof (soundness over
        // optimization), as in the CLIF lowering.
        _ => true,
    }
}

fn expr_assigns_to(e: &hir::Expr, name: &str) -> bool {
    use hir::ExprKind as K;
    match &e.kind {
        K::Assign { target, value, .. } => {
            let hits = matches!(&target.kind, K::Local(n) if n == name);
            hits || expr_assigns_to(target, name) || expr_assigns_to(value, name)
        }
        K::Unary { operand, .. } => expr_assigns_to(operand, name),
        K::Binary { left, right, .. } => {
            expr_assigns_to(left, name) || expr_assigns_to(right, name)
        }
        K::Cast(inner) => expr_assigns_to(inner, name),
        K::Call { callee, args } => {
            let in_callee = match callee {
                hir::Callee::Value(v) => expr_assigns_to(v, name),
                hir::Callee::Method { recv, .. } => expr_assigns_to(recv, name),
                _ => false,
            };
            in_callee || args.iter().any(|a| expr_assigns_to(a, name))
        }
        K::New { args, .. } => args.iter().any(|a| expr_assigns_to(a, name)),
        K::Field { obj, .. } => expr_assigns_to(obj, name),
        K::Length(obj) => expr_assigns_to(obj, name),
        K::Index { obj, index } => {
            expr_assigns_to(obj, name) || expr_assigns_to(index, name)
        }
        K::ArrayLit(elems) => elems.iter().any(|x| expr_assigns_to(x, name)),
        K::Template(parts) => parts.iter().any(|p| match p {
            hir::TplPart::Expr(x) => expr_assigns_to(x, name),
            _ => false,
        }),
        K::Cond { cond, then, els } => {
            expr_assigns_to(cond, name)
                || expr_assigns_to(then, name)
                || expr_assigns_to(els, name)
        }
        K::Yield(arg) => arg.as_deref().is_some_and(|x| expr_assigns_to(x, name)),
        K::Lambda { .. } => false,
        K::Int(_)
        | K::Float(_)
        | K::Bool(_)
        | K::Str(_)
        | K::Null
        | K::This
        | K::Local(_)
        | K::Global(_)
        | K::FuncRef(_)
        | K::EnumMember { .. } => false,
        _ => true,
    }
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
    /// Yield sites seen so far (resume-label counter).
    yields: u32,
    /// Cursor into the frame's `let` fields, consumed in emission order.
    let_cursor: usize,
    /// Frame field name for each `let`, in emission order.
    let_fields: Vec<String>,
    /// C type of the yielded value.
    yield_ct: String,
}

struct Emitter<'m> {
    module: &'m hir::Module,
    /// C-ABI layouts, shared with the CLIF path: used for the exact same
    /// managed-word counts so the shadow frame the collector scans has
    /// the identical shape (M1).
    layouts: Layouts,
    /// Proven interval of currently-visible loop counters (§10.1).
    ranges: HashMap<String, Interval>,
    /// `this` context of the current function.
    this: ThisCtx,
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
    /// live handle held in one survives `collect()`).
    managed_scope: Vec<(String, String)>,
    /// Next managed-`let` shadow slot to assign in emission order.
    shadow_cursor: u32,
    /// True when the current function pushed a shadow frame (so its exits
    /// pop it).
    has_shadow: bool,
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
}

impl<'m> Emitter<'m> {
    fn new(module: &'m hir::Module) -> Result<Emitter<'m>, String> {
        Ok(Emitter {
            module,
            layouts: Layouts::build(module)?,
            ranges: HashMap::new(),
            this: ThisCtx::None,
            gen: None,
            gen_locals: Vec::new(),
            local_types: Vec::new(),
            managed_scope: Vec::new(),
            shadow_cursor: 0,
            has_shadow: false,
            positions: Vec::new(),
            tmp: 0,
            label: 0,
            lambda: 0,
            protos: String::new(),
            helpers: String::new(),
            wrappers: HashSet::new(),
            emitted_types: HashSet::new(),
            loops: Vec::new(),
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

    /// The C header descriptor struct name for a `(pointer, count)` array
    /// pair over `elem`, plus the element-pointer cast the compound literal
    /// needs. Scalar elements use the header's `SubSlice*` / `SubBufferView`
    /// const descriptors (`const void*` → `const T*` is implicit, no cast).
    /// A value-class element uses its mutable out-array descriptor
    /// (§14.3/§14.5): the element pointer is non-const, so the const-
    /// qualified `sub_rt_array_data` result is cast to the element pointer
    /// type (using the raw C header struct name, layout-identical to the
    /// language value class — invariant 1). This path is coupled to the
    /// synthetic header exactly like the scalar descriptor names.
    fn interop_array_pair_desc(&self, elem: &Type) -> Result<(String, String), String> {
        match elem {
            Type::U32 => Ok(("SubBufferView".to_string(), String::new())),
            Type::F32 => Ok(("SubSliceF32".to_string(), String::new())),
            Type::I32 => Ok(("SubSliceI32".to_string(), String::new())),
            Type::F64 => Ok(("SubSliceF64".to_string(), String::new())),
            Type::I64 => Ok(("SubSliceI64".to_string(), String::new())),
            Type::Class(id) if self.is_value_class(*id)? => {
                let name = self.class(*id)?.name.clone();
                let desc = match name.as_str() {
                    "SubWaitEntry" => "SubWaitList",
                    other => {
                        return Err(format!(
                            "no interop array-pair descriptor for value-class element {other}"
                        ))
                    }
                };
                Ok((desc.to_string(), format!("({name}*)")))
            }
            other => Err(format!(
                "no interop array-pair descriptor for element type {other:?}"
            )),
        }
    }

    /// C type for a value of `ty` (as a variable, parameter, field, or
    /// return). Aggregates (value classes, `FixedArray`, `IterResult`,
    /// function values) get their own named struct types.
    fn ctype(&self, ty: &Type) -> Result<String, String> {
        Ok(match ty {
            Type::I32 => "int32_t".to_string(),
            Type::U32 => "uint32_t".to_string(),
            Type::I64 => "int64_t".to_string(),
            Type::U64 => "uint64_t".to_string(),
            Type::F32 => "float".to_string(),
            Type::F64 => "double".to_string(),
            Type::Bool => "int32_t".to_string(),
            Type::Enum(_) => "int32_t".to_string(),
            Type::Void => "void".to_string(),
            Type::Str
            | Type::Object
            | Type::Array(_)
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
            Type::I32 => "i32".to_string(),
            Type::U32 => "u32".to_string(),
            Type::I64 => "i64".to_string(),
            Type::U64 => "u64".to_string(),
            Type::F32 => "f32".to_string(),
            Type::F64 => "f64".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Enum(id) => format!("enum{}", id.0),
            Type::Str | Type::Object | Type::Array(_) | Type::Generator(_) | Type::Nullable(_)
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

    fn emit(&mut self) -> Result<CProgram, String> {
        // Validate the entry point exists (mirrors lower_module_with).
        let has_main = self.module.functions.iter().any(|f| {
            f.name == "main"
                && f.exported
                && !f.is_generator
                && f.params.is_empty()
                && f.ret == Type::Void
        });
        if !has_main {
            return Err("no exported `main(): void` entry point".to_string());
        }

        // Aggregate type definitions, in dependency order.
        let mut typedefs = String::new();
        self.emit_type_definitions(&mut typedefs)?;

        // Globals.
        let mut globals = String::new();
        for g in &self.module.globals {
            let _ = writeln!(globals, "static {} g_{};", self.ctype(&g.ty)?, sanitize(&g.name));
        }

        // Bodies (which append prototypes and helper definitions as they
        // discover lambdas and function-reference wrappers).
        let mut bodies = String::new();

        // Prototypes for every constructor, method, free function, and
        // reference-class `new` wrapper.
        for (ci, c) in self.module.classes.iter().enumerate() {
            if c.ctor.is_some() {
                let proto = self.ctor_signature(ci, c)?;
                let _ = writeln!(self.protos, "{proto};");
                if !c.is_value {
                    let np = self.new_wrapper_signature(ci, c)?;
                    let _ = writeln!(self.protos, "{np};");
                }
            }
            for m in &c.methods {
                let proto = self.method_signature(ci, m)?;
                let _ = writeln!(self.protos, "{proto};");
            }
        }
        for f in &self.module.functions {
            if f.is_generator {
                let cp = self.gen_creator_signature(f)?;
                let rp = self.gen_resume_signature(f)?;
                let _ = writeln!(self.protos, "{cp};");
                let _ = writeln!(self.protos, "{rp};");
            } else {
                let proto = self.fn_signature(f)?;
                let _ = writeln!(self.protos, "{proto};");
            }
        }

        // Definitions.
        for (ci, c) in self.module.classes.iter().enumerate() {
            if c.ctor.is_some() {
                self.emit_constructor(&mut bodies, ci, c)?;
                if !c.is_value {
                    self.emit_new_wrapper(&mut bodies, ci, c)?;
                }
            }
            for m in &c.methods {
                self.emit_method(&mut bodies, ci, m)?;
            }
        }
        for f in &self.module.functions {
            if f.is_generator {
                self.emit_generator(&mut bodies, f)?;
            } else {
                self.emit_function(&mut bodies, f)?;
            }
        }

        // Module initializer and exported entry surface.
        self.emit_init(&mut bodies)?;
        self.emit_exports(&mut bodies)?;

        let mut out = String::new();
        out.push_str(PREAMBLE);
        // Foreign C-header binding (P5.2b): the ship tier includes the real
        // synthetic header so boundary struct layouts and foreign
        // prototypes come from one source (compiler.md §12.4; the link
        // provides `interop.c`). The generic callback trampoline is
        // declared here because its type mentions `SubStringView`.
        if !self.module.foreign_fns.is_empty() {
            out.push_str("#include \"interop.h\"\n");
            out.push_str("extern void sub_rt_cb_trampoline(SubStringView message, void* userdata1, void* userdata2);\n\n");
        }
        out.push_str(&typedefs);
        out.push('\n');
        out.push_str(&globals);
        out.push('\n');
        out.push_str(&self.protos);
        out.push('\n');
        out.push_str(&self.helpers);
        out.push_str(&bodies);

        Ok(CProgram {
            source: out,
            positions: std::mem::take(&mut self.positions),
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
                let _ = writeln!(out, "typedef struct {{ {} a[{n}]; }} {name};", self.ctype(elem)?);
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
                for field in &self.class(*id)?.fields {
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
        format!("ss_fn_{}", sanitize(&f.name))
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
            Ok(format!("static {cname} ss_ctor{ci}(void* ctx{sep}{params})"))
        } else {
            Ok(format!("static void ss_ctor{ci}(void* ctx, void* _this{}{params})",
                if params.is_empty() { "" } else { ", " }))
        }
    }

    fn new_wrapper_signature(&self, ci: usize, c: &hir::ClassDef) -> Result<String, String> {
        let ctor = c.ctor.as_ref().ok_or("constructor missing")?;
        let params = self.param_list(&ctor.params)?;
        let sep = if params.is_empty() { "" } else { ", " };
        Ok(format!("static void* ss_new{ci}(void* ctx, uint32_t _pos{sep}{params})"))
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
            "static {ret} ss_m{ci}_{}(void* ctx, {recv} _this{sep}{params})",
            sanitize(&m.name)
        ))
    }

    fn gen_creator_signature(&self, f: &hir::Function) -> Result<String, String> {
        let params = self.param_list(&f.params)?;
        if params.is_empty() {
            Ok(format!("static void* ss_fn_{}(void* ctx)", sanitize(&f.name)))
        } else {
            Ok(format!("static void* ss_fn_{}(void* ctx, {params})", sanitize(&f.name)))
        }
    }

    fn gen_resume_signature(&self, f: &hir::Function) -> Result<String, String> {
        Ok(format!(
            "static int32_t ss_resume_{}(void* ctx, void* _frame, void* _out)",
            sanitize(&f.name)
        ))
    }

    // ----- constructors -----

    fn emit_constructor(&mut self, out: &mut String, ci: usize, c: &hir::ClassDef) -> Result<(), String> {
        let ctor = c.ctor.as_ref().ok_or("constructor missing")?;
        let sig = self.ctor_signature(ci, c)?;
        let cname = self.class_name(ClassId(ci))?;
        let _ = writeln!(out, "{sig} {{");
        self.begin_fn(if c.is_value { ThisCtx::ValueLValue } else { ThisCtx::Reference });
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
                    let _ = writeln!(out, "    (({cname}*)_this)->{} = {v};", sanitize(&field.name));
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

    fn emit_new_wrapper(&mut self, out: &mut String, ci: usize, c: &hir::ClassDef) -> Result<(), String> {
        let ctor = c.ctor.as_ref().ok_or("constructor missing")?;
        let sig = self.new_wrapper_signature(ci, c)?;
        let cname = self.class_name(ClassId(ci))?;
        let args: Vec<String> = ctor.params.iter().map(|p| sanitize(&p.name)).collect();
        let sep = if args.is_empty() { "" } else { ", " };
        let _ = writeln!(out, "{sig} {{");
        let _ = writeln!(
            out,
            "    void* _this = sub_rt_alloc(ctx, sizeof({cname}), {}u, _pos);",
            ci
        );
        let _ = writeln!(out, "    if (_this == 0) return 0;");
        let _ = writeln!(out, "    ss_ctor{ci}(ctx, _this{sep}{});", args.join(", "));
        let _ = writeln!(out, "    return _this;");
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    fn emit_method(&mut self, out: &mut String, ci: usize, m: &hir::Function) -> Result<(), String> {
        let sig = self.method_signature(ci, m)?;
        let is_value = self.class(ClassId(ci))?.is_value;
        let _ = writeln!(out, "{sig} {{");
        self.begin_fn(if is_value { ThisCtx::ValuePtr } else { ThisCtx::Reference });
        self.emit_prologue(out, &m.params, &m.body, 1)?;
        self.emit_block(out, &m.body, 1)?;
        self.emit_exit(out, &m.ret, 1)?;
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    fn emit_function(&mut self, out: &mut String, f: &hir::Function) -> Result<(), String> {
        let sig = self.fn_signature(f)?;
        let _ = writeln!(out, "{sig} {{");
        self.begin_fn(ThisCtx::None);
        self.emit_prologue(out, &f.params, &f.body, 1)?;
        self.emit_block(out, &f.body, 1)?;
        self.emit_exit(out, &f.ret, 1)?;
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    /// Resets per-function emitter state.
    fn begin_fn(&mut self, this: ThisCtx) {
        self.ranges.clear();
        self.this = this;
        self.gen = None;
        self.gen_locals.clear();
        self.local_types.clear();
        self.managed_scope.clear();
        self.shadow_cursor = 0;
        self.has_shadow = false;
    }

    /// Emits the shadow-frame prologue and records parameter types (M1,
    /// C2). Every parameter or local that is a Context allocation, or an
    /// aggregate whose interior holds Context handles (a `FixedArray` of
    /// references/strings, an `IterResult` of a managed type), lives in a
    /// per-call shadow frame the collector conservatively word-scans, so
    /// a live handle held in one survives `collect()`; the frame is
    /// pushed here and popped at every exit, exactly as the CLIF path's
    /// `shadow_push`/`shadow_pop` do (the P2 M1 fix, on the CLIF side).
    fn emit_prologue(&mut self, out: &mut String, params: &[hir::Param], body: &[hir::Stmt], depth: usize) -> Result<(), String> {
        for p in params {
            self.local_types.push((p.name.clone(), p.ty.clone()));
        }
        let n = self.shadow_words(params, body)?;
        if n == 0 {
            return Ok(());
        }
        let ind = indent(depth);
        let _ = writeln!(out, "{ind}void* _ssroots[{n}]; memset(_ssroots, 0, sizeof _ssroots);");
        let _ = writeln!(out, "{ind}sub_rt_shadow_push(ctx, _ssroots, {n}ull);");
        self.has_shadow = true;
        let mut slot = 0u32;
        for p in params {
            let w = managed_words(&self.layouts, &p.ty)?;
            if w == 0 {
                continue;
            }
            let access = self.root_slot_store(out, &p.ty, slot, &sanitize(&p.name), depth)?;
            self.managed_scope.push((p.name.clone(), access));
            slot += w;
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
    fn root_slot_store(&mut self, out: &mut String, ty: &Type, slot: u32, value: &str, depth: usize) -> Result<String, String> {
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
            let _ = writeln!(out, "{}sub_rt_shadow_pop(ctx);", indent(depth));
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
            n += managed_words(&self.layouts, &p.ty)?;
        }
        let mut lets: Vec<(&str, &Type)> = Vec::new();
        walk_lets(body, &mut lets);
        for (_, ty) in lets {
            n += managed_words(&self.layouts, ty)?;
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
            Type::I32 | Type::U32 | Type::I64 | Type::U64 | Type::Bool | Type::Enum(_) => {
                "0".to_string()
            }
            Type::Str | Type::Object | Type::Array(_) | Type::Generator(_) | Type::Nullable(_)
            | Type::Null => "0".to_string(),
            Type::Class(id) if !self.is_value_class(*id)? => "0".to_string(),
            _ => format!("({}){{0}}", self.ctype(ty)?),
        })
    }

    // ----- init and exports -----

    fn emit_init(&mut self, out: &mut String) -> Result<(), String> {
        let _ = writeln!(out, "void ss_init(void* ctx) {{");
        self.begin_fn(ThisCtx::None);
        let globals: Vec<hir::Global> = self.module.globals.to_vec();
        for g in &globals {
            let v = self.eval(&g.init, out, 1)?;
            let _ = writeln!(out, "    g_{} = {v};", sanitize(&g.name));
            // A managed global (or managed-interior aggregate global) is
            // a permanent collection root (M1): `managed_words` words,
            // as in the CLIF path's `root_add`.
            let words = managed_words(&self.layouts, &g.ty)?;
            if words > 0 {
                let _ = writeln!(out, "    sub_rt_root_add(ctx, &g_{}, {words}ull);", sanitize(&g.name));
            }
        }
        let _ = writeln!(out, "}}\n");
        Ok(())
    }

    fn emit_exports(&mut self, out: &mut String) -> Result<(), String> {
        for f in &self.module.functions {
            if f.exported && !f.is_generator && f.params.is_empty() && f.ret == Type::Void {
                let cn = Emitter::fn_c_name(f);
                let _ = writeln!(out, "void ss_export_{}(void* ctx) {{ {cn}(ctx); }}", sanitize(&f.name));
            }
        }
        Ok(())
    }

    // ----- statements -----

    fn emit_block(&mut self, out: &mut String, stmts: &[hir::Stmt], depth: usize) -> Result<(), String> {
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
            hir::Stmt::If { cond, then, els, .. } => {
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
            hir::Stmt::For { init, cond, step, body, .. } => {
                self.emit_for(out, init.as_deref(), cond.as_ref(), step.as_ref(), body, depth)
            }
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
            other => Err(format!("statement {other:?} is outside the run set's scope")),
        }
    }

    fn emit_let(&mut self, out: &mut String, name: &str, ty: &Type, init: &hir::Expr, depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        // Generator locals live in the frame, not as C variables.
        if self.gen.is_some() {
            let field = self.gen_next_let_field(name)?;
            let v = self.eval(init, out, depth)?;
            let _ = writeln!(out, "{ind}_f->{field} = {v};");
            return Ok(());
        }
        // A managed local — or an aggregate whose interior holds managed
        // handles — lives in the shadow frame so `collect()` sees its
        // handle(s) (M1); its storage is shadow slot(s), not a C var.
        if self.needs_rooting(ty)? {
            let w = managed_words(&self.layouts, ty)?;
            let slot = self.shadow_cursor;
            self.shadow_cursor += w;
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

    fn emit_return(&mut self, out: &mut String, value: Option<&hir::Expr>, depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        // Inside a generator resume body, `return` completes the
        // coroutine: terminal state, done = 1 (C8).
        if self.gen.is_some() {
            let _ = writeln!(out, "{ind}_f->_state = {GEN_DONE}; return 1;");
            return Ok(());
        }
        match value {
            None => {
                self.emit_shadow_pop(out, depth);
                let _ = writeln!(out, "{ind}return;");
            }
            Some(v) => {
                // The return value is computed before the frame is
                // popped; no collection runs between the pop and the
                // return, and the shadow array's memory outlives the pop
                // (it is unregistered, not freed), so reading it is safe.
                let text = self.eval(v, out, depth)?;
                self.emit_shadow_pop(out, depth);
                let _ = writeln!(out, "{ind}return {text};");
            }
        }
        Ok(())
    }

    fn emit_while(&mut self, out: &mut String, cond: &hir::Expr, body: &[hir::Stmt], depth: usize) -> Result<(), String> {
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
        // §10.1: publish the counter's proven interval for the body's
        // FixedArray bounds-check decisions, then restore on exit.
        let proof = self.induction_interval(init, cond, step, body);
        let saved = proof.as_ref().map(|(n, _)| (n.clone(), self.ranges.get(n).copied()));

        if let Some(i) = init {
            self.emit_stmt(out, i, depth + 1)?;
        }
        if let Some((name, iv)) = &proof {
            self.ranges.insert(name.clone(), *iv);
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

        if let Some((name, prev)) = saved {
            match prev {
                Some(iv) => {
                    self.ranges.insert(name, iv);
                }
                None => {
                    self.ranges.remove(&name);
                }
            }
        }
        Ok(())
    }

    fn emit_switch(&mut self, out: &mut String, disc: &hir::Expr, cases: &[hir::SwitchCase], depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        let ind1 = indent(depth + 1);
        let dv = self.eval(disc, out, depth)?;
        let dty = self.ctype(&disc.ty)?;
        let brk = self.fresh_label();
        let labels: Vec<String> = cases.iter().map(|_| self.fresh_label()).collect();
        let default_idx = cases.iter().position(|c| c.test.is_none());
        let _ = writeln!(out, "{ind}{{");
        let _ = writeln!(out, "{ind1}{dty} _disc = {dv};");
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

    fn emit_expr_stmt(&mut self, out: &mut String, e: &hir::Expr, depth: usize) -> Result<(), String> {
        use hir::ExprKind as K;
        let ind = indent(depth);
        match &e.kind {
            K::Assign { op, target, value } => self.emit_assign(out, *op, target, value, depth),
            K::Call { callee: hir::Callee::Ambient(a), args } => {
                self.emit_ambient(out, *a, args, &e.pos, depth)
            }
            K::Call { callee: hir::Callee::Method { recv, name }, args }
                if is_array_mutator(&recv.ty, name) =>
            {
                self.emit_array_mutator(out, recv, name, args, &e.pos, depth)
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

    fn emit_ambient(&mut self, out: &mut String, a: hir::AmbientFn, args: &[hir::Expr], pos: &Pos, depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        match a {
            hir::AmbientFn::Print => {
                let arg = args.first().ok_or("print arity")?;
                let h = self.eval(arg, out, depth)?;
                let _ = writeln!(out, "{ind}sub_rt_print(ctx, {h});");
            }
            hir::AmbientFn::Collect => {
                let _ = writeln!(out, "{ind}sub_rt_collect(ctx);");
            }
            hir::AmbientFn::UnsafeDelete => {
                let arg = args.first().ok_or("unsafeDelete arity")?;
                let p = self.eval(arg, out, depth)?;
                let pid = self.pos_id(pos);
                let _ = writeln!(out, "{ind}sub_rt_delete(ctx, {p}, {pid}u);");
            }
            _ => return Err("unknown ambient function".to_string()),
        }
        Ok(())
    }

    fn emit_array_mutator(&mut self, out: &mut String, recv: &hir::Expr, name: &str, args: &[hir::Expr], pos: &Pos, depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        let elem = match &recv.ty {
            Type::Array(e) => (**e).clone(),
            other => return Err(format!("array method on {other:?}")),
        };
        let h = self.eval(recv, out, depth)?;
        let ect = self.ctype(&elem)?;
        let pid = self.pos_id(pos);
        match name {
            "push" => {
                let arg = args.first().ok_or("push arity")?;
                let v = self.eval(arg, out, depth)?;
                let _ = writeln!(
                    out,
                    "{ind}{{ {ect} _e = {v}; sub_rt_array_push(ctx, {h}, &_e, {pid}u); }}"
                );
            }
            "pop" => {
                let _ = writeln!(
                    out,
                    "{ind}{{ {ect} _d; sub_rt_array_pop(ctx, {h}, &_d, {pid}u); }}"
                );
            }
            other => return Err(format!("array mutator `{other}`")),
        }
        Ok(())
    }

    /// Assignment as a statement, carrying C2 copy semantics and
    /// growth-safe dynamic-array element stores (N3).
    fn emit_assign(&mut self, out: &mut String, op: Option<hir::BinOp>, target: &hir::Expr, value: &hir::Expr, depth: usize) -> Result<(), String> {
        let ind = indent(depth);
        // Dynamic-array element store: resolve the (checked) address
        // after the RHS so growth cannot dangle it.
        if let hir::ExprKind::Index { obj, index } = &target.kind {
            if let Type::Array(elem) = &obj.ty {
                let ect = self.ctype(elem)?;
                let h = self.eval(obj, out, depth)?;
                let idx = self.eval(index, out, depth)?;
                let pid = self.pos_id(&target.pos);
                match op {
                    None => {
                        let v = self.eval(value, out, depth)?;
                        let _ = writeln!(
                            out,
                            "{ind}{{ {ect} _v = {v}; *({ect}*)ss_arr_at(ctx, {h}, {idx}, {pid}u) = _v; }}"
                        );
                    }
                    Some(bin) => {
                        let v = self.eval(value, out, depth)?;
                        let sym = binop_sym(bin)?;
                        let _ = writeln!(
                            out,
                            "{ind}{{ {ect} _v = {v}; {ect}* _p = ({ect}*)ss_arr_at(ctx, {h}, {idx}, {pid}u); *_p = *_p {sym} _v; }}"
                        );
                    }
                }
                return Ok(());
            }
        }
        let place = self.place(target, out, depth)?;
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
                    let v = self.eval(value, out, depth)?;
                    let pid = self.pos_id(&target.pos);
                    let _ = writeln!(out, "{ind}{place} = sub_rt_str_concat(ctx, {place}, {v}, {pid}u);");
                } else if target.ty.is_integer() && matches!(bin, hir::BinOp::Div | hir::BinOp::Rem) {
                    let v = self.eval(value, out, depth)?;
                    let helper = divrem_helper(&target.ty, bin == hir::BinOp::Div)?;
                    let pid = self.pos_id(&target.pos);
                    let _ = writeln!(out, "{ind}{place} = {helper}(ctx, {place}, {v}, {pid}u);");
                } else {
                    let sym = binop_sym(bin)?;
                    let v = self.eval(value, out, depth)?;
                    let _ = writeln!(out, "{ind}{place} = {place} {sym} {v};");
                }
            }
        }
        Ok(())
    }

    /// A C lvalue for an assignable place (never a dynamic-array
    /// element, which `emit_assign` handles directly).
    fn place(&mut self, e: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Local(name) => Ok(self.local_ref(name)),
            K::Global(name) => Ok(format!("g_{}", sanitize(name))),
            K::This => Ok(self.this.this_expr()?.to_string()),
            K::Field { obj, name } => {
                let (base, arrow) = self.field_base(obj, out, depth)?;
                Ok(format!("{base}{arrow}{}", sanitize(name)))
            }
            K::Index { obj, index } => match &obj.ty {
                Type::FixedArray(_, n) => {
                    let base = self.place(obj, out, depth)?;
                    let idx = self.eval(index, out, depth)?;
                    if self.index_in_bounds(index, *n) {
                        Ok(format!("{base}.a[{idx}]"))
                    } else {
                        let elem = match &obj.ty {
                            Type::FixedArray(el, _) => (**el).clone(),
                            _ => unreachable!(),
                        };
                        let ect = self.ctype(&elem)?;
                        let pid = self.pos_id(&e.pos);
                        Ok(format!("(*({ect}*)ss_fa_at(ctx, {base}.a, {n}, {idx}, sizeof({ect}), {pid}u))"))
                    }
                }
                other => Err(format!("assignment target index on {other:?}")),
            },
            other => Err(format!("assignment target {other:?}")),
        }
    }

    /// The base expression and member operator (`.`/`->`) for a field
    /// access on `obj`.
    fn field_base(&mut self, obj: &hir::Expr, out: &mut String, depth: usize) -> Result<(String, &'static str), String> {
        match &obj.ty {
            Type::Class(id) => {
                if self.is_value_class(*id)? {
                    Ok((self.place_or_eval(obj, out, depth)?, "."))
                } else {
                    let cname = self.class_name(*id)?;
                    let o = self.eval(obj, out, depth)?;
                    Ok((format!("(({cname}*)({o}))"), "->"))
                }
            }
            Type::IterResult(_) => Ok((self.place_or_eval(obj, out, depth)?, ".")),
            other => Err(format!("field access on {other:?}")),
        }
    }

    /// For an assignable value-class receiver, an lvalue; otherwise the
    /// evaluated (by-value) expression wrapped so `.field` is legal.
    fn place_or_eval(&mut self, obj: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        use hir::ExprKind as K;
        match &obj.kind {
            K::Local(_) | K::Global(_) | K::Field { .. } | K::Index { .. } | K::This => {
                self.place(obj, out, depth)
            }
            _ => {
                let v = self.eval(obj, out, depth)?;
                Ok(format!("({v})"))
            }
        }
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

    // ----- expressions -----

    /// Evaluates `e` to a C expression, emitting any preceding
    /// statements (temporaries, hoisted chains) into `out` at `depth`.
    fn eval(&mut self, e: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Int(v) => Ok(int_literal(*v, &e.ty)),
            K::Float(v) => Ok(float_literal(*v, &e.ty)),
            K::Bool(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
            K::Str(s) => self.string_literal(s.as_bytes(), &e.pos),
            K::Null => Ok("((void*)0)".to_string()),
            K::This => Ok(self.this.this_expr()?.to_string()),
            K::Local(name) => Ok(self.local_ref(name)),
            K::Global(name) => Ok(format!("g_{}", sanitize(name))),
            K::FuncRef(name) => self.func_ref_value(name),
            K::EnumMember { value, .. } => Ok(value.to_string()),
            K::Unary { op, operand } => {
                let v = self.eval(operand, out, depth)?;
                Ok(match op {
                    hir::UnOp::Neg => format!("(-({v}))"),
                    hir::UnOp::Not => format!("(!({v}))"),
                    hir::UnOp::BitNot => format!("(~({v}))"),
                    _ => return Err("unknown unary operator".to_string()),
                })
            }
            K::Binary { op, left, right } => self.eval_binary(*op, left, right, &e.pos, out, depth),
            K::Assign { op, target, value } => self.eval_assign_expr(*op, target, value, out, depth),
            K::Cast(inner) => {
                let v = self.eval(inner, out, depth)?;
                self.eval_cast(&v, &inner.ty, &e.ty)
            }
            K::Call { callee, args } => self.eval_call(callee, args, &e.ty, &e.pos, out, depth),
            K::New { class, args } => self.eval_new(*class, args, &e.pos, out, depth),
            K::Field { obj, name } => self.eval_field(obj, name, out, depth),
            K::Length(obj) => match &obj.ty {
                Type::Array(_) => {
                    let h = self.eval(obj, out, depth)?;
                    Ok(format!("sub_rt_array_len(ctx, {h})"))
                }
                Type::Str => {
                    let h = self.eval(obj, out, depth)?;
                    Ok(format!("sub_rt_str_len(ctx, {h})"))
                }
                Type::FixedArray(_, n) => Ok(n.to_string()),
                other => Err(format!("length of {other:?}")),
            },
            K::Index { obj, index } => self.eval_index(obj, index, &e.pos, out, depth),
            K::ArrayLit(elems) => self.eval_array_lit(&e.ty, elems, &e.pos, out, depth),
            K::Template(parts) => self.eval_template(parts, &e.pos, out, depth),
            K::Lambda { params, ret, body, captures } => {
                self.eval_lambda(params, ret, body, captures, out, depth)
            }
            K::Yield(arg) => self.eval_yield(arg.as_deref(), out, depth),
            K::Cond { cond, then, els } => self.eval_cond(cond, then, els, &e.ty, out, depth),
            other => Err(format!("expression {other:?} is outside the run set's scope")),
        }
    }

    fn eval_list(&mut self, elems: &[hir::Expr], out: &mut String, depth: usize) -> Result<String, String> {
        let mut parts = Vec::with_capacity(elems.len());
        for e in elems {
            parts.push(self.eval(e, out, depth)?);
        }
        Ok(parts.join(", "))
    }

    fn string_literal(&mut self, bytes: &[u8], pos: &Pos) -> Result<String, String> {
        let pid = self.pos_id(pos);
        Ok(format!(
            "sub_rt_str_lit(ctx, (const unsigned char*){}, {}ull, {pid}u)",
            c_string_literal(bytes),
            bytes.len()
        ))
    }

    fn eval_binary(&mut self, op: hir::BinOp, left: &hir::Expr, right: &hir::Expr, pos: &Pos, out: &mut String, depth: usize) -> Result<String, String> {
        use hir::BinOp as B;
        let operand_ty = if left.ty == Type::Null { right.ty.clone() } else { left.ty.clone() };

        if operand_ty == Type::Str {
            let l = self.eval(left, out, depth)?;
            let r = self.eval(right, out, depth)?;
            return match op {
                B::Add => {
                    let pid = self.pos_id(pos);
                    Ok(format!("sub_rt_str_concat(ctx, {l}, {r}, {pid}u)"))
                }
                B::Eq => Ok(format!("(sub_rt_str_eq(ctx, {l}, {r}) != 0)")),
                B::Ne => Ok(format!("(sub_rt_str_eq(ctx, {l}, {r}) == 0)")),
                _ => Err("string operator outside the run set's scope".to_string()),
            };
        }

        let l = self.eval(left, out, depth)?;
        let r = self.eval(right, out, depth)?;
        let float = operand_ty.is_float();
        match op {
            B::Div if !float => {
                let f = divrem_helper(&operand_ty, true)?;
                let pid = self.pos_id(pos);
                return Ok(format!("{f}(ctx, {l}, {r}, {pid}u)"));
            }
            B::Rem => {
                let f = divrem_helper(&operand_ty, false)?;
                let pid = self.pos_id(pos);
                return Ok(format!("{f}(ctx, {l}, {r}, {pid}u)"));
            }
            _ => {}
        }
        let sym = binop_sym(op)?;
        Ok(format!("({l} {sym} {r})"))
    }

    fn eval_assign_expr(&mut self, op: Option<hir::BinOp>, target: &hir::Expr, value: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        // Assignment used as an expression (loop steps, `i += 1`). Only
        // simple scalar places reach here; aggregate/array assigns are
        // statement-only.
        let place = self.place(target, out, depth)?;
        let v = self.eval(value, out, depth)?;
        match op {
            None => Ok(format!("({place} = {v})")),
            Some(bin) => {
                let sym = binop_sym(bin)?;
                Ok(format!("({place} = {place} {sym} {v})"))
            }
        }
    }

    fn eval_cast(&self, v: &str, from: &Type, to: &Type) -> Result<String, String> {
        // Reference narrowing (`object`/`object | null` -> class) is not
        // exercised by the run set; every other cast is a C cast, except
        // that enum sources behave as i32.
        let from = if matches!(from, Type::Enum(_)) { Type::I32 } else { from.clone() };
        if from == *to {
            return Ok(format!("({v})"));
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
    fn eval_field(&mut self, obj: &hir::Expr, name: &str, out: &mut String, depth: usize) -> Result<String, String> {
        let base = self.eval(obj, out, depth)?;
        match &obj.ty {
            Type::Class(id) if self.is_value_class(*id)? => {
                Ok(format!("({base}).{}", sanitize(name)))
            }
            Type::Class(id) => {
                let cname = self.class_name(*id)?;
                Ok(format!("(({cname}*)({base}))->{}", sanitize(name)))
            }
            Type::IterResult(_) => Ok(format!("({base}).{}", sanitize(name))),
            other => Err(format!("field access on {other:?}")),
        }
    }

    fn eval_index(&mut self, obj: &hir::Expr, index: &hir::Expr, pos: &Pos, out: &mut String, depth: usize) -> Result<String, String> {
        match &obj.ty {
            Type::FixedArray(elem, n) => {
                let base = self.eval(obj, out, depth)?;
                let idx = self.eval(index, out, depth)?;
                if self.index_in_bounds(index, *n) {
                    Ok(format!("({base}).a[{idx}]"))
                } else {
                    let ect = self.ctype(elem)?;
                    let pid = self.pos_id(pos);
                    Ok(format!("(*({ect}*)ss_fa_at(ctx, ({base}).a, {n}, {idx}, sizeof({ect}), {pid}u))"))
                }
            }
            Type::Array(elem) => {
                let ect = self.ctype(elem)?;
                let h = self.eval(obj, out, depth)?;
                let idx = self.eval(index, out, depth)?;
                let pid = self.pos_id(pos);
                Ok(format!("(*({ect}*)ss_arr_at(ctx, {h}, {idx}, {pid}u))"))
            }
            other => Err(format!("index on {other:?}")),
        }
    }


    fn eval_array_lit(&mut self, ty: &Type, elems: &[hir::Expr], pos: &Pos, out: &mut String, depth: usize) -> Result<String, String> {
        let ind = indent(depth);
        match ty {
            Type::FixedArray(_, _) => {
                let cty = self.ctype(ty)?;
                let vals = self.eval_list(elems, out, depth)?;
                Ok(format!("(({cty}){{ {{ {vals} }} }})"))
            }
            Type::Array(elem) => {
                let ect = self.ctype(elem)?;
                let pid = self.pos_id(pos);
                let h = self.fresh_tmp();
                let _ = writeln!(out, "{ind}void* {h} = sub_rt_array_new(ctx, sizeof({ect}), {pid}u);");
                for e in elems {
                    let v = self.eval(e, out, depth)?;
                    let epid = self.pos_id(&e.pos);
                    let _ = writeln!(
                        out,
                        "{ind}{{ {ect} _e = {v}; sub_rt_array_push(ctx, {h}, &_e, {epid}u); }}"
                    );
                }
                Ok(h)
            }
            other => Err(format!("array literal of {other:?}")),
        }
    }

    fn eval_template(&mut self, parts: &[hir::TplPart], pos: &Pos, out: &mut String, depth: usize) -> Result<String, String> {
        let ind = indent(depth);
        let acc = self.fresh_tmp();
        let _ = writeln!(out, "{ind}void* {acc} = 0;");
        for part in parts {
            let piece = match part {
                hir::TplPart::Text(t) => self.string_literal(t.as_bytes(), pos)?,
                hir::TplPart::Expr(e) => {
                    let v = self.eval(e, out, depth)?;
                    self.format_value(&v, &e.ty, &e.pos)?
                }
                other => return Err(format!("template part {other:?}")),
            };
            let pid = self.pos_id(pos);
            let _ = writeln!(
                out,
                "{ind}{acc} = ({acc} == 0) ? ({piece}) : sub_rt_str_concat(ctx, {acc}, {piece}, {pid}u);"
            );
        }
        // An empty template is the empty string.
        let empty = self.string_literal(b"", pos)?;
        Ok(format!("(({acc} == 0) ? {empty} : {acc})"))
    }

    fn format_value(&mut self, v: &str, ty: &Type, pos: &Pos) -> Result<String, String> {
        let pid = self.pos_id(pos);
        let f = match ty {
            Type::Str => return Ok(v.to_string()),
            Type::I32 | Type::Enum(_) => "sub_rt_fmt_i32",
            Type::U32 => "sub_rt_fmt_u32",
            Type::I64 => "sub_rt_fmt_i64",
            Type::U64 => "sub_rt_fmt_u64",
            Type::F32 => "sub_rt_fmt_f32",
            Type::F64 => "sub_rt_fmt_f64",
            Type::Bool => "sub_rt_fmt_bool",
            other => return Err(format!("interpolation of {other:?}")),
        };
        Ok(format!("{f}(ctx, {v}, {pid}u)"))
    }

    fn eval_cond(&mut self, cond: &hir::Expr, then: &hir::Expr, els: &hir::Expr, ty: &Type, out: &mut String, depth: usize) -> Result<String, String> {
        let ind = indent(depth);
        // Evaluate the arms into a shared temporary via if/else so each
        // arm's side effects run only on its branch.
        let c = self.eval(cond, out, depth)?;
        let cty = self.ctype(ty)?;
        let res = self.fresh_tmp();
        let _ = writeln!(out, "{ind}{cty} {res};");
        let _ = writeln!(out, "{ind}if ({c}) {{");
        let tv = self.eval(then, out, depth + 1)?;
        let _ = writeln!(out, "{}{res} = {tv};", indent(depth + 1));
        let _ = writeln!(out, "{ind}}} else {{");
        let ev = self.eval(els, out, depth + 1)?;
        let _ = writeln!(out, "{}{res} = {ev};", indent(depth + 1));
        let _ = writeln!(out, "{ind}}}");
        Ok(res)
    }

    // ----- calls -----

    fn eval_call(&mut self, callee: &hir::Callee, args: &[hir::Expr], ret_ty: &Type, pos: &Pos, out: &mut String, depth: usize) -> Result<String, String> {
        match callee {
            hir::Callee::Func(name) => {
                let f = self.hir_fn(name)?;
                let argv = self.call_args(&f.params.clone(), args, out, depth)?;
                let sep = if argv.is_empty() { "" } else { ", " };
                Ok(format!("ss_fn_{}(ctx{sep}{argv})", sanitize(name)))
            }
            hir::Callee::Value(v) => {
                let ft = match &v.ty {
                    Type::Func(ft) => (**ft).clone(),
                    other => return Err(format!("call of {other:?}")),
                };
                let fv = self.eval(v, out, depth)?;
                let fvt = self.fresh_tmp();
                let _ = writeln!(out, "{}SubFn {fvt} = {fv};", indent(depth));
                let cast = self.fn_ptr_cast(&ft)?;
                let mut parts = vec![format!("({fvt}).env")];
                for (t, a) in ft.params.iter().zip(args) {
                    let av = self.eval(a, out, depth)?;
                    let _ = t;
                    parts.push(av);
                }
                Ok(format!("(({cast})({fvt}).code)(ctx, {})", parts.join(", ")))
            }
            hir::Callee::Method { recv, name } => self.eval_method(recv, name, args, ret_ty, pos, out, depth),
            hir::Callee::Foreign(name) => self.eval_foreign_call(name, args, out, depth),
            other => Err(format!("callee {other:?} is outside the run set's scope")),
        }
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

    /// For a boundary struct-pointer target (`Struct | null`), the foreign
    /// header pointer type an emitted pointer expression is cast to
    /// (`SubChainHeader*`) — the header struct name, not the language name.
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
    fn boundary_struct_ptr_expr(&mut self, arg: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        if let Type::Class(cid) = arg.ty {
            if self.is_value_class(cid)? {
                return self.value_recv_ptr(arg, cid, out, depth);
            }
        }
        self.eval(arg, out, depth)
    }

    /// Emits a foreign C-ABI call (`Callee::Foreign`, P5.2b): a direct
    /// call of the header symbol with each argument marshaled per Q13. The
    /// C compiler resolves the ABI; the symbol resolves from the linked
    /// `interop.c` (compiler.md §12.4).
    fn eval_foreign_call(&mut self, name: &str, args: &[hir::Expr], out: &mut String, depth: usize) -> Result<String, String> {
        let ff = self.module.foreign_fns.iter().find(|f| f.name == name)
            .ok_or_else(|| format!("unknown foreign function `{name}`"))?
            .clone();
        let mut parts = Vec::new();
        for (p, a) in ff.params.iter().zip(args) {
            parts.push(self.marshal_foreign_c_arg(&p.ty, a, out, depth)?);
        }
        let call = format!("{name}({})", parts.join(", "));
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
                let _ = writeln!(out, "{ind}{lang_ty} {t}; memcpy(&{t}, &{h}, sizeof {t});");
                return Ok(t);
            }
        }
        Ok(call)
    }

    /// Marshals one argument of a foreign call to a C expression (Q13),
    /// emitting any needed temporaries into `out`.
    fn marshal_foreign_c_arg(&mut self, pty: &Type, arg: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        let ind = indent(depth);
        match pty {
            Type::Str => {
                let h = self.eval(arg, out, depth)?;
                let t = self.fresh_tmp();
                let _ = writeln!(out, "{ind}void* {t} = {h};");
                Ok(format!(
                    "((SubStringView){{ (const char*)sub_rt_str_data(ctx, {t}), (size_t)sub_rt_str_len(ctx, {t}) }})"
                ))
            }
            Type::Array(elem) => {
                // A (pointer, count) array-pair descriptor is passed BY
                // VALUE, so C requires the compound literal to name the
                // exact header aggregate type of the parameter. The mirror
                // absorbs every such descriptor into `T[]` (Q13), discarding
                // its C name, so the name is reconstructed from the element
                // type by the synthetic header's convention (this
                // foreign-call path is the P5 interop-slice marshaler and is
                // coupled to that header, as the SubStringView / SubLogCallback
                // spellings elsewhere in this function already are). The JIT
                // tier needs no name — it passes the two components (data,
                // count) per the ABI directly.
                let (desc, elem_cast) = self.interop_array_pair_desc(elem)?;
                let h = self.eval(arg, out, depth)?;
                let t = self.fresh_tmp();
                let _ = writeln!(out, "{ind}void* {t} = {h};");
                Ok(format!(
                    "(({desc}){{ {elem_cast}sub_rt_array_data(ctx, {t}), (size_t)sub_rt_array_len(ctx, {t}) }})"
                ))
            }
            Type::Class(id) if self.is_value_class(*id)? => {
                self.marshal_boundary_c_struct(*id, arg, out, depth)
            }
            _ if self.is_boundary_struct_ptr(pty)? => {
                // Struct | null pointer: address of a value struct's
                // storage (chain-slot address-of), or an existing pointer
                // (`null`, or a `Struct | null` value). Cast to the foreign
                // header pointer type: the language struct is layout-
                // identical (invariant 1) so the pointer is ABI-safe, but
                // nominally distinct, and the cast documents that intent
                // and compiles clean on any clang.
                let cast = self.boundary_ptr_cast(pty)?
                    .ok_or_else(|| "boundary struct ptr lacks a header type".to_string())?;
                let expr = self.boundary_struct_ptr_expr(arg, out, depth)?;
                Ok(format!("({cast})({expr})"))
            }
            _ => self.eval(arg, out, depth),
        }
    }

    /// Marshals a by-value boundary struct to the corresponding C header
    /// struct: pointer/scalar fields pass through; a function-pointer
    /// field becomes the generic trampoline plus a binding built from the
    /// following userdata slot (the callback-info idiom), so the C API
    /// sees `(fnptr, void* userdata)`.
    fn marshal_boundary_c_struct(&mut self, cid: ClassId, arg: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
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
                    // The callback field is followed by one or two userdata
                    // slots (§14.4). Both are bound into one binding the
                    // trampoline reads; the C struct's first userdata slot
                    // carries the binding, any second slot carries null (the
                    // binding is authoritative for both language userdata).
                    let ud1 = fields.get(i + 1)
                        .ok_or_else(|| "a callback field needs a following userdata slot".to_string())?;
                    let has_ud2 = fields.get(i + 2)
                        .map(|f| is_userdata_slot(&f.ty))
                        .unwrap_or(false);
                    let ud2_expr = if has_ud2 {
                        format!("{t}.{}", sanitize(&fields[i + 2].name))
                    } else {
                        "NULL".to_string()
                    };
                    parts.push("(SubLogCallback)&sub_rt_cb_trampoline".to_string());
                    parts.push(format!(
                        "sub_rt_cb_bind(ctx, {t}.{}.code, {t}.{}.env, {t}.{}, {})",
                        sanitize(&f.name), sanitize(&f.name), sanitize(&ud1.name), ud2_expr
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
                    parts.push(format!("(size_t)sub_rt_array_len(ctx, {t}.{fld})"));
                    parts.push(format!("sub_rt_array_data(ctx, {t}.{fld})"));
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
    fn boundary_field_init(&mut self, fty: &Type, arg: &hir::Expr, out: &mut String, depth: usize) -> Result<String, String> {
        if let Some(cast) = self.boundary_ptr_cast(fty)? {
            // Same header-pointer cast as at a direct foreign-call argument
            // (see `marshal_foreign_c_arg`): layout-identical, ABI-safe,
            // nominally distinct.
            let expr = self.boundary_struct_ptr_expr(arg, out, depth)?;
            return Ok(format!("({cast})({expr})"));
        }
        self.eval(arg, out, depth)
    }

    fn call_args(&mut self, params: &[hir::Param], args: &[hir::Expr], out: &mut String, depth: usize) -> Result<String, String> {
        let mut parts = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let text = match args.get(i) {
                Some(a) => self.eval(a, out, depth)?,
                None => {
                    let d = p.default.as_ref().ok_or_else(|| format!("missing argument `{}`", p.name))?;
                    self.eval(d, out, depth)?
                }
            };
            parts.push(text);
        }
        Ok(parts.join(", "))
    }

    fn eval_method(&mut self, recv: &hir::Expr, name: &str, args: &[hir::Expr], ret_ty: &Type, pos: &Pos, out: &mut String, depth: usize) -> Result<String, String> {
        match recv.ty.clone() {
            Type::Str => {
                let h = self.eval(recv, out, depth)?;
                if name != "slice" {
                    return Err(format!("string method `{name}`"));
                }
                let a0 = self.eval(args.first().ok_or("slice arity")?, out, depth)?;
                let a1 = self.eval(args.get(1).ok_or("slice arity")?, out, depth)?;
                let pid = self.pos_id(pos);
                Ok(format!("sub_rt_str_slice(ctx, {h}, {a0}, {a1}, {pid}u)"))
            }
            Type::Array(elem) => {
                // `pop` used as a value (mutators-as-statements are
                // handled by emit_array_mutator).
                if name != "pop" {
                    return Err(format!("array method `{name}` in value position"));
                }
                let h = self.eval(recv, out, depth)?;
                let ect = self.ctype(&elem)?;
                let pid = self.pos_id(pos);
                let d = self.fresh_tmp();
                let _ = writeln!(out, "{}{ect} {d}; sub_rt_array_pop(ctx, {h}, &{d}, {pid}u);", indent(depth));
                Ok(d)
            }
            Type::Generator(y) => {
                if name != "next" {
                    return Err(format!("generator method `{name}`"));
                }
                let g = self.eval(recv, out, depth)?;
                let ir = self.iter_result_name(&y)?;
                let creator = self.generator_of(recv)?;
                let step = self.fresh_tmp();
                let ind = indent(depth);
                let _ = writeln!(out, "{ind}{ir} {step}; memset(&{step}, 0, sizeof {step});");
                let _ = writeln!(out, "{ind}{step}.done = ss_resume_{}(ctx, {g}, &{step}.value);", sanitize(&creator));
                Ok(step)
            }
            Type::Class(cid) => {
                let m = self.hir_method(cid.0, name)?;
                // C2: a value receiver is passed by pointer to its
                // storage (so a mutating method mutates the receiver); a
                // reference receiver passes its handle.
                let recv_c = if self.is_value_class(cid)? {
                    self.value_recv_ptr(recv, cid, out, depth)?
                } else {
                    self.eval(recv, out, depth)?
                };
                let argv = self.call_args(&m.params.clone(), args, out, depth)?;
                let sep = if argv.is_empty() { "" } else { ", " };
                let _ = ret_ty;
                Ok(format!("ss_m{}_{}(ctx, {recv_c}{sep}{argv})", cid.0, sanitize(name)))
            }
            other => Err(format!("method on {other:?}")),
        }
    }

    /// A `Sub*` pointing at a value-class receiver's storage (C2). When
    /// the receiver is an lvalue its address is taken so a mutating
    /// method mutates it; an rvalue is materialized into a temporary
    /// first, so a mutation of the temporary is correctly lost, matching
    /// the CLIF path (whose rvalue receiver is a temp too).
    fn value_recv_ptr(&mut self, recv: &hir::Expr, cid: ClassId, out: &mut String, depth: usize) -> Result<String, String> {
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

    fn eval_new(&mut self, class: ClassId, args: &[hir::Expr], pos: &Pos, out: &mut String, depth: usize) -> Result<String, String> {
        let c = self.class(class)?;
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
                    c.name, fields.len(), args.len()
                ));
            }
            let mut parts = Vec::new();
            for (i, field) in fields.iter().enumerate() {
                parts.push(self.boundary_field_init(&field.ty, &args[i], out, depth)?);
            }
            return Ok(format!("(({cname}){{ {} }})", parts.join(", ")));
        }
        let ctor = c.ctor.as_ref();
        let argv = match ctor {
            Some(ctor) => self.call_args(&ctor.params.clone(), args, out, depth)?,
            None => String::new(),
        };
        if self.is_value_class(class)? {
            if ctor.is_some() {
                let sep = if argv.is_empty() { "" } else { ", " };
                Ok(format!("ss_ctor{}(ctx{sep}{argv})", class.0))
            } else {
                Ok(format!("({}){{0}}", self.class_name(class)?))
            }
        } else {
            let pid = self.pos_id(pos);
            let sep = if argv.is_empty() { "" } else { ", " };
            Ok(format!("ss_new{}(ctx, {pid}u{sep}{argv})", class.0))
        }
    }

    // ----- function values and lambdas -----

    fn func_ref_value(&mut self, name: &str) -> Result<String, String> {
        let wrap = format!("ss_wrap_{}", sanitize(name));
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
        let call = format!("ss_fn_{}(ctx{asep}{})", sanitize(name), argv.join(", "));
        let _ = writeln!(self.helpers, "{sig} {{ (void)_env; {}{call}; }}",
            if f.ret == Type::Void { "" } else { "return " });
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

    fn eval_lambda(&mut self, params: &[hir::Param], ret: &Type, body: &[hir::Stmt], captures: &[String], out: &mut String, depth: usize) -> Result<String, String> {
        let n = self.lambda;
        self.lambda += 1;
        let name = format!("ss_lambda{n}");
        let env_ty = format!("EnvL{n}");
        let ind = indent(depth);

        // Environment: captured values by value (C5), non-escaping so it
        // may live in the creating frame.
        let mut cap_tys: Vec<(String, Type)> = Vec::new();
        for cap in captures {
            let ty = self.capture_type(cap)?;
            cap_tys.push((cap.clone(), ty));
        }
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
            let _ = writeln!(out, "{ind}static {env_ty} {etmp};");
            for (cn, _) in &cap_tys {
                let _ = writeln!(out, "{ind}{etmp}.{} = {};", sanitize(cn), self.local_ref(cn));
            }
            format!("(void*)&{etmp}")
        };

        // Emit the lambda function into helpers.
        self.emit_lambda_fn(&name, &env_ty, params, ret, body, &cap_tys)?;
        Ok(format!("((SubFn){{ (void*)&{name}, {env_expr} }})"))
    }

    fn emit_lambda_fn(&mut self, name: &str, env_ty: &str, params: &[hir::Param], ret: &Type, body: &[hir::Stmt], caps: &[(String, Type)]) -> Result<(), String> {
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
        let saved_ranges = std::mem::take(&mut self.ranges);
        self.this = ThisCtx::None;
        self.shadow_cursor = 0;
        self.has_shadow = false;

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
            let _ = writeln!(fbody, "    {} {} = _e->{};", self.ctype(ct)?, sanitize(cn), sanitize(cn));
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
        self.ranges = saved_ranges;
        Ok(())
    }

    /// The declared type of a captured local, from the enclosing
    /// function's scope (C2).
    fn capture_type(&self, name: &str) -> Result<Type, String> {
        for (n, t) in self.local_types.iter().rev() {
            if n == name {
                return Ok(t.clone());
            }
        }
        Err(format!("captured local `{name}` has no known type"))
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
        let g = self.gen.as_mut().ok_or("generator let outside a generator")?;
        let field = g.let_fields.get(g.let_cursor).cloned().ok_or("generator frame let cursor exhausted")?;
        g.let_cursor += 1;
        self.gen_locals.push((name.to_string(), format!("_f->{field}")));
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
            let _ = writeln!(struct_body, "    {} {};", self.ctype(&p.ty)?, sanitize(&p.name));
        }
        for (i, (_, ty)) in lets.iter().enumerate() {
            let field = format!("g{i}");
            let _ = writeln!(struct_body, "    {} {};", self.ctype(ty)?, field);
            let_fields.push(field);
        }
        let _ = writeln!(out, "typedef struct {gen_struct} {{\n{struct_body}}} {gen_struct};");

        // Creator.
        let creator_sig = self.gen_creator_signature(f)?;
        let _ = writeln!(out, "{creator_sig} {{");
        let pid = self.pos_id(&f.pos);
        let _ = writeln!(out, "    void* _frame = sub_rt_alloc(ctx, sizeof({gen_struct}), {}u, {pid}u);", rtc::CLASS_GENERATOR);
        let _ = writeln!(out, "    if (_frame == 0) return 0;");
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

        self.begin_fn(ThisCtx::None);
        self.gen = Some(GenState {
            yields: 0,
            let_cursor: 0,
            let_fields,
            yield_ct: self.ctype(&yield_ty)?,
        });
        for p in &f.params {
            self.gen_locals.push((p.name.clone(), format!("_f->{}", sanitize(&p.name))));
        }
        self.emit_block(out, &f.body, 1)?;
        // Fell off the end: done.
        let _ = writeln!(out, "    _f->_state = {GEN_DONE}; return 1;");
        let _ = writeln!(out, "}}\n");
        self.gen = None;
        self.gen_locals.clear();
        Ok(())
    }

    fn eval_yield(&mut self, arg: Option<&hir::Expr>, out: &mut String, depth: usize) -> Result<String, String> {
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

    // ----- interval analysis (§10.1) -----

    fn interval_of(&self, e: &hir::Expr) -> Option<Interval> {
        use hir::ExprKind as K;
        if !e.ty.is_integer() {
            return None;
        }
        match &e.kind {
            K::Int(v) => Some(Interval::point(*v)),
            K::EnumMember { value, .. } => Some(Interval::point(*value)),
            K::Local(name) => self.ranges.get(name).copied(),
            K::Length(obj) => match &obj.ty {
                Type::FixedArray(_, n) => Some(Interval::point(i64::from(*n))),
                _ => None,
            },
            K::Binary { op, left, right } => {
                let l = self.interval_of(left)?;
                let r = self.interval_of(right)?;
                match op {
                    hir::BinOp::Add => l.add(r),
                    hir::BinOp::Sub => l.sub(r),
                    hir::BinOp::Mul => l.mul(r),
                    _ => None,
                }
            }
            K::Cast(inner) => {
                if !inner.ty.is_integer() {
                    return None;
                }
                let iv = self.interval_of(inner)?;
                let target = int_type_range(&e.ty)?;
                if iv.lo >= target.lo && iv.hi <= target.hi {
                    Some(iv)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn index_in_bounds(&self, index: &hir::Expr, n: u32) -> bool {
        match self.interval_of(index) {
            Some(iv) => iv.lo >= 0 && iv.hi < i64::from(n),
            None => false,
        }
    }

    fn induction_interval(
        &self,
        init: Option<&hir::Stmt>,
        cond: Option<&hir::Expr>,
        step: Option<&hir::Expr>,
        body: &[hir::Stmt],
    ) -> Option<(String, Interval)> {
        use hir::ExprKind as K;
        let (name, ty, start_iv) = match init? {
            hir::Stmt::Let { name, ty, init, .. } if ty.is_integer() => {
                (name.clone(), ty.clone(), self.interval_of(init)?)
            }
            _ => return None,
        };
        if start_iv.lo != start_iv.hi {
            return None;
        }
        let start = start_iv.lo;
        let (op, bound_iv) = match &cond?.kind {
            K::Binary { op, left, right } => match &left.kind {
                K::Local(n) if *n == name => (*op, self.interval_of(right)?),
                _ => return None,
            },
            _ => return None,
        };
        let hi = match op {
            hir::BinOp::Lt => bound_iv.hi.checked_sub(1)?,
            hir::BinOp::Le => bound_iv.hi,
            _ => return None,
        };
        let step_iv = match &step?.kind {
            K::Assign { op: Some(hir::BinOp::Add), target, value } => match &target.kind {
                K::Local(n) if *n == name => self.interval_of(value)?,
                _ => return None,
            },
            _ => return None,
        };
        if step_iv.lo != step_iv.hi || step_iv.lo <= 0 {
            return None;
        }
        let stepv = step_iv.lo;
        if start > hi {
            return None;
        }
        let tr = int_type_range(&ty)?;
        if start < tr.lo || hi.checked_add(stepv)? > tr.hi {
            return None;
        }
        if stmts_assign_to(body, &name) {
            return None;
        }
        Some((name, Interval { lo: start, hi }))
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

/// The synthetic interop header's C aggregate name for a `(pointer, count)`
/// array-pair descriptor of the given element type (P5 interop slice,
/// `specs/blocks/compiler.md` §12). The header spells the u32 descriptor
/// `SubBufferView` (historical) and the other primitive descriptors
/// `SubSlice<T>`. An element type without a header descriptor is a loud
/// codegen error, never a silent mis-marshal (dev-JIT ≠ ship-C otherwise).
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
    matches!(ty, Type::Object)
        || matches!(ty, Type::Nullable(inner) if **inner == Type::Object)
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
        Type::Array(e) | Type::Nullable(e) | Type::Generator(e) => collect_aggr_ty(e, set),
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
        K::Field { obj, .. } => collect_aggr_expr(obj, set),
        K::Length(obj) => collect_aggr_expr(obj, set),
        K::Index { obj, index } => {
            collect_aggr_expr(obj, set);
            collect_aggr_expr(index, set);
        }
        K::ArrayLit(elems) => {
            for x in elems {
                collect_aggr_expr(x, set);
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
        K::Lambda { params, ret, body, .. } => {
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
            hir::Stmt::If { cond, then, els, .. } => {
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
            hir::Stmt::For { init, cond, step, body, .. } => {
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
            hir::Stmt::If { cond, then, els, .. } => {
                n += count_yields_expr(cond) + count_yields(then);
                if let Some(e) = els {
                    n += count_yields(e);
                }
            }
            hir::Stmt::While { cond, body, .. } => n += count_yields_expr(cond) + count_yields(body),
            hir::Stmt::For { init, cond, step, body, .. } => {
                if let Some(i) = init {
                    n += count_yields(std::slice::from_ref(&**i));
                }
                n += cond.as_ref().map_or(0, count_yields_expr);
                n += step.as_ref().map_or(0, count_yields_expr);
                n += count_yields(body);
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
        K::Field { obj, .. } => count_yields_expr(obj),
        K::Length(obj) => count_yields_expr(obj),
        K::Index { obj, index } => count_yields_expr(obj) + count_yields_expr(index),
        K::ArrayLit(elems) => elems.iter().map(count_yields_expr).sum(),
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
        (Type::I32, true) => "ss_sdiv_i32",
        (Type::I32, false) => "ss_srem_i32",
        (Type::U32, true) => "ss_udiv_u32",
        (Type::U32, false) => "ss_urem_u32",
        (Type::I64, true) => "ss_sdiv_i64",
        (Type::I64, false) => "ss_srem_i64",
        (Type::U64, true) => "ss_udiv_u64",
        (Type::U64, false) => "ss_urem_u64",
        (other, _) => return Err(format!("integer div/rem on {other:?}")),
    })
}

fn float_to_int_helper(to: &Type) -> Result<&'static str, String> {
    Ok(match to {
        Type::I32 => "ss_f2i32",
        Type::U32 => "ss_f2u32",
        Type::I64 => "ss_f2i64",
        Type::U64 => "ss_f2u64",
        other => return Err(format!("float to {other:?}")),
    })
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
        "auto" | "break" | "case" | "char" | "const" | "continue" | "default" | "do"
            | "double" | "else" | "enum" | "extern" | "float" | "for" | "goto" | "if"
            | "inline" | "int" | "long" | "register" | "restrict" | "return" | "short"
            | "signed" | "sizeof" | "static" | "struct" | "switch" | "typedef" | "union"
            | "unsigned" | "void" | "volatile" | "while" | "_Bool" | "_Complex"
            | "ctx"
    )
}

fn int_literal(v: i64, ty: &Type) -> String {
    match ty {
        Type::U32 => format!("{}u", v as u32),
        Type::U64 => format!("{}ull", v as u64),
        Type::I64 => format!("{v}ll"),
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
/// integer-div/rem helpers, saturating float→int helpers, and the safe
/// checked-index accessors (which return a scratch pointer after a trap
/// so a post-trap dereference does not fault — the host entry discards a
/// trapped run's output).
const PREAMBLE: &str = r#"/* Generated by subscript's C emitter — the ship tier
 * (specs/blocks/compiler.md 11). Do not edit; fix the generator.
 * This translation unit carries the language's semantics and links the
 * runtime static library (sub_rt_*), so arrays, strings, Q14 formatting,
 * and traps are identical to the dev-JIT tier. Compile -O2
 * -ffp-contract=off and link with the runtime archive and the host
 * entry (AOT_ENTRY_C). */

#include <stdint.h>
#include <string.h>

/* Runtime C-ABI boundary (runtime/src/ffi.rs). Handles are void*. */
extern void sub_rt_print(void* ctx, const void* s);
extern void sub_rt_collect(void* ctx);
extern void* sub_rt_alloc(void* ctx, uint64_t size, uint32_t class_id, uint32_t pos_id);
extern void sub_rt_delete(void* ctx, void* payload, uint32_t pos_id);
extern void sub_rt_trap(void* ctx, uint32_t kind, uint32_t pos_id);
extern void sub_rt_root_add(void* ctx, void* base, uint64_t words);
extern void sub_rt_shadow_push(void* ctx, void* base, uint64_t slots);
extern void sub_rt_shadow_pop(void* ctx);
extern void* sub_rt_str_lit(void* ctx, const unsigned char* ptr, uint64_t len, uint32_t pos_id);
extern int32_t sub_rt_str_len(void* ctx, const void* s);
extern void* sub_rt_str_concat(void* ctx, const void* a, const void* b, uint32_t pos_id);
extern void* sub_rt_str_slice(void* ctx, const void* s, int32_t start, int32_t end, uint32_t pos_id);
extern int32_t sub_rt_str_eq(void* ctx, const void* a, const void* b);
extern void* sub_rt_fmt_i32(void* ctx, int32_t v, uint32_t pos_id);
extern void* sub_rt_fmt_u32(void* ctx, uint32_t v, uint32_t pos_id);
extern void* sub_rt_fmt_i64(void* ctx, int64_t v, uint32_t pos_id);
extern void* sub_rt_fmt_u64(void* ctx, uint64_t v, uint32_t pos_id);
extern void* sub_rt_fmt_f32(void* ctx, float v, uint32_t pos_id);
extern void* sub_rt_fmt_f64(void* ctx, double v, uint32_t pos_id);
extern void* sub_rt_fmt_bool(void* ctx, uint32_t v, uint32_t pos_id);
extern void* sub_rt_array_new(void* ctx, uint64_t elem_size, uint32_t pos_id);
extern int32_t sub_rt_array_len(void* ctx, const void* a);
extern int32_t sub_rt_array_push(void* ctx, void* a, const void* src, uint32_t pos_id);
extern void sub_rt_array_pop(void* ctx, void* a, void* dst, uint32_t pos_id);
extern void* sub_rt_array_ptr(void* ctx, void* a, int32_t idx, uint32_t pos_id);
/* C-boundary marshaling (P5.2b): string/array data pointers and the
 * callback binding constructor. The generic trampoline itself is declared
 * with the boundary include below, since its type mentions SubStringView. */
extern const void* sub_rt_str_data(void* ctx, const void* s);
extern const void* sub_rt_array_data(void* ctx, const void* a);
extern void* sub_rt_cb_bind(void* ctx, const void* code, const void* env, void* userdata1, void* userdata2);

/* Trap kinds (runtime/src/trap.rs). */
enum { SS_TRAP_OOB = 1, SS_TRAP_DIV0 = 10 };

/* A non-capturing function value / capturing lambda: (code, env). */
typedef struct { void* code; void* env; } SubFn;

/* Scratch returned by the checked accessors after a trap, so a post-trap
 * dereference stays in bounds; the host entry discards a trapped run's
 * output, so its value is never observed. */
static unsigned char ss_scratch[256];

/* Mirror of the runtime ArrayHeader (runtime/src/context.rs, repr(C),
 * compiler.md invariant 1 / §10a). The in-bounds fast path is inlined so
 * the host C compiler can optimize the surrounding loops; an out-of-bounds
 * index falls to sub_rt_array_ptr, the sole producer of the trap and its
 * exact message, so behaviour stays byte-identical to the runtime path. */
typedef struct { uint64_t len; uint64_t cap; uint64_t elem_size; unsigned char* data; } SsArrayHeader;

static void* ss_arr_at(void* ctx, void* a, int32_t idx, uint32_t pos) {
    SsArrayHeader* h = (SsArrayHeader*)a;
    if (idx >= 0 && (uint64_t)idx < h->len) {
        return h->data + (int64_t)idx * (int64_t)h->elem_size;
    }
    void* p = sub_rt_array_ptr(ctx, a, idx, pos);
    return p ? p : (void*)ss_scratch;
}

static void* ss_fa_at(void* ctx, void* base, int64_t n, int32_t idx, int64_t elem, uint32_t pos) {
    if ((uint32_t)idx >= (uint32_t)n) {
        sub_rt_trap(ctx, SS_TRAP_OOB, pos);
        return (void*)ss_scratch;
    }
    (void)elem;
    return (unsigned char*)base + (int64_t)idx * elem;
}

/* Integer div/rem with the language's semantics: trap on a zero divisor;
 * two's-complement wrap for signed MIN / -1 and MIN % -1. */
static int32_t ss_sdiv_i32(void* ctx, int32_t a, int32_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return (int32_t)(0u - (uint32_t)a);
    return a / b;
}
static int32_t ss_srem_i32(void* ctx, int32_t a, int32_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return 0;
    return a % b;
}
static uint32_t ss_udiv_u32(void* ctx, uint32_t a, uint32_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a / b;
}
static uint32_t ss_urem_u32(void* ctx, uint32_t a, uint32_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a % b;
}
static int64_t ss_sdiv_i64(void* ctx, int64_t a, int64_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return (int64_t)(0ull - (uint64_t)a);
    return a / b;
}
static int64_t ss_srem_i64(void* ctx, int64_t a, int64_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    if (b == -1) return 0;
    return a % b;
}
static uint64_t ss_udiv_u64(void* ctx, uint64_t a, uint64_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a / b;
}
static uint64_t ss_urem_u64(void* ctx, uint64_t a, uint64_t b, uint32_t pos) {
    if (b == 0) { sub_rt_trap(ctx, SS_TRAP_DIV0, pos); return 0; }
    return a % b;
}

/* Saturating float->int, matching the CLIF fcvt_to_*_sat choice. */
static int32_t ss_f2i32(double v) {
    if (v != v) return 0;
    if (v <= -2147483648.0) return (int32_t)(-2147483647 - 1);
    if (v >= 2147483647.0) return 2147483647;
    return (int32_t)v;
}
static uint32_t ss_f2u32(double v) {
    if (v != v) return 0;
    if (v <= 0.0) return 0;
    if (v >= 4294967295.0) return 4294967295u;
    return (uint32_t)v;
}
static int64_t ss_f2i64(double v) {
    if (v != v) return 0;
    if (v <= -9223372036854775808.0) return (-9223372036854775807ll - 1);
    if (v >= 9223372036854775807.0) return 9223372036854775807ll;
    return (int64_t)v;
}
static uint64_t ss_f2u64(double v) {
    if (v != v) return 0;
    if (v <= 0.0) return 0;
    if (v >= 18446744073709551615.0) return 18446744073709551615ull;
    return (uint64_t)v;
}

"#;

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
    fn emits_the_host_entry_surface() {
        let c = emit("export function main(): void {\n  const x: f32 = 1.5;\n  print(`${x}`);\n}\n");
        assert!(c.contains("void ss_init(void* ctx)"));
        assert!(c.contains("void ss_export_main(void* ctx)"));
        assert!(c.contains("sub_rt_fmt_f32"));
        assert!(c.contains("1.5f"));
    }

    #[test]
    fn value_class_is_a_by_value_struct() {
        let c = emit("@value\nclass V { x: f32; y: f32;\n constructor(x: f32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void {\n  const v: V = new V(1.0, 2.0);\n  print(`${v.x}`);\n}\n");
        assert!(c.contains("typedef struct Sub_0_V"));
        assert!(c.contains("ss_ctor0(void* ctx"));
    }

    #[test]
    fn reference_class_uses_the_runtime_allocator() {
        let c = emit("class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  print(`${c.x}`);\n  unsafeDelete(c);\n}\n");
        assert!(c.contains("sub_rt_alloc"));
        assert!(c.contains("sub_rt_delete"));
        assert!(c.contains("ss_new0(void* ctx"));
    }

    #[test]
    fn fixed_array_proven_index_is_unchecked() {
        let c = emit("export function main(): void {\n  const xs: FixedArray<i32, 4> = [10, 20, 30, 40];\n  let sum: i32 = 0;\n  for (let i: i32 = 0; i < 4; i += 1) {\n    sum += xs[i];\n  }\n  print(`${sum}`);\n}\n");
        assert!(!c.contains("ss_fa_at(ctx,"), "proven index must not be checked");
        assert!(c.contains("(xs).a[i]"));
    }

    #[test]
    fn dynamic_array_index_is_checked() {
        let c = emit("export function main(): void {\n  const xs: i32[] = [];\n  xs.push(7);\n  print(`${xs[0]}`);\n}\n");
        assert!(c.contains("ss_arr_at"));
        assert!(c.contains("sub_rt_array_push"));
    }
}



