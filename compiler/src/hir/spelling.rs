//! The absence spelling at the HIR boundary (`compiler.md` §195.7).
//!
//! The checker types carry the `undefined` spelling of a nullable type.
//! HIR carries the `null` spelling only: [`erase_undefined`] maps each
//! `undefined` spelling to `null` once, when the checker assembles the
//! module. One walk visits every type that the module holds, so the check
//! of §195.1 rule 6 and the erasure read the same slots.

use super::{Callee, Expr, ExprKind, Function, HirChildMut, Module, Stmt};
use crate::diag::Pos;
use crate::types::Type;

/// The kind of HIR position that holds a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeSlot {
    /// The type of an expression.
    Expression,
    /// The type of a local: a declaration, a read, a capture, or a loop,
    /// `catch`, or `using` binding.
    Local,
    /// The declared result of a module-level function or a class method.
    FunctionResult,
    /// Any other storage or signature: a parameter, a lambda result, a
    /// field, a module variable, an index signature, a foreign signature,
    /// or a type argument of a storage-byte operation.
    Storage,
}

/// Visits every type slot of the module in declaration order.
pub(crate) fn for_each_type(module: &mut Module, visit: &mut dyn FnMut(&mut Type, TypeSlot, &Pos)) {
    for class in &mut module.classes {
        for field in &mut class.fields {
            visit(&mut field.ty, TypeSlot::Storage, &field.pos);
            if let Some(init) = &mut field.init {
                expression(init, visit);
            }
        }
        if let Some(signature) = &mut class.index_signature {
            visit(&mut signature.index_ty, TypeSlot::Storage, &class.pos);
            visit(&mut signature.element_ty, TypeSlot::Storage, &class.pos);
        }
        if let Some(constructor) = &mut class.ctor {
            function(constructor, visit);
        }
        for method in &mut class.methods {
            function(method, visit);
        }
    }
    for global in &mut module.globals {
        visit(&mut global.ty, TypeSlot::Storage, &global.pos);
        expression(&mut global.init, visit);
    }
    for declared in &mut module.functions {
        function(declared, visit);
    }
    for foreign in &mut module.foreign_fns {
        for parameter in &mut foreign.params {
            visit(&mut parameter.ty, TypeSlot::Storage, &parameter.pos);
        }
        visit(&mut foreign.ret, TypeSlot::Storage, &foreign.pos);
    }
    for statement in &mut module.top_level {
        stmt(statement, visit);
    }
    for signature in &mut module.operation_signatures {
        for parameter in &mut signature.parameter_types {
            visit(parameter, TypeSlot::Storage, &module.entry_pos);
        }
        if let Some(result) = &mut signature.return_type {
            visit(result, TypeSlot::Storage, &module.entry_pos);
        }
        if let super::OperationSignatureTarget::ContextBytes(_, ty) = &mut signature.target {
            visit(ty, TypeSlot::Storage, &module.entry_pos);
        }
    }
    for entry in &mut module.host_entries {
        for parameter in &mut entry.signature.parameters {
            visit(parameter, TypeSlot::Storage, &entry.pos);
        }
    }
}

fn function(declared: &mut Function, visit: &mut dyn FnMut(&mut Type, TypeSlot, &Pos)) {
    for parameter in &mut declared.params {
        visit(&mut parameter.ty, TypeSlot::Storage, &parameter.pos);
        if let Some(default) = &mut parameter.default {
            expression(default, visit);
        }
    }
    visit(&mut declared.ret, TypeSlot::FunctionResult, &declared.pos);
    for statement in &mut declared.body {
        stmt(statement, visit);
    }
}

fn stmt(statement: &mut Stmt, visit: &mut dyn FnMut(&mut Type, TypeSlot, &Pos)) {
    match statement {
        Stmt::Let { ty, pos, .. }
        | Stmt::ForOf { ty, pos, .. }
        | Stmt::GeneratorForOf { ty, pos, .. } => visit(ty, TypeSlot::Local, pos),
        Stmt::Try {
            binding: Some((_, ty)),
            pos,
            ..
        } => visit(ty, TypeSlot::Local, pos),
        Stmt::Using { bindings, .. } => {
            for binding in bindings {
                visit(&mut binding.ty, TypeSlot::Local, &binding.pos);
            }
        }
        Stmt::Expr(_)
        | Stmt::Return { .. }
        | Stmt::If { .. }
        | Stmt::While { .. }
        | Stmt::For { .. }
        | Stmt::Switch { .. }
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::Block(_)
        | Stmt::Throw { .. }
        | Stmt::Try { binding: None, .. } => {}
    }
    for child in statement.children_mut() {
        match child {
            HirChildMut::Expr(child) => expression(child, visit),
            HirChildMut::Stmt(child) => stmt(child, visit),
        }
    }
}

fn expression(value: &mut Expr, visit: &mut dyn FnMut(&mut Type, TypeSlot, &Pos)) {
    visit(&mut value.ty, TypeSlot::Expression, &value.pos);
    match &mut value.kind {
        ExprKind::Local(_, ty, _) => visit(ty, TypeSlot::Local, &value.pos),
        ExprKind::Call {
            callee: Callee::ContextBytes { ty, .. },
            ..
        } => visit(ty, TypeSlot::Storage, &value.pos),
        ExprKind::Lambda {
            params,
            ret,
            captures,
            ..
        } => {
            for parameter in params {
                visit(&mut parameter.ty, TypeSlot::Storage, &parameter.pos);
            }
            visit(ret, TypeSlot::Storage, &value.pos);
            for capture in captures {
                visit(&mut capture.ty, TypeSlot::Local, &value.pos);
            }
        }
        _ => {}
    }
    for child in value.children_mut() {
        match child {
            HirChildMut::Expr(child) => expression(child, visit),
            HirChildMut::Stmt(child) => stmt(child, visit),
        }
    }
}

/// Maps each `undefined` spelling in the module to `null` (§195.7 rule 2).
pub(crate) fn erase_undefined(module: &mut Module) {
    for_each_type(module, &mut |ty, _, _| ty.erase_undefined());
}
