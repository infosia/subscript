//! The "can raise" fact of `compiler.md` §115.6 rule 3.
//!
//! A function can raise when its body holds a `throw`, a built-in call
//! that takes a script callback, an indirect call, or a call to a
//! function that can raise. The fact is derived one time, here, after the
//! check. `Expr::trap_sites` reads it to give a call its raise site, and
//! each engine reads the copy that LIR carries. No engine derives it
//! again.
//!
//! A `JSON.parse` call is a call to its generated root helper, whose body
//! holds the `throw` of each parse failure (`compiler.md` §115.7), so the
//! rule makes it a raise site of its caller with no case of its own.
//!
//! An `async` function and a generator never raise to a caller. Their
//! bodies are boundaries: an exception that leaves the body becomes the
//! uncaught-exception trap there (§115.4 items 2 and 3). So an async call,
//! a held-handle creation, an `await`, and a generator `next()` are not
//! raise sites, and each keeps only the trap check it has.
//!
//! A lambda body is a separate function. Its `throw` makes the lambda
//! raise, not the function that creates the lambda: the lambda runs only
//! through an indirect call or a built-in callback, and both are raise
//! sites of their own.

use crate::hir::{
    Callee, ClassDef, Expr, ExprKind, ExpressionOwner, ExpressionOwnerMut, Function, HirChild,
    HirChildMut, Module, Stmt,
};
use crate::types::Type;

/// Sets [`Function::can_raise`] on every function, constructor, and
/// method of `module`, to the least fixed point of the rule.
pub(crate) fn decide_can_raise(module: &mut Module) {
    loop {
        let mut raising = Vec::new();
        for (index, function) in module.functions.iter().enumerate() {
            if !function.can_raise
                && !is_boundary(function)
                && module.function_body_can_raise(function)
            {
                raising.push(Owner::Free(index));
            }
        }
        for (class_index, class) in module.classes.iter().enumerate() {
            if let Some(constructor) = &class.ctor {
                if !constructor.can_raise && module.function_body_can_raise(constructor) {
                    raising.push(Owner::Constructor(class_index));
                }
            }
            for (method_index, method) in class.methods.iter().enumerate() {
                if !method.can_raise
                    && !is_boundary(method)
                    && module.function_body_can_raise(method)
                {
                    raising.push(Owner::Method(class_index, method_index));
                }
            }
        }
        if raising.is_empty() {
            decide_lambdas_and_initializer(module);
            return;
        }
        for owner in raising {
            let function = match owner {
                Owner::Free(index) => module.functions.get_mut(index),
                Owner::Constructor(class) => module
                    .classes
                    .get_mut(class)
                    .and_then(|class| class.ctor.as_mut()),
                Owner::Method(class, method) => module
                    .classes
                    .get_mut(class)
                    .and_then(|class| class.methods.get_mut(method)),
            };
            if let Some(function) = function {
                function.can_raise = true;
            }
        }
    }
}

/// Sets the `can_raise` field of each lambda and
/// [`Module::initializer_can_raise`] under the settled function facts. A
/// lambda body is its own function (`compiler.md` §115.6 rule 3).
fn decide_lambdas_and_initializer(module: &mut Module) {
    let mut lambdas = Vec::new();
    for owner in module.expression_owners() {
        match owner {
            ExpressionOwner::Expr(expression) => lambda_facts(module, expression, &mut lambdas),
            ExpressionOwner::Body { statements, .. } => {
                for statement in statements {
                    lambda_facts_in_statement(module, statement, &mut lambdas);
                }
            }
        }
    }
    let mut facts = lambdas.into_iter();
    for owner in module.expression_owners_mut() {
        match owner {
            ExpressionOwnerMut::Expr(expression) => set_lambda_facts(expression, &mut facts),
            ExpressionOwnerMut::Body(statements) => {
                for statement in statements.iter_mut() {
                    set_lambda_facts_in_statement(statement, &mut facts);
                }
            }
        }
    }
    module.initializer_can_raise = module.statements_can_raise(&module.top_level)
        || module
            .globals
            .iter()
            .any(|global| module.expression_can_raise(&global.init));
}

/// The fact of each lambda below `expression`, in pre-order.
fn lambda_facts(module: &Module, expression: &Expr, facts: &mut Vec<bool>) {
    if let ExprKind::Lambda { body, .. } = &expression.kind {
        facts.push(module.statements_can_raise(body));
    }
    for child in expression.children() {
        match child {
            HirChild::Expr(child) => lambda_facts(module, child, facts),
            HirChild::Stmt(child) => lambda_facts_in_statement(module, child, facts),
        }
    }
}

