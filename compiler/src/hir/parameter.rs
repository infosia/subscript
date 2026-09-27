//! Parameter contracts in checked HIR.
use super::{Expr, ForeignTypeProvenance};
use crate::{diag::Pos, types::Type};

/// One function parameter.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Resolved type.
    pub ty: Type,
    /// Checked default value, when declared (`a11`).
    pub default: Option<Expr>,
    /// C spelling absorbed at this foreign boundary parameter.
    pub foreign_provenance: Option<ForeignTypeProvenance>,
    /// Whether this parameter reaches an escape boundary (compiler.md §118).
    pub escapes: bool,
    /// Position of the parameter.
    pub pos: Pos,
}

impl super::Module {
    /// Whether this type can carry a closure environment (compiler.md §118).
    #[must_use]
    pub fn carries_capture(&self, ty: &Type) -> bool {
        fn carries(
            m: &super::Module,
            ty: &Type,
            seen: &mut std::collections::HashSet<usize>,
        ) -> bool {
            match ty {
                Type::Func(_) | Type::Generator(_) => true,
                Type::Array(t)
                | Type::FixedArray(t, _)
                | Type::Set(t)
                | Type::Nullable(t)
                | Type::IterResult(t) => carries(m, t, seen),
                Type::Map(k, v) => carries(m, k, seen) || carries(m, v, seen),
                Type::Class(id) => {
                    seen.insert(id.0)
                        && m.classes
                            .get(id.0)
                            .is_some_and(|c| c.fields.iter().any(|f| carries(m, &f.ty, seen)))
                }
                _ => false,
            }
        }
        carries(self, ty, &mut std::collections::HashSet::new())
    }
}
