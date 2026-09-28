use super::*;

impl ExprKind {
    /// Reports whether this expression kind can produce a fresh async owner.
    pub fn produces_fresh_async_owner(&self) -> bool {
        match self {
            Self::AsyncHandleCreate { .. }
            | Self::AsyncHandleTransfer { .. }
            | Self::Call { .. }
            | Self::ArrayLit(_)
            | Self::ArraySpreadLit(_) => true,
            Self::Cond { then, els, .. } => {
                then.kind.produces_fresh_async_owner() && els.kind.produces_fresh_async_owner()
            }
            _ => false,
        }
    }
}

impl AsyncCallee {
    /// Returns the receiver of an async method target, if this is one.
    #[must_use]
    pub fn receiver(&self) -> Option<&Expr> {
        match self {
            Self::Function(_) => None,
            Self::Method { receiver, .. } => Some(receiver),
        }
    }

    /// Returns the receiver mutably for HIR analysis/rewriting passes.
    #[must_use]
    pub(crate) fn receiver_mut(&mut self) -> Option<&mut Expr> {
        match self {
            Self::Function(_) => None,
            Self::Method { receiver, .. } => Some(receiver),
        }
    }
}

impl From<IterKind> for SpreadKind {
    fn from(kind: IterKind) -> Self {
        match kind {
            IterKind::Array => Self::Array,
            IterKind::FixedArray => Self::FixedArray,
            IterKind::SetValues => Self::SetValues,
            IterKind::StringCodePoints => Self::StringCodePoints,
        }
    }
}

impl From<IterKind> for ForOfKind {
    fn from(kind: IterKind) -> Self {
        match kind {
            IterKind::Array => Self::ArrayValues,
            IterKind::FixedArray => Self::FixedArrayValues,
            IterKind::SetValues => Self::SetValues,
            IterKind::StringCodePoints => Self::StringCodePoints,
        }
    }
}

impl Expr {
    /// Returns the leaves that can flow into this expression's value.
    pub fn flow_leaves(&self) -> impl Iterator<Item = &Expr> {
        fn collect<'a>(expression: &'a Expr, leaves: &mut Vec<&'a Expr>) {
            match &expression.kind {
                ExprKind::Cast(inner) | ExprKind::Assign { value: inner, .. } => {
                    collect(inner, leaves);
                }
                ExprKind::Cond { then, els, .. } => {
                    collect(then, leaves);
                    collect(els, leaves);
                }
                ExprKind::ArrayLit(elements) => {
                    for element in elements {
                        collect(element, leaves);
                    }
                }
                _ => leaves.push(expression),
            }
        }