fn lambda_facts_in_statement(module: &Module, statement: &Stmt, facts: &mut Vec<bool>) {
    for child in statement.children() {
        match child {
            HirChild::Expr(child) => lambda_facts(module, child, facts),
            HirChild::Stmt(child) => lambda_facts_in_statement(module, child, facts),
        }
    }
}

/// Writes the facts of [`lambda_facts`] back, in the same pre-order.
fn set_lambda_facts(expression: &mut Expr, facts: &mut impl Iterator<Item = bool>) {
    if let ExprKind::Lambda { can_raise, .. } = &mut expression.kind {
        *can_raise = facts.next().unwrap_or(true);
    }
    for child in expression.children_mut() {
        match child {
            HirChildMut::Expr(child) => set_lambda_facts(child, facts),
            HirChildMut::Stmt(child) => set_lambda_facts_in_statement(child, facts),
        }
    }
}

fn set_lambda_facts_in_statement(statement: &mut Stmt, facts: &mut impl Iterator<Item = bool>) {
    for child in statement.children_mut() {
        match child {
            HirChildMut::Expr(child) => set_lambda_facts(child, facts),
            HirChildMut::Stmt(child) => set_lambda_facts_in_statement(child, facts),
        }
    }
}

/// Whether the body of `function` is an exception boundary: an `async`
/// body or a generator body (`compiler.md` §115.4 items 2 and 3).
fn is_boundary(function: &Function) -> bool {
    function.is_async || function.is_generator
}

#[derive(Clone, Copy)]
enum Owner {
    Free(usize),
    Constructor(usize),
    Method(usize, usize),
}

impl Module {
    /// Whether these statements can leave an exception pending, under the
    /// current [`Function::can_raise`] facts of the module.
    ///
    /// The walk does not enter a lambda body.
    #[must_use]
    pub fn statements_can_raise(&self, statements: &[Stmt]) -> bool {
        statements
            .iter()
            .any(|statement| self.statement_can_raise(statement))
    }

    /// Whether this expression can leave an exception pending, under the
    /// current [`Function::can_raise`] facts of the module.
    ///
    /// The walk does not enter a lambda body.
    #[must_use]
    pub fn expression_can_raise(&self, expression: &Expr) -> bool {
        let own = match &expression.kind {
            ExprKind::Call { callee, args } => {
                call_can_raise(self, callee, args) || self.call_defaults_can_raise(callee)
            }
            ExprKind::New { class, .. } => self
                .classes
                .get(class.0)
                .is_some_and(|class| self.construction_can_raise(class)),
            ExprKind::Lambda { .. } => return false,
            _ => false,
        };
        own || expression.children().into_iter().any(|child| match child {
            crate::hir::HirChild::Expr(child) => self.expression_can_raise(child),
            crate::hir::HirChild::Stmt(child) => self.statement_can_raise(child),
        })
    }

    fn statement_can_raise(&self, statement: &Stmt) -> bool {
        match statement {
            Stmt::Throw { .. } => return true,
            // A hook that raises on a normal exit leaves the scope with
            // an exception (`compiler.md` §115.5 rule 3). A hook that
            // raises on the exception edge traps (rule 7).
            Stmt::Using { bindings, body, .. } => {
                return self.statements_can_raise(body)
                    || bindings
                        .iter()
                        .any(|binding| self.statement_can_raise(&binding.hook()));
            }
            Stmt::Let { .. }
            | Stmt::Expr(_)
            | Stmt::Return { .. }
            | Stmt::If { .. }
            | Stmt::While { .. }
            | Stmt::For { .. }
            | Stmt::ForOf { .. }
            | Stmt::Switch { .. }
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::Block(_)
            | Stmt::Try { .. } => {}
        }
        statement.children().into_iter().any(|child| match child {
            crate::hir::HirChild::Expr(child) => self.expression_can_raise(child),
            crate::hir::HirChild::Stmt(child) => self.statement_can_raise(child),
        })
    }

    fn function_body_can_raise(&self, function: &Function) -> bool {
        self.statements_can_raise(&function.body)
    }

    /// A construction runs the field initializers, the defaults of the
    /// absent arguments, and the constructor (`compiler.md` §57.1).
    fn construction_can_raise(&self, class: &ClassDef) -> bool {
        class
            .fields
            .iter()
            .filter_map(|field| field.init.as_ref())
            .any(|init| self.expression_can_raise(init))
            || class.ctor.as_ref().is_some_and(|constructor| {
                constructor.can_raise
                    || constructor
                        .params
                        .iter()
                        .filter_map(|parameter| parameter.default.as_ref())
                        .any(|default| self.expression_can_raise(default))
            })
    }

