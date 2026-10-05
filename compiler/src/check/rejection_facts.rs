//! Facts that separate ordinary mistakes from subset restrictions.

use super::hir;

impl super::Checker<'_> {
    pub(super) fn string_alias_switch_exhaustive(
        &self,
        disc: &hir::Expr,
        cases: &[hir::SwitchCase],
    ) -> bool {
        let crate::types::Type::StringAlias(id) = self.apparent_type(&disc.ty) else {
            return false;
        };
        self.string_aliases[id.0].members.iter().enumerate().all(|(index, _)| {
            cases.iter().any(|case| {
                case.test.as_ref().is_some_and(
                    |test| matches!(test.kind, hir::ExprKind::Int(actual) if actual == index as i64),
                )
            })
        })
    }

    pub(super) fn source_enum_switch_exhaustive(
        &self,
        disc: &hir::Expr,
        cases: &[hir::SwitchCase],
    ) -> bool {
        let crate::types::Type::Enum(id) = self.apparent_type(&disc.ty) else {
            return false;
        };
        self.enums[id.0].members.iter().all(|(_, value)| {
            cases.iter().any(|case| {
                case.test.as_ref().is_some_and(|test| {
                    matches!(test.kind,
                hir::ExprKind::EnumMember { value: actual, .. } if actual == *value)
                })
            })
        })
    }

    /// TypeScript closes a source-enum switch over all declared member values.
    pub(super) fn ts_return_coverage(&self, body: &[hir::Stmt]) -> bool {
        use crate::types::Type;
        !super::fallthrough::sequence_can_fall_through(body)
            || body.iter().any(|statement| match statement {
                hir::Stmt::Block(body) => self.ts_return_coverage(body),
                hir::Stmt::If {
                    cond, then, els, ..
                } => match cond.kind {
                    hir::ExprKind::Bool(true) => self.ts_return_coverage(then),
                    hir::ExprKind::Bool(false) => els
                        .as_ref()
                        .is_some_and(|body| self.ts_return_coverage(body)),
                    _ => {
                        self.ts_return_coverage(then)
                            && els
                                .as_ref()
                                .is_some_and(|body| self.ts_return_coverage(body))
                    }
                },
                hir::Stmt::Switch { disc, cases, .. } => {
                    let Type::Enum(id) = self.apparent_type(&disc.ty) else {
                        return false;
                    };
                    self.enums[id.0].members.iter().all(|(_, value)| {
                        cases.iter().any(|case| {
                            case.test.as_ref().is_some_and(|test| {
                                matches!(test.kind,
                    hir::ExprKind::EnumMember { value: actual, .. } if actual == *value)
                            })
                        })
                    }) && cases.iter().all(|case| self.ts_return_coverage(&case.body))
                }
                _ => false,
            })
    }
}

/// A bodyless overload names a declaration that also has an implementation.
pub(super) fn has_function_implementation(
    module: &swc_ecma_ast::Module,
    function: &swc_ecma_ast::Function,
) -> bool {
    use swc_ecma_ast as ast;
    for item in &module.body {
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
            _ => continue,
        };
        match declaration {
            ast::Decl::Fn(f) if f.function.span == function.span => {
                return f.declare || module.body.iter().any(|item| {
                    let declaration = match item {
                        ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
                        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
                        _ => return false,
                    };
                    matches!(declaration, ast::Decl::Fn(other) if other.ident.sym == f.ident.sym && other.function.body.is_some())
                });
            }
            ast::Decl::Class(class) => {
                for member in &class.class.body {
                    let ast::ClassMember::Method(method) = member else {
                        continue;
                    };
                    if method.function.span != function.span {
                        continue;
                    }
                    let name = super::Checker::class_method_name(&method.key);
                    return class.declare
                        || class.class.body.iter().any(|member| {
                            matches!(member, ast::ClassMember::Method(other)
                            if other.is_static == method.is_static
                                && super::Checker::class_method_name(&other.key) == name
                                && other.function.body.is_some())
                        });
                }
            }
            _ => {}
        }
    }
    false
}

impl super::Checker<'_> {
    /// A source function shadows the ambient decorator even before collection reaches it.
    pub(super) fn source_function_declared(&self, name: &str) -> bool {
        use swc_ecma_ast as ast;
        self.prog.files[self.cur_file]
            .module
            .body
            .iter()
            .any(|item| {
                let declaration = match item {
                    ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
                    ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
                    _ => return false,
                };
                matches!(declaration, ast::Decl::Fn(function) if function.ident.sym == name)
            })
    }
}

