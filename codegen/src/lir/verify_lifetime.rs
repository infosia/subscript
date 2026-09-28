//! Total read-through operand contracts (compiler.md §120.2 item 3).

use super::*;
use subscript_compiler::types::{HandleClass, HandleKind};

pub(super) fn verify(module: &l::Module, errors: &mut Vec<VerifyError>) {
    let classes = module
        .classes
        .iter()
        .map(|class| {
            if !class.is_value {
                HandleClass::Reference
            } else if class.is_boundary {
                HandleClass::BoundaryValue
            } else {
                HandleClass::Value
            }
        })
        .collect::<Vec<_>>();
    for function in &module.functions {
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                for message in site_errors(module, function, instruction, &classes) {
                    errors.push(super::verify::finding(
                        function,
                        format!("block {} instruction {index} {message}", block.id.0),
                    ));
                }
            }
        }
    }
}

fn site_errors(
    module: &l::Module,
    function: &l::Function,
    instruction: &l::Instruction,
    classes: &[HandleClass],
) -> Vec<String> {
    let (reads, guarded) = match read_operands(module, function, instruction, classes) {
        Ok(reads) => reads,
        Err(error) => return vec![error],
    };
    let mut errors = Vec::new();
    for index in &reads {
        if needs_lifetime(function, instruction, *index, classes)
            && !instruction.traps.iter().any(|trap| {
                matches!(trap.kind,
                l::TrapKind::DevOnlyLifetime(i) | l::TrapKind::DevOnlyRelease(i) if i == *index)
            })
        {
            errors.push(format!("read-through operand {index} has no lifetime site"));
        }
    }
    for trap in &instruction.traps {
        if let l::TrapKind::DevOnlyLifetime(index) | l::TrapKind::DevOnlyRelease(index) = trap.kind
        {
            if !needs_lifetime(function, instruction, index, classes) {
                errors.push(format!(
                    "lifetime operand {index} type does not need a lifetime trap"
                ));
            }
            let release = matches!(trap.kind, l::TrapKind::DevOnlyRelease(_))
                && matches!(&instruction.kind, l::InstructionKind::Call(target)
                    if matches!(&target.kind, l::CallTargetKind::Intrinsic(intrinsic)
                        if module.intrinsic_operations.iter().any(|operation|
                            operation.family == intrinsic.family && operation.operation == intrinsic.operation
                                && operation.semantic_name == "UnsafeDelete")))
                && index == 0;
            if !reads.contains(&index) && !guarded.contains(&index) && !release {
                errors.push(format!(
                    "lifetime operand {index} is neither read-through nor guarded"
                ));
            }
        }
    }
    errors
}

fn needs_lifetime(
    function: &l::Function,
    instruction: &l::Instruction,
    index: usize,
    classes: &[HandleClass],
) -> bool {
    instruction
        .operands
        .get(index)
        .and_then(|operand| super::verify::operand_type(function, operand))
        .is_some_and(|ty| {
            matches!(ty, l::ValueType::Data(ty)
            if ty.handle_kind(classes).is_some_and(HandleKind::needs_lifetime_trap))
        })
}

