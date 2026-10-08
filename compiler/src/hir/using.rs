//! The bindings of a `using` scope (`compiler.md` §115.5 rule 5).

use super::{BinOp, Callee, Expr, ExprKind, Stmt, Symbol, DISPOSE_METHOD_NAME};
use crate::diag::Pos;
use crate::types::Type;

/// One binding of a [`Stmt::Using`] scope.
///
/// The binding names the local that holds the resource. A `switch`
/// binding names its storage and its active flag (`compiler.md` §97.1
/// rule 7), because the declaration in a case arm is not visible at the
/// exits of the `switch`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct UsingBinding {
    /// The local that holds the resource.
    pub name: String,
    /// The declared type: a reference class, or that class or `null`.
    pub ty: Type,
    /// The `switch` active flag, set when the declaration runs.
    pub active: Option<String>,
    /// Position of the declaration: the position of each hook call.
    pub pos: Pos,
}

impl UsingBinding {
    /// A binding of the local `name` with the declared type `ty`.
    #[must_use]
    pub fn new(name: String, ty: Type, active: Option<String>, pos: Pos) -> Self {
        Self {
            name,
            ty,
            active,
            pos,
        }
    }

    /// Whether the binding can hold `null` (`compiler.md` §97.1 rule 4).
    #[must_use]
    pub fn nullable(&self) -> bool {
        matches!(self.ty, Type::Nullable(_))
    }

    /// The hook call of the binding with its guards: the active flag first,
    /// then the null test (`compiler.md` §97.1 rules 4 and 7). The call
    /// receiver carries the non-null class type.
    #[must_use]
    pub fn hook(&self) -> Stmt {
        let pos = &self.pos;
        let local = |name: &str, ty: Type| Expr {
            pending_work: None,
            kind: ExprKind::Local(
                name.to_string(),
                if name == self.name {
                    self.ty.clone()
                } else {
                    ty.clone()
                },
                false,
            ),
            ty,
            pos: pos.clone(),
        };
        let receiver_type = match &self.ty {
            Type::Nullable(inner) => inner.as_ref().clone(),
            other => other.clone(),
        };
        let mut hook = Stmt::Expr(Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: Callee::Method {
                    recv: Box::new(local(&self.name, receiver_type)),
                    name: Symbol::from_full_text(DISPOSE_METHOD_NAME),
                },
                args: Vec::new(),
            },
            ty: Type::Void,
            pos: pos.clone(),
        });
        if self.nullable() {
            hook = Stmt::If {
                cond: Expr {
                    pending_work: None,
                    kind: ExprKind::Binary {
                        op: BinOp::Ne,
                        left: Box::new(local(&self.name, self.ty.clone())),
                        right: Box::new(Expr {
                            pending_work: None,
                            kind: ExprKind::Null,
                            ty: Type::Null,
                            pos: pos.clone(),
                        }),
                    },
                    ty: Type::Bool,
                    pos: pos.clone(),
                },
                then: vec![hook],
                els: None,
                pos: pos.clone(),
            };
        }
        if let Some(active) = &self.active {
            hook = Stmt::If {
                cond: local(active, Type::Bool),
                then: vec![hook],
                els: None,
                pos: pos.clone(),
            };
        }
        hook
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClassId;

    fn call_receiver(statement: &Stmt) -> &Expr {
        let Stmt::Expr(Expr {
            kind:
                ExprKind::Call {
                    callee: Callee::Method { recv, name },
                    args,
                },
            ..
        }) = statement
        else {
            panic!("a hook call: {statement:?}");
        };
        assert_eq!(name.full_text(), DISPOSE_METHOD_NAME);
        assert!(args.is_empty());
        recv
    }

    #[test]
    fn a_plain_binding_disposes_with_a_bare_call() {
        let pos = Pos::new("using.ts", 2, 3);
        let binding = UsingBinding::new("r".to_string(), Type::Class(ClassId(1)), None, pos);
        assert!(!binding.nullable());
        let hook = binding.hook();
        let receiver = call_receiver(&hook);
        assert_eq!(
            receiver.kind,
            ExprKind::Local("r".to_string(), binding.ty.clone(), false)
        );
        assert_eq!(receiver.ty, Type::Class(ClassId(1)));
    }

    #[test]
    fn a_switch_binding_tests_its_flag_before_its_null_test() {
        let pos = Pos::new("using.ts", 4, 5);
        let ty = Type::Nullable(Box::new(Type::Class(ClassId(2))));
        let binding = UsingBinding::new(
            "storage".to_string(),
            ty.clone(),
            Some("active".to_string()),
            pos,
        );
        assert!(binding.nullable());
        let Stmt::If {
            cond: flag,
            then,
            els: None,
            pos: _,
        } = binding.hook()
        else {
            panic!("the active-flag guard");
        };
        assert_eq!(
            flag.kind,
            ExprKind::Local("active".to_string(), Type::Bool, false)
        );
        let [Stmt::If {
            cond: null_test,
            then: call,
            els: None,
            pos: _,
        }] = then.as_slice()
        else {
            panic!("the null guard inside the flag guard");
        };
        let ExprKind::Binary {
            op: BinOp::Ne,
            left,
            right,
        } = &null_test.kind
        else {
            panic!("a null comparison");
        };
        assert_eq!(left.ty, ty);
        assert_eq!(right.kind, ExprKind::Null);
        let [call] = call.as_slice() else {
            panic!("one call");
        };
        assert_eq!(call_receiver(call).ty, Type::Class(ClassId(2)));
    }
}
