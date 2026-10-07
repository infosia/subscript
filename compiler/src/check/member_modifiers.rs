//! Class modifier facts and checks for compiler.md §155.

use std::collections::{HashMap, HashSet};
use swc_ecma_ast as ast;

use super::{rejection::RejectionSite, Checker, FnCtx};
use crate::{diag::Pos, types::ClassId};

#[derive(Debug, Clone, Default)]
pub(super) struct ClassModifiers {
    accessibility: HashMap<(bool, bool, String), ast::Accessibility>,
    readonly_fields: HashSet<String>,
    constructor_accessibility: Option<ast::Accessibility>,
    is_abstract: bool,
}

impl ClassModifiers {
    /// Distinct class declarations can share only public instance members.
    pub(super) fn structural_member_is_public(&self, name: &str) -> bool {
        [false, true].into_iter().all(|write| {
            self.accessibility
                .get(&(false, write, name.to_owned()))
                .copied()
                .and_then(restricted)
                .is_none()
        })
    }

    pub(super) fn from_class(class: &ast::Class) -> Self {
        let mut facts = Self {
            is_abstract: class.is_abstract,
            ..Self::default()
        };
        for member in &class.body {
            let (key, is_static, accessibility, access) = match member {
                ast::ClassMember::ClassProp(property) => {
                    if let ast::PropName::Ident(name) = &property.key {
                        if property.readonly && !property.is_static {
                            facts.readonly_fields.insert(name.sym.to_string());
                        }
                    }
                    (
                        &property.key,
                        property.is_static,
                        property.accessibility,
                        None,
                    )
                }
                ast::ClassMember::Method(method) => (
                    &method.key,
                    method.is_static,
                    method.accessibility,
                    match method.kind {
                        ast::MethodKind::Getter => Some(false),
                        ast::MethodKind::Setter => Some(true),
                        ast::MethodKind::Method => None,
                    },
                ),
                ast::ClassMember::Constructor(constructor) => {
                    facts.constructor_accessibility = constructor.accessibility;
                    continue;
                }
                _ => continue,
            };
            if let (ast::PropName::Ident(name), Some(accessibility)) = (key, accessibility) {
                for write in [false, true] {
                    if access.is_none_or(|access| access == write) {
                        facts
                            .accessibility
                            .insert((is_static, write, name.sym.to_string()), accessibility);
                    }
                }
            }
        }
        facts
    }
}

fn restricted(accessibility: ast::Accessibility) -> Option<&'static str> {
    match accessibility {
        ast::Accessibility::Private => Some("private"),
        ast::Accessibility::Protected => Some("protected"),
        ast::Accessibility::Public => None,
    }
}

impl Checker<'_> {
    fn inside_class_body(&self, class: ClassId, fx: &FnCtx) -> bool {
        fx.lexical_class.is_some_and(|owner| {
            owner == class
                || matches!(
                    (self.instance_arguments.get(&owner), self.instance_arguments.get(&class)),
                    (Some((owner, _)), Some((class, _))) if owner == class
                )
        })
    }

    pub(super) fn reject_member_access(
        &mut self,
        class: ClassId,
        name: &str,
        is_static: bool,
        write: bool,
        fx: &FnCtx,
        pos: Pos,
    ) -> bool {
        if self.inside_class_body(class, fx) {
            return false;
        }
        let modifier = self.class_sigs[class.0]
            .modifiers
            .accessibility
            .get(&(is_static, write, name.to_owned()))
            .copied()
            .and_then(restricted);
        let Some(modifier) = modifier else {
            return false;
        };
        self.reject_subset(
            RejectionSite::RestrictedMemberOutsideClass,
            format!(
                "member `{name}` is {modifier} in class `{}`",
                self.classes[class.0].name
            ),
            pos,
        );
        true
    }

    pub(super) fn reject_instance_modifier(
        &mut self,
        class: ClassId,
        name: &str,
        // None is a read. Some records whether a write uses direct `this`.
        write: Option<bool>,
        fx: &FnCtx,
        pos: Pos,
    ) -> bool {
        if self.reject_member_access(class, name, false, write.is_some(), fx, pos.clone()) {
            return true;
        }
        write.is_some_and(|direct_this| {
            self.reject_readonly_field_write(class, name, direct_this, fx, pos)
        })
    }

    pub(super) fn is_readonly_field(&self, class: ClassId, name: &str) -> bool {
        self.class_sigs[class.0]
            .modifiers
            .readonly_fields
            .contains(name)
    }

    pub(super) fn reject_readonly_field_write(
        &mut self,
        class: ClassId,
        name: &str,
        direct_this: bool,
        fx: &FnCtx,
        pos: Pos,
    ) -> bool {
        let direct_constructor = fx.lexical_class == Some(class)
            && fx.constructor_body
            && fx.frames.len() == 1
            && direct_this;
        if self.is_readonly_field(class, name) && !direct_constructor {
            self.reject_subset(
                RejectionSite::ReadonlyFieldWriteOutsideConstructor,
                format!("field `{name}` is readonly in class `{}`; writes require `this` directly in its constructor body", self.classes[class.0].name),
                pos,
            );
            return true;
        }
        false
    }

    pub(super) fn reject_construction_modifier(
        &mut self,
        class: ClassId,
        fx: &FnCtx,
        pos: Pos,
    ) -> bool {
        let facts = &self.class_sigs[class.0].modifiers;
        if facts.is_abstract {
            self.reject_subset(
                RejectionSite::AbstractClassConstructed,
                format!(
                    "class `{}` is abstract; a `new` of it is rejected",
                    self.classes[class.0].name
                ),
                pos,
            );
            return true;
        }
        if !self.inside_class_body(class, fx) {
            if let Some(modifier) = facts.constructor_accessibility.and_then(restricted) {
                self.reject_subset(
                    RejectionSite::RestrictedConstructorOutsideClass,
                    format!(
                        "constructor is {modifier} in class `{}`",
                        self.classes[class.0].name
                    ),
                    pos,
                );
                return true;
            }
        }
        false
    }
}