/// A repeated source function signature has neither an implementation nor `declare`.
pub(super) fn unimplemented_function_group(module: &swc_ecma_ast::Module, name: &str) -> bool {
    use swc_ecma_ast as ast;
    let declarations = module
        .body
        .iter()
        .filter_map(|item| {
            let declaration = match item {
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
                _ => return None,
            };
            match declaration {
                ast::Decl::Fn(f) if f.ident.sym == name => Some(f),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    declarations.len() > 1
        && declarations
            .iter()
            .all(|f| !f.declare && f.function.body.is_none())
}

impl super::Checker<'_> {
    /// An unresolved bare computed key, including declarations collected later.
    pub(super) fn computed_field_unbound_name(&self, key: &swc_ecma_ast::PropName) -> bool {
        use swc_ecma_ast as ast;
        let ast::PropName::Computed(key) = key else {
            return false;
        };
        let ast::Expr::Ident(name) = &*key.expr else {
            return false;
        };
        self.peek_scope_item(name.sym.as_ref()).is_none()
            && !self.prog.files[self.cur_file]
                .module
                .body
                .iter()
                .any(|item| {
                    let declaration = match item {
                        ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
                        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
                        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) => {
                            return import.specifiers.iter().any(|specifier| match specifier {
                                ast::ImportSpecifier::Named(s) => s.local.sym == name.sym,
                                ast::ImportSpecifier::Default(s) => s.local.sym == name.sym,
                                ast::ImportSpecifier::Namespace(s) => s.local.sym == name.sym,
                            })
                        }
                        _ => return false,
                    };
                    super::exports::declaration_names(declaration)
                        .iter()
                        .any(|binding| binding.sym == name.sym)
                })
    }
}

/// An explicit field type without TypeScript's any/undefined initialization exemption.
impl super::Checker<'_> {
    fn descriptor_field_needs_value(&self, ty: &swc_ecma_ast::TsType) -> bool {
        use swc_ecma_ast as ast;
        match ty {
            ast::TsType::TsKeywordType(keyword) => !matches!(
                keyword.kind,
                ast::TsKeywordTypeKind::TsAnyKeyword
                    | ast::TsKeywordTypeKind::TsUnknownKeyword
                    | ast::TsKeywordTypeKind::TsUndefinedKeyword
            ),
            ast::TsType::TsArrayType(_)
            | ast::TsType::TsTupleType(_)
            | ast::TsType::TsFnOrConstructorType(_)
            | ast::TsType::TsLitType(_) => true,
            ast::TsType::TsParenthesizedType(parenthesized) => {
                self.descriptor_field_needs_value(&parenthesized.type_ann)
            }
            ast::TsType::TsUnionOrIntersectionType(union) => match union {
                ast::TsUnionOrIntersectionType::TsUnionType(union) => union
                    .types
                    .iter()
                    .all(|ty| self.descriptor_field_needs_value(ty)),
                ast::TsUnionOrIntersectionType::TsIntersectionType(intersection) => intersection
                    .types
                    .iter()
                    .all(|ty| self.descriptor_field_needs_value(ty)),
            },
            ast::TsType::TsTypeRef(reference) => {
                let ast::TsEntityName::Ident(name) = &reference.type_name else {
                    return false;
                };
                match self.type_scope_item(name.sym.as_ref()) {
                    Some(
                        super::ScopeItem::Class(_)
                        | super::ScopeItem::GenericClass(_)
                        | super::ScopeItem::Enum(_)
                        | super::ScopeItem::StringAlias(_),
                    ) => true,
                    Some(super::ScopeItem::TypeAlias(ty)) => {
                        self.apparent_type(&ty) != crate::types::Type::Error
                    }
                    Some(_) => false,
                    None => {
                        crate::ambient::sized_alias(name.sym.as_ref()).is_some()
                            || matches!(
                                name.sym.as_ref(),
                                "Array"
                                    | "FixedArray"
                                    | "Map"
                                    | "Set"
                                    | "Date"
                                    | "RegExp"
                                    | "Error"
                                    | "TypeError"
                                    | "RangeError"
                                    | "Promise"
                            )
                    }
                }
            }
            _ => false,
        }
    }

    /// Classify only constructor shapes for which source assignment flow is represented.
    pub(super) fn descriptor_field_unassigned(
        &self,
        class: &swc_ecma_ast::Class,
        prop: &swc_ecma_ast::ClassProp,
    ) -> bool {
        if !prop
            .type_ann
            .as_ref()
            .is_some_and(|annotation| self.descriptor_field_needs_value(&annotation.type_ann))
        {
            return false;
        }
        let swc_ecma_ast::PropName::Ident(name) = &prop.key else {
            return false;
        };
        let constructor = class.body.iter().find_map(|member| match member {
            swc_ecma_ast::ClassMember::Constructor(constructor) => Some(constructor),
            _ => None,
        });
        match constructor {
            None => true,
            Some(constructor) => constructor.body.as_ref().is_some_and(|body| {
                ast_field_on_normal_exit(&body.stmts, name.sym.as_ref()) == Some(false)
            }),
        }
    }
}