fn read_operands(
    module: &l::Module,
    function: &l::Function,
    instruction: &l::Instruction,
    classes: &[HandleClass],
) -> Result<(Vec<usize>, Vec<usize>), String> {
    use l::InstructionKind::*;
    let reads = match &instruction.kind {
        // A type-preserving handle Copy is an explicit alias guard, used before static reduce loops.
        Copy => {
            let input = instruction
                .operands
                .first()
                .and_then(|operand| super::verify::operand_type(function, operand));
            let output = instruction
                .result
                .and_then(|value| super::verify::value_type(function, value));
            let guarded =
                needs_lifetime(function, instruction, 0, classes) && input.as_ref() == output;
            return Ok((vec![], if guarded { vec![0] } else { vec![] }));
        }
        AddressOfField(_) => return Ok((vec![], vec![0])),
        AddressOfIndex { .. }
        | LoadField(_)
        | Length
        | ForeignArrayData
        | SetFromSource(_)
        | MapFromSource
        | IteratorCreate { .. }
        | AsyncHandleRetain
        | AsyncHandleRelease
        | AsyncHandleArrayRetain
        | AsyncHandleArrayRelease => vec![0],
        Cast => {
            let result_is_class = instruction
                .result
                .and_then(|value| super::verify::value_type(function, value))
                .is_some_and(|ty| matches!(ty, l::ValueType::Data(Type::Class(_))));
            let input_is_object = instruction
                .operands
                .first()
                .and_then(|operand| super::verify::operand_type(function, operand))
                .is_some_and(|ty| match ty {
                    l::ValueType::Data(Type::Object) => true,
                    l::ValueType::Data(Type::Nullable(inner)) => *inner == Type::Object,
                    _ => false,
                });
            if result_is_class && input_is_object {
                vec![0]
            } else {
                vec![]
            }
        }
        ArraySpreadLiteral(spreads) => spreads
            .iter()
            .enumerate()
            .filter_map(|(index, spread)| spread.is_some().then_some(index))
            .collect(),
        Call(target) | AsyncHandleCreate(target) => {
            call_reads(module, target, instruction.operands.len())?
        }
        StringLiteral(_)
        | LoadLocal(_)
        | StoreLocal(_)
        | AddressOfLocal(_)
        | LoadGlobal(_)
        | StoreGlobal(_)
        | AddressOfGlobal(_)
        | FunctionRef(_)
        | Unary(_)
        | Binary(_)
        | Coerce
        | NarrowNonNull(_)
        | AllocateClass(_)
        | BoxBoundaryValue { .. }
        | AddressOfValue
        | LoadAddress
        | StoreAddress
        | ArrayLiteral
        | ArrayWithCapacity
        | Template(_)
        | MakeClosure(_)
        | IteratorHasNext
        | IteratorValue
        | IteratorBound
        | IteratorAdvance
        | Zero
        | Throw
        | CatchEntry
        | ExceptionPark
        | ExceptionResume
        | AwaitRaise => vec![],
    };
    Ok((reads, vec![]))
}

fn call_reads(
    module: &l::Module,
    target: &l::CallTarget,
    count: usize,
) -> Result<Vec<usize>, String> {
    Ok(match &target.kind {
        l::CallTargetKind::Function(id) => {
            if module
                .functions
                .get(id.0 as usize)
                .is_some_and(|function| function.kind == l::FunctionKind::SynthesizedHelper)
            {
                (0..count).collect()
            } else {
                vec![]
            }
        }
        l::CallTargetKind::Method(id) => {
            if super::verify_instruction::declared_method_function(module, *id).is_some_and(
                |function| matches!(function.kind, l::FunctionKind::Constructor { .. }),
            ) {
                vec![]
            } else {
                vec![0]
            }
        }
        l::CallTargetKind::BuiltinMethod(
            l::BuiltinMethod::ArrayPop
            | l::BuiltinMethod::StringSlice
            | l::BuiltinMethod::GeneratorNext,
        ) => vec![0],
        l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPush) => vec![0],
        l::CallTargetKind::StaticClosure(_)
        | l::CallTargetKind::Foreign(_)
        | l::CallTargetKind::Indirect => vec![],
        l::CallTargetKind::Intrinsic(intrinsic) => {
            let Some(operation) = module.intrinsic_operations.iter().find(|operation| {
                operation.family == intrinsic.family && operation.operation == intrinsic.operation
            }) else {
                return Err("intrinsic operation record is missing".to_string());
            };
            use l::IntrinsicFamily::*;
            let stored: &[usize] = match (intrinsic.family, operation.semantic_name.as_str()) {
                (Ambient, "UnsafeDelete") => return Ok(vec![]),
                (Array, "Fill" | "Unshift") | (Set, "Add") => &[1],
                (Map, "Set") => &[1, 2],
                (
                    Ambient | ContextBytes | Math | Number | Date | Json | Text | String | Regex
                    | Array | Map | Set | Worker,
                    _,
                ) => &[],
            };
            (0..count).filter(|index| !stored.contains(index)).collect()
        }
    })
}