    /// The caller evaluates the defaults of the absent arguments.
    fn call_defaults_can_raise(&self, callee: &Callee) -> bool {
        let function = match callee {
            Callee::Func(name) => self
                .functions
                .iter()
                .find(|function| function.name == *name),
            Callee::Method { recv, name } => self.method(&recv.ty, name),
            _ => None,
        };
        function.is_some_and(|function| {
            function
                .params
                .iter()
                .filter_map(|parameter| parameter.default.as_ref())
                .any(|default| self.expression_can_raise(default))
        })
    }

    fn method(&self, receiver: &Type, name: &str) -> Option<&Function> {
        let Type::Class(class) = receiver else {
            return None;
        };
        self.classes
            .get(class.0)?
            .methods
            .iter()
            .find(|method| method.name == name)
    }
}

/// Whether a call to `callee` with `args` is a raise site
/// (`compiler.md` §115.6 rule 3).
///
/// A direct call or a method call raises when its target can raise. An
/// indirect call always raises. A built-in call raises when an argument
/// is a script callback, because the callback's exception propagates to
/// the caller of the built-in (§115.4). A foreign call never raises: a
/// host callback is a boundary (§115.4 item 5).
pub(crate) fn call_can_raise(module: &Module, callee: &Callee, args: &[Expr]) -> bool {
    let takes_callback = || args.iter().any(|arg| matches!(arg.ty, Type::Func(_)));
    match callee {
        Callee::Func(name) => module
            .functions
            .iter()
            .find(|function| function.name == *name)
            .is_some_and(|function| function.can_raise),
        Callee::Method { recv, name } => module
            .method(&recv.ty, name)
            .is_some_and(|method| method.can_raise),
        Callee::Value(_) => true,
        Callee::Foreign(_) | Callee::Worker(_) | Callee::Json(_) => false,
        Callee::Ambient(_)
        | Callee::ContextBytes { .. }
        | Callee::Math(_)
        | Callee::Num(_)
        | Callee::Date(_)
        | Callee::Str(_)
        | Callee::Regex(_)
        | Callee::Arr(_)
        | Callee::Map(_)
        | Callee::Set(_) => takes_callback(),
    }
}

/// Whether a construction of `class` is a raise site: its constructor
/// can raise.
pub(crate) fn construction_call_can_raise(class: &ClassDef) -> bool {
    class
        .ctor
        .as_ref()
        .is_some_and(|constructor| constructor.can_raise)
}

#[cfg(test)]
mod tests {
    use crate::{check_program, SourceFile};

    fn module(source: &str) -> crate::hir::Module {
        check_program(&[SourceFile::new("raise.ts", source)]).expect("checks clean")
    }