        let mut leaves = Vec::new();
        collect(self, &mut leaves);
        leaves.into_iter()
    }

    /// Returns every immediate expression or statement child in source order.
    pub fn children(&self) -> Vec<HirChild<'_>> {
        use ExprKind as K;

        match &self.kind {
            K::Unary { operand, .. }
            | K::Cast(operand)
            | K::Length(operand)
            | K::AsyncHandleAwait(operand)
            | K::AsyncHandleTransfer { value: operand, .. } => {
                vec![HirChild::Expr(operand)]
            }
            K::AbsenceTest { value, .. } => vec![HirChild::Expr(value)],
            K::Binary { left, right, .. }
            | K::Assign {
                target: left,
                value: right,
                ..
            } => vec![HirChild::Expr(left), HirChild::Expr(right)],
            K::Call { callee, args } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                match callee {
                    Callee::Value(value) => children.push(HirChild::Expr(value)),
                    Callee::Method { recv, .. } => children.push(HirChild::Expr(recv)),
                    _ => {}
                }
                children.extend(args.iter().map(HirChild::Expr));
                children
            }
            K::New { args, .. } => args.iter().map(HirChild::Expr).collect(),
            K::DescriptorLit { fields, .. } => {
                fields.iter().flatten().map(HirChild::Expr).collect()
            }
            K::Field { obj, .. } => vec![HirChild::Expr(obj)],
            K::Index { obj, index, .. } => {
                vec![HirChild::Expr(obj), HirChild::Expr(index)]
            }
            K::ArrayLit(elements) => elements.iter().map(HirChild::Expr).collect(),
            K::ArraySpreadLit(elements) => elements
                .iter()
                .map(|element| HirChild::Expr(&element.expr))
                .collect(),
            K::Template(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    TplPart::Expr(expr) => Some(HirChild::Expr(expr)),
                    TplPart::Text(_) => None,
                })
                .collect(),
            K::Lambda { params, body, .. } => {
                let mut children = Vec::new();
                children.extend(
                    params
                        .iter()
                        .filter_map(|parameter| parameter.default.as_ref())
                        .map(HirChild::Expr),
                );
                children.extend(body.iter().map(HirChild::Stmt));
                children
            }
            K::Yield(Some(value)) => vec![HirChild::Expr(value)],
            K::AsyncCall { callee, args } | K::AsyncHandleCreate { callee, args, .. } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                if let Some(receiver) = callee.receiver() {
                    children.push(HirChild::Expr(receiver));
                }
                children.extend(args.iter().map(HirChild::Expr));
                children
            }
            K::Cond { cond, then, els } => vec![
                HirChild::Expr(cond),
                HirChild::Expr(then),
                HirChild::Expr(els),
            ],
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::Local(..)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::RawNew { .. }
            | K::Yield(None)
            | K::AsyncSuspend => Vec::new(),
        }
    }

    /// Returns every mutable immediate child in source order.
    pub(crate) fn children_mut(&mut self) -> Vec<HirChildMut<'_>> {
        use ExprKind as K;

        match &mut self.kind {
            K::Unary { operand, .. }
            | K::Cast(operand)
            | K::Length(operand)
            | K::AsyncHandleAwait(operand)
            | K::AsyncHandleTransfer { value: operand, .. } => {
                vec![HirChildMut::Expr(operand)]
            }
            K::AbsenceTest { value, .. } => vec![HirChildMut::Expr(value)],
            K::Binary { left, right, .. }
            | K::Assign {
                target: left,
                value: right,
                ..
            } => vec![HirChildMut::Expr(left), HirChildMut::Expr(right)],
            K::Call { callee, args } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                match callee {
                    Callee::Value(value) => children.push(HirChildMut::Expr(value)),
                    Callee::Method { recv, .. } => children.push(HirChildMut::Expr(recv)),
                    _ => {}
                }
                children.extend(args.iter_mut().map(HirChildMut::Expr));
                children
            }
            K::New { args, .. } => args.iter_mut().map(HirChildMut::Expr).collect(),
            K::DescriptorLit { fields, .. } => {
                fields.iter_mut().flatten().map(HirChildMut::Expr).collect()
            }
            K::Field { obj, .. } => vec![HirChildMut::Expr(obj)],
            K::Index { obj, index, .. } => {
                vec![HirChildMut::Expr(obj), HirChildMut::Expr(index)]
            }
            K::ArrayLit(elements) => elements.iter_mut().map(HirChildMut::Expr).collect(),
            K::ArraySpreadLit(elements) => elements
                .iter_mut()
                .map(|element| HirChildMut::Expr(&mut element.expr))
                .collect(),
            K::Template(parts) => parts
                .iter_mut()
                .filter_map(|part| match part {
                    TplPart::Expr(expr) => Some(HirChildMut::Expr(expr)),
                    TplPart::Text(_) => None,
                })
                .collect(),
            K::Lambda { params, body, .. } => {
                let mut children = Vec::new();
                children.extend(
                    params
                        .iter_mut()
                        .filter_map(|parameter| parameter.default.as_mut())
                        .map(HirChildMut::Expr),
                );
                children.extend(body.iter_mut().map(HirChildMut::Stmt));
                children
            }
            K::Yield(Some(value)) => vec![HirChildMut::Expr(value)],
            K::AsyncCall { callee, args } | K::AsyncHandleCreate { callee, args, .. } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                if let Some(receiver) = callee.receiver_mut() {
                    children.push(HirChildMut::Expr(receiver));
                }
                children.extend(args.iter_mut().map(HirChildMut::Expr));
                children
            }
            K::Cond { cond, then, els } => vec![
                HirChildMut::Expr(cond),
                HirChildMut::Expr(then),
                HirChildMut::Expr(els),
            ],
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::Local(..)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::RawNew { .. }
            | K::Yield(None)
            | K::AsyncSuspend => Vec::new(),
        }
    }
}

