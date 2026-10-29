//! Type-only interfaces and their constraints (compiler.md §208).
//!
//! An interface has no layout and no run-time form. It is the constraint
//! of a type parameter only, so each generic instance calls the member of
//! its type argument directly.

use super::*;
use crate::check::rejection::RejectionSite;
use crate::types::InterfaceType;

/// One member of an interface: a method signature or a read accessor
/// signature (§208.1 rule 1).
#[derive(Debug, Clone)]
pub(crate) struct InterfaceMember {
    name: String,
    accessor: bool,
    params: Vec<Type>,
    ret: Type,
}

impl InterfaceMember {
    /// The type of the member: a function type for a method signature,
    /// and the result type for a read accessor signature.
    fn ty(&self) -> Type {
        if self.accessor {
            self.ret.clone()
        } else {
            Type::Func(Box::new(crate::types::FuncType {
                params: self.params.clone(),
                ret: self.ret.clone(),
            }))
        }
    }
}

/// True when `ty` contains the error type: the declaration that gives
/// it already reported its diagnostic.
fn poisoned(ty: &Type) -> bool {
    *ty == Type::Error || ty.contained_types().into_iter().any(poisoned)
}

/// A module-level interface declaration (§208.1 rule 1).
#[derive(Debug, Clone)]
pub(crate) struct InterfaceDef {
    name: String,
    file: usize,
    /// False when the declaration has a form outside the decided surface
    /// (§208.1 rule 2); the name is then poisoned.
    accepted: bool,
    declaration: ast::TsInterfaceDecl,
    members: Vec<InterfaceMember>,
}

/// One class with an `implements` clause that names interfaces.
#[derive(Debug, Clone)]
struct Implementing {
    name: String,
    pos: Pos,
    interfaces: Vec<usize>,
}

/// The interfaces that each class declares (§208.1 rules 3 and 4).
#[derive(Debug, Clone, Default)]
pub(crate) struct ImplementsClauses {
    /// Non-generic classes, in source order.
    classes: Vec<(ClassId, Implementing)>,
    /// Generic class templates, by template key.
    templates: HashMap<String, Implementing>,
}

const MEMBER_FORM: &str = "has a member form outside the decided surface";

impl<'p> Checker<'p> {
    /// Pass A: records a module-level interface declaration and binds its
    /// name when every form is in the decided surface (§208.1 rules 1–2).
    pub(super) fn collect_interface(&mut self, file: usize, declaration: &ast::TsInterfaceDecl) {
        let name = declaration.id.sym.to_string();
        let mut outside = Vec::new();
        if let Some(parameters) = &declaration.type_params {
            outside.push(parameters.span);
        }
        outside.extend(declaration.extends.iter().map(|clause| clause.span));
        let mut names = HashSet::new();
        for element in &declaration.body.body {
            match element {
                ast::TsTypeElement::TsMethodSignature(method) => {
                    let named = match &*method.key {
                        ast::Expr::Ident(key) if !method.computed => {
                            names.insert(key.sym.to_string())
                        }
                        _ => false,
                    };
                    if !named || method.optional || method.type_params.is_some() {
                        outside.push(method.span);
                    }
                    outside.extend(
                        method
                            .params
                            .iter()
                            .filter(|parameter| !matches!(parameter, ast::TsFnParam::Ident(_)))
                            .map(|parameter| parameter.span()),
                    );
                }
                ast::TsTypeElement::TsGetterSignature(getter) => {
                    let named = match &*getter.key {
                        ast::Expr::Ident(key) if !getter.computed => {
                            names.insert(key.sym.to_string())
                        }
                        _ => false,
                    };
                    if !named {
                        outside.push(getter.span);
                    }
                }
                other => outside.push(other.span()),
            }
        }
        // `tsc` merges a second declaration of one name.
        if self
            .interfaces
            .iter()
            .any(|interface| interface.file == file && interface.name == name)
        {
            outside.push(declaration.id.span);
        }
        for span in &outside {
            self.reject_subset(
                RejectionSite::InterfaceMemberForm,
                format!("interface `{name}` {MEMBER_FORM}"),
                self.pos(*span),
            );
        }
        let accepted = outside.is_empty();
        let index = self.interfaces.len();
        self.interfaces.push(InterfaceDef {
            name: name.clone(),
            file,
            accepted,
            declaration: declaration.clone(),
            members: Vec::new(),
        });
        if accepted {
            let pos = self.pos(declaration.id.span);
            self.register_scope_item(file, &name, ScopeItem::Interface(index), pos);
        }
    }

