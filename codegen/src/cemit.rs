//! Typed-HIR-to-C emitter (P4.2 C-emission measurement spike,
//! `specs/tracking/p4-performance.md`).
//!
//! This is **not** the C backend; it is a bounded, faithful C emitter
//! whose only purpose is to answer one measured question: how much of
//! clang/LLVM's `-O2` win on the a22 matmul survives when the C is
//! emitted from the same typed HIR the CLIF lowering consumes, carrying
//! the language's semantics rather than a hand-optimized rewrite.
//!
//! Faithfulness (or the measurement is void) — this emitter mirrors the
//! CLIF lowering's *semantics*, in particular:
//!
//! - **C2 value-class copy semantics.** A `@value class` becomes a C
//!   `struct`; it is copied on assign, on pass (by-value parameters),
//!   on index-read, and on return, exactly where the CLIF path copies
//!   (`copy_to_temp` on aggregate args, `store_val` copies on assign).
//!   The hand-written baseline passes matrices by `const*` and so
//!   elides copy-on-pass; this emitter does not.
//! - **Checked growable arrays.** A `T[]` becomes a `(data, len, cap)`
//!   struct with a per-access bounds check ([`SubArray`]-style
//!   `sub_arr_at`) and realloc-on-push growth doubling the capacity
//!   (`sub_arr_push`), mirroring the runtime's `array_elem_ptr` /
//!   `array_push`. The baseline uses one flat `malloc`.
//! - **Proof-based `FixedArray` bounds-check elision (§10.1).** A
//!   `FixedArray<T, N>` becomes an in-place C array `T[N]`; an index
//!   proven in `[0, N)` by the same interval/induction analysis the
//!   CLIF path uses is emitted as a plain unchecked `a[i]` (as CLIF
//!   elides the check), and any index the analysis cannot prove keeps a
//!   bounds check (`sub_fa_at`) — so the inner matmul is unchecked
//!   exactly where CLIF's is.
//! - **f32 stays f32.** Float locals/expressions are `float`; float
//!   literals carry the `f` suffix whenever the HIR node's type is
//!   `f32`, so no accidental double promotion changes the result. The
//!   program is compiled `-ffp-contract=off` to match the language,
//!   which never contracts a multiply-add.
//! - **Q14 formatting.** The final `${result}` is formatted by the same
//!   shortest-round-trip rule the runtime uses (`sub_fmt_f32`:
//!   `%.*g` widened until it round-trips; integral floats print without
//!   a decimal point).
//!
//! The emitted translation unit is **self-contained** (no link against
//! the Rust runtime static library): the array, format, and trap
//! helpers are emitted as C so the whole workload is one translation
//! unit clang optimizes end to end — which is exactly what the spike
//! measures. Where a helper replicates runtime logic, it matches the
//! runtime's behaviour (growth schedule, bounds-check condition, Q14).
//!
//! Scope: the emitter handles the subset of HIR the a22 corpus entry
//! exercises — module globals, one `@value` class with a constructor,
//! free functions (value-class by value, dynamic-array by reference,
//! scalar returns), `FixedArray` literals/indexing, dynamic-array
//! `push`/`length`/indexing/assignment, `for`/`if`, compound
//! assignment, numeric casts, and a `print` of a template literal.
//! Constructs outside that subset (reference classes, `Nullable`,
//! lambdas, generators, strings beyond a printed template, methods,
//! `switch`, `while`, ternary) are reported as an error rather than
//! emitted wrong; a22 uses none of them.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use subscript_compiler::hir;
use subscript_compiler::types::{ClassId, Type};

/// An inclusive integer interval `[lo, hi]`, computed in `i64` with the
/// same widen-in-`i128`-then-narrow discipline as the CLIF lowering's
/// interval lattice (`codegen/src/lower/func.rs`), so it never wraps.
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

/// Emits a C translation unit for a checked HIR module.
///
/// The unit has its own `main` implementing the P4 harness protocol
/// (`argv` = `warmup timed`; `sample <i> <ns>` and `checksum-stable
/// <0|1>` on stderr; the program's output bytes on stdout), so it is a
/// drop-in subject for the §9 methodology, timed the same way as the
/// AOT subject (the whole exported-`main` call, output captured to an
/// in-memory sink and flushed after the timed span).
///
/// # Errors
///
/// Returns an error string when the module uses an HIR construct
/// outside this spike's scope, or has no exported `main(): void`.
pub fn emit_c(module: &hir::Module) -> Result<String, String> {
    Emitter::new(module).emit()
}

struct Emitter<'m> {
    module: &'m hir::Module,
    /// Proven interval of currently-visible loop counters (§10.1).
    ranges: HashMap<String, Interval>,
    /// Names of the current function's parameters that are dynamic
    /// arrays (passed as `SubArray*`, so they are used without `&`).
    ptr_arrays: HashSet<String>,
    /// C name of `this` inside the constructor body, if any.
    this_name: Option<&'static str>,
}