impl super::Checker<'_> {
    /// An array-pattern mirror parameter with a concrete noniterable scalar annotation.
    pub(super) fn mirror_array_parameter_noniterable(&self, pat: &swc_ecma_ast::Pat) -> bool {
        use swc_ecma_ast as ast;
        let ast::Pat::Array(array) = pat else {
            return false;
        };
        let Some(annotation) = &array.type_ann else {
            return false;
        };
        match &*annotation.type_ann {
            ast::TsType::TsKeywordType(keyword) => matches!(
                keyword.kind,
                ast::TsKeywordTypeKind::TsNumberKeyword
                    | ast::TsKeywordTypeKind::TsBooleanKeyword
                    | ast::TsKeywordTypeKind::TsBigIntKeyword
                    | ast::TsKeywordTypeKind::TsSymbolKeyword
            ),
            ast::TsType::TsTypeRef(reference) => {
                matches!(&reference.type_name, ast::TsEntityName::Ident(name) if self.type_scope_item(name.sym.as_ref()).is_none() && crate::ambient::sized_alias(name.sym.as_ref()).is_some())
            }
            _ => false,
        }
    }
}

/// Descriptor shape checking precedes HIR; the same normal-exit assignment fact in AST.
fn ast_field_on_normal_exit(body: &[swc_ecma_ast::Stmt], field: &str) -> Option<bool> {
    use swc_ecma_ast as ast;
    fn assigned(expression: &ast::Expr, field: &str) -> bool {
        let ast::Expr::Assign(assignment) = expression else {
            return false;
        };
        if assignment.op != ast::AssignOp::Assign {
            return false;
        }
        matches!(&assignment.left, ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) if matches!(&*member.obj, ast::Expr::This(_)) && matches!(&member.prop, ast::MemberProp::Ident(name) if name.sym == field))
            || assigned(&assignment.right, field)
    }
    fn walk(body: &[ast::Stmt], field: &str, mut held: bool, bad: &mut bool) -> Option<bool> {
        for statement in body {
            match statement {
                ast::Stmt::Expr(expression) => held |= assigned(&expression.expr, field),
                ast::Stmt::Throw(_) => return None,
                ast::Stmt::Return(_) => {
                    *bad |= !held;
                    return None;
                }
                ast::Stmt::Block(block) => held = walk(&block.stmts, field, held, bad)?,
                ast::Stmt::If(branch) => {
                    let yes_body = std::slice::from_ref(&*branch.cons);
                    let no_body = branch
                        .alt
                        .as_deref()
                        .map(std::slice::from_ref)
                        .unwrap_or(&[]);
                    let (yes, no) = match &*branch.test {
                        ast::Expr::Lit(ast::Lit::Bool(value)) if value.value => {
                            (walk(yes_body, field, held, bad), None)
                        }
                        ast::Expr::Lit(ast::Lit::Bool(_)) => {
                            (None, walk(no_body, field, held, bad))
                        }
                        _ => (
                            walk(yes_body, field, held, bad),
                            walk(no_body, field, held, bad),
                        ),
                    };
                    held = match (yes, no) {
                        (Some(a), Some(b)) => a && b,
                        (Some(a), None) | (None, Some(a)) => a,
                        (None, None) => return None,
                    };
                }
                ast::Stmt::While(loop_) if !matches!(&*loop_.test, ast::Expr::Lit(ast::Lit::Bool(value)) if !value.value) =>
                {
                    walk(std::slice::from_ref(&*loop_.body), field, held, bad);
                }
                ast::Stmt::For(loop_) => {
                    walk(std::slice::from_ref(&*loop_.body), field, held, bad);
                }
                _ => {}
            }
        }
        Some(held)
    }
    // These source forms have no checked descriptor constructor HIR. Do not
    // infer rejection from statements this flow does not represent.
    fn represented(body: &[ast::Stmt], field: &str) -> bool {
        body.iter().all(|statement| match statement {
            ast::Stmt::Expr(expression) => assigned(&expression.expr, field) || matches!(&*expression.expr, ast::Expr::Assign(assignment) if assignment.op == ast::AssignOp::Assign && matches!(&*assignment.right, ast::Expr::Ident(_) | ast::Expr::Lit(_))),
            ast::Stmt::Throw(_) | ast::Stmt::Empty(_) => true,
            ast::Stmt::Return(return_) => return_.arg.is_none(),
            ast::Stmt::Block(block) => represented(&block.stmts, field),
            ast::Stmt::If(branch) => matches!(&*branch.test, ast::Expr::Ident(_) | ast::Expr::Lit(_)) && represented(std::slice::from_ref(&*branch.cons), field) && branch.alt.as_deref().is_none_or(|alternative| represented(std::slice::from_ref(alternative), field)),
            _ => false,
        })
    }
    if !represented(body, field) {
        return None;
    }
    let mut bad = false;
    let fallthrough = walk(body, field, false, &mut bad);
    Some(!bad && fallthrough != Some(false))
}

