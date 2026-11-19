//! Borrowed member parts retain the node kind and its source span (compiler.md §341 rule 5).
use oxc_ast::ast;
use oxc_span::{GetSpan, Span};

pub(super) enum MemberKey<'s, 'a> {
    Ident(&'s ast::IdentifierName<'a>),
    Computed(&'s ast::ComputedMemberExpression<'a>),
    PrivateName(&'s ast::PrivateIdentifier<'a>),
}
pub(super) struct MemberView<'s, 'a> {
    node: &'s ast::MemberExpression<'a>,
    pub span: Span,
    pub object: &'s ast::Expression<'a>,
    pub prop: MemberKey<'s, 'a>,
}
impl<'a> std::ops::Deref for MemberView<'_, 'a> {
    type Target = ast::MemberExpression<'a>;
    fn deref(&self) -> &Self::Target {
        self.node
    }
}
pub(super) fn member_view<'s, 'a>(node: &'s ast::MemberExpression<'a>) -> MemberView<'s, 'a> {
    let prop = match node {
        ast::MemberExpression::StaticMemberExpression(member) => MemberKey::Ident(&member.property),
        ast::MemberExpression::ComputedMemberExpression(member) => MemberKey::Computed(member),
        ast::MemberExpression::PrivateFieldExpression(member) => {
            MemberKey::PrivateName(&member.field)
        }
    };
    MemberView {
        node,
        span: node.span(),
        object: node.object(),
        prop,
    }
}