    /// Replaces the pass A copy of a declaration with the declaration
    /// whose namespace qualifiers are resolved.
    pub(super) fn replace_interface_declaration(
        &mut self,
        file: usize,
        declaration: &ast::TsInterfaceDecl,
    ) {
        if let Some(interface) = self.interfaces.iter_mut().find(|interface| {
            interface.file == file && interface.declaration.span == declaration.span
        }) {
            interface.declaration = declaration.clone();
        }
    }

    /// Pass B: resolves the member types of each accepted interface.
    pub(super) fn resolve_interface_signatures(&mut self) {
        for index in 0..self.interfaces.len() {
            if !self.interfaces[index].accepted {
                continue;
            }
            let declaration = self.interfaces[index].declaration.clone();
            self.cur_file = self.interfaces[index].file;
            self.subst.clear();
            let mut members = Vec::new();
            for element in &declaration.body.body {
                let (key, accessor, params, annotation, span) = match element {
                    ast::TsTypeElement::TsMethodSignature(method) => (
                        &method.key,
                        false,
                        method.params.as_slice(),
                        method.type_ann.as_deref(),
                        method.span,
                    ),
                    ast::TsTypeElement::TsGetterSignature(getter) => (
                        &getter.key,
                        true,
                        &[][..],
                        getter.type_ann.as_deref(),
                        getter.span,
                    ),
                    _ => continue,
                };
                let ast::Expr::Ident(key) = &**key else {
                    continue;
                };
                let params = params
                    .iter()
                    .map(|parameter| match parameter {
                        ast::TsFnParam::Ident(binding) => self
                            .resolve_param_pat(&ast::Pat::Ident(binding.clone()))
                            .ty()
                            .clone(),
                        _ => Type::Error,
                    })
                    .collect();
                let ret = match annotation {
                    Some(annotation) => {
                        // The result of a member is a method result (§195.1 rule 4).
                        self.undefined_result = true;
                        let ret = self.resolve_result_type(&annotation.type_ann);
                        self.undefined_result = false;
                        ret
                    }
                    None => {
                        self.reject_subset(
                            RejectionSite::InterfaceResultAnnotationMissing,
                            "interface members require a result type annotation",
                            self.pos(span),
                        );
                        Type::Error
                    }
                };
                members.push(InterfaceMember {
                    name: key.sym.to_string(),
                    accessor,
                    params,
                    ret,
                });
            }
            self.interfaces[index].members = members;
        }
    }

    /// The constraint type of a type parameter: an interface name is the
    /// interface type (§208.1 rule 4); every other form is a value type.
    pub(crate) fn resolve_constraint(&mut self, constraint: &ast::TsType) -> Type {
        if let ast::TsType::TsTypeRef(reference) = constraint {
            if let ast::TsEntityName::Ident(ident) = &reference.type_name {
                let name = ident.sym.as_ref();
                if !self.subst.contains_key(name) {
                    if let Some(ScopeItem::Interface(index)) = self.type_scope_item(name) {
                        if reference.type_params.is_some() {
                            self.reject_subset(
                                RejectionSite::NonGenericClassTypeArguments,
                                format!("`{name}` is not generic"),
                                self.pos(ident.span),
                            );
                            return Type::Error;
                        }
                        return self.interface_type(index);
                    }
                }
            }
        }
        self.resolve_type(constraint)
    }

    fn interface_type(&self, index: usize) -> Type {
        Type::Interface(Box::new(InterfaceType {
            identity: index,
            name: self.interfaces[index].name.clone(),
        }))
    }

    /// §208.1 rule 5: an interface in a type position other than a
    /// constraint.
    pub(super) fn reject_interface_value_type(&mut self, name: &str, pos: Pos) -> Type {
        self.reject_subset(
            RejectionSite::InterfaceValueType,
            format!(
                "interface `{name}` is a constraint only; write a type parameter `T extends {name}`"
            ),
            pos,
        );
        Type::Error
    }

