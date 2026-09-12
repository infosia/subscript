//! The address-taken analysis that decides which bindings need frame storage.

use super::*;

struct AddressTaken<'a> {
    module: &'a hir::Module,
    classes: &'a [l::Class],
    scopes: Vec<HashMap<String, BindingSite>>,
    taken: HashSet<BindingSite>,
}

pub(super) fn address_taken_bindings(
    module: &hir::Module,
    classes: &[l::Class],
    function: &FunctionInput,
    captures: &[hir::Capture],
) -> HashSet<BindingSite> {
    let mut analysis = AddressTaken {
        module,
        classes,
        scopes: vec![HashMap::new()],
        taken: HashSet::new(),
    };
    for capture in captures {
        analysis.declare(&capture.name, &function.pos);
    }
    for parameter in &function.params {
        analysis.declare(&parameter.name, &parameter.pos);
    }
    analysis.statements(&function.body);
    analysis.taken
}

impl AddressTaken<'_> {
    fn declare(&mut self, name: &str, pos: &Pos) {
        self.scopes
            .last_mut()
            .expect("one address-analysis scope")
            .insert(name.to_string(), BindingSite::new(name, pos));
    }

    fn mark(&mut self, name: &str) {
        if let Some(site) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
        {
            self.taken.insert(site);
        }
    }

    fn scoped(&mut self, statements: &[hir::Stmt]) {
        self.scopes.push(HashMap::new());
        self.statements(statements);
        self.scopes.pop();
    }

    fn statements(&mut self, statements: &[hir::Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &hir::Stmt) {
        match statement {
            hir::Stmt::Let {
                name, init, pos, ..
            } => {
                self.expr(init);
                self.declare(name, pos);
            }
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                self.expr(cond);
                self.scoped(then);
                if let Some(els) = els {
                    self.scoped(els);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.scoped(body);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                self.scopes.push(HashMap::new());
                if let Some(init) = init {
                    self.statement(init);
                }
                if let Some(cond) = cond {
                    self.expr(cond);
                }
                self.scoped(body);
                if let Some(step) = step {
                    self.expr(step);
                }
                self.scopes.pop();
            }
            hir::Stmt::ForOf {
                name,
                subject,
                body,
                pos,
                ..
            } => {
                self.expr(subject);
                self.scopes.push(HashMap::new());
                self.declare(name, pos);
                self.scoped(body);
                self.scopes.pop();
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                self.expr(disc);
                for case in cases {
                    if let Some(test) = &case.test {
                        self.expr(test);
                    }
                    self.scoped(&case.body);
                }
            }
            hir::Stmt::Block(statements) => self.scoped(statements),
            _ => {
                for child in statement.children() {
                    match child {
                        hir::HirChild::Expr(expr) => self.expr(expr),
                        hir::HirChild::Stmt(statement) => self.statement(statement),
                    }
                }
            }
        }
    }

    fn expr(&mut self, expr: &hir::Expr) {
        use hir::ExprKind as K;
        match &expr.kind {
            K::Assign { target, value, .. } => {
                match target.kind {
                    K::Local(_) | K::Global(_) => {}
                    _ => self.place(target),
                }
                self.expr(value);
                return;
            }
            K::Call { callee, args } => {
                match callee {
                    hir::Callee::Value(value) => self.expr(value),
                    hir::Callee::Method { recv, .. } if is_value_class(self.module, &recv.ty) => {
                        self.place(recv);
                    }
                    hir::Callee::Method { recv, .. } => self.expr(recv),
                    _ => {}
                }
                let parameter_types = match callee {
                    hir::Callee::Func(name) => self
                        .module
                        .functions
                        .iter()
                        .find(|function| function.name == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    hir::Callee::Foreign(name) => self
                        .module
                        .foreign_fns
                        .iter()
                        .find(|function| function.name == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    hir::Callee::Value(value) => match &value.ty {
                        Type::Func(signature) => Some(signature.params.clone()),
                        _ => None,
                    },
                    hir::Callee::Method { recv, name } => match recv.ty {
                        Type::Class(class) => self
                            .module
                            .classes
                            .get(class.0)
                            .and_then(|definition| {
                                definition
                                    .methods
                                    .iter()
                                    .find(|method| method.name == *name)
                            })
                            .map(|method| {
                                method
                                    .params
                                    .iter()
                                    .map(|parameter| parameter.ty.clone())
                                    .collect::<Vec<_>>()
                            }),
                        _ => None,
                    },
                    _ => None,
                };
                for (index, argument) in args.iter().enumerate() {
                    if matches!(callee, hir::Callee::Foreign(_))
                        && parameter_types
                            .as_ref()
                            .and_then(|params| params.get(index))
                            .is_some_and(|parameter| {
                                self.is_boundary_struct_pointer(parameter)
                                    && matches!(argument.ty, Type::Class(_))
                                    && is_place_expr(argument)
                                    && !self.is_embedded_header_store(parameter, argument)
                            })
                    {
                        self.place(argument);
                    } else {
                        self.expr(argument);
                    }
                }
                return;
            }
            K::Index { .. } => {
                self.place(expr);
                return;
            }
            K::Lambda { .. } => return,
            _ => {}
        }
        for child in expr.children() {
            match child {
                hir::HirChild::Expr(expr) => self.expr(expr),
                hir::HirChild::Stmt(statement) => self.statement(statement),
            }
        }
    }

    fn place(&mut self, expr: &hir::Expr) {
        match &expr.kind {
            hir::ExprKind::Local(name) => self.mark(name),
            hir::ExprKind::Field { obj, .. } => {
                if is_stored_aggregate(self.module, &obj.ty) && is_place_expr(obj) {
                    self.place(obj);
                } else {
                    self.expr(obj);
                }
            }
            hir::ExprKind::Index { obj, index, .. } => {
                if matches!(obj.ty, Type::FixedArray(..)) && is_place_expr(obj) {
                    self.place(obj);
                } else {
                    self.expr(obj);
                }
                self.expr(index);
            }
            hir::ExprKind::Global(_) | hir::ExprKind::This => {}
            _ => self.expr(expr),
        }
    }

    fn is_boundary_struct_pointer(&self, ty: &Type) -> bool {
        boundary_box_class(self.module, ty).is_some()
    }

    fn is_embedded_header_store(&self, expected: &Type, expr: &hir::Expr) -> bool {
        let Some(header) = boundary_box_class(self.module, expected) else {
            return false;
        };
        let hir::ExprKind::Field { obj, name } = &expr.kind else {
            return false;
        };
        let Type::Class(extension) = obj.ty else {
            return false;
        };
        self.classes
            .get(extension.0)
            .filter(|class| class.is_value && class.is_boundary)
            .and_then(|class| class.fields.first())
            .is_some_and(|field| {
                field.source_name == *name
                    && field.ty == Type::Class(header)
                    && self
                        .classes
                        .get(header.0)
                        .is_some_and(|header| header.is_embedded_header)
            })
    }
}
