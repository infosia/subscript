//! Rejects a method call on an array element if the method can move that
//! array (compiler.md §221).
//!
//! One walk over the checked program builds the reversed call graph. It
//! records each operation that moves a dynamic array and each call on an
//! unstable receiver. A breadth-first search over the reversed graph then
//! runs once for each element type that an unstable receiver crosses. The
//! cost is O(program + types × graph), and a program with no unstable receiver
//! stops after the walk.

use std::collections::{HashMap, HashSet, VecDeque};

use super::capture::type_name;
use super::rejection::{diagnostic, RejectionSite};
use super::ClassSig;
use crate::diag::{Diagnostic, Pos};
use crate::hir::{self, Callee, ElementMove, Expr, ExprKind, HirChild, Stmt, Symbol};
use crate::types::{ClassId, Type};

/// One operation that moves a dynamic array of `element`.
struct Move<'a> {
    element: &'a Type,
    effect: ElementMove,
    operation: &'static str,
    pos: &'a Pos,
    unit: usize,
}

/// One call of a value-class method or accessor on an unstable receiver.
/// `unit` is the unit of the called method.
struct Site<'a> {
    call: &'a Expr,
    receiver: &'a Expr,
    class: ClassId,
    name: &'a Symbol,
    unit: usize,
    element: &'a Type,
}

/// The call graph of one checked program.
///
/// A unit is a body that a call can run: a function, a method, a class
/// construction, a lambda, or the module body. One more unit stands for each
/// function or generator type, so an indirect call adds one edge and not one
/// edge per target. The foreign unit stands for C during a foreign call, and
/// it reaches each function value that crosses the boundary (rule 10).
struct Graph<'a> {
    module: &'a hir::Module,
    functions: HashMap<&'a Symbol, usize>,
    generators: HashSet<&'a Symbol>,
    methods: HashMap<(ClassId, &'a Symbol), usize>,
    constructors: Vec<usize>,
    foreign: usize,
    /// The units that reach each unit in one step.
    callers: Vec<Vec<usize>>,
    /// The unit of each function type and generator type.
    targets: HashMap<&'a Type, usize>,
    moves: Vec<Move<'a>>,
    sites: Vec<Site<'a>>,
}

impl<'a> Graph<'a> {
    fn new(module: &'a hir::Module) -> Self {
        let mut graph = Self {
            module,
            functions: HashMap::new(),
            generators: HashSet::new(),
            methods: HashMap::new(),
            constructors: Vec::with_capacity(module.classes.len()),
            foreign: 1,
            callers: vec![Vec::new(), Vec::new()],
            targets: HashMap::new(),
            moves: Vec::new(),
            sites: Vec::new(),
        };
        for function in &module.functions {
            let unit = graph.unit();
            graph.functions.insert(&function.symbol, unit);
            if function.is_generator {
                graph.generators.insert(&function.symbol);
            }
        }
        for (index, class) in module.classes.iter().enumerate() {
            let unit = graph.unit();
            graph.constructors.push(unit);
            for method in &class.methods {
                let unit = graph.unit();
                graph.methods.insert((ClassId(index), &method.symbol), unit);
            }
        }
        graph
    }

    fn unit(&mut self) -> usize {
        self.callers.push(Vec::new());
        self.callers.len() - 1
    }

    fn call(&mut self, caller: usize, callee: Option<usize>) {
        if let Some(callee) = callee {
            self.callers[callee].push(caller);
        }
    }

    fn target(&mut self, ty: &'a Type) -> usize {
        if let Some(unit) = self.targets.get(ty) {
            return *unit;
        }
        let unit = self.unit();
        self.targets.insert(ty, unit);
        unit
    }

    fn function(&self, symbol: &Symbol) -> Option<usize> {
        self.functions.get(symbol).copied()
    }

    fn method(&self, class: ClassId, name: &Symbol) -> Option<usize> {
        self.methods.get(&(class, name)).copied()
    }

    fn walk_program(&mut self) {
        let module = self.module;
        for function in &module.functions {
            let unit = self.function(&function.symbol).unwrap_or(0);
            self.walk_function(unit, function);
            if function.is_generator {
                let target = self.target(&function.ret);
                self.call(target, Some(unit));
            }
        }
        for (index, class) in module.classes.iter().enumerate() {
            let construction = self.constructors[index];
            for field in &class.fields {
                if let Some(initializer) = &field.init {
                    self.expr(construction, initializer);
                }
            }
            if let Some(constructor) = &class.ctor {
                self.walk_function(construction, constructor);
            }
            for method in &class.methods {
                let unit = self
                    .method(ClassId(index), &method.symbol)
                    .unwrap_or(construction);
                self.walk_function(unit, method);
            }
        }
        for global in &module.globals {
            self.expr(0, &global.init);
        }
        self.stmts(0, &module.top_level);
    }