    /// An interface name in a value position. The use site reports once.
    pub(super) fn reject_interface_value_use(&mut self, name: &str, pos: &Pos) {
        if self
            .type_only_value_uses
            .insert((self.cur_file, pos.line, pos.col))
        {
            self.reject_subset(
                RejectionSite::InterfaceValueUse,
                format!("interface `{name}` is a type; it has no value"),
                pos.clone(),
            );
        }
    }

    /// True when `argument` is a class that declares `implements` of the
    /// interface `index` in its own clause (§208.1 rule 4).
    pub(crate) fn declares_interface(&self, argument: &Type, index: usize) -> bool {
        let Type::Class(id) = argument else {
            return false;
        };
        if let Some((_, class)) = self
            .implements
            .classes
            .iter()
            .find(|(class, _)| class == id)
        {
            return class.interfaces.contains(&index);
        }
        self.instance_arguments
            .get(id)
            .and_then(|(key, _)| self.implements.templates.get(key))
            .is_some_and(|template| template.interfaces.contains(&index))
    }

    /// The §208.1 rule 4 diagnostic for a type argument that does not
    /// declare the interface of its constraint.
    pub(crate) fn reject_undeclared_interface(
        &mut self,
        site: RejectionSite,
        argument: &Type,
        interface: &InterfaceType,
        pos: Pos,
    ) {
        let argument = self.type_name(argument);
        self.reject_subset(
            site,
            format!(
                "`{argument}` does not declare `implements {}`",
                interface.name
            ),
            pos,
        );
    }

    /// Records the interfaces of each `implements` clause after every
    /// name is bound (§208.1 rule 3). A name that is not an interface
    /// keeps the §205 diagnostic.
    pub(super) fn collect_implements_clauses(&mut self) {
        let prog = self.prog;
        for (file, source) in prog.files.iter().enumerate() {
            if source.dts {
                continue;
            }
            self.cur_file = file;
            for item in &source.module.body {
                let Some(ast::Decl::Class(class)) = module_decl(item) else {
                    continue;
                };
                let mut interfaces = Vec::new();
                for clause in &class.class.implements {
                    if let Some(index) = self.clause_interface(clause) {
                        interfaces.push(index);
                    }
                }
                if interfaces.is_empty() {
                    continue;
                }
                let name = class.ident.sym.to_string();
                let symbol = self.declaration_symbol(file, &name);
                let implementing = Implementing {
                    name,
                    pos: self.pos(class.ident.span),
                    interfaces,
                };
                if class.class.type_params.is_some() {
                    self.implements.templates.insert(symbol, implementing);
                } else if let Some(&id) = self.class_ids.get(&symbol) {
                    self.implements.classes.push((id, implementing));
                }
            }
        }
    }

    /// The interface that one clause names. A poisoned name gives `None`
    /// with no diagnostic; another name gives the §205 diagnostic.
    fn clause_interface(&mut self, clause: &ast::TsExprWithTypeArgs) -> Option<usize> {
        let pos = self.pos(clause.span);
        let name = match &*clause.expr {
            ast::Expr::Ident(ident) => ident.sym.to_string(),
            ast::Expr::Member(member) => match (&*member.obj, &member.prop) {
                (ast::Expr::Ident(object), ast::MemberProp::Ident(property)) => {
                    format!("{}.{}", object.sym, property.sym)
                }
                _ => "the clause".to_string(),
            },
            _ => "the clause".to_string(),
        };
        if let ast::Expr::Ident(ident) = &*clause.expr {
            match self.type_scope_item(ident.sym.as_ref()) {
                Some(ScopeItem::Poisoned) => return None,
                Some(ScopeItem::Interface(index)) => {
                    if clause.type_args.is_some() {
                        self.reject_subset(
                            RejectionSite::NonGenericClassTypeArguments,
                            format!("`{name}` is not generic"),
                            pos,
                        );
                        return None;
                    }
                    return Some(index);
                }
                _ => {}
            }
        }
        self.reject_subset(
            RejectionSite::ClassImplementsClause,
            format!("`implements` names an interface; `{name}` is not an interface"),
            pos,
        );
        None
    }

