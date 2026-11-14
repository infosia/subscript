//! One lowered function for each checked parameter default.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum DefaultOwner {
    Function(l::FunctionId),
    Method(l::MethodId),
    Lambda(hir::LambdaId),
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

#[derive(Clone, Copy)]
pub(super) struct DefaultEnvironment<'a> {
    pub(super) receiver: Option<&'a PreparedBase>,
    pub(super) closure: Option<&'a l::Operand>,
}

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn lower_default_function(
        &mut self,
        owner: DefaultOwner,
        params: &[CallParam],
        index: usize,
        default: &hir::Expr,
        environment: DefaultEnvironment<'_>,
        pending: &PendingArguments,
    ) -> Result<l::Operand, LowerError> {
        let DefaultEnvironment { receiver, closure } = environment;
        let environment = if let (DefaultOwner::Lambda(id), Some(_)) = (owner, closure) {
            Some(
                self.lowering
                    .lambda_environments
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| {
                        self.error(&default.pos, "lambda default environment is missing")
                    })?,
            )
        } else {
            None
        };
        let environment_function = environment.as_ref().map(|(id, _, _)| *id);
        let mut operands: Vec<_> = closure.cloned().into_iter().collect();
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
        if closure.is_none() {
            default_locals(default, &mut free);
        }
        for (name, ty) in free {
            if parameters.iter().any(|(parameter, _)| *parameter == name) {
                continue;
            }
            let binding = self.lookup_binding(&name, &default.pos)?;
            operands.push(self.read_binding(binding, &default.pos)?);
            parameters.push((name, ty));
        }
        let key = (owner, index, closure.is_some());
        let id = if let Some(id) = self.lowering.default_functions.get(&key) {
            *id
        } else {
            let id = self.lowering.allocate_function_id();
            // Publish the identity before the body requests recursive defaults.
            self.lowering.default_functions.insert(key, id);
            let input = FunctionInput {
                lambda_id: None,
                owned_environment: environment.as_ref().is_some_and(|(_, owned, _)| *owned),
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
                if closure.is_some() {
                    l::FunctionKind::Lambda
                } else {
                    l::FunctionKind::SynthesizedHelper
                },
                receiver,
                environment.map_or_else(Vec::new, |(_, _, captures)| captures),
            )?;
            id
        };
        let parameter_types = operands
            .iter()
            .map(|value| self.operand_type(value, &default.pos))
            .collect::<Result<Vec<_>, _>>()?;
        // compiler.md §256 rule 1: the call passes each earlier argument to
        // the default and to the callee, so each pass takes a count of its
        // own. A value with one owner only is retained twice before each
        // default call, and then one count is released. The default call and
        // the callee each move one retained count: the LIR verifier takes a
        // fresh owner at a store only for a single use. Another value
        // acquires its count as an argument does.
        let mut stored = Vec::new();
        for (index, ty) in parameter_types.iter().enumerate() {
            if (index == 0 && closure.is_some()) || !matches!(ty, l::ValueType::Data(_)) {
                continue;
            }
            let single_owner = matches!(&operands[index], l::Operand::Value(value)
                if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner)
                    && !self.moved_async_owners.contains(value));
            let action = if single_owner && is_async_owner_type(ty) {
                let kind = if matches!(ty, l::ValueType::Data(Type::AsyncHandle(_))) {
                    l::InstructionKind::AsyncHandleRetain
                } else {
                    l::InstructionKind::AsyncHandleArrayRetain
                };
                for _ in 0..2 {
                    self.emit(
                        kind.clone(),
                        vec![operands[index].clone()],
                        None,
                        false,
                        self.read_lifetime(ty, &default.pos),
                        default.pos.clone(),
                    )?;
                }
                self.release_owner(operands[index].clone(), ty, &default.pos)?;
                OwnerStoreAction::Move
            } else {
                OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument)
            };
            stored.push(StoredOperand {
                index,
                ty: ty.clone(),
                action,
                pos: default.pos.clone(),
            });
        }
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
                kind: if let Some(lambda) = environment_function {
                    l::CallTargetKind::ClosureDefault { helper: id, lambda }
                } else {
                    l::CallTargetKind::Function(id)
                },
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
    if let hir::ExprKind::Local(name, ty, _) = &value.kind {
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