impl super::Checker<'_> {
    /// TypeScript assignability after the prelude erases sized numeric types.
    pub(super) fn ts_erased_assignable(
        &self,
        from: &crate::types::Type,
        to: &crate::types::Type,
    ) -> bool {
        self.ts_erased_assignable_inner(from, to, &mut Vec::new(), false)
    }

    /// Nominal assignments compare the collected structural class signatures too.
    pub(super) fn ts_nominal_assignable(
        &self,
        from: &crate::types::Type,
        to: &crate::types::Type,
    ) -> bool {
        self.ts_erased_assignable_inner(from, to, &mut Vec::new(), true)
    }

    fn ts_erased_assignable_inner(
        &self,
        from: &crate::types::Type,
        to: &crate::types::Type,
        seen: &mut Vec<(crate::types::Type, crate::types::Type)>,
        structural_classes: bool,
    ) -> bool {
        use crate::types::Type;
        // A constraint does not make a concrete value assignable to an unknown T.
        if self.involves_type_parameter(to) {
            return self.assignable(from, to);
        }
        let from = self.apparent_type(from);
        let to = self.apparent_type(to);
        if from == to {
            return true;
        }
        if (from.is_numeric() || matches!(from, Type::GenericNumber))
            && (to.is_numeric() || matches!(to, Type::GenericNumber | Type::Enum(_)))
        {
            return true;
        }
        if seen.contains(&(from.clone(), to.clone())) {
            return true;
        }
        seen.push((from.clone(), to.clone()));
        let assignable = match (&from, &to) {
            (Type::Enum(_), number) if self.apparent_type(number).is_numeric() => true,
            (Type::Nullable(a), Type::Nullable(b)) => {
                self.ts_erased_assignable_inner(a, b, seen, structural_classes)
            }
            (Type::Null, Type::Nullable(_)) => true,
            (a, Type::Nullable(b)) => {
                self.ts_erased_assignable_inner(a, b, seen, structural_classes)
            }
            (Type::Array(a), Type::Array(b))
            | (Type::Set(a), Type::Set(b))
            | (Type::Generator(a), Type::Generator(b))
            | (Type::AsyncHandle(a), Type::AsyncHandle(b))
            | (Type::IterResult(a), Type::IterResult(b))
            | (Type::Inbox(a), Type::Inbox(b))
            | (Type::Outbox(a), Type::Outbox(b)) => {
                self.ts_erased_assignable_inner(a, b, seen, structural_classes)
            }
            (Type::FixedArray(a, n), Type::FixedArray(b, m)) => {
                n == m && self.ts_erased_assignable_inner(a, b, seen, structural_classes)
            }
            (Type::Map(a, b), Type::Map(c, d)) | (Type::Worker(a, b), Type::Worker(c, d)) => {
                self.ts_erased_assignable_inner(a, c, seen, structural_classes)
                    && self.ts_erased_assignable_inner(b, d, seen, structural_classes)
            }
            (Type::Class(a), Type::Class(b)) => match (
                self.instance_arguments.get(a),
                self.instance_arguments.get(b),
            ) {
                (Some((a, xs)), Some((b, ys))) => {
                    a == b
                        && xs.len() == ys.len()
                        && xs.iter().zip(ys).all(|(x, y)| {
                            self.ts_erased_assignable_inner(x, y, seen, structural_classes)
                        })
                }
                _ if structural_classes => {
                    self.classes[b.0].fields.iter().all(|target| {
                        self.classes[a.0].fields.iter().any(|source| {
                            source.name == target.name
                                && self.ts_erased_assignable_inner(
                                    &source.ty,
                                    &target.ty,
                                    seen,
                                    structural_classes,
                                )
                        })
                    }) && self.class_sigs[b.0].methods.iter().all(|(name, target)| {
                        self.class_sigs[a.0]
                            .methods
                            .get(name)
                            .is_some_and(|source| {
                                source.params.len() == target.params.len()
                                    && source.params.iter().zip(&target.params).all(|(x, y)| {
                                        self.ts_erased_assignable_inner(
                                            y.ty(),
                                            x.ty(),
                                            seen,
                                            structural_classes,
                                        )
                                    })
                                    && self.ts_erased_assignable_inner(
                                        &source.ret,
                                        &target.ret,
                                        seen,
                                        structural_classes,
                                    )
                            })
                    })
                }
                _ => false,
            },
            (Type::Func(a), Type::Func(b)) => {
                a.params.len() == b.params.len()
                    && a.params.iter().zip(&b.params).all(|(x, y)| {
                        self.ts_erased_assignable_inner(y, x, seen, structural_classes)
                    })
                    && (self.apparent_type(&b.ret) == Type::Void
                        || self.ts_erased_assignable_inner(
                            &a.ret,
                            &b.ret,
                            seen,
                            structural_classes,
                        ))
            }
            _ => false,
        };
        seen.pop();
        assignable
    }
}