    /// §208.1 rule 3 for each non-generic class, after every declaration
    /// type is decided.
    pub(super) fn check_class_implements(&mut self) {
        for (id, class) in self.implements.classes.clone() {
            self.check_implemented_members(id, &class);
        }
    }

    /// §208.1 rule 3 for a generic class template, at its opaque instance:
    /// a member that names a type parameter matches only that type.
    pub(super) fn check_template_implements(&mut self, key: &str, id: ClassId) {
        if let Some(class) = self.implements.templates.get(key).cloned() {
            self.check_implemented_members(id, &class);
        }
    }

    fn check_implemented_members(&mut self, id: ClassId, class: &Implementing) {
        for &index in &class.interfaces {
            let interface = self.interfaces[index].clone();
            for member in &interface.members {
                let required = member.ty();
                if poisoned(&required) {
                    continue;
                }
                let found = match self.class_member_type(id, member, &class.pos) {
                    Some(found) if found == required || poisoned(&found) => continue,
                    found => found,
                };
                match found {
                    Some(found) => {
                        let (found, required) = (self.type_name(&found), self.type_name(&required));
                        self.reject_subset(
                            RejectionSite::InterfaceMemberType,
                            format!(
                                "class `{}` implements `{}` of interface `{}` with type `{found}`; \
                                 the interface requires `{required}`",
                                class.name, member.name, interface.name
                            ),
                            class.pos.clone(),
                        );
                    }
                    None => self.reject_subset(
                        RejectionSite::InterfaceMemberMissing,
                        format!(
                            "class `{}` does not implement `{}` of interface `{}`",
                            class.name, member.name, interface.name
                        ),
                        class.pos.clone(),
                    ),
                }
            }
        }
    }

    /// The type of the public instance member of the class that `member`
    /// names, in the form of [`InterfaceMember::ty`] (§208.1 rule 3). An
    /// `async` method with result `R` has the type of a member with
    /// result `Promise<R>`. `None` when the class has no member of the
    /// same kind.
    fn class_member_type(
        &mut self,
        id: ClassId,
        member: &InterfaceMember,
        pos: &Pos,
    ) -> Option<Type> {
        let name = member.name.as_str();
        let signatures = &self.class_sigs[id.0];
        if !signatures.modifiers.structural_member_is_public(name) {
            return None;
        }
        if member.accessor {
            if signatures.has_read_accessor(name) {
                return signatures
                    .methods
                    .get(name)
                    .map(|accessor| accessor.ret.clone());
            }
            if signatures.has_accessor(name)
                || !self.classes[id.0]
                    .fields
                    .iter()
                    .any(|field| field.name == name)
            {
                return None;
            }
            return Some(self.decide_field(id, name, pos.clone()));
        }
        if signatures.has_accessor(name) {
            return None;
        }
        let Some(method) = signatures
            .methods
            .get(name)
            .filter(|method| !method.generic)
        else {
            // A declared method with no signature failed its own check.
            let declared = matches!(
                signatures.member_namespace.get(name),
                Some(ClassMemberNamespaceEntry::Method { .. })
            );
            return (declared && !signatures.generic_methods.contains_key(name))
                .then_some(Type::Error);
        };
        if method.is_generator && !method.yield_known {
            return Some(Type::Error);
        }
        Some(Type::Func(Box::new(crate::types::FuncType {
            params: method
                .params
                .iter()
                .map(|parameter| parameter.ty().clone())
                .collect(),
            ret: if method.is_async {
                Type::AsyncHandle(Box::new(method.ret.clone()))
            } else {
                method.ret.clone()
            },
        })))
    }

    fn interface_member(&self, interface: &InterfaceType, name: &str) -> Option<InterfaceMember> {
        self.interfaces
            .get(interface.identity)?
            .members
            .iter()
            .find(|member| member.name == name)
            .cloned()
    }

