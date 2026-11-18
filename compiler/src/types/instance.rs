//! Concrete types with declared constraints (compiler.md §335 rule 1).

use super::Type;

/// An instance substitution with its source parameter and interface constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ConstrainedInstance {
    /// The declaration's source identity.
    pub identity: u32,
    /// The parameter's source name.
    pub name: String,
    /// The declared interface constraint; None marks an unconstrained parameter in a result template.
    pub constraint: Option<Type>,
    /// The type used for assignment, layout, and lowering.
    pub instance: Type,
}

impl Type {
    /// Removes instance constraints at every depth (compiler.md §335 rule 1).
    #[must_use]
    pub fn instance_type(&self) -> Type {
        self.map_instances(&|instance| Some(instance.instance.instance_type()))
    }

    /// Applies call-site arguments to declared result parameters (compiler.md §335 rule 1b).
    pub(crate) fn substitute_instances(&self, arguments: &[(u32, Type)]) -> Type {
        self.map_instances(&|instance| {
            arguments
                .iter()
                .find(|(identity, _)| *identity == instance.identity)
                .map(|(_, ty)| ty.clone())
        })
    }

    fn map_instances(&self, replace: &impl Fn(&ConstrainedInstance) -> Option<Type>) -> Type {
        let map = |ty: &Type| Box::new(ty.map_instances(replace));
        match self {
            Self::ConstrainedInstance(instance) => {
                replace(instance).unwrap_or_else(|| self.clone())
            }
            Self::Array(inner) => Self::Array(map(inner)),
            Self::FixedArray(inner, n) => Self::FixedArray(map(inner), *n),
            Self::Nullable(inner, absence) => Self::Nullable(map(inner), *absence),
            Self::Map(key, value) => Self::Map(map(key), map(value)),
            Self::Set(inner) => Self::Set(map(inner)),
            Self::Worker(input, output) => Self::Worker(map(input), map(output)),
            Self::Inbox(inner) => Self::Inbox(map(inner)),
            Self::Outbox(inner) => Self::Outbox(map(inner)),
            Self::Generator(inner) => Self::Generator(map(inner)),
            Self::AsyncHandle(inner) => Self::AsyncHandle(map(inner)),
            Self::IterResult(inner) => Self::IterResult(map(inner)),
            Self::Func(signature) => {
                let mut signature = signature.clone();
                signature.params = signature
                    .params
                    .iter()
                    .map(|ty| ty.map_instances(replace))
                    .collect();
                signature.ret = signature.ret.map_instances(replace);
                Self::Func(signature)
            }
            Self::GenericUnion(members) => {
                Self::GenericUnion(members.iter().map(|ty| ty.map_instances(replace)).collect())
            }
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ClassId, HandleClass, HandleKind, InterfaceType};

    #[test]
    fn substituted_types_keep_identity_and_use_the_instance_representation() {
        let constraint = Type::Interface(Box::new(InterfaceType {
            identity: 0,
            name: "R".into(),
        }));
        let parameter = |name: &str, identity| {
            Type::ConstrainedInstance(Box::new(ConstrainedInstance {
                name: name.into(),
                identity,
                constraint: Some(constraint.clone()),
                instance: Type::Class(ClassId(0)),
            }))
        };
        let u = parameter("U", 1);
        let v = parameter("V", 2);
        assert_eq!(
            crate::hir::AssocKeyKind::of(&u, &|_| false),
            Some(crate::hir::AssocKeyKind::Ref)
        );
        assert_eq!(
            crate::hir::ArrElemKind::of(&u, &|_| false),
            Some(crate::hir::ArrElemKind::Int)
        );
        assert_eq!(crate::hir::AssocKeyKind::of(&u, &|_| true), None);
        assert_eq!(crate::hir::ArrElemKind::of(&u, &|_| true), None);
        assert_eq!(u, v);
        assert_eq!(u, Type::Class(ClassId(0)));
        assert_eq!(Type::Class(ClassId(0)), v);
        assert_ne!(u, Type::Class(ClassId(1)));
        assert_eq!(u.instance_type(), v.instance_type());
        assert_eq!(u.instance_type(), Type::Class(ClassId(0)));
        assert_eq!(
            u.handle_kind(&[HandleClass::Reference]),
            Some(HandleKind::ReferenceClass)
        );
        assert!(
            !u.zero_is_value(&[HandleClass::Reference], &|_| Vec::<&Type>::new(), &|_| {
                None
            })
        );
        let function = Type::func(vec![Type::array(u.clone())], Type::nullable(v.clone()));
        assert_eq!(
            function.instance_type(),
            Type::func(
                vec![Type::array(Type::Class(ClassId(0)))],
                Type::nullable(Type::Class(ClassId(0)))
            )
        );
        assert_eq!(
            Type::map(Type::I32, u.clone()).instance_type(),
            Type::map(Type::I32, Type::Class(ClassId(0)))
        );
        assert_eq!(u.contained_types(), vec![&Type::Class(ClassId(0))]);
        let Type::ConstrainedInstance(instance) = u else {
            panic!("the substituted type keeps its declaration");
        };
        assert_eq!(instance.name, "U");
        assert_eq!(instance.identity, 1);
        assert_eq!(instance.constraint, Some(constraint));
    }

    #[test]
    fn equality_and_hash_ignore_instance_constraints_at_every_depth() {
        use std::collections::hash_map::DefaultHasher;
        use std::collections::HashSet;
        use std::hash::{Hash, Hasher};
        let plain = Type::Class(ClassId(0));
        let parameter = |identity| {
            Type::ConstrainedInstance(Box::new(ConstrainedInstance {
                identity,
                name: format!("T{identity}"),
                constraint: Some(Type::Interface(Box::new(InterfaceType {
                    identity: identity as usize,
                    name: format!("I{identity}"),
                }))),
                instance: plain.clone(),
            }))
        };
        let shapes: &[fn(Type) -> Type] = &[
            |ty| ty,
            Type::array,
            |ty| Type::FixedArray(Box::new(ty), 3),
            |ty| Type::map(ty.clone(), ty),
            |ty| Type::Set(Box::new(ty)),
            |ty| Type::Worker(Box::new(ty.clone()), Box::new(ty)),
            |ty| Type::Inbox(Box::new(ty)),
            |ty| Type::Outbox(Box::new(ty)),
            |ty| Type::Generator(Box::new(ty)),
            Type::async_handle,
            |ty| Type::IterResult(Box::new(ty)),
            Type::nullable,
            |ty| Type::func(vec![Type::array(ty.clone())], Type::nullable(ty)),
            |ty| Type::GenericUnion(vec![Type::I32, ty].into_boxed_slice()),
        ];
        let hash = |ty: &Type| {
            let mut state = DefaultHasher::new();
            ty.hash(&mut state);
            state.finish()
        };
        for shape in shapes {
            let types = [
                shape(plain.clone()),
                shape(parameter(1)),
                shape(parameter(2)),
            ];
            for left in &types {
                for right in &types {
                    assert_eq!(left, right);
                    assert_eq!(hash(left), hash(right));
                }
            }
            for first in &types {
                let mut keys = HashSet::from([first.clone()]);
                for ty in &types {
                    assert!(keys.contains(ty));
                    assert!(!keys.insert(ty.clone()));
                }
                assert_eq!(keys.len(), 1);
            }
        }
        assert_ne!(
            Type::TypeParameter(Box::new(crate::types::TypeParameterType {
                identity: 1,
                name: "U".into(),
                constraint: None
            })),
            Type::TypeParameter(Box::new(crate::types::TypeParameterType {
                identity: 2,
                name: "V".into(),
                constraint: None
            }))
        );
    }
}
