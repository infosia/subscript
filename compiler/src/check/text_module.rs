//! The `subscript:text` standard module (compiler.md §193).
//!
//! The import is the only opt-in: the module is computation in the
//! runtime, so no build option enables it. Each imported name binds to
//! one [`StrFn`] operation, and a call lowers to that runtime function
//! with the string argument first, as a `String` method call does.

use super::{Checker, FnCtx, ParamSig, ScopeItem};
use crate::check::initializer::TypeState;
use crate::check::rejection::RejectionSite;
use crate::diag::Pos;
use crate::hir::{self, Callee, ExprKind, StrFn};
use crate::types::Type;
use swc_common::Spanned;
use swc_ecma_ast as ast;

/// The module specifier.
pub(crate) const SPECIFIER: &str = "subscript:text";

/// The accepted import and call forms, for each diagnostic of the module.
pub(super) const FORMS: &str = "import named functions from subscript:text; accepted forms: graphemeLength(s: string): i32; sliceGraphemes(s: string, start: i32, end?: i32): string";

/// The operation that an exported name of the module binds to.
fn export(name: &str) -> Option<StrFn> {
    StrFn::ALL
        .into_iter()
        .find(|f| !f.is_method() && f.name() == name)
}

fn param(ty: Type, has_default: bool) -> ParamSig {
    ParamSig {
        annotated: false,
        name: String::new(),
        state: TypeState::decided(ty),
        initializer: None,
        has_default,
    }
}

impl Checker<'_> {
    /// Binds the specifiers of one `subscript:text` import. A named
    /// import of an export binds the operation; every other form is
    /// rejected.
    pub(super) fn text_module_import(
        &mut self,
        import: &ast::ImportDecl,
        additions: &mut Vec<(String, ScopeItem, Pos, bool)>,
    ) {
        if import.specifiers.is_empty() {
            self.reject_subset(
                RejectionSite::TextModuleImportForm,
                FORMS,
                self.pos(import.src.span),
            );
        }
        for spec in &import.specifiers {
            match spec {
                ast::ImportSpecifier::Named(named) => {
                    let local = named.local.sym.to_string();
                    let imported = named
                        .imported
                        .as_ref()
                        .map_or_else(|| local.clone(), |name| name.atom().to_string());
                    let item = if let Some(operation) = export(&imported) {
                        ScopeItem::StandardText(operation)
                    } else {
                        self.reject_subset(
                            RejectionSite::TextModuleMember,
                            format!("subscript:text has no export `{imported}`; {FORMS}"),
                            self.pos(spec.span()),
                        );
                        ScopeItem::Poisoned
                    };
                    additions.push((
                        local,
                        item,
                        self.pos(named.local.span),
                        super::signatures::type_only_import(import, named),
                    ));
                }
                ast::ImportSpecifier::Default(ast::ImportDefaultSpecifier { local, .. })
                | ast::ImportSpecifier::Namespace(ast::ImportStarAsSpecifier { local, .. }) => {
                    self.reject_subset(
                        RejectionSite::TextModuleImportForm,
                        FORMS,
                        self.pos(spec.span()),
                    );
                    additions.push((
                        local.sym.to_string(),
                        ScopeItem::Poisoned,
                        self.pos(local.span),
                        false,
                    ));
                }
            }
        }
    }

    /// A call of a `subscript:text` function. The string argument
    /// becomes the receiver of the [`Callee::Str`] call, and an omitted
    /// `end` is the `i32::MAX` "to the end" value of `slice`.
    pub(super) fn check_text_call(
        &mut self,
        operation: StrFn,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let optional_end = operation == StrFn::SliceGraphemes;
        let mut params = vec![param(Type::Str, false)];
        let count = operation.params().len();
        params.extend((0..count).map(|index| param(Type::I32, optional_end && index == 1)));
        let required = params.iter().filter(|p| !p.has_default).count();
        let mut args = self.check_args(
            RejectionSite::TextModuleArguments,
            &params,
            &call.args,
            fx,
            &pos,
            operation.name(),
        );
        if args.len() < required || args.len() > params.len() {
            return self.err_expr(pos);
        }
        if args.len() < params.len() {
            args.push(hir::Expr {
                pending_work: None,
                kind: ExprKind::Int(i64::from(i32::MAX)),
                ty: Type::I32,
                pos: pos.clone(),
            });
        }
        let ty = if operation == StrFn::GraphemeLength {
            Type::I32
        } else {
            Type::Str
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Str(operation),
                args,
            },
            ty,
            pos,
        }
    }

    /// A `subscript:text` function read as a value.
    pub(super) fn reject_text_function_value(&mut self, name: &str, pos: Pos) -> hir::Expr {
        self.reject_subset(
            RejectionSite::TextModuleFunctionValue,
            format!("`{name}` from subscript:text can only be called; {FORMS}"),
            pos.clone(),
        );
        self.err_expr(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exports_are_the_two_grapheme_functions() {
        assert_eq!(export("graphemeLength"), Some(StrFn::GraphemeLength));
        assert_eq!(export("sliceGraphemes"), Some(StrFn::SliceGraphemes));
        assert_eq!(export("normalize"), None);
        assert_eq!(export("slice"), None);
        for f in StrFn::ALL.into_iter().filter(|f| !f.is_method()) {
            assert!(FORMS.contains(f.api_signature()), "{}", f.name());
        }
    }
}