    fn function<'m>(module: &'m crate::hir::Module, name: &str) -> &'m crate::hir::Function {
        module
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("function")
    }

    #[test]
    fn the_fact_follows_throw_calls_and_indirect_calls() {
        let module = module(
            "function thrower(): void { throw new Error(\"x\"); }\n\
             function caller(): void { thrower(); }\n\
             function outer(): void { caller(); }\n\
             function quiet(): i32 { return 1; }\n\
             function indirect(f: () => void): void { f(); }\n\
             function callback(values: i32[]): void {\n\
               values.forEach((value: i32): void => { print(`${value}`); });\n\
             }\n\
             function makesLambda(): void {\n\
               const f = (): void => { throw new Error(\"y\"); };\n\
             }\n\
             export function main(): void { quiet(); }\n",
        );
        assert!(function(&module, "thrower").can_raise);
        assert!(function(&module, "caller").can_raise);
        assert!(function(&module, "outer").can_raise);
        assert!(!function(&module, "quiet").can_raise);
        assert!(function(&module, "indirect").can_raise);
        assert!(function(&module, "callback").can_raise);
        assert!(
            !function(&module, "makesLambda").can_raise,
            "a lambda body raises for the lambda, not for its creator"
        );
        assert!(!function(&module, "main").can_raise);
    }

    #[test]
    fn an_async_function_and_a_generator_never_raise_to_a_caller() {
        let module = module(
            "async function load(): Promise<i32> { throw new Error(\"x\"); }\n\
             function* numbers(): Generator<i32> { throw new Error(\"y\"); }\n\
             function thrower(): void { throw new Error(\"z\"); }\n\
             async function caller(): Promise<i32> { thrower(); return await load(); }\n\
             function consumer(): void { for (const n of numbers()) { print(`${n}`); } }\n\
             export async function main(): Promise<void> {\n\
             \x20 const value: i32 = await caller();\n\
             \x20 consumer();\n\
             \x20 print(`${value}`);\n\
             }\n",
        );
        assert!(!function(&module, "load").can_raise);
        assert!(!function(&module, "numbers").can_raise);
        assert!(
            !function(&module, "caller").can_raise,
            "an async body that calls a raising function is still a boundary"
        );
        assert!(
            function(&module, "thrower").can_raise,
            "the firing control: a plain function with the same body raises"
        );
        assert!(!function(&module, "consumer").can_raise);
        assert!(!function(&module, "main").can_raise);
        let caller = function(&module, "caller");
        assert!(
            module.statements_can_raise(&caller.body),
            "the body itself can raise; the boundary converts it"
        );
    }

    #[test]
    fn a_construction_raises_through_its_constructor() {
        let module = module(
            "class Strict {\n\
               value: i32;\n\
               constructor(value: i32) {\n\
                 if (value < 0) { throw new Error(\"negative\"); }\n\
                 this.value = value;\n\
               }\n\
             }\n\
             function build(): i32 { return new Strict(1).value; }\n\
             export function main(): void { print(`${build()}`); }\n",
        );
        let strict = module
            .classes
            .iter()
            .find(|class| class.name == "Strict")
            .expect("class");
        assert!(strict.ctor.as_ref().expect("constructor").can_raise);
        assert!(function(&module, "build").can_raise);
        assert!(function(&module, "main").can_raise);
    }

    #[test]
    fn a_using_scope_raises_through_its_body_or_a_hook() {
        let module = module(
            "class Quiet { [Symbol.dispose](): void { print(\"q\"); } }\n\
             class Loud { [Symbol.dispose](): void { throw new Error(\"x\"); } }\n\
             function thrower(): void { throw new Error(\"y\"); }\n\
             function quietScope(): void { using q = new Quiet(); print(\"body\"); }\n\
             function loudHook(): void { using l = new Loud(); print(\"body\"); }\n\
             function raisingBody(): void { using q = new Quiet(); thrower(); }\n\
             export function main(): void { quietScope(); }\n",
        );
        assert!(
            !function(&module, "quietScope").can_raise,
            "the control: no raise in the body or the hook"
        );
        assert!(
            function(&module, "loudHook").can_raise,
            "a hook that raises on a normal exit (compiler.md §115.5 rule 3)"
        );
        assert!(function(&module, "raisingBody").can_raise);
        let loud = function(&module, "loudHook");
        assert!(matches!(
            loud.body.as_slice(),
            [crate::hir::Stmt::Let { .. }, crate::hir::Stmt::Using { .. }]
        ));
        assert!(module.statements_can_raise(&loud.body[1..]));
    }

    #[test]
    fn a_lambda_and_the_initializer_carry_their_own_fact() {
        fn lambda_facts(statements: &[crate::hir::Stmt]) -> Vec<bool> {
            statements
                .iter()
                .filter_map(|statement| match statement {
                    crate::hir::Stmt::Let { init, .. } => match &init.kind {
                        crate::hir::ExprKind::Lambda { can_raise, .. } => Some(*can_raise),
                        _ => None,
                    },
                    _ => None,
                })
                .collect()
        }
        let raising = module(
            "function thrower(): i32 { throw new Error(\"x\"); }\n\
             const start: i32 = thrower();\n\
             function makes(): void {\n\
               const f = (): void => { throw new Error(\"y\"); };\n\
               const g = (): void => { print(\"g\"); };\n\
               const h = (): void => { g(); };\n\
             }\n\
             export function main(): void { print(`${start}`); }\n",
        );
        assert_eq!(
            lambda_facts(&function(&raising, "makes").body),
            [true, false, true],
            "a throw, a quiet body, and an indirect call"
        );
        assert!(raising.initializer_can_raise);
        let quiet = module(
            "const start: i32 = 1;\n\
             export function main(): void { print(`${start}`); }\n",
        );
        assert!(
            !quiet.initializer_can_raise,
            "the control: a quiet initializer"
        );
    }

    #[test]
    fn statements_and_expressions_answer_under_the_module_facts() {
        let module = module(
            "function thrower(): i32 { throw new Error(\"x\"); }\n\
             export function main(): void { print(`${thrower()}`); }\n",
        );
        let main = function(&module, "main");
        assert!(module.statements_can_raise(&main.body));
        let quiet = module
            .classes
            .iter()
            .find(|class| class.name == "Error")
            .expect("the Error class");
        assert!(!quiet.ctor.as_ref().expect("constructor").can_raise);
        let crate::hir::Stmt::Expr(print) = &main.body[0] else {
            panic!("expression statement");
        };
        assert!(module.expression_can_raise(print));
    }
}
