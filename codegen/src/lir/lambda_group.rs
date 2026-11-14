//! Shared code and environments for local lambda groups (compiler.md §293).
use super::*;

pub(super) fn members(module: &hir::Module) -> HashMap<hir::LambdaId, Vec<hir::Expr>> {
    fn visit(child: hir::HirChild<'_>, groups: &mut HashMap<hir::LambdaId, Vec<hir::Expr>>) {
        let children = match child {
            hir::HirChild::Expr(e) => {
                if let hir::ExprKind::Lambda {
                    group: Some(group), ..
                } = e.kind
                {
                    groups.entry(group).or_default().push(e.clone());
                }
                e.children()
            }
            hir::HirChild::Stmt(s) => s.children(),
        };
        for child in children {
            visit(child, groups);
        }
    }
    let mut groups = HashMap::new();
    for owner in module.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(e) => visit(hir::HirChild::Expr(e), &mut groups),
            hir::ExpressionOwner::Body { statements, .. } => {
                for s in statements {
                    visit(hir::HirChild::Stmt(s), &mut groups);
                }
            }
        }
    }
    groups
}

impl FunctionBuilder<'_, '_> {
    fn current_environment(&self) -> Vec<l::Operand> {
        self.parameters
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    l::ParameterKind::Capture | l::ParameterKind::OwnedEnvironment
                )
            })
            .map(|p| l::Operand::Value(p.value))
            .collect()
    }

    pub(super) fn lower_group_value(
        &mut self,
        id: hir::LambdaId,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let function = self
            .lowering
            .lambda_environments
            .get(&id)
            .map(|(id, _, _)| *id)
            .ok_or_else(|| self.error(&expr.pos, "group member code is missing"))?;
        // §293 rule 3: stack closures copy captures; owned closures retain the same environment object.
        self.emit(
            l::InstructionKind::MakeClosure(function),
            self.current_environment(),
            Some(l::ValueType::Data(expr.ty.clone())),
            false,
            Vec::new(),
            expr.pos.clone(),
        )?
        .ok_or_else(|| self.error(&expr.pos, "group value is missing"))
    }

    pub(super) fn lower_group_lambda(
        &mut self,
        group: hir::LambdaId,
        expr: &hir::Expr,
    ) -> Result<(l::FunctionId, l::Operand), LowerError> {
        let hir::ExprKind::Lambda {
            id: member_id,
            captures,
            owns_environment,
            ..
        } = &expr.kind
        else {
            return Err(self.error(&expr.pos, "a group member lowers from a lambda expression"));
        };
        if !self.group_environments.contains_key(&group) {
            let members = self.lowering.group_members[&group].clone();
            let mut values = captures
                .iter()
                .map(|c| {
                    let binding = self.lookup_binding(&c.name, &expr.pos)?;
                    self.read_binding(binding, &expr.pos)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let owned = *owns_environment && !captures.is_empty();
            let mut lowered_captures = captures.clone();
            if owned {
                let class = ClassId(self.lowering.classes.len());
                let first = self
                    .lowering
                    .classes
                    .iter()
                    .map(|c| c.fields.len())
                    .sum::<usize>() as u32;
                self.lowering.classes.push(l::Class {
                    id: class,
                    source_name: format!("<group environment {}>", group.0),
                    is_value: false,
                    is_descriptor: false,
                    is_boundary: false,
                    copies_boundary_bytes: false,
                    boundary_header: None,
                    is_embedded_header: false,
                    callback_lifetime: subscript_compiler::types::CallbackLifetime::Context,
                    alignment: None,
                    fields: captures
                        .iter()
                        .enumerate()
                        .map(|(i, c)| l::Field {
                            id: l::FieldId(first + i as u32),
                            source_name: c.name.clone(),
                            ty: c.ty.clone(),
                            is_defaulted: false,
                            is_absence_capable: false,
                            foreign_provenance: None,
                            pos: expr.pos.clone(),
                        })
                        .collect(),
                    field_releases: Vec::new(),
                    constructor: None,
                    methods: Vec::new(),
                    index_signature: None,
                    pos: expr.pos.clone(),
                });
                self.lowering
                    .handle_classes
                    .push(subscript_compiler::types::HandleClass::Reference);
                let environment = self
                    .emit(
                        l::InstructionKind::AllocateClass(class),
                        Vec::new(),
                        Some(l::ValueType::Data(Type::Class(class))),
                        false,
                        vec![l::Trap {
                            kind: l::TrapKind::Allocation,
                            pos: expr.pos.clone(),
                        }],
                        expr.pos.clone(),
                    )?
                    .ok_or_else(|| self.error(&expr.pos, "group environment is missing"))?;
                for (i, value) in values.into_iter().enumerate() {
                    self.store_class_field(class, i, environment.clone(), value, &expr.pos)?;
                }
                values = vec![environment];
                let mut capture = captures[0].clone();
                capture.name = "[[group environment]]".to_string();
                capture.ty = Type::Class(class);
                lowered_captures.insert(0, capture);
            }
            let mut functions = Vec::new();
            for member in members {
                let hir::ExprKind::Lambda {
                    id,
                    params,
                    ret,
                    body,
                    can_raise,
                    is_async,
                    ..
                } = &member.kind
                else {
                    return Err(self.error(
                        &member.pos,
                        "a group member lowers from a lambda expression",
                    ));
                };
                let function_id = self.lowering.allocate_function_id();
                self.lowering
                    .lambda_environments
                    .insert(*id, (function_id, owned, lowered_captures.clone()));
                functions.push((
                    function_id,
                    FunctionInput {
                        lambda_id: Some(*id),
                        owned_environment: owned,
                        name: format!(
                            "<lambda {}:{}:{}>",
                            member.pos.file, member.pos.line, member.pos.col
                        ),
                        exported: false,
                        is_generator: false,
                        is_async: *is_async,
                        creation_traps: if *is_async {
                            vec![hir::TrapSite::Allocation {
                                pos: member.pos.clone(),
                            }]
                        } else {
                            Vec::new()
                        },
                        host_entry_traps: None,
                        can_raise: *can_raise,
                        params: params.clone(),
                        ret: ret.clone(),
                        body: body.clone(),
                        pos: member.pos.clone(),
                    },
                ));
            }
            let root = functions[0].0;
            for (id, _) in &functions {
                self.lowering.group_functions.insert(*id, root);
            }
            for (id, function) in functions {
                self.lowering.lower_function_input(
                    id,
                    function,
                    l::FunctionKind::Lambda,
                    None,
                    lowered_captures.clone(),
                )?;
            }
            self.group_environments.insert(group, values);
        }
        let id = self.lowering.lambda_environments[member_id].0;
        let closure = self
            .emit(
                l::InstructionKind::MakeClosure(id),
                self.group_environments[&group].clone(),
                Some(l::ValueType::Data(expr.ty.clone())),
                false,
                convert_traps(&expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload)),
                expr.pos.clone(),
            )?
            .ok_or_else(|| self.error(&expr.pos, "group closure is missing"))?;
        Ok((id, closure))
    }
}
