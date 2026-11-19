//! Facts that separate ordinary mistakes from subset restrictions.

use super::hir;
use super::DeclarationName;
use oxc_span::GetSpan;

impl<'p> super::Checker<'p> {
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

    /// Adds the `default` arm that ends each tail switch over every member
    /// of a source enum, in a function that returns `ret` (§200.1 rules 1,
    /// 2, 2a, and 2b). The arm traps with `unreachable-reached` at the
    /// `switch`. A switch is in tail position when the end of the body
    /// follows its end and no statement runs between them. A trap runs
    /// no finalizer, so a `finally` block or a dispose hook does not end
    /// tail position.
    pub(super) fn end_tail_enum_switches(&self, ret: &crate::types::Type, body: &mut [hir::Stmt]) {
        if matches!(
            self.apparent_type(ret),
            crate::types::Type::Void | crate::types::Type::Error
        ) {
            return;
        }
        self.end_tail_switch(body);
    }

    fn end_tail_switch(&self, body: &mut [hir::Stmt]) {
        let Some(last) = body.last_mut() else {
            return;
        };
        match last {
            hir::Stmt::Block(body) => self.end_tail_switch(body),
            hir::Stmt::If {
                then,
                els,
                cond: _,
                pos: _,
            } => {
                self.end_tail_switch(then);
                if let Some(els) = els {
                    self.end_tail_switch(els);
                }
            }
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                self.end_tail_switch(body);
                self.end_tail_switch(handler);
            }
            hir::Stmt::Using {
                body,
                finalizer,
                bindings: _,
                pos: _,
            } => {
                self.end_tail_switch(body);
                if let Some(finalizer) = finalizer {
                    self.end_tail_switch(finalizer);
                }
            }
            hir::Stmt::Switch { disc, cases, pos } => {
                if let Some(case) = cases.last_mut() {
                    self.end_tail_switch(&mut case.body);
                }
                if let Some(arm) = self.covered_enum_switch_arm(disc, cases, pos) {
                    cases.push(arm);
                }
            }
            _ => {}
        }
    }

    /// The trap arm of a switch over every member of a source enum whose
    /// cases do not complete it normally. The arm is `None` when the
    /// switch has a `default`, misses a member, has a case that completes
    /// it normally, or switches on an enum with no member (§200.1 rule 2b).
    fn covered_enum_switch_arm(
        &self,
        disc: &hir::Expr,
        cases: &[hir::SwitchCase],
        pos: &crate::diag::Pos,
    ) -> Option<hir::SwitchCase> {
        let crate::types::Type::Enum(id) = self.apparent_type(&disc.ty) else {
            return None;
        };
        if self.enums[id.0].members.is_empty()
            || cases.iter().any(|case| case.test.is_none())
            || !self.source_enum_switch_exhaustive(disc, cases)
            || super::fallthrough::switch_cases_complete_normally(cases)
        {
            return None;
        }
        let trap = hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Call {
                callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                args: Vec::new(),
            },
            ty: crate::types::Type::Void,
            pos: pos.clone(),
        };
        Some(hir::SwitchCase {
            test: None,
            body: vec![hir::Stmt::Expr(trap)],
            pos: pos.clone(),
        })
    }
}

impl<'p> super::Checker<'p> {
    /// True when TypeScript proves that a body returns on each path: each
    /// switch over every member of a source enum whose cases do not
    /// complete it normally ends the flow, at any position (§200.2 item
    /// 5). The body is the checked body that falls through.
    pub(super) fn ts_return_coverage(&self, body: &[hir::Stmt]) -> bool {
        let mut body = body.to_vec();
        self.end_every_enum_switch(&mut body);
        !super::fallthrough::sequence_can_fall_through(&body)
    }

