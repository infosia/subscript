//! Resolves namespace qualifiers to ordinary imported declaration bindings.

use super::*;

impl<'p> Checker<'p> {
    /// Creates a private lookup key for the exported declaration identity.
    /// The key is checker-only; HIR retains the declaration's existing symbol.
    pub(super) fn namespace_member_ident(
        &mut self,
        namespace: &ast::Ident,
        member: &ast::IdentName,
    ) -> Option<ast::Ident> {
        let ScopeItem::Namespace { module, source } =
            self.type_scope_item(namespace.sym.as_ref())?
        else {
            return None;
        };
        let item = module.and_then(|module| self.exports[module].get(member.sym.as_ref()).cloned());
        let item = item.unwrap_or_else(|| {
            if module.is_some() {
                self.resolution_error(
                    RuleCode::S016,
                    format!("`{}` is not exported by `{source}`", member.sym),
                    self.pos(member.span),
                );
            }
            ScopeItem::Poisoned
        });
        let key = format!("{}.{}", namespace.sym, member.sym);
        self.file_scopes[self.cur_file].insert(
            key.clone(),
            ScopeBinding {
                item,
                imported: true,
                type_only: false,
            },
        );
        Some(ast::Ident::new_no_ctxt(key.into(), member.span))
    }

    /// Resolves only the qualifier spine. Arguments and local scopes stay untouched.
    pub(super) fn resolve_namespace_expr(
        &mut self,
        expr: &ast::Expr,
        fx: &FnCtx,
    ) -> Option<ast::Expr> {
        match expr {
            ast::Expr::Paren(paren) => {
                let resolved = self.resolve_namespace_expr(&paren.expr, fx)?;
                let mut paren = paren.clone();
                paren.expr = Box::new(resolved);
                Some(ast::Expr::Paren(paren))
            }
            ast::Expr::Member(member) => {
                if let (ast::Expr::Ident(namespace), ast::MemberProp::Ident(name)) =
                    (super::expr::unparen_expr(&member.obj), &member.prop)
                {
                    if !fx.owns_local_name(namespace.sym.as_ref()) {
                        if let Some(ident) = self.namespace_member_ident(namespace, name) {
                            return Some(ast::Expr::Ident(ident));
                        }
                    }
                }
                let object = self.resolve_namespace_expr(&member.obj, fx)?;
                let mut member = member.clone();
                member.obj = Box::new(object);
                Some(ast::Expr::Member(member))
            }
            _ => None,
        }
    }
}