    fn walk_function(&mut self, unit: usize, function: &'a hir::Function) {
        for default in function.params.iter().filter_map(|p| p.default.as_ref()) {
            self.expr(unit, default);
        }
        self.stmts(unit, &function.body);
    }

    fn stmts(&mut self, unit: usize, statements: &'a [Stmt]) {
        for statement in statements {
            self.stmt(unit, statement);
        }
    }

    fn stmt(&mut self, unit: usize, statement: &'a Stmt) {
        match statement {
            // The checker wraps each `using` binding in this form.
            Stmt::Using { bindings, .. } => {
                for binding in bindings {
                    self.dispose(unit, &binding.ty);
                }
            }
            Stmt::GeneratorForOf { subject, .. } => {
                let target = self.target(&subject.ty);
                self.call(unit, Some(target));
            }
            Stmt::Let { .. }
            | Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::If { .. }
            | Stmt::While { .. }
            | Stmt::DoWhile { .. }
            | Stmt::For { .. }
            | Stmt::ForOf { .. }
            | Stmt::Switch { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Block(_)
            | Stmt::Throw { .. }
            | Stmt::Try { .. } => {}
        }
        self.children(unit, statement.children());
    }

    fn dispose(&mut self, unit: usize, ty: &Type) {
        let class = match ty {
            Type::Class(class) => Some(*class),
            Type::Nullable(inner, _) => match **inner {
                Type::Class(class) => Some(class),
                _ => None,
            },
            _ => None,
        };
        let name = Symbol::from_full_text(hir::DISPOSE_METHOD_NAME);
        let hook = class.and_then(|class| self.method(class, &name));
        self.call(unit, hook);
    }

    fn children(&mut self, unit: usize, children: Vec<HirChild<'a>>) {
        for child in children {
            match child {
                HirChild::Expr(expression) => self.expr(unit, expression),
                HirChild::Stmt(statement) => self.stmt(unit, statement),
            }
        }
    }

    fn expr(&mut self, unit: usize, expression: &'a Expr) {
        let mut body = unit;
        match &expression.kind {
            ExprKind::Call { callee, args } => self.callee(unit, expression, callee, args),
            ExprKind::New { class, args } => {
                self.call(unit, self.constructors.get(class.0).copied());
                if self.is_mirror_class(*class) {
                    self.cross_boundary(args);
                }
            }
            ExprKind::DescriptorLit { class, fields } => {
                self.call(unit, self.constructors.get(class.0).copied());
                if self.is_mirror_class(*class) {
                    for field in fields.iter().flatten() {
                        self.cross_boundary(std::slice::from_ref(field));
                    }
                }
            }
            ExprKind::Assign { target, value, .. } => {
                if let ExprKind::Field { obj, .. } = &target.kind {
                    if matches!(obj.ty, Type::Class(class) if self.is_mirror_class(class)) {
                        self.cross_boundary(std::slice::from_ref(value));
                    }
                }
            }
            ExprKind::AsyncCall { callee, .. } | ExprKind::AsyncHandleCreate { callee, .. } => {
                let target = match callee {
                    hir::AsyncCallee::Function(symbol) => self.function(symbol),
                    hir::AsyncCallee::Method { class, name, .. } => self.method(*class, name),
                };
                self.call(unit, target);
            }
            ExprKind::Lambda { .. } => {
                body = self.unit();
                self.call(unit, Some(body));
                let target = self.target(&expression.ty);
                self.call(target, Some(body));
            }
            ExprKind::FuncRef(symbol) => {
                let target = self.target(&expression.ty);
                self.call(target, self.function(symbol));
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Null
            | ExprKind::This
            | ExprKind::Local(..)
            | ExprKind::Global(_)
            | ExprKind::EnumMember { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::AbsenceTest { .. }
            | ExprKind::Cast(_)
            | ExprKind::Unassigned
            | ExprKind::Zero
            | ExprKind::RawNew { .. }
            | ExprKind::Field { .. }
            | ExprKind::Length(_)
            | ExprKind::Index { .. }
            | ExprKind::ArrayLit(_)
            | ExprKind::ArraySpreadLit(_)
            | ExprKind::Template(_)
            | ExprKind::Yield(_)
            | ExprKind::AsyncSuspend
            | ExprKind::AsyncHandleAwait(_)
            | ExprKind::TaskGroup { .. }
            | ExprKind::AsyncAll { .. }
            | ExprKind::AsyncHandleTransfer { .. }
            | ExprKind::Cond { .. }
            | ExprKind::Seq { .. } => {}
        }
        self.children(body, expression.children());
    }

    fn callee(&mut self, unit: usize, call: &'a Expr, callee: &'a Callee, args: &'a [Expr]) {
        match callee {
            Callee::Func(symbol) => {
                // A generator body runs at `next`, not at the call.
                if !self.generators.contains(symbol) {
                    self.call(unit, self.function(symbol));
                }
            }
            Callee::Value(value) => {
                let target = self.target(&value.ty);
                self.call(unit, Some(target));
            }
            Callee::Method { recv, name } => self.method_call(unit, call, callee, recv, name),
            Callee::Arr(function) => {
                if let Some(effect) = function.element_move() {
                    self.record_move(unit, args.first(), (function.name(), effect), &call.pos);
                }
                self.callbacks(unit, args);
            }
            Callee::Map(_) | Callee::Set(_) => self.callbacks(unit, args),
            Callee::Foreign(_) => {
                self.call(unit, Some(self.foreign));
                self.cross_boundary(args);
            }
            Callee::Standard(_)
            | Callee::Ambient(_)
            | Callee::ContextBytes { .. }
            | Callee::Math(_)
            | Callee::Num(_)
            | Callee::Date(_)
            | Callee::Json(_)
            | Callee::Text(_)
            | Callee::Str(_)
            | Callee::Regex(_)
            | Callee::Worker(_) => {}
        }
    }

    fn method_call(
        &mut self,
        unit: usize,
        call: &'a Expr,
        callee: &'a Callee,
        receiver: &'a Expr,
        name: &'a Symbol,
    ) {
        if let Some((hir::OperationSignatureTarget::BuiltinMethod(method), _)) =
            hir::operation_signature_target(callee)
        {
            if let Some(operation) = method.element_move() {
                self.record_move(unit, Some(receiver), operation, &call.pos);
            }
            if method == hir::BuiltinMethod::GeneratorNext {
                let target = self.target(&receiver.ty);
                self.call(unit, Some(target));
            }
            return;
        }
        let Type::Class(class) = receiver.ty else {
            return;
        };
        let method = self.method(class, name);
        self.call(unit, method);
        let is_value = self
            .module
            .classes
            .get(class.0)
            .is_some_and(|definition| definition.is_value);
        if let (true, Some(unit)) = (is_value, method) {
            if let Some(element) = self.crossed_element(receiver) {
                self.sites.push(Site {
                    call,
                    receiver,
                    class,
                    name,
                    unit,
                    element,
                });
            }
        }
    }

    fn is_mirror_class(&self, class: ClassId) -> bool {
        self.module
            .classes
            .get(class.0)
            .is_some_and(|definition| definition.is_boundary)
    }

    /// Rule 10: C holds each function value in `values` after this call.
    fn cross_boundary(&mut self, values: &'a [Expr]) {
        for value in values {
            let mut ty = &value.ty;
            while let Type::Nullable(inner, _) | Type::Array(inner) | Type::FixedArray(inner, _) =
                ty
            {
                ty = inner;
            }
            if matches!(ty, Type::Func(_)) {
                let target = self.target(ty);
                self.call(self.foreign, Some(target));
            }
        }
    }

    /// An argument of a function type is a value that a built-in calls.
    fn callbacks(&mut self, unit: usize, args: &'a [Expr]) {
        for argument in args {
            if matches!(argument.ty, Type::Func(_)) {
                let target = self.target(&argument.ty);
                self.call(unit, Some(target));
            }
        }
    }

    fn record_move(
        &mut self,
        unit: usize,
        array: Option<&'a Expr>,
        (operation, effect): (&'static str, ElementMove),
        pos: &'a Pos,
    ) {
        if let Some(Type::Array(element)) = array.map(|array| &array.ty) {
            self.moves.push(Move {
                element,
                effect,
                operation,
                pos,
                unit,
            });
        }
    }

    /// Rule 1: the element type of the dynamic array that the receiver place
    /// crosses after its last reference handle. The array value is itself a
    /// handle, so the walk ends at the first dynamic array.
    fn crossed_element(&self, receiver: &'a Expr) -> Option<&'a Type> {
        let mut place = receiver;
        loop {
            match &place.kind {
                ExprKind::Field { obj, .. } if self.is_value_class(&obj.ty) => place = obj,
                ExprKind::Index { obj, .. } => match &obj.ty {
                    Type::Array(element) => return Some(element),
                    Type::FixedArray(..) => place = obj,
                    _ => return None,
                },
                _ => return None,
            }
        }
    }

    fn is_value_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(class)
            if self.module.classes.get(class.0).is_some_and(|definition| definition.is_value))
    }

    /// The move that each unit reaches for one element type and one effect: a
    /// breadth-first search from the units that hold such a move, over the
    /// reversed graph.
    fn reach(&self, element: &Type, effect: ElementMove) -> Vec<Option<usize>> {
        let mut reached = vec![None; self.callers.len()];
        let mut queue = VecDeque::new();
        for (index, operation) in self.moves.iter().enumerate() {
            if operation.element == element
                && operation.effect == effect
                && reached[operation.unit].is_none()
            {
                reached[operation.unit] = Some(index);
                queue.push_back(operation.unit);
            }
        }
        while let Some(unit) = queue.pop_front() {
            for &caller in &self.callers[unit] {
                if reached[caller].is_none() {
                    reached[caller] = reached[unit];
                    queue.push_back(caller);
                }
            }
        }
        reached
    }

    fn diagnostics(&self, class_sigs: &[ClassSig]) -> Vec<Diagnostic> {
        let mut reaches: HashMap<(&Type, ElementMove), Vec<Option<usize>>> = HashMap::new();
        let mut reported = HashSet::new();
        let mut diagnostics = Vec::new();
        for site in &self.sites {
            // Rule 6: a reachable rearrangement selects the text, because the
            // copy of a growth is wrong after it.
            let found = [ElementMove::Rearrangement, ElementMove::Growth]
                .into_iter()
                .find_map(|effect| {
                    reaches
                        .entry((site.element, effect))
                        .or_insert_with(|| self.reach(site.element, effect))[site.unit]
                });
            let Some(index) = found else {
                continue;
            };
            let found = self.diagnostic(site, &self.moves[index], class_sigs);
            if reported.insert(format!("{:?} {}", found.pos, found.message)) {
                diagnostics.push(found);
            }
        }
        diagnostics
    }

    fn diagnostic(
        &self,
        site: &Site<'a>,
        operation: &Move<'a>,
        class_sigs: &[ClassSig],
    ) -> Diagnostic {
        let classes = &self.module.classes;
        let member = site.name.full_text();
        let label = classes.get(site.class.0).map_or_else(
            || hir::source_name(member),
            |class| super::identity::class_member_label(classes, class, member),
        );
        let place = spelling(site.receiver);
        let subject = place
            .as_ref()
            .map_or_else(|| "the receiver".to_string(), |place| format!("`{place}`"));
        let fix = match (operation.effect, &place) {
            (ElementMove::Rearrangement, _) => format!(
                "after `{0}` the index no longer names the element, so run `{0}` after the \
                 method returns",
                operation.operation
            ),
            (ElementMove::Growth, Some(place)) => {
                let accessor = class_sigs
                    .get(site.class.0)
                    .is_some_and(|signature| signature.has_accessor(member));
                let call = if accessor {
                    format!("t.{member}")
                } else {
                    let method = self.module.method(&site.receiver.ty, site.name);
                    let empty = method.is_some_and(|method| method.params.is_empty());
                    format!("t.{member}({})", if empty { "" } else { "..." })
                };
                let class = type_name(self.module, &site.receiver.ty);
                format!("call it on a copy: `const t: {class} = {place}; {call}; {place} = t;`")
            }
            (ElementMove::Growth, None) => "copy the element to a `const`, call the method on \
                 the copy, and assign the copy back"
                .to_string(),
        };
        diagnostic(
            RejectionSite::ElementMethodMovesArray,
            format!(
                "`{label}` can move a dynamic array of `{}` by `{}` at {}:{}:{}, and {subject} is \
                 an element of such an array; {fix}",
                type_name(self.module, site.element),
                operation.operation,
                operation.pos.file,
                operation.pos.line,
                operation.pos.col,
            ),
            site.call.pos.clone(),
        )
    }
}

/// The source form of a plain receiver place. A place with another form has
/// no spelling in the checked program.
fn spelling(place: &Expr) -> Option<String> {
    match &place.kind {
        ExprKind::This => Some("this".to_string()),
        ExprKind::Local(name, ..) => Some(name.clone()),
        ExprKind::Global(symbol) => Some(symbol.source_name()),
        ExprKind::Field { obj, name } => Some(format!("{}.{name}", spelling(obj)?)),
        ExprKind::Index { obj, index, .. } => {
            let index = match &index.kind {
                ExprKind::Int(value) => value.to_string(),
                _ => spelling(index)?,
            };
            Some(format!("{}[{index}]", spelling(obj)?))
        }
        _ => None,
    }
}

/// Reports each call on an unstable receiver whose method can reach an
/// operation that moves a dynamic array of one of the crossed element types.
pub(super) fn check(module: &hir::Module, class_sigs: &[ClassSig]) -> Result<(), Vec<Diagnostic>> {
    let mut graph = Graph::new(module);
    graph.walk_program();
    if graph.sites.is_empty() {
        return Ok(());
    }
    let diagnostics = graph.diagnostics(class_sigs);
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}