    fn end_every_enum_switch(&self, body: &mut [hir::Stmt]) {
        for statement in body {
            match statement {
                hir::Stmt::Block(body)
                | hir::Stmt::While { body, .. }
                | hir::Stmt::DoWhile { body, .. }
                | hir::Stmt::ForOf { body, .. }
                | hir::Stmt::GeneratorForOf { body, .. } => self.end_every_enum_switch(body),
                hir::Stmt::For {
                    init, step, body, ..
                } => {
                    if let Some(init) = init {
                        self.end_every_enum_switch(std::slice::from_mut(&mut **init));
                    }
                    self.end_every_enum_switch(step);
                    self.end_every_enum_switch(body);
                }
                hir::Stmt::If { then, els, .. } => {
                    self.end_every_enum_switch(then);
                    if let Some(els) = els {
                        self.end_every_enum_switch(els);
                    }
                }
                hir::Stmt::Try { body, handler, .. } => {
                    self.end_every_enum_switch(body);
                    self.end_every_enum_switch(handler);
                }
                hir::Stmt::Using {
                    body, finalizer, ..
                } => {
                    self.end_every_enum_switch(body);
                    if let Some(finalizer) = finalizer {
                        self.end_every_enum_switch(finalizer);
                    }
                }
                hir::Stmt::Switch { disc, cases, pos } => {
                    for case in cases.iter_mut() {
                        self.end_every_enum_switch(&mut case.body);
                    }
                    if let Some(arm) = self.covered_enum_switch_arm(disc, cases, pos) {
                        cases.push(arm);
                    }
                }
                _ => {}
            }
        }
    }
}

/// A bodyless overload names a declaration that also has an implementation.
pub(super) fn has_function_implementation(
    module: &oxc_ast::ast::Program,
    function: &oxc_ast::ast::Function,
) -> bool {
    use oxc_ast::ast;
    for item in &module.body {
        let Some(declaration) = super::module_decl(item) else {
            continue;
        };
        match declaration {
            ast::Declaration::FunctionDeclaration(f) if f.span == function.span => {
                return f.declare || module.body.iter().any(|item| {
                    let Some(declaration) = super::module_decl(item) else { return false; };
                    matches!(declaration, ast::Declaration::FunctionDeclaration(other) if other.declaration_name() == f.declaration_name() && other.body.is_some())
                });
            }
            ast::Declaration::ClassDeclaration(class) => {
                for member in &class.body.body {
                    let ast::ClassElement::MethodDefinition(method) = member else {
                        continue;
                    };
                    if method.value.span != function.span {
                        continue;
                    }
                    let name = super::Checker::class_method_name(&method.key);
                    return class.declare
                        || class.body.body.iter().any(|member| {
                            matches!(member, ast::ClassElement::MethodDefinition(other)
                            if other.r#static == method.r#static
                                && super::Checker::class_method_name(&other.key) == name
                                && other.value.body.is_some())
                        });
                }
            }
            _ => {}
        }
    }
    false
}

impl<'p> super::Checker<'p> {
    /// A source function shadows the ambient decorator even before collection reaches it.
    pub(super) fn source_function_declared(&self, name: &str) -> bool {
        use oxc_ast::ast;
        self.prog.files[self.cur_file]
            .module
            .body
            .iter()
            .any(|item| {
                let Some(declaration) = super::module_decl(item) else { return false; };
                matches!(declaration, ast::Declaration::FunctionDeclaration(function) if function.declaration_name() == name)
            })
    }
}

