//! Whole-program capture and parameter escape facts (compiler.md §118).
use crate::check::rejection::{diagnostic, RejectionSite};
use crate::hir::{self, ArrFn, Callee, Expr, ExprKind as E, HirChild, MapFn, SetFn, Stmt};
use crate::{Diagnostic, Type};
use std::collections::{HashMap, HashSet};

type Env = super::Shared<HashMap<String, usize>>;
struct Analysis<'a> {
    module: &'a hir::Module,
    facts: Vec<bool>,
    bindings: HashMap<usize, usize>,
    equations: Vec<(usize, &'a Expr)>,
    escapes: Vec<(String, &'a Expr)>,
    parameters: Vec<(usize, &'a hir::Param)>,
    calls: Vec<(String, &'a hir::Param, &'a Expr)>,
    escaping: HashSet<usize>,
    capture_bindings: HashMap<usize, Vec<usize>>,
    infer: bool,
}
impl<'a> Analysis<'a> {
    fn carries(&self, t: &Type) -> bool {
        self.module.carries_capture(t)
    }
    fn bind(&mut self, env: &mut Env, name: &str, initial: bool) -> usize {
        let id = self.facts.len();
        self.facts.push(initial);
        env.insert(name.into(), id);
        id
    }
    fn params(&mut self, params: &'a [hir::Param], env: &mut Env) {
        for p in params {
            let id = self.bind(env, &p.name, self.carries(&p.ty));
            self.parameters.push((id, p));
        }
        for p in params {
            if let Some(default) = &p.default {
                self.expr(default, env);
                if let Some(id) = env.get(&p.name) {
                    self.equations.push((*id, default));
                }
            }
        }
    }
    fn sequence(&mut self, body: &'a [Stmt], env: &mut Env) {
        for s in body {
            self.stmt(s, env);
        }
    }
    fn stmt(&mut self, s: &'a Stmt, env: &mut Env) {
        match s {
            Stmt::Let { name, init, .. } => {
                self.expr(init, env);
                let id = self.bind(env, name, false);
                self.equations.push((id, init));
            }
            Stmt::Return { value: Some(v), .. } => {
                self.escapes.push(("return".to_owned(), v));
                self.expr(v, env);
            }
            Stmt::Block(body) | Stmt::Using { body, .. } => self.sequence(body, &mut env.clone()),
            Stmt::If {
                cond, then, els, ..
            } => {
                self.expr(cond, env);
                self.sequence(then, &mut env.clone());
                if let Some(body) = els {
                    self.sequence(body, &mut env.clone());
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond, env);
                self.sequence(body, &mut env.clone());
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let mut scope = env.clone();
                if let Some(s) = init {
                    self.stmt(s, &mut scope);
                }
                if let Some(e) = cond {
                    self.expr(e, &scope);
                }
                self.sequence(body, &mut scope.clone());
                if let Some(e) = step {
                    self.expr(e, &scope);
                }
            }
            Stmt::ForOf {
                name,
                subject,
                ty,
                body,
                ..
            } => {
                self.expr(subject, env);
                let mut scope = env.clone();
                let id = self.bind(&mut scope, name, false);
                if self.carries(ty) {
                    self.equations.push((id, subject));
                }
                self.sequence(body, &mut scope);
            }
            Stmt::Switch { disc, cases, .. } => {
                self.expr(disc, env);
                let mut scope = env.clone();
                for c in cases {
                    if let Some(t) = &c.test {
                        self.expr(t, &scope);
                    }
                    self.sequence(&c.body, &mut scope);
                }
            }
            Stmt::Try {
                body,
                binding,
                handler,
                ..
            } => {
                self.sequence(body, &mut env.clone());
                let mut scope = env.clone();
                if let Some((name, ty)) = binding {
                    self.bind(&mut scope, name, self.carries(ty));
                }
                self.sequence(handler, &mut scope);
            }
            _ => {
                for child in s.children() {
                    self.child(child, env);
                }
            }
        }
    }
    fn child(&mut self, child: HirChild<'a>, env: &Env) {
        match child {
            HirChild::Expr(e) => self.expr(e, env),
            HirChild::Stmt(s) => self.stmt(s, &mut env.clone()),
        }
    }
    fn expr(&mut self, e: &'a Expr, env: &Env) {
        match &e.kind {
            E::Local(name, _, _) => {
                if let Some(id) = env.get(name) {
                    self.bindings.insert(e as *const Expr as usize, *id);
                }
            }
            E::Lambda {
                params,
                body,
                captures,
                ..
            } => {
                self.capture_bindings.insert(
                    e as *const Expr as usize,
                    captures
                        .iter()
                        .filter_map(|c| env.get(&c.name).copied())
                        .collect(),
                );
                let mut scope = env.clone();
                self.params(params, &mut scope);
                self.sequence(body, &mut scope);
                return;
            }
            E::Assign { target, value, .. } => match &target.kind {
                E::Local(name, _, _) => {
                    if let Some(id) = env.get(name) {
                        self.equations.push((*id, value));
                    }
                }
                E::Global(_) => self.escapes.push(("global store".to_owned(), value)),
                E::Field { .. } => self.escapes.push(("field store".to_owned(), value)),
                E::Index { .. } => self.escapes.push(("element store".to_owned(), value)),
                _ => {}
            },
            E::Yield(Some(v)) => self.escapes.push(("yield".to_owned(), v)),
            E::ArrayLit(values) => {
                for v in values {
                    self.escapes.push(("array literal".to_owned(), v));
                }
            }
            E::ArraySpreadLit(values) => {
                for v in values {
                    self.escapes.push(("array literal".to_owned(), &v.expr));
                }
            }
            E::DescriptorLit { fields, .. } => {
                for v in fields.iter().flatten() {
                    self.escapes.push(("field literal".to_owned(), v));
                }
            }
            E::New { class, args } if self.module.classes[class.0].is_boundary => {
                for v in args {
                    self.escapes.push(("aggregate constructor".to_owned(), v));
                }
            }
            E::AsyncHandleCreate { args, .. } => {
                for v in args {
                    self.escapes.push(("held async argument".to_owned(), v));
                }
            }
            E::Call { callee, args } => match callee {
                Callee::Foreign(_) => {
                    for v in args {
                        self.escapes.push(("C callback".to_owned(), v));
                    }
                }
                Callee::Arr(ArrFn::Fill | ArrFn::Unshift) => {
                    if let Some(v) = args.get(1) {
                        self.escapes.push(("array store".to_owned(), v));
                    }
                }
                Callee::Map(MapFn::Set) => {
                    for v in args.iter().skip(1) {
                        self.escapes.push(("Map store".to_owned(), v));
                    }
                }
                Callee::Set(SetFn::Add) => {
                    if let Some(v) = args.get(1) {
                        self.escapes.push(("Set store".to_owned(), v));
                    }
                }
                Callee::Method { recv, name }
                    if matches!(recv.ty, Type::Array(_)) && name.full_text() == "push" =>
                {
                    for v in args {
                        self.escapes.push(("array push".to_owned(), v));
                    }
                }
                _ => {}
            },
            _ => {}
        }
        match &e.kind {
            E::Call { callee, args } => {
                if let Some(f) = self.function(callee) {
                    self.call(
                        &match callee {
                            Callee::Method { recv, name } => {
                                format!(
                                    "{}.{}",
                                    type_name(self.module, &recv.ty),
                                    name.source_name()
                                )
                            }
                            _ => super::identity::module_declaration_label(self.module, &f.symbol),
                        },
                        &f.params,
                        args,
                    );
                } else if let Callee::Value(value) = callee {
                    for (index, arg) in args.iter().enumerate() {
                        self.escapes.push((
                            format!(
                                "indirect call argument to {} parameter #{}",
                                value_name(self.module, value),
                                index + 1
                            ),
                            arg,
                        ));
                    }
                }
            }
            E::New { class, args } => {
                if let Some(f) = &self.module.classes[class.0].ctor {
                    self.call(
                        &format!("{} constructor", self.module.classes[class.0].name),
                        &f.params,
                        args,
                    );
                }
            }
            E::AsyncCall { callee, args } | E::AsyncHandleCreate { callee, args, .. } => {
                let f = match callee {
                    hir::AsyncCallee::Function(name) => {
                        self.module.functions.iter().find(|f| &f.symbol == name)
                    }
                    hir::AsyncCallee::Method { class, name, .. } => {
                        self.module.method(&Type::Class(*class), name)
                    }
                };
                if let Some(f) = f {
                    self.call(
                        &match callee {
                            hir::AsyncCallee::Method { class, name, .. } => {
                                super::identity::class_member_label(
                                    &self.module.classes,
                                    &self.module.classes[class.0],
                                    name.full_text(),
                                )
                            }
                            _ => super::identity::module_declaration_label(self.module, &f.symbol),
                        },
                        &f.params,
                        args,
                    );
                }
            }
            _ => {}
        }
        for child in e.children() {
            self.child(child, env);
        }
    }
    fn function(&self, callee: &Callee) -> Option<&'a hir::Function> {
        match callee {
            Callee::Func(name) => self.module.functions.iter().find(|f| &f.symbol == name),
            Callee::Method { recv, name } => self.module.method(&recv.ty, name),
            _ => None,
        }
    }
    fn call(&mut self, name: &str, params: &'a [hir::Param], args: &'a [Expr]) {
        for (p, arg) in params.iter().zip(args) {
            if self.carries(&p.ty) {
                self.calls.push((name.to_owned(), p, arg));
            }
        }
    }
    // Join all assignments before any boundary reads a local fact.
    fn solve(&mut self) {
        loop {
            let mut changed = false;
            for (id, e) in &self.equations {
                if !self.facts[*id] && self.fact(e) {
                    self.facts[*id] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    // Trace each parameter independently, then close call edges to a fixed point.
    fn infer_parameters(&mut self) {
        self.infer = true;
        loop {
            let mut changed = false;
            for index in 0..self.parameters.len() {
                let (id, p) = self.parameters[index];
                let key = p as *const hir::Param as usize;
                if self.escaping.contains(&key) || !self.carries(&p.ty) {
                    continue;
                }
                self.facts.fill(false);
                self.facts[id] = true;
                self.solve();
                if self.escapes.iter().any(|(_, e)| self.fact(e))
                    || self.calls.iter().any(|(_, p, e)| {
                        self.escaping.contains(&(*p as *const hir::Param as usize)) && self.fact(e)
                    })
                {
                    self.escaping.insert(key);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        self.infer = false;
        self.facts.fill(false);
        for (id, p) in &self.parameters {
            self.facts[*id] =
                self.carries(&p.ty) && !self.escaping.contains(&(*p as *const hir::Param as usize));
        }
    }
    fn generator(&self, callee: &Callee) -> bool {
        self.function(callee).is_some_and(|f| f.is_generator)
    }
    fn fact(&self, e: &Expr) -> bool {
        if !self.carries(&e.ty) {
            return false;
        }
        match &e.kind {
            E::New { args, .. } => args.iter().any(|e| self.fact(e)),
            E::Null | E::Zero | E::FuncRef(_) | E::Global(_) | E::This | E::AsyncHandleAwait(_) => {
                false
            }
            E::Local(..) => self
                .bindings
                .get(&(e as *const Expr as usize))
                .map_or(!self.infer, |id| self.facts[*id]),
            E::Lambda { captures, .. } => {
                if self.infer {
                    self.capture_bindings
                        .get(&(e as *const Expr as usize))
                        .is_some_and(|ids| ids.iter().any(|id| self.facts[*id]))
                } else {
                    !captures.is_empty()
                }
            }
            E::Field { obj, .. } if matches!(obj.ty, Type::IterResult(_)) => self.fact(obj),
            E::Field { .. } => false,
            E::Index { obj, .. } => self.fact(obj),
            E::Cast(v) | E::Assign { value: v, .. } => self.fact(v),
            E::Cond { then, els, .. } => self.fact(then) || self.fact(els),
            E::ArrayLit(v) => v.iter().any(|e| self.fact(e)),
            E::ArraySpreadLit(v) => v.iter().any(|e| self.fact(&e.expr)),
            E::DescriptorLit { fields, .. } => fields.iter().flatten().any(|e| self.fact(e)),
            E::Call {
                callee: Callee::Method { recv, .. },
                ..
            } if !matches!(recv.ty, Type::Class(_)) => self.fact(recv),
            E::Call { callee, args } if self.generator(callee) => {
                args.iter().any(|e| self.fact(e))
                    || match callee {
                        Callee::Value(v) => self.fact(v),
                        Callee::Method { recv, .. } => self.fact(recv),
                        _ => false,
                    }
            }
            E::Call {
                callee: Callee::Map(MapFn::GetOr),
                args,
            } => {
                args.first().is_some_and(|arg| self.fact(arg))
                    || args.get(2).is_some_and(|arg| self.fact(arg))
            }
            E::Call {
                callee: Callee::Arr(_) | Callee::Map(_) | Callee::Set(_),
                args,
            } => args.first().is_some_and(|arg| self.fact(arg)),
            E::Call { .. } | E::AsyncCall { .. } => false,
            _ => !self.infer,
        }
    }
    fn finish(mut self) -> (Vec<Diagnostic>, HashSet<usize>) {
        self.infer_parameters();
        self.solve();
        let mut diagnostics = Vec::new();
        for (kind, e) in &self.escapes {
            if self.fact(e) {
                let d = diagnostic(
                    RejectionSite::CaptureEffectEscapes,
                    format!("{} may capture at {kind}", value_name(self.module, e)),
                    e.pos.clone(),
                );
                diagnostics.push(d);
            }
        }
        for (callee, p, e) in &self.calls {
            if self.escaping.contains(&(*p as *const hir::Param as usize)) && self.fact(e) {
                let d = diagnostic(
                    RejectionSite::CaptureEffectArgument,
                    format!(
                        "call `{callee}` parameter `{}` requires a clean argument; {} may capture",
                        p.name,
                        value_name(self.module, e)
                    ),
                    e.pos.clone(),
                );
                diagnostics.push(d);
            }
        }
        (diagnostics, self.escaping)
    }
}
pub(super) fn check(module: &mut hir::Module) -> Result<(), Vec<Diagnostic>> {
    let mut a = Analysis {
        module,
        facts: vec![],
        bindings: HashMap::new(),
        equations: vec![],
        escapes: vec![],
        parameters: vec![],
        calls: vec![],
        escaping: HashSet::new(),
        capture_bindings: HashMap::new(),
        infer: false,
    };
    for e in module.globals.iter().map(|g| &g.init).chain(
        module
            .classes
            .iter()
            .flat_map(|c| c.fields.iter().filter_map(|f| f.init.as_ref())),
    ) {
        a.expr(e, &Env::default());
        a.escapes.push(("initializer".to_owned(), e));
    }
    for owner in module.expression_owners() {
        let mut env = Env::default();
        match owner {
            hir::ExpressionOwner::Expr(_) => {}
            hir::ExpressionOwner::Body {
                statements,
                function,
            } => {
                if let Some(f) = function {
                    a.params(&f.params, &mut env);
                }
                a.sequence(statements, &mut env);
            }
        }
    }
    let (diagnostics, escaping) = a.finish();
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    for function in &mut module.functions {
        record_parameters(&mut function.params, &escaping);
    }
    for class in &mut module.classes {
        if let Some(ctor) = &mut class.ctor {
            record_parameters(&mut ctor.params, &escaping);
        }
        for method in &mut class.methods {
            record_parameters(&mut method.params, &escaping);
        }
    }
    for owner in module.expression_owners_mut() {
        match owner {
            hir::ExpressionOwnerMut::Expr(e) => record_child(hir::HirChildMut::Expr(e), &escaping),
            hir::ExpressionOwnerMut::Body(body) => {
                for stmt in body {
                    record_child(hir::HirChildMut::Stmt(stmt), &escaping);
                }
            }
        }
    }
    Ok(())
}

fn type_name(m: &hir::Module, t: &Type) -> String {
    match t {
        Type::Class(id) => m.classes[id.0].name.clone(),
        Type::Array(t) => {
            let name = type_name(m, t);
            if matches!(
                t.as_ref(),
                Type::Nullable(_) | Type::GenericUnion(_) | Type::Func(_)
            ) {
                format!("({name})[]")
            } else {
                format!("{name}[]")
            }
        }
        Type::FixedArray(t, n) => format!("FixedArray<{}, {n}>", type_name(m, t)),
        Type::Nullable(t) if matches!(t.as_ref(), Type::Func(_)) => {
            format!("({}) | null", type_name(m, t))
        }
        Type::Nullable(t) => format!("{} | null", type_name(m, t)),
        Type::Generator(t) => format!("Generator<{}>", type_name(m, t)),
        Type::Map(k, v) => format!("Map<{}, {}>", type_name(m, k), type_name(m, v)),
        Type::Set(t) => format!("Set<{}>", type_name(m, t)),
        _ => t.to_string(),
    }
}

// Parameter addresses identify declarations while the completed module stays in place.
fn record_parameters(params: &mut [hir::Param], escaping: &HashSet<usize>) {
    for param in params {
        param.escapes = escaping.contains(&(param as *const hir::Param as usize));
    }
}
fn record_child(child: hir::HirChildMut<'_>, escaping: &HashSet<usize>) {
    match child {
        hir::HirChildMut::Expr(expr) => {
            if let E::Lambda { params, .. } = &mut expr.kind {
                record_parameters(params, escaping);
            }
            for child in expr.children_mut() {
                record_child(child, escaping);
            }
        }
        hir::HirChildMut::Stmt(stmt) => {
            for child in stmt.children_mut() {
                record_child(child, escaping);
            }
        }
    }
}

fn value_name(module: &hir::Module, expr: &Expr) -> String {
    match &expr.kind {
        E::Local(name, _, _) => format!("value `{name}`"),
        E::Global(symbol) => format!(
            "value `{}`",
            super::identity::module_declaration_label(module, symbol)
        ),
        E::Lambda { captures, .. } => format!(
            "lambda capturing {}",
            captures
                .iter()
                .map(|c| format!("`{}`", c.name))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        E::Call {
            callee: Callee::Func(name),
            ..
        } => format!(
            "result of `{}`",
            super::identity::module_declaration_label(module, name)
        ),
        _ => format!("value of type `{}`", type_name(module, &expr.ty)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_names_preserve_element_precedence() {
        let module = crate::check_program(&[crate::SourceFile::new(
            "names.ts",
            "class Box { v: i32 = 1; }",
        )])
        .unwrap();
        let class = Type::Class(crate::types::ClassId(
            module
                .classes
                .iter()
                .position(|class| class.name == "Box")
                .unwrap(),
        ));
        for (element, expected) in [
            (Type::Nullable(Box::new(class.clone())), "(Box | null)[]"),
            (
                Type::Func(Box::new(crate::types::FuncType {
                    params: vec![Type::I32],
                    ret: Type::I32,
                })),
                "((i32) => i32)[]",
            ),
            (
                Type::GenericUnion(vec![Type::I32, Type::Bool].into_boxed_slice()),
                "(i32 | boolean)[]",
            ),
        ] {
            assert_eq!(type_name(&module, &Type::array(element)), expected);
        }
    }
}
