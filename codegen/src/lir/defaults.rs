//! One lowered function for each checked parameter default.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum DefaultOwner {
    Function(l::FunctionId),
    Method(l::MethodId),
}

impl DefaultOwner {
    pub(super) fn from_target(target: &l::CallTargetKind) -> Option<Self> {
        match target {
            l::CallTargetKind::Function(id) => Some(Self::Function(*id)),
            l::CallTargetKind::Method(id) => Some(Self::Method(*id)),
            _ => None,
        }
    }
}

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn lower_default_function(
        &mut self,
        owner: DefaultOwner,
        params: &[CallParam],
        index: usize,
        default: &hir::Expr,
        receiver: Option<&PreparedBase>,
        pending: &PendingArguments,
    ) -> Result<l::Operand, LowerError> {
        let mut operands = Vec::new();
        let receiver = if let Some(receiver) = receiver {
            let value = match receiver {
                PreparedBase::Value(value) => value.clone(),
                PreparedBase::Place(place) => {
                    self.materialize_address_inner(place, &default.pos, false)?
                }
            };
            let ty = self.operand_type(&value, &default.pos)?;
            let class = match ty {
                l::ValueType::Data(Type::Class(class)) => class,
                l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(class),
                    ..
                }) => class,
                _ => return Err(self.error(&default.pos, "default receiver has no class")),
            };
            operands.push(value);
            Some(class)
        } else {
            None
        };
        let mut parameters: Vec<_> = params[..index]
            .iter()
            .map(|p| (p.name.clone(), p.ty.clone()))
            .collect();
        operands.extend(pending.values.iter().cloned());
        let mut free = Vec::new();
        default_locals(default, &mut free);
        for (name, ty) in free {
            if parameters.iter().any(|(parameter, _)| *parameter == name) {
                continue;
            }
            let binding = self.lookup_binding(&name, &default.pos)?;
            operands.push(self.read_binding(binding, &default.pos)?);
            parameters.push((name, ty));
        }
        let key = (owner, index);
        let id = if let Some(id) = self.lowering.default_functions.get(&key) {
            *id
        } else {
            let id = self.lowering.allocate_function_id();
            // Publish the identity before the body requests recursive defaults.
            self.lowering.default_functions.insert(key, id);
            let input = FunctionInput {
                name: format!(
                    "<default {}:{}:{}>",
                    default.pos.file, default.pos.line, default.pos.col
                ),
                exported: false,
                is_generator: false,
                is_async: false,
                creation_traps: Vec::new(),
                host_entry_traps: None,
                can_raise: params[index].default_can_raise,
                params: parameters
                    .iter()
                    .map(|(name, ty)| {
                        hir::Param::new(name.clone(), ty.clone(), default.pos.clone())
                    })
                    .collect(),
                ret: params[index].ty.clone(),
                body: vec![hir::Stmt::Return {
                    value: Some(default.clone()),
                    pos: default.pos.clone(),
                }],
                pos: default.pos.clone(),
            };
            self.lowering.lower_function_input(
                id,
                input,
                l::FunctionKind::SynthesizedHelper,
                receiver,
                Vec::new(),
            )?;
            id
        };
        let parameter_types = operands
            .iter()
            .map(|value| self.operand_type(value, &default.pos))
            .collect::<Result<Vec<_>, _>>()?;
        let stored = parameter_types
            .iter()
            .enumerate()
            .filter(|(_, ty)| matches!(ty, l::ValueType::Data(_)))
            .map(|(index, ty)| StoredOperand {
                index,
                ty: ty.clone(),
                action: OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument),
                pos: default.pos.clone(),
            })
            .collect();
        let mut traps = if params[index].default_can_raise || self.lowering.reload {
            vec![l::Trap {
                kind: l::TrapKind::Raise(l::RaiseEdge::Propagate),
                pos: default.pos.clone(),
            }]
        } else {
            Vec::new()
        };
        traps.push(l::Trap {
            kind: l::TrapKind::Call,
            pos: default.pos.clone(),
        });
        for (index, ty) in parameter_types.iter().enumerate() {
            for mut trap in self.read_lifetime(ty, &default.pos) {
                trap.kind = l::TrapKind::DevOnlyLifetime(index);
                traps.push(trap);
            }
        }
        self.emit_store_instruction(
            l::InstructionKind::Call(l::CallTarget {
                kind: l::CallTargetKind::Function(id),
                parameter_types,
                return_type: Some(l::ValueType::Data(params[index].ty.clone())),
            }),
            operands,
            stored,
            (Some(l::ValueType::Data(params[index].ty.clone())), true),
            traps,
            default.pos.clone(),
        )?
        .ok_or_else(|| self.error(&default.pos, "default function produced no result"))
    }
}

fn default_locals(value: &hir::Expr, out: &mut Vec<(String, Type)>) {
    if let hir::ExprKind::Lambda { captures, .. } = &value.kind {
        out.extend(
            captures
                .iter()
                .map(|capture| (capture.name.clone(), capture.ty.clone())),
        );
        return;
    }
    if let hir::ExprKind::Local(name, ty) = &value.kind {
        if !out.iter().any(|(local, _)| local == name) {
            out.push((name.clone(), ty.clone()));
        }
    }
    for child in value.children() {
        if let hir::HirChild::Expr(child) = child {
            default_locals(child, out);
        }
    }
}