/// A repeated source function signature has neither an implementation nor `declare`.
pub(super) fn unimplemented_function_group(module: &oxc_ast::ast::Program, name: &str) -> bool {
    use oxc_ast::ast;
    let declarations = module
        .body
        .iter()
        .filter_map(|item| {
            let declaration = super::module_decl(item)?;
            match declaration {
                ast::Declaration::FunctionDeclaration(f) if f.declaration_name() == name => Some(f),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    declarations.len() > 1 && declarations.iter().all(|f| !f.declare && f.body.is_none())
}

impl<'p> super::Checker<'p> {
    /// An unresolved bare computed key, including declarations collected later.
    pub(super) fn computed_field_unbound_name(&self, key: &oxc_ast::ast::PropertyKey) -> bool {
        use oxc_ast::ast;
        let Some(ast::Expression::Identifier(name)) = key.as_expression() else {
            return false;
        };
        self.peek_scope_item(name.name.as_ref()).is_none()
            && !self.prog.files[self.cur_file]
                .module
                .body
                .iter()
                .any(|item| {
                    if let ast::Statement::ImportDeclaration(import) = item {
                        return import
                            .specifiers
                            .iter()
                            .flatten()
                            .any(|specifier| specifier.local().name == name.name);
                    }
                    let Some(declaration) = super::module_decl(item) else {
                        return false;
                    };
                    super::exports::declaration_names(declaration)
                        .iter()
                        .any(|binding| binding.name == name.name)
                })
    }
}

/// An explicit field type without TypeScript's any/undefined initialization exemption.
impl<'p> super::Checker<'p> {
    fn descriptor_field_needs_value(&self, ty: &oxc_ast::ast::TSType) -> bool {
        use oxc_ast::ast;
        match ty {
            ast::TSType::TSAnyKeyword(_)
            | ast::TSType::TSUnknownKeyword(_)
            | ast::TSType::TSUndefinedKeyword(_) => false,
            ast::TSType::TSArrayType(_)
            | ast::TSType::TSTupleType(_)
            | ast::TSType::TSFunctionType(_)
            | ast::TSType::TSConstructorType(_)
            | ast::TSType::TSLiteralType(_) => true,
            ast::TSType::TSParenthesizedType(parenthesized) => {
                self.descriptor_field_needs_value(&parenthesized.type_annotation)
            }
            ast::TSType::TSUnionType(union) => union
                .types
                .iter()
                .all(|ty| self.descriptor_field_needs_value(ty)),
            ast::TSType::TSIntersectionType(intersection) => intersection
                .types
                .iter()
                .all(|ty| self.descriptor_field_needs_value(ty)),
            ty if ty.is_keyword() => true,
            ast::TSType::TSTypeReference(reference) => {
                let ast::TSTypeName::IdentifierReference(name) = &reference.type_name else {
                    return false;
                };
                match self.type_scope_item(name.name.as_ref()) {
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
                        crate::ambient::sized_alias(name.name.as_ref()).is_some()
                            || matches!(
                                name.name.as_ref(),
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
        class: &oxc_ast::ast::Class,
        prop: &oxc_ast::ast::PropertyDefinition,
    ) -> bool {
        if !prop.type_annotation.as_ref().is_some_and(|annotation| {
            self.descriptor_field_needs_value(&annotation.type_annotation)
        }) {
            return false;
        }
        let oxc_ast::ast::PropertyKey::StaticIdentifier(name) = &prop.key else {
            return false;
        };
        let constructor = class.body.body.iter().find_map(|member| match member {
            oxc_ast::ast::ClassElement::MethodDefinition(constructor)
                if constructor.kind == oxc_ast::ast::MethodDefinitionKind::Constructor =>
            {
                Some(constructor)
            }
            _ => None,
        });
        match constructor {
            None => true,
            Some(constructor) => constructor.value.body.as_ref().is_some_and(|body| {
                ast_field_on_normal_exit(&body.statements, name.name.as_ref()) == Some(false)
            }),
        }
    }
}

impl<'p> super::Checker<'p> {
    /// An array-pattern mirror parameter with a concrete noniterable scalar annotation.
    pub(super) fn mirror_array_parameter_noniterable(
        &self,
        param: &oxc_ast::ast::FormalParameter,
    ) -> bool {
        use oxc_ast::ast;
        let ast::BindingPattern::ArrayPattern(_) = &param.pattern else {
            return false;
        };
        let Some(annotation) = &param.type_annotation else {
            return false;
        };
        match &annotation.type_annotation {
            ast::TSType::TSNumberKeyword(_)
            | ast::TSType::TSBooleanKeyword(_)
            | ast::TSType::TSBigIntKeyword(_)
            | ast::TSType::TSSymbolKeyword(_) => true,
            ast::TSType::TSTypeReference(reference) => {
                matches!(&reference.type_name, ast::TSTypeName::IdentifierReference(name) if self.type_scope_item(name.name.as_ref()).is_none() && crate::ambient::sized_alias(name.name.as_ref()).is_some())
            }
            _ => false,
        }
    }
}

/// Descriptor shape checking precedes HIR; the same normal-exit assignment fact in AST.
fn ast_field_on_normal_exit(body: &[oxc_ast::ast::Statement], field: &str) -> Option<bool> {
    use oxc_ast::ast;
    fn assigned(expression: &ast::Expression, field: &str) -> bool {
        let ast::Expression::AssignmentExpression(assignment) = expression else {
            return false;
        };
        if assignment.operator != ast::AssignmentOperator::Assign {
            return false;
        }
        matches!(&assignment.left, ast::AssignmentTarget::StaticMemberExpression(member) if matches!(&member.object, ast::Expression::ThisExpression(_)) && member.property.name == field)
            || assigned(&assignment.right, field)
    }
    fn walk(body: &[ast::Statement], field: &str, mut held: bool, bad: &mut bool) -> Option<bool> {
        for statement in body {
            match statement {
                ast::Statement::ExpressionStatement(expression) => {
                    held |= assigned(&expression.expression, field)
                }
                ast::Statement::ThrowStatement(_) => return None,
                ast::Statement::ReturnStatement(_) => {
                    *bad |= !held;
                    return None;
                }
                ast::Statement::BlockStatement(block) => {
                    held = walk(&block.body, field, held, bad)?
                }
                ast::Statement::IfStatement(branch) => {
                    let yes_body = std::slice::from_ref(&branch.consequent);
                    let no_body = branch.alternate.as_slice();
                    let (yes, no) = match &branch.test {
                        ast::Expression::BooleanLiteral(value) if value.value => {
                            (walk(yes_body, field, held, bad), None)
                        }
                        ast::Expression::BooleanLiteral(_) => {
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
                ast::Statement::WhileStatement(loop_) if !matches!(&loop_.test, ast::Expression::BooleanLiteral(value) if !value.value) =>
                {
                    walk(std::slice::from_ref(&loop_.body), field, held, bad);
                }
                ast::Statement::ForStatement(loop_) => {
                    walk(std::slice::from_ref(&loop_.body), field, held, bad);
                }
                _ => {}
            }
        }
        Some(held)
    }
    // These source forms have no checked descriptor constructor HIR. Do not
    // infer rejection from statements this flow does not represent.
    fn represented(body: &[ast::Statement], field: &str) -> bool {
        body.iter().all(|statement| match statement {
            ast::Statement::ExpressionStatement(expression) => assigned(&expression.expression, field) || matches!(&expression.expression, ast::Expression::AssignmentExpression(assignment) if assignment.operator == ast::AssignmentOperator::Assign && matches!(&assignment.right, ast::Expression::Identifier(_) | ast::Expression::NumericLiteral(_) | ast::Expression::StringLiteral(_) | ast::Expression::BooleanLiteral(_) | ast::Expression::NullLiteral(_) | ast::Expression::BigIntLiteral(_) | ast::Expression::RegExpLiteral(_))),
            ast::Statement::ThrowStatement(_) | ast::Statement::EmptyStatement(_) | ast::Statement::DebuggerStatement(_) => true,
            ast::Statement::ReturnStatement(return_) => return_.argument.is_none(),
            ast::Statement::BlockStatement(block) => represented(&block.body, field),
            ast::Statement::IfStatement(branch) => matches!(&branch.test, ast::Expression::Identifier(_) | ast::Expression::NumericLiteral(_) | ast::Expression::StringLiteral(_) | ast::Expression::BooleanLiteral(_) | ast::Expression::NullLiteral(_) | ast::Expression::BigIntLiteral(_) | ast::Expression::RegExpLiteral(_)) && represented(std::slice::from_ref(&branch.consequent), field) && branch.alternate.as_ref().is_none_or(|alternative| represented(std::slice::from_ref(alternative), field)),
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

/// The `tsc` relation that the model applies (compiler.md §195.11 rule 3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TsRelation {
    /// Assignability: a nullable type converts to a nullable type of the
    /// same absence spelling only.
    Conversion,
    /// Comparability: the two absence spellings are comparable.
    Comparison,
}

impl<'p> super::Checker<'p> {
    /// TypeScript assignability after the prelude erases sized numeric types.
    pub(super) fn ts_erased_assignable(
        &self,
        from: &crate::types::Type,
        to: &crate::types::Type,
    ) -> bool {
        self.ts_erased_assignable_inner(from, to, &mut Vec::new(), false, TsRelation::Conversion)
    }

    /// TypeScript comparability of two equality operands after the prelude
    /// erases sized numeric types (compiler.md §195.11 rule 3).
    pub(super) fn ts_erased_comparable(
        &self,
        from: &crate::types::Type,
        to: &crate::types::Type,
    ) -> bool {
        self.ts_erased_assignable_inner(from, to, &mut Vec::new(), false, TsRelation::Comparison)
    }

    /// Nominal assignments compare the collected structural class signatures too.
    pub(super) fn ts_nominal_assignable(
        &self,
        from: &crate::types::Type,
        to: &crate::types::Type,
    ) -> bool {
        self.ts_erased_assignable_inner(from, to, &mut Vec::new(), true, TsRelation::Conversion)
    }

    fn ts_erased_assignable_inner(
        &self,
        from: &crate::types::Type,
        to: &crate::types::Type,
        seen: &mut Vec<(crate::types::Type, crate::types::Type)>,
        structural_classes: bool,
        relation: TsRelation,
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
            // §195.11 rule 3: a conversion keeps the absence spelling; a
            // comparison accepts both spellings.
            (Type::Nullable(a, from_spelling), Type::Nullable(b, to_spelling)) => {
                (relation == TsRelation::Comparison || from_spelling == to_spelling)
                    && self.ts_erased_assignable_inner(a, b, seen, structural_classes, relation)
            }
            // §195.1 rule 2: `tsc` converts `null` to the `null` spelling only.
            (Type::Null, Type::Nullable(_, spelling)) => {
                relation == TsRelation::Comparison || *spelling == crate::types::Absence::Null
            }
            (a, Type::Nullable(b, _)) => {
                self.ts_erased_assignable_inner(a, b, seen, structural_classes, relation)
            }
            (Type::Array(a), Type::Array(b))
            | (Type::Set(a), Type::Set(b))
            | (Type::Generator(a), Type::Generator(b))
            | (Type::AsyncHandle(a), Type::AsyncHandle(b))
            | (Type::IterResult(a), Type::IterResult(b))
            | (Type::Inbox(a), Type::Inbox(b))
            | (Type::Outbox(a), Type::Outbox(b)) => {
                self.ts_erased_assignable_inner(a, b, seen, structural_classes, relation)
            }
            (Type::FixedArray(a, n), Type::FixedArray(b, m)) => {
                n == m && self.ts_erased_assignable_inner(a, b, seen, structural_classes, relation)
            }
            (Type::Map(a, b), Type::Map(c, d)) | (Type::Worker(a, b), Type::Worker(c, d)) => {
                self.ts_erased_assignable_inner(a, c, seen, structural_classes, relation)
                    && self.ts_erased_assignable_inner(b, d, seen, structural_classes, relation)
            }
            (Type::Class(a), Type::Class(b)) => match (
                self.instance_arguments.get(a),
                self.instance_arguments.get(b),
            ) {
                (Some((a, xs)), Some((b, ys))) => {
                    a == b
                        && xs.len() == ys.len()
                        && xs.iter().zip(ys).all(|(x, y)| {
                            self.ts_erased_assignable_inner(
                                x,
                                y,
                                seen,
                                structural_classes,
                                relation,
                            )
                        })
                }
                _ if structural_classes => {
                    self.classes[b.0].fields.iter().all(|target| {
                        self.class_sigs[a.0]
                            .modifiers
                            .structural_member_is_public(&target.name)
                            && self.class_sigs[b.0]
                                .modifiers
                                .structural_member_is_public(&target.name)
                            && self.classes[a.0].fields.iter().any(|source| {
                                source.name == target.name
                                    && self.ts_erased_assignable_inner(
                                        &source.ty,
                                        &target.ty,
                                        seen,
                                        structural_classes,
                                        relation,
                                    )
                            })
                    }) && self.class_sigs[b.0].methods.iter().all(|(name, target)| {
                        self.class_sigs[a.0]
                            .modifiers
                            .structural_member_is_public(name)
                            && self.class_sigs[b.0]
                                .modifiers
                                .structural_member_is_public(name)
                            && self.class_sigs[a.0]
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
                                                relation,
                                            )
                                        })
                                        && self.ts_erased_assignable_inner(
                                            &source.ret,
                                            &target.ret,
                                            seen,
                                            structural_classes,
                                            relation,
                                        )
                                })
                    })
                }
                _ => false,
            },
            (Type::Func(a), Type::Func(b)) => {
                a.params.len() == b.params.len()
                    && a.params.iter().zip(&b.params).all(|(x, y)| {
                        self.ts_erased_assignable_inner(y, x, seen, structural_classes, relation)
                    })
                    && (self.apparent_type(&b.ret) == Type::Void
                        || self.ts_erased_assignable_inner(
                            &a.ret,
                            &b.ret,
                            seen,
                            structural_classes,
                            relation,
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
    module: &oxc_ast::ast::Program,
    function: &oxc_ast::ast::Function,
) -> bool {
    use oxc_ast::ast;
    module.body.iter().any(|item| {
        let Some(declaration) = super::module_decl(item) else { return false; };
        matches!(declaration, ast::Declaration::ClassDeclaration(class) if class.body.body.iter().any(|member| matches!(member, ast::ClassElement::MethodDefinition(method) if method.value.span == function.span && method.r#type == ast::MethodDefinitionType::TSAbstractMethodDefinition)))
    })
}

impl<'p> super::Checker<'p> {
    /// Reject a static this member from the separately collected class namespace.
    pub(super) fn reject_static_this_member(
        &mut self,
        member: &oxc_ast::ast::MemberExpression,
        fx: &super::FnCtx,
    ) -> bool {
        use oxc_ast::ast;
        let Some(id) = fx.frames.last().and_then(|frame| frame.static_this_class) else {
            return false;
        };
        if !matches!(
            super::expr::unparen_expr(member.object()),
            ast::Expression::ThisExpression(_)
        ) {
            return false;
        }
        let name = match member {
            ast::MemberExpression::StaticMemberExpression(member) => {
                Some(member.property.name.as_ref())
            }
            ast::MemberExpression::ComputedMemberExpression(member) => {
                match super::expr::unparen_expr(&member.expression) {
                    ast::Expression::StringLiteral(name) => Some(name.value.as_ref()),
                    _ => None,
                }
            }
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
            self.pos(member.span()),
        );
        true
    }
}

/// A source declare function takes the mirror example (§312 rule 1).
pub(super) fn source_ambient_function(
    module: &oxc_ast::ast::Program,
    function: &oxc_ast::ast::Function,
) -> bool {
    use oxc_ast::ast;
    module.body.iter().any(|item| {
        let Some(declaration) = super::module_decl(item) else { return false; };
        matches!(declaration, ast::Declaration::FunctionDeclaration(f) if f.declare && f.span == function.span)
    })
}