    /// `x.m(...)` where the apparent type of `x` is an interface: the
    /// opaque check of a generic body (§208.1 rule 4). Each instance
    /// checks the call against the class of its type argument.
    pub(super) fn check_interface_method_call(
        &mut self,
        recv: hir::Expr,
        interface: &InterfaceType,
        property: &ast::IdentName,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let name = property.sym.as_ref();
        let prop_pos = self.pos(property.span);
        let Some(member) = self
            .interface_member(interface, name)
            .filter(|member| !member.accessor)
        else {
            self.reject_subset(
                RejectionSite::ClassUndeclaredMethodCall,
                format!("`{}` has no method `{name}`", interface.name),
                prop_pos,
            );
            self.check_poisoned_arguments(&call.args, fx);
            return self.err_expr(pos);
        };
        let params: Vec<ParamSig> = member
            .params
            .iter()
            .cloned()
            .map(ParamSig::positional)
            .collect();
        let args = self.check_args(
            RejectionSite::InstanceMethodArgumentCount,
            &params,
            &call.args,
            fx,
            &pos,
            name,
        );
        hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Call {
                callee: hir::Callee::Method {
                    recv: Box::new(recv),
                    name: hir::Symbol::from_full_text(name),
                },
                args,
            },
            ty: member.ret,
            pos,
        }
    }

    /// `await x.m(...)` where the apparent type of `x` is an interface: a
    /// member with result `Promise<R>` gives `R` (§208.1 rule 3).
    pub(super) fn await_interface_method(
        &mut self,
        recv: hir::Expr,
        interface: &InterfaceType,
        property: &ast::IdentName,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let recv = self.apparent_expr(recv);
        let handle = self.check_interface_method_call(
            recv,
            interface,
            property,
            call,
            fx,
            self.pos(call.span),
        );
        if let Type::AsyncHandle(value) = self.apparent_type(&handle.ty) {
            let origins = self.expr_async_origins(&handle, fx);
            fx.handle_async_origins(&origins);
            return hir::Expr {
                pending_work: None,
                kind: hir::ExprKind::AsyncHandleAwait(Box::new(handle)),
                ty: *value,
                pos,
            };
        }
        if self.apparent_type(&handle.ty) != Type::Error {
            self.reject_subset(
                RejectionSite::AwaitSynchronousMethod,
                format!(
                    "method `{}` is synchronous and cannot be awaited",
                    property.sym
                ),
                self.pos(property.span),
            );
        }
        self.err_expr(pos)
    }

    /// `x.g` where the apparent type of `x` is an interface: a read
    /// accessor signature is a read (§208.1 rule 4).
    pub(super) fn check_interface_member_read(
        &mut self,
        obj: hir::Expr,
        interface: &InterfaceType,
        name: &str,
        prop_pos: Pos,
        for_write: bool,
    ) -> hir::Expr {
        let member = self.interface_member(interface, name);
        match member {
            Some(member) if member.accessor && !for_write => {
                let call_pos = obj.pos.clone();
                hir::Expr {
                    pending_work: None,
                    kind: hir::ExprKind::Call {
                        callee: hir::Callee::Method {
                            recv: Box::new(obj),
                            name: hir::Symbol::from_full_text(name),
                        },
                        args: Vec::new(),
                    },
                    ty: member.ret,
                    pos: call_pos,
                }
            }
            Some(member) if !member.accessor && !for_write => {
                self.reject_subset(
                    RejectionSite::SynchronousMethodValue,
                    format!("method `{name}` may only be called, not read as a value"),
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
            Some(member) if member.accessor => {
                self.reject_subset(
                    RejectionSite::ReadonlyAccessorAssignment,
                    format!("`x.{name} = v` cannot write through a read-only accessor"),
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
            Some(member) if !member.accessor => {
                // compiler.md §209.1 rule 1: the target names the method.
                self.reject_subset(
                    RejectionSite::MethodAssignment,
                    format!(
                        "method `{name}` of `{}` cannot be assigned: a method is a function, not a field",
                        interface.name
                    ),
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
            _ if for_write => {
                self.reject_subset(
                    RejectionSite::ClassUndeclaredPropertyWrite,
                    format!(
                        "nominal types are closed: `{}` has no property `{name}`",
                        interface.name
                    ),
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
            _ => {
                self.reject_subset(
                    RejectionSite::ClassUndeclaredMemberRead,
                    format!("`{}` has no member `{name}`", interface.name),
                    prop_pos.clone(),
                );
                self.err_expr(prop_pos)
            }
        }
    }
}