impl<'m> Emitter<'m> {
    fn new(module: &'m hir::Module) -> Emitter<'m> {
        Emitter {
            module,
            ranges: HashMap::new(),
            ptr_arrays: HashSet::new(),
            this_name: None,
        }
    }

    fn class_name(&self, id: ClassId) -> Result<String, String> {
        self.module
            .classes
            .get(id.0)
            .map(|c| format!("Sub_{}", sanitize(&c.name)))
            .ok_or_else(|| format!("class id {} out of range", id.0))
    }

    // ----- type mapping -----

    /// C type for a scalar/value-class type (not `FixedArray`, which is
    /// only ever a field/local/param and is handled at its site).
    fn ctype(&self, ty: &Type) -> Result<String, String> {
        Ok(match ty {
            Type::I32 => "int32_t".to_string(),
            Type::U32 => "uint32_t".to_string(),
            Type::I64 => "int64_t".to_string(),
            Type::U64 => "uint64_t".to_string(),
            Type::F32 => "float".to_string(),
            Type::F64 => "double".to_string(),
            Type::Bool => "int32_t".to_string(),
            Type::Void => "void".to_string(),
            Type::Class(id) => {
                if self.is_value_class(*id)? {
                    self.class_name(*id)?
                } else {
                    return Err("reference classes are outside this spike's scope".to_string());
                }
            }
            other => {
                return Err(format!("type {other:?} is outside this spike's scope"))
            }
        })
    }

    fn is_value_class(&self, id: ClassId) -> Result<bool, String> {
        self.module
            .classes
            .get(id.0)
            .map(|c| c.is_value)
            .ok_or_else(|| format!("class id {} out of range", id.0))
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

    /// Recognizes a count-up `for` whose counter has a proven,
    /// non-wrapping range (identical conditions to the CLIF lowering's
    /// `induction_interval`).
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
            K::Assign {
                op: Some(hir::BinOp::Add),
                target,
                value,
            } => match &target.kind {
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

    // ----- top-level emit -----

    fn emit(&mut self) -> Result<String, String> {
        // Validate the entry point exists (mirrors lower_module_with).
        let main = self
            .module
            .functions
            .iter()
            .find(|f| f.name == "main" && f.exported && f.params.is_empty() && f.ret == Type::Void)
            .ok_or_else(|| "no exported `main(): void` entry point".to_string())?;
        if main.is_generator {
            return Err("`main` may not be a generator".to_string());
        }

        let mut out = String::new();
        out.push_str(PREAMBLE);

        // Value-class struct definitions and constructors.
        for (ci, class) in self.module.classes.iter().enumerate() {
            if !class.is_value {
                return Err("reference classes are outside this spike's scope".to_string());
            }
            if !class.methods.is_empty() {
                return Err("class methods are outside this spike's scope".to_string());
            }
            self.emit_class_struct(&mut out, ci, class)?;
        }
        for (ci, class) in self.module.classes.iter().enumerate() {
            if class.ctor.is_some() {
                self.emit_constructor(&mut out, ci, class)?;
            }
        }

        // Globals + their initializer.
        for g in &self.module.globals {
            let cty = self.ctype(&g.ty)?;
            let _ = writeln!(out, "static {cty} g_{};", sanitize(&g.name));
        }
        out.push_str("\nstatic void ss_init(void) {\n");
        for g in &self.module.globals {
            let v = self.emit_expr(&g.init)?;
            let _ = writeln!(out, "    g_{} = {v};", sanitize(&g.name));
        }
        out.push_str("}\n\n");

        // Forward declarations, then bodies.
        for f in &self.module.functions {
            let sig = self.fn_signature(f)?;
            let _ = writeln!(out, "{sig};");
        }
        out.push('\n');
        for f in &self.module.functions {
            self.emit_function(&mut out, f)?;
        }

        out.push_str(HARNESS_MAIN);
        Ok(out)
    }

    // ----- classes -----

    fn emit_class_struct(
        &self,
        out: &mut String,
        ci: usize,
        class: &hir::ClassDef,
    ) -> Result<(), String> {
        let name = self.class_name(ClassId(ci))?;
        let _ = writeln!(out, "typedef struct {name} {{");
        for field in &class.fields {
            let decl = self.field_decl(&field.name, &field.ty)?;
            let _ = writeln!(out, "    {decl};");
        }
        let _ = writeln!(out, "}} {name};\n");
        Ok(())
    }

    /// A struct-field / local declaration fragment `"<type> <name>"`
    /// (or `"<elem> <name>[N]"` for a `FixedArray`).
    fn field_decl(&self, name: &str, ty: &Type) -> Result<String, String> {
        match ty {
            Type::FixedArray(elem, n) => {
                let ect = self.ctype(elem)?;
                Ok(format!("{ect} {name}[{n}]"))
            }
            _ => Ok(format!("{} {name}", self.ctype(ty)?)),
        }
    }

    fn emit_constructor(
        &mut self,
        out: &mut String,
        ci: usize,
        class: &hir::ClassDef,
    ) -> Result<(), String> {
        let ctor = class
            .ctor
            .as_ref()
            .ok_or_else(|| "constructor missing".to_string())?;
        let cname = self.class_name(ClassId(ci))?;
        let params = self.param_list(&ctor.params)?;
        let _ = writeln!(out, "static {cname} {cname}_new({params}) {{");
        let _ = writeln!(out, "    {cname} _this;");
        let _ = writeln!(out, "    memset(&_this, 0, sizeof _this);");
        // Field initializers (declaration order), then the ctor body.
        self.this_name = Some("_this");
        self.ptr_arrays.clear();
        self.register_array_params(&ctor.params);
        for field in &class.fields {
            if let Some(init) = &field.init {
                let v = self.emit_expr(init)?;
                let _ = writeln!(out, "    _this.{} = {v};", sanitize(&field.name));
            }
        }
        self.emit_block(out, &ctor.body, 1)?;
        self.this_name = None;
        let _ = writeln!(out, "    return _this;\n}}\n");
        Ok(())
    }

    // ----- functions -----

    fn c_fn_name(f: &hir::Function) -> String {
        format!("ss_{}", sanitize(&f.name))
    }

    fn ret_ctype(&self, ty: &Type) -> Result<String, String> {
        match ty {
            Type::FixedArray(..) => {
                Err("returning a FixedArray by value is outside this spike's scope".to_string())
            }
            _ => self.ctype(ty),
        }
    }

    /// The C parameter list, mapping dynamic arrays to `SubArray*`,
    /// `FixedArray` to a decayed `const T name[N]`, value classes to
    /// by-value structs, and scalars to their C type.
    fn param_list(&self, params: &[hir::Param]) -> Result<String, String> {
        if params.is_empty() {
            return Ok("void".to_string());
        }
        let mut parts = Vec::new();
        for p in params {
            let decl = match &p.ty {
                Type::Array(_) => format!("SubArray* {}", sanitize(&p.name)),
                Type::FixedArray(elem, n) => {
                    format!("const {} {}[{n}]", self.ctype(elem)?, sanitize(&p.name))
                }
                _ => format!("{} {}", self.ctype(&p.ty)?, sanitize(&p.name)),
            };
            parts.push(decl);
        }
        Ok(parts.join(", "))
    }

    fn fn_signature(&self, f: &hir::Function) -> Result<String, String> {
        if f.is_generator {
            return Err("generators are outside this spike's scope".to_string());
        }
        let ret = self.ret_ctype(&f.ret)?;
        let name = Emitter::c_fn_name(f);
        let params = self.param_list(&f.params)?;
        Ok(format!("static {ret} {name}({params})"))
    }

    fn register_array_params(&mut self, params: &[hir::Param]) {
        for p in params {
            if matches!(p.ty, Type::Array(_)) {
                self.ptr_arrays.insert(sanitize(&p.name));
            }
        }
    }

    fn emit_function(&mut self, out: &mut String, f: &hir::Function) -> Result<(), String> {
        let sig = self.fn_signature(f)?;
        let _ = writeln!(out, "{sig} {{");
        self.ranges.clear();
        self.ptr_arrays.clear();
        self.this_name = None;
        self.register_array_params(&f.params);
        self.emit_block(out, &f.body, 1)?;
        out.push_str("}\n\n");
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

    fn indent(depth: usize) -> String {
        "    ".repeat(depth)
    }

    fn emit_stmt(
        &mut self,
        out: &mut String,
        s: &hir::Stmt,
        depth: usize,
    ) -> Result<(), String> {
        let ind = Emitter::indent(depth);
        match s {
            hir::Stmt::Let { name, ty, init, .. } => {
                self.emit_let(out, name, ty, init, depth)
            }
            hir::Stmt::Expr(e) => self.emit_expr_stmt(out, e, depth),
            hir::Stmt::Return { value, .. } => {
                match value {
                    None => {
                        let _ = writeln!(out, "{ind}return;");
                    }
                    Some(v) => {
                        let text = self.emit_expr(v)?;
                        let _ = writeln!(out, "{ind}return {text};");
                    }
                }
                Ok(())
            }
            hir::Stmt::If { cond, then, els, .. } => {
                let c = self.emit_expr(cond)?;
                let _ = writeln!(out, "{ind}if ({c}) {{");
                self.emit_block(out, then, depth + 1)?;
                if let Some(e) = els {
                    let _ = writeln!(out, "{ind}}} else {{");
                    self.emit_block(out, e, depth + 1)?;
                }
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            hir::Stmt::For {
                init, cond, step, body, ..
            } => self.emit_for(out, init.as_deref(), cond.as_ref(), step.as_ref(), body, depth),
            hir::Stmt::Block(b) => {
                let _ = writeln!(out, "{ind}{{");
                self.emit_block(out, b, depth + 1)?;
                let _ = writeln!(out, "{ind}}}");
                Ok(())
            }
            other => Err(format!(
                "statement {other:?} is outside this spike's scope"
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
        let ind = Emitter::indent(depth);
        let cname = sanitize(name);
        match ty {
            Type::FixedArray(elem, n) => {
                // In-place C array; a literal initializes it brace-wise.
                match &init.kind {
                    hir::ExprKind::ArrayLit(elems) => {
                        let vals = self.emit_fixed_array_elems(elems)?;
                        let ect = self.ctype(elem)?;
                        let _ = writeln!(out, "{ind}{ect} {cname}[{n}] = {{ {vals} }};");
                        Ok(())
                    }
                    _ => Err(
                        "FixedArray locals must be initialized from a literal in this spike"
                            .to_string(),
                    ),
                }
            }
            Type::Array(elem) => {
                // Growable array value; starts empty, then any literal
                // elements are pushed (checked growth), matching the
                // runtime's array_new + array_push.
                let _ = writeln!(out, "{ind}SubArray {cname} = {{0}};");
                if let hir::ExprKind::ArrayLit(elems) = &init.kind {
                    let ect = self.ctype(elem)?;
                    for e in elems {
                        let v = self.emit_expr(e)?;
                        let _ = writeln!(
                            out,
                            "{ind}{{ {ect} _e = {v}; sub_arr_push(&{cname}, &_e, sizeof({ect})); }}"
                        );
                    }
                }
                Ok(())
            }
            Type::Class(id) if self.is_value_class(*id)? => {
                let cty = self.class_name(*id)?;
                let v = self.emit_expr(init)?;
                let _ = writeln!(out, "{ind}{cty} {cname} = {v};");
                Ok(())
            }
            _ => {
                let cty = self.ctype(ty)?;
                let v = self.emit_expr(init)?;
                let _ = writeln!(out, "{ind}{cty} {cname} = {v};");
                Ok(())
            }
        }
    }

    fn emit_fixed_array_elems(&mut self, elems: &[hir::Expr]) -> Result<String, String> {
        let mut parts = Vec::with_capacity(elems.len());
        for e in elems {
            parts.push(self.emit_expr(e)?);
        }
        Ok(parts.join(", "))
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
        let ind = Emitter::indent(depth);
        // §10.1: publish the counter's proven interval for the body's
        // FixedArray bounds-check decisions, then restore on exit.
        let proof = self.induction_interval(init, cond, step, body);
        let saved = proof
            .as_ref()
            .map(|(name, _)| (name.clone(), self.ranges.get(name).copied()));

        let init_c = match init {
            Some(hir::Stmt::Let { name, ty, init, .. }) => {
                let cname = sanitize(name);
                let cty = self.ctype(ty)?;
                let v = self.emit_expr(init)?;
                format!("{cty} {cname} = {v}")
            }
            Some(hir::Stmt::Expr(e)) => self.emit_expr(e)?,
            None => String::new(),
            Some(other) => {
                return Err(format!("for-init {other:?} is outside this spike's scope"))
            }
        };
        let cond_c = match cond {
            Some(c) => self.emit_expr(c)?,
            None => String::new(),
        };
        let step_c = match step {
            Some(s) => self.emit_expr(s)?,
            None => String::new(),
        };

        if let Some((name, iv)) = &proof {
            self.ranges.insert(name.clone(), *iv);
        }
        let _ = writeln!(out, "{ind}for ({init_c}; {cond_c}; {step_c}) {{");
        self.emit_block(out, body, depth + 1)?;
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

    fn emit_expr_stmt(
        &mut self,
        out: &mut String,
        e: &hir::Expr,
        depth: usize,
    ) -> Result<(), String> {
        use hir::ExprKind as K;
        let ind = Emitter::indent(depth);
        match &e.kind {
            K::Assign { op, target, value } => self.emit_assign(out, *op, target, value, depth),
            K::Call {
                callee: hir::Callee::Ambient(hir::AmbientFn::Print),
                args,
            } => self.emit_print(out, args, depth),
            K::Call {
                callee: hir::Callee::Method { recv, name },
                args,
            } if name == "push" => self.emit_push(out, recv, args, depth),
            _ => {
                let text = self.emit_expr(e)?;
                let _ = writeln!(out, "{ind}{text};");
                Ok(())
            }
        }
    }

    fn emit_push(
        &mut self,
        out: &mut String,
        recv: &hir::Expr,
        args: &[hir::Expr],
        depth: usize,
    ) -> Result<(), String> {
        let ind = Emitter::indent(depth);
        let elem = match &recv.ty {
            Type::Array(elem) => (**elem).clone(),
            other => return Err(format!("push on {other:?}")),
        };
        let ptr = self.emit_array_ptr(recv)?;
        let arg = args.first().ok_or_else(|| "push arity".to_string())?;
        let v = self.emit_expr(arg)?;
        let ect = self.ctype(&elem)?;
        let _ = writeln!(
            out,
            "{ind}{{ {ect} _e = {v}; sub_arr_push({ptr}, &_e, sizeof({ect})); }}"
        );
        Ok(())
    }

    fn emit_print(
        &mut self,
        out: &mut String,
        args: &[hir::Expr],
        depth: usize,
    ) -> Result<(), String> {
        let ind = Emitter::indent(depth);
        let arg = args.first().ok_or_else(|| "print arity".to_string())?;
        let _ = writeln!(out, "{ind}{{");
        let _ = writeln!(out, "{ind}    char _line[512]; _line[0] = 0;");
        match &arg.kind {
            hir::ExprKind::Template(parts) => {
                for part in parts {
                    self.emit_print_part(out, part, depth + 1)?;
                }
            }
            hir::ExprKind::Str(s) => {
                let _ = writeln!(out, "{ind}    strcat(_line, {});", c_string_literal(s));
            }
            _ => {
                // A non-template string expression: only a plain string
                // value is supported here.
                if arg.ty == Type::Str {
                    return Err(
                        "print of a computed string is outside this spike's scope".to_string(),
                    );
                }
                return Err("print expects a string / template argument".to_string());
            }
        }
        let _ = writeln!(out, "{ind}    sub_print(_line);");
        let _ = writeln!(out, "{ind}}}");
        Ok(())
    }

    fn emit_print_part(
        &mut self,
        out: &mut String,
        part: &hir::TplPart,
        depth: usize,
    ) -> Result<(), String> {
        let ind = Emitter::indent(depth);
        match part {
            hir::TplPart::Text(t) => {
                let _ = writeln!(out, "{ind}strcat(_line, {});", c_string_literal(t));
                Ok(())
            }
            hir::TplPart::Expr(e) => {
                let v = self.emit_expr(e)?;
                let (fmt, cast) = match &e.ty {
                    Type::F32 => ("sub_fmt_f32", "(float)"),
                    Type::F64 => ("sub_fmt_f64", "(double)"),
                    Type::I32 => ("sub_fmt_i32", "(int32_t)"),
                    Type::U32 => ("sub_fmt_u32", "(uint32_t)"),
                    Type::I64 => ("sub_fmt_i64", "(int64_t)"),
                    Type::U64 => ("sub_fmt_u64", "(uint64_t)"),
                    Type::Bool => ("sub_fmt_bool", "(int32_t)"),
                    Type::Str => {
                        return Err(
                            "string interpolation is outside this spike's scope".to_string()
                        )
                    }
                    other => return Err(format!("interpolation of {other:?}")),
                };
                let _ = writeln!(
                    out,
                    "{ind}{{ char _t[64]; {fmt}({cast}({v}), _t, sizeof _t); strcat(_line, _t); }}"
                );
                Ok(())
            }
            other => Err(format!("template part {other:?}")),
        }
    }

    /// Emits an assignment as a C statement, carrying C2 copy semantics.
    fn emit_assign(
        &mut self,
        out: &mut String,
        op: Option<hir::BinOp>,
        target: &hir::Expr,
        value: &hir::Expr,
        depth: usize,
    ) -> Result<(), String> {
        let ind = Emitter::indent(depth);
        // Dynamic-array element store: resolve the (bounds-checked)
        // address *after* the RHS (growth-safe, N3), and copy the value
        // into the element (C2 copy-on-assign).
        if let hir::ExprKind::Index { obj, index } = &target.kind {
            if let Type::Array(elem) = &obj.ty {
                if op.is_some() {
                    return Err(
                        "compound assignment to a dynamic-array element is outside this spike's scope"
                            .to_string(),
                    );
                }
                let ect = self.ctype(elem)?;
                let ptr = self.emit_array_ptr(obj)?;
                let idx = self.emit_expr(index)?;
                let v = self.emit_expr(value)?;
                let _ = writeln!(
                    out,
                    "{ind}{{ {ect} _t = {v}; *({ect}*)sub_arr_at({ptr}, {idx}, sizeof({ect})) = _t; }}"
                );
                return Ok(());
            }
        }
        // FixedArray-valued assignment (e.g. `this.elements = elements`)
        // is a whole-array copy (C cannot assign arrays): memcpy, which
        // is the C2 copy the field-store performs.
        let tty = &target.ty;
        if matches!(tty, Type::FixedArray(..)) {
            if op.is_some() {
                return Err("compound assignment to a FixedArray is not valid".to_string());
            }
            let lv = self.emit_place(target)?;
            let v = self.emit_expr(value)?;
            let _ = writeln!(out, "{ind}memcpy({lv}, {v}, sizeof({lv}));");
            return Ok(());
        }
        // Scalar or whole value-class assignment.
        let lv = self.emit_place(target)?;
        let opc = match op {
            None => "=",
            Some(b) => compound_op(b)?,
        };
        // Integer compound `%=`/`/=` would need the checked helper; a22
        // has none, so only the arithmetic/bitwise ops reach here.
        let v = self.emit_expr(value)?;
        let _ = writeln!(out, "{ind}{lv} {opc} {v};");
        Ok(())
    }

    /// A C lvalue for an assignable place (not a dynamic-array element,
    /// which `emit_assign` handles directly).
    fn emit_place(&mut self, e: &hir::Expr) -> Result<String, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Local(name) => Ok(sanitize(name)),
            K::Global(name) => Ok(format!("g_{}", sanitize(name))),
            K::Field { obj, name } => {
                let o = self.emit_expr(obj)?;
                Ok(format!("({o}).{}", sanitize(name)))
            }
            K::Index { obj, index } => match &obj.ty {
                Type::FixedArray(_, n) => {
                    let o = self.emit_expr(obj)?;
                    let idx = self.emit_expr(index)?;
                    if self.index_in_bounds(index, *n) {
                        Ok(format!("({o})[{idx}]"))
                    } else {
                        // Unproven: a checked element pointer (kept where
                        // CLIF keeps the check). Not exercised by a22.
                        let elem_ty = match &obj.ty {
                            Type::FixedArray(elem, _) => (**elem).clone(),
                            _ => unreachable!(),
                        };
                        let ect = self.ctype(&elem_ty)?;
                        Ok(format!(
                            "(*({ect}*)sub_fa_at((void*)({o}), {n}, {idx}, sizeof({ect}), 0))"
                        ))
                    }
                }
                other => Err(format!("assignment target index on {other:?}")),
            },
            other => Err(format!("assignment target {other:?}")),
        }
    }

    /// A `SubArray*` for a dynamic-array-typed operand (indexing, push,
    /// length). Locals that are parameters are already pointers; other
    /// locals and globals are addressed with `&`.
    fn emit_array_ptr(&self, e: &hir::Expr) -> Result<String, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Local(name) => {
                let cname = sanitize(name);
                if self.ptr_arrays.contains(&cname) {
                    Ok(cname)
                } else {
                    Ok(format!("&{cname}"))
                }
            }
            K::Global(name) => Ok(format!("&g_{}", sanitize(name))),
            other => Err(format!("dynamic array operand {other:?}")),
        }
    }

    // ----- expressions -----

    fn emit_expr(&mut self, e: &hir::Expr) -> Result<String, String> {
        use hir::ExprKind as K;
        match &e.kind {
            K::Int(v) => Ok(int_literal(*v, &e.ty)),
            K::Float(v) => Ok(float_literal(*v, &e.ty)),
            K::Bool(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
            K::Null => Err("null is outside this spike's scope".to_string()),
            K::This => {
                let n = self
                    .this_name
                    .ok_or_else(|| "`this` outside a constructor".to_string())?;
                Ok(n.to_string())
            }
            K::Local(name) => Ok(sanitize(name)),
            K::Global(name) => Ok(format!("g_{}", sanitize(name))),
            K::EnumMember { value, .. } => Ok(value.to_string()),
            K::Unary { op, operand } => {
                let v = self.emit_expr(operand)?;
                Ok(match op {
                    hir::UnOp::Neg => format!("(-({v}))"),
                    hir::UnOp::Not => format!("(!({v}))"),
                    hir::UnOp::BitNot => format!("(~({v}))"),
                    _ => return Err("unknown unary operator".to_string()),
                })
            }
            K::Binary { op, left, right } => self.emit_binary(*op, left, right),
            K::Assign { op, target, value } => {
                // Assignment used as an expression: only a simple place
                // (loop step `i += 1`, scalar update). Dynamic-array /
                // FixedArray-copy assigns are statement-only.
                let lv = self.emit_place(target)?;
                let opc = match op {
                    None => "=",
                    Some(b) => compound_op(*b)?,
                };
                let v = self.emit_expr(value)?;
                Ok(format!("({lv} {opc} {v})"))
            }
            K::Cast(inner) => {
                let v = self.emit_expr(inner)?;
                let ct = self.ctype(&e.ty)?;
                Ok(format!("(({ct})({v}))"))
            }
            K::Call { callee, args } => self.emit_call(callee, args, &e.ty),
            K::New { class, args } => self.emit_new(*class, args),
            K::Field { obj, name } => {
                let o = self.emit_expr(obj)?;
                Ok(format!("({o}).{}", sanitize(name)))
            }
            K::Length(obj) => match &obj.ty {
                Type::Array(_) => {
                    let ptr = self.emit_array_ptr(obj)?;
                    Ok(format!("sub_arr_len({ptr})"))
                }
                Type::FixedArray(_, n) => Ok(n.to_string()),
                other => Err(format!("length of {other:?}")),
            },
            K::Index { obj, index } => self.emit_index_read(obj, index),
            K::ArrayLit(elems) => match &e.ty {
                Type::FixedArray(elem, _) => {
                    let vals = self.emit_fixed_array_elems(elems)?;
                    Ok(format!("({}[]){{ {vals} }}", self.ctype(elem)?))
                }
                _ => Err("dynamic array literals are only supported in initializers".to_string()),
            },
            K::Template(_) => {
                Err("template literals are only supported as a print argument".to_string())
            }
            other => Err(format!("expression {other:?} is outside this spike's scope")),
        }
    }

    fn emit_index_read(
        &mut self,
        obj: &hir::Expr,
        index: &hir::Expr,
    ) -> Result<String, String> {
        match &obj.ty {
            Type::FixedArray(elem, n) => {
                let o = self.emit_expr(obj)?;
                let idx = self.emit_expr(index)?;
                if self.index_in_bounds(index, *n) {
                    // Proven in range: unchecked, exactly as CLIF elides.
                    Ok(format!("({o})[{idx}]"))
                } else {
                    // Unproven: keep a bounds check (as CLIF does).
                    let ect = self.ctype(elem)?;
                    Ok(format!(
                        "(*({ect}*)sub_fa_at((void*)({o}), {n}, {idx}, sizeof({ect}), 0))"
                    ))
                }
            }
            Type::Array(elem) => {
                let ect = self.ctype(elem)?;
                let ptr = self.emit_array_ptr(obj)?;
                let idx = self.emit_expr(index)?;
                Ok(format!(
                    "(*({ect}*)sub_arr_at({ptr}, {idx}, sizeof({ect})))"
                ))
            }
            other => Err(format!("index on {other:?}")),
        }
    }

    fn emit_binary(
        &mut self,
        op: hir::BinOp,
        left: &hir::Expr,
        right: &hir::Expr,
    ) -> Result<String, String> {
        use hir::BinOp as B;
        let operand_ty = if left.ty == Type::Null {
            right.ty.clone()
        } else {
            left.ty.clone()
        };
        if operand_ty == Type::Str {
            return Err("string operations are outside this spike's scope".to_string());
        }
        let l = self.emit_expr(left)?;
        let r = self.emit_expr(right)?;
        let float = operand_ty.is_float();
        // Integer div/rem trap on a zero divisor (and signed wrap for
        // MIN/-1), via the emitted checked helpers — as the CLIF path
        // emits explicit checks rather than trusting the hardware.
        match op {
            B::Div if !float => {
                let f = div_helper(&operand_ty, true)?;
                return Ok(format!("{f}({l}, {r})"));
            }
            B::Rem => {
                let f = div_helper(&operand_ty, false)?;
                return Ok(format!("{f}({l}, {r})"));
            }
            _ => {}
        }
        let sym = match op {
            B::Add => "+",
            B::Sub => "-",
            B::Mul => "*",
            B::Div => "/", // float only (integer handled above)
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
            B::UShr => ">>", // operands are unsigned C types → logical
            _ => return Err("unknown binary operator".to_string()),
        };
        Ok(format!("({l} {sym} {r})"))
    }

    fn emit_call(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        _ret: &Type,
    ) -> Result<String, String> {
        match callee {
            hir::Callee::Func(name) => {
                let f = self
                    .module
                    .functions
                    .iter()
                    .find(|f| &f.name == name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                if f.is_generator {
                    return Err("generator calls are outside this spike's scope".to_string());
                }
                let argv = self.emit_call_args(&f.params, args)?;
                Ok(format!("ss_{}({argv})", sanitize(name)))
            }
            hir::Callee::Method { recv, name } if name == "length" => {
                // `length` is a property, but if it ever appears as a
                // method call, treat it as one.
                let ptr = self.emit_array_ptr(recv)?;
                Ok(format!("sub_arr_len({ptr})"))
            }
            other => Err(format!("callee {other:?} is outside this spike's scope")),
        }
    }

    /// Emits call arguments, carrying C2 copy-on-pass: value-class
    /// arguments are passed by value (C copies the struct), dynamic
    /// arrays by their `SubArray*` handle, `FixedArray` as a decayed
    /// array, scalars directly.
    fn emit_call_args(
        &mut self,
        params: &[hir::Param],
        args: &[hir::Expr],
    ) -> Result<String, String> {
        let mut parts = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let a = args.get(i);
            let text = match a {
                Some(a) => match &p.ty {
                    Type::Array(_) => self.emit_array_ptr(a)?,
                    _ => self.emit_expr(a)?,
                },
                None => {
                    let d = p
                        .default
                        .as_ref()
                        .ok_or_else(|| format!("missing argument `{}`", p.name))?;
                    self.emit_expr(d)?
                }
            };
            parts.push(text);
        }
        Ok(parts.join(", "))
    }

    fn emit_new(&mut self, class: ClassId, args: &[hir::Expr]) -> Result<String, String> {
        if !self.is_value_class(class)? {
            return Err("reference classes are outside this spike's scope".to_string());
        }
        let cname = self.class_name(class)?;
        let ctor = self
            .module
            .classes
            .get(class.0)
            .and_then(|c| c.ctor.as_ref())
            .ok_or_else(|| "value class without a constructor".to_string())?;
        let argv = self.emit_call_args(&ctor.params, args)?;
        Ok(format!("{cname}_new({argv})"))
    }
}

// ----- literal / name helpers -----

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
    s
}

fn int_literal(v: i64, ty: &Type) -> String {
    match ty {
        Type::U32 => format!("{}u", v as u32),
        Type::U64 => format!("{}ull", v as u64),
        Type::I64 => format!("{v}ll"),
        // i32 / enum: plain decimal (i32::MIN handled below).
        _ => {
            if v == i64::from(i32::MIN) {
                // Avoid the unary-minus-of-2147483648 pitfall.
                "(-2147483647 - 1)".to_string()
            } else {
                v.to_string()
            }
        }
    }
}

fn float_literal(v: f64, ty: &Type) -> String {
    // For an f32 literal, round to f32 first (as the CLIF lowering does
    // with `*v as f32`) and print the shortest *f32* decimal, so the C
    // constant parses back to exactly that f32 with a single rounding —
    // no double-rounding through f64. For an f64 literal, print the
    // shortest f64 decimal directly.
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

fn c_string_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for b in s.bytes() {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(b as char),
            other => {
                let _ = write!(out, "\\x{other:02x}");
            }
        }
    }
    out.push('"');
    out
}

fn compound_op(op: hir::BinOp) -> Result<&'static str, String> {
    use hir::BinOp as B;
    Ok(match op {
        B::Add => "+=",
        B::Sub => "-=",
        B::Mul => "*=",
        B::BitAnd => "&=",
        B::BitOr => "|=",
        B::BitXor => "^=",
        B::Shl => "<<=",
        B::Shr | B::UShr => ">>=",
        // `/=` and `%=` would need the checked helper; not exercised.
        other => return Err(format!("compound operator {other:?}")),
    })
}

/// The checked integer div/rem helper name for an operand type.
fn div_helper(ty: &Type, is_div: bool) -> Result<&'static str, String> {
    Ok(match (ty, is_div) {
        (Type::I32, true) => "sub_sdiv_i32",
        (Type::I32, false) => "sub_srem_i32",
        (Type::U32, true) => "sub_udiv_u32",
        (Type::U32, false) => "sub_urem_u32",
        (Type::I64, true) => "sub_sdiv_i64",
        (Type::I64, false) => "sub_srem_i64",
        (Type::U64, true) => "sub_udiv_u64",
        (Type::U64, false) => "sub_urem_u64",
        (other, _) => return Err(format!("integer div/rem on {other:?}")),
    })
}

/// The fixed prelude: includes, the checked growable array, the trap
/// stub, the div/rem helpers, and the Q14 formatters.
const PREAMBLE: &str = r#"/* Generated by subscript's C emitter (P4.2 measurement spike).
 * Do not edit; this is emitted from the typed HIR of a corpus entry.
 * It carries the language's semantics (C2 value-class copies, checked
 * growable-array indexing and push growth, f32-precision arithmetic,
 * Q14 formatting) so that measuring it answers what emitted C through
 * clang does with this workload. Self-contained: no runtime link. */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* Trap kinds (a22 never traps; the stub keeps behaviour defined). */
enum { SUB_TRAP_OOB = 1, SUB_TRAP_DIV0 = 2, SUB_TRAP_OOM = 3 };

static void sub_trap(int kind, int pos) {
    fprintf(stderr, "trap %d %d\n", kind, pos);
    exit(1);
}

/* Dynamic array (T[]): a checked growable (data, len, cap), mirroring
 * the runtime's array header, element pointer, and push growth. */
typedef struct { unsigned char* data; int64_t len; int64_t cap; } SubArray;

static void* sub_arr_at(SubArray* a, int32_t idx, int64_t elem) {
    /* Unsigned compare rejects negative and >= len at once, as the
     * runtime's array_elem_ptr does. */
    if ((uint32_t)idx >= (uint32_t)a->len) sub_trap(SUB_TRAP_OOB, 0);
    return a->data + (int64_t)idx * elem;
}

static void sub_arr_push(SubArray* a, const void* src, int64_t elem) {
    if (a->len == a->cap) {
        int64_t nc = a->cap == 0 ? 4 : a->cap * 2;
        unsigned char* nd = (unsigned char*)malloc((size_t)(nc * elem));
        if (nd == NULL) sub_trap(SUB_TRAP_OOM, 0);
        if (a->data != NULL) {
            memcpy(nd, a->data, (size_t)(a->len * elem));
            free(a->data);
        }
        a->data = nd;
        a->cap = nc;
    }
    memcpy(a->data + a->len * elem, src, (size_t)elem);
    a->len += 1;
}

static int32_t sub_arr_len(SubArray* a) { return (int32_t)a->len; }

/* Checked FixedArray element pointer, kept where a bounds check cannot
 * be proven dead (a22 proves them all, so this is never called there). */
static void* sub_fa_at(void* base, int64_t n, int32_t idx, int64_t elem, int32_t pos) {
    if ((uint32_t)idx >= (uint32_t)n) sub_trap(SUB_TRAP_OOB, pos);
    return (unsigned char*)base + (int64_t)idx * elem;
}

/* Integer div/rem with the language's semantics: trap on a zero
 * divisor; two's-complement wrap for signed MIN / -1 and MIN % -1. */
static int32_t sub_sdiv_i32(int32_t a, int32_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    if (b == -1) return (int32_t)(0u - (uint32_t)a);
    return a / b;
}
static int32_t sub_srem_i32(int32_t a, int32_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    if (b == -1) return 0;
    return a % b;
}
static uint32_t sub_udiv_u32(uint32_t a, uint32_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    return a / b;
}
static uint32_t sub_urem_u32(uint32_t a, uint32_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    return a % b;
}
static int64_t sub_sdiv_i64(int64_t a, int64_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    if (b == -1) return (int64_t)(0ull - (uint64_t)a);
    return a / b;
}
static int64_t sub_srem_i64(int64_t a, int64_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    if (b == -1) return 0;
    return a % b;
}
static uint64_t sub_udiv_u64(uint64_t a, uint64_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    return a / b;
}
static uint64_t sub_urem_u64(uint64_t a, uint64_t b) {
    if (b == 0) sub_trap(SUB_TRAP_DIV0, 0);
    return a % b;
}

/* Q14 shortest-round-trip formatting, matching the runtime: the
 * shortest %g precision that round-trips; integral values print with no
 * decimal point or exponent; specials spelled -0/NaN/Infinity. */
static void sub_fmt_f32(float value, char* buf, size_t size) {
    if (value != value) { snprintf(buf, size, "NaN"); return; }
    if (value > 3.0e38f && value * 0.5f == value) { snprintf(buf, size, "Infinity"); return; }
    if (value < -3.0e38f && value * 0.5f == value) { snprintf(buf, size, "-Infinity"); return; }
    for (int p = 1; p <= 9; p += 1) {
        snprintf(buf, size, "%.*g", p, (double)value);
        if (strtof(buf, NULL) == value) return;
    }
    snprintf(buf, size, "%.9g", (double)value);
}
static void sub_fmt_f64(double value, char* buf, size_t size) {
    if (value != value) { snprintf(buf, size, "NaN"); return; }
    if (value > 1.0e308 && value * 0.5 == value) { snprintf(buf, size, "Infinity"); return; }
    if (value < -1.0e308 && value * 0.5 == value) { snprintf(buf, size, "-Infinity"); return; }
    for (int p = 1; p <= 17; p += 1) {
        snprintf(buf, size, "%.*g", p, value);
        if (strtod(buf, NULL) == value) return;
    }
    snprintf(buf, size, "%.17g", value);
}
static void sub_fmt_i32(int32_t v, char* buf, size_t size) { snprintf(buf, size, "%d", v); }
static void sub_fmt_u32(uint32_t v, char* buf, size_t size) { snprintf(buf, size, "%u", v); }
static void sub_fmt_i64(int64_t v, char* buf, size_t size) { snprintf(buf, size, "%lld", (long long)v); }
static void sub_fmt_u64(uint64_t v, char* buf, size_t size) { snprintf(buf, size, "%llu", (unsigned long long)v); }
static void sub_fmt_bool(int32_t v, char* buf, size_t size) { snprintf(buf, size, "%s", v ? "true" : "false"); }

/* Output sink: print appends bytes plus a newline, exactly like the
 * runtime's print_line; the harness flushes it after the timed span. */
static unsigned char g_sink[1 << 16];
static size_t g_sink_len;
static void sub_print(const char* s) {
    size_t n = strlen(s);
    if (g_sink_len + n + 1 <= sizeof g_sink) {
        memcpy(g_sink + g_sink_len, s, n);
        g_sink_len += n;
        g_sink[g_sink_len++] = '\n';
    }
}

"#;

/// The harness `main`: same protocol as `bench/a22-baseline.c` /
/// `bench/aot-entry.c` (argv = warmup timed; `sample`/`checksum-stable`
/// on stderr; output bytes on stdout). The timed span is the whole
/// `ss_main` call, matching the AOT subject; globals are reset and the
/// sink cleared before each run, outside the timed span.
const HARNESS_MAIN: &str = r#"
static uint64_t sub_monotonic_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

int main(int argc, char** argv) {
    int warmup = 3;
    int timed = 11;
    if (argc >= 3) {
        warmup = atoi(argv[1]);
        timed = atoi(argv[2]);
    }
    if (warmup < 0 || timed < 1) {
        fprintf(stderr, "usage: a22-cemit <warmup-runs> <timed-runs>\n");
        return 2;
    }

    unsigned char first[1 << 16];
    size_t first_len = 0;
    int have_first = 0;
    int stable = 1;

    for (int run = 0; run < warmup + timed; run += 1) {
        ss_init();
        g_sink_len = 0;

        const uint64_t start = sub_monotonic_ns();
        ss_main();
        const uint64_t end = sub_monotonic_ns();

        if (!have_first) {
            first_len = g_sink_len;
            memcpy(first, g_sink, g_sink_len);
            have_first = 1;
        } else if (g_sink_len != first_len || memcmp(g_sink, first, first_len) != 0) {
            stable = 0;
        }

        if (run >= warmup) {
            fprintf(stderr, "sample %d %llu\n", run - warmup, (unsigned long long)(end - start));
        }
    }

    fprintf(stderr, "checksum-stable %d\n", stable);
    if (first_len > 0) {
        fwrite(first, 1, first_len, stdout);
    }
    fflush(stdout);
    return 0;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    fn module_of(src: &str) -> hir::Module {
        check_program(&[SourceFile::new("t.ts", src)]).expect("clean check")
    }

    #[test]
    fn emits_a_translation_unit_for_a_minimal_program() {
        let m = module_of(
            "export function main(): void {\n  const x: f32 = 1.5;\n  print(`${x}`);\n}\n",
        );
        let c = emit_c(&m).expect("emit");
        assert!(c.contains("int main(int argc"));
        assert!(c.contains("static void ss_main(void)"));
        assert!(c.contains("1.5f"));
        assert!(c.contains("sub_fmt_f32"));
    }

    #[test]
    fn value_class_becomes_a_struct_with_a_by_value_constructor() {
        let m = module_of(
            "@value\nclass V { x: f32; y: f32;\n constructor(x: f32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void {\n  const v: V = new V(1.0, 2.0);\n  print(`${v.x}`);\n}\n",
        );
        let c = emit_c(&m).expect("emit");
        assert!(c.contains("typedef struct Sub_V"));
        assert!(c.contains("static Sub_V Sub_V_new("));
        assert!(c.contains("Sub_V_new("));
    }

    #[test]
    fn fixed_array_index_proven_in_range_is_unchecked() {
        // Every index is a proven induction variable, so no sub_fa_at
        // call is emitted (the check is elided, as CLIF elides it).
        let m = module_of(
            "export function main(): void {\n  const xs: FixedArray<i32, 4> = [10, 20, 30, 40];\n  let sum: i32 = 0;\n  for (let i: i32 = 0; i < 4; i += 1) {\n    sum += xs[i];\n  }\n  print(`${sum}`);\n}\n",
        );
        let c = emit_c(&m).expect("emit");
        // `sub_fa_at` is defined in the preamble; a proven index must
        // never *call* it (the call form takes a `(void*)` cast base).
        assert!(
            !c.contains("sub_fa_at((void*)"),
            "proven index must not be checked"
        );
        assert!(c.contains("(xs)[i]"));
    }

    #[test]
    fn dynamic_array_index_is_checked() {
        let m = module_of(
            "export function main(): void {\n  const xs: i32[] = [];\n  xs.push(7);\n  print(`${xs[0]}`);\n}\n",
        );
        let c = emit_c(&m).expect("emit");
        assert!(c.contains("sub_arr_at"), "dynamic index must be checked");
        assert!(c.contains("sub_arr_push"));
    }

    #[test]
    fn reference_class_is_rejected_as_out_of_scope() {
        let m = module_of(
            "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {\n  const c: C = new C();\n  print(`${c.x}`);\n}\n",
        );
        assert!(emit_c(&m).is_err());
    }
}