/// A bodyless method carries the abstract modifier in its source declaration.
pub(super) fn abstract_method(
    module: &swc_ecma_ast::Module,
    function: &swc_ecma_ast::Function,
) -> bool {
    use swc_ecma_ast as ast;
    module.body.iter().any(|item| {
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => d,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => &e.decl,
            _ => return false,
        };
        matches!(declaration, ast::Decl::Class(class) if class.class.body.iter().any(|member| matches!(member, ast::ClassMember::Method(method) if method.function.span == function.span && method.is_abstract)))
    })
}

impl super::Checker<'_> {
    /// Reject a static this member from the separately collected class namespace.
    pub(super) fn reject_static_this_member(
        &mut self,
        member: &swc_ecma_ast::MemberExpr,
        fx: &super::FnCtx,
    ) -> bool {
        use swc_ecma_ast as ast;
        let Some(id) = fx.frames.last().and_then(|frame| frame.static_this_class) else {
            return false;
        };
        if !matches!(super::expr::unparen_expr(&member.obj), ast::Expr::This(_)) {
            return false;
        }
        let name = match &member.prop {
            ast::MemberProp::Ident(name) => Some(name.sym.as_ref()),
            ast::MemberProp::Computed(key) => match super::expr::unparen_expr(&key.expr) {
                ast::Expr::Lit(ast::Lit::Str(name)) => Some(name.value.as_ref()),
                _ => None,
            },
            _ => None,
        };
        let exists = name.is_some_and(|name| {
            let signature = &self.class_sigs[id.0];
            signature.static_fields.contains_key(name)
                || signature.static_methods.contains_key(name)
                || signature.has_static_accessor(name)
                || signature.has_generic_method(name, true)
        });
        self.reject_subset(
            if exists {
                super::RejectionSite::ThisStaticMethodMember
            } else {
                super::RejectionSite::InstanceMemberInStaticMethod
            },
            if exists {
                "a static method must name its class instead of `this`"
            } else {
                "the member is not declared on the static class receiver"
            },
            self.pos(member.span),
        );
        true
    }
}