impl Stmt {
    /// Returns every immediate expression or statement child in source order.
    pub fn children(&self) -> Vec<HirChild<'_>> {
        match self {
            Stmt::Let { init, .. } | Stmt::Expr(init) => vec![HirChild::Expr(init)],
            Stmt::Return { value, .. } => value.iter().map(HirChild::Expr).collect(),
            Stmt::If {
                cond, then, els, ..
            } => {
                let mut children =
                    Vec::with_capacity(1 + then.len() + els.as_ref().map_or(0, Vec::len));
                children.push(HirChild::Expr(cond));
                children.extend(then.iter().map(HirChild::Stmt));
                children.extend(els.iter().flatten().map(HirChild::Stmt));
                children
            }
            Stmt::While { cond, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChild::Expr(cond));
                children.extend(body.iter().map(HirChild::Stmt));
                children
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let mut children = Vec::with_capacity(3 + body.len());
                children.extend(init.iter().map(|stmt| HirChild::Stmt(stmt)));
                children.extend(cond.iter().map(HirChild::Expr));
                children.extend(step.iter().map(HirChild::Expr));
                children.extend(body.iter().map(HirChild::Stmt));
                children
            }
            Stmt::ForOf { subject, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChild::Expr(subject));
                children.extend(body.iter().map(HirChild::Stmt));
                children
            }
            Stmt::Switch { disc, cases, .. } => {
                let mut children = Vec::new();
                children.push(HirChild::Expr(disc));
                for case in cases {
                    children.extend(case.test.iter().map(HirChild::Expr));
                    children.extend(case.body.iter().map(HirChild::Stmt));
                }
                children
            }
            Stmt::Block(body) => body.iter().map(HirChild::Stmt).collect(),
            Stmt::Break(_) | Stmt::Continue(_) => Vec::new(),
            Stmt::Throw { value, .. } => vec![HirChild::Expr(value)],
            Stmt::Try { body, handler, .. } => {
                body.iter().chain(handler).map(HirChild::Stmt).collect()
            }
            Stmt::Using { body, .. } => body.iter().map(HirChild::Stmt).collect(),
        }
    }

    /// Returns every mutable immediate child in source order.
    pub(crate) fn children_mut(&mut self) -> Vec<HirChildMut<'_>> {
        match self {
            Stmt::Let { init, .. } | Stmt::Expr(init) => vec![HirChildMut::Expr(init)],
            Stmt::Return { value, .. } => value.iter_mut().map(HirChildMut::Expr).collect(),
            Stmt::If {
                cond, then, els, ..
            } => {
                let mut children =
                    Vec::with_capacity(1 + then.len() + els.as_ref().map_or(0, Vec::len));
                children.push(HirChildMut::Expr(cond));
                children.extend(then.iter_mut().map(HirChildMut::Stmt));
                children.extend(els.iter_mut().flatten().map(HirChildMut::Stmt));
                children
            }
            Stmt::While { cond, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChildMut::Expr(cond));
                children.extend(body.iter_mut().map(HirChildMut::Stmt));
                children
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let mut children = Vec::with_capacity(3 + body.len());
                children.extend(init.iter_mut().map(|stmt| HirChildMut::Stmt(stmt)));
                children.extend(cond.iter_mut().map(HirChildMut::Expr));
                children.extend(step.iter_mut().map(HirChildMut::Expr));
                children.extend(body.iter_mut().map(HirChildMut::Stmt));
                children
            }
            Stmt::ForOf { subject, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChildMut::Expr(subject));
                children.extend(body.iter_mut().map(HirChildMut::Stmt));
                children
            }
            Stmt::Switch { disc, cases, .. } => {
                let mut children = Vec::new();
                children.push(HirChildMut::Expr(disc));
                for case in cases {
                    children.extend(case.test.iter_mut().map(HirChildMut::Expr));
                    children.extend(case.body.iter_mut().map(HirChildMut::Stmt));
                }
                children
            }
            Stmt::Block(body) => body.iter_mut().map(HirChildMut::Stmt).collect(),
            Stmt::Break(_) | Stmt::Continue(_) => Vec::new(),
            Stmt::Throw { value, .. } => vec![HirChildMut::Expr(value)],
            Stmt::Try { body, handler, .. } => body
                .iter_mut()
                .chain(handler)
                .map(HirChildMut::Stmt)
                .collect(),
            Stmt::Using { body, .. } => body.iter_mut().map(HirChildMut::Stmt).collect(),
        }
    }
}
