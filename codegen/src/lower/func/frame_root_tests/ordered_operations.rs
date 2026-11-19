//! Native operations retain their order within each LIR point.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Write,
    Zero,
}

pub(super) type FieldOperations = BTreeMap<Point, Vec<(String, Operation)>>;

pub(super) struct ByteOperation {
    pub offset: u32,
    pub size: u32,
    pub operation: Operation,
}

// Convert byte operations in source order. A complete zero establishes a field clear.
// A partial clear keeps the existing full-field policy of the native checker.
pub(super) fn jit_operations(
    function: &l::Function,
    operations: &BTreeMap<Point, Vec<ByteOperation>>,
    fields: &[Field],
) -> FieldOperations {
    let mut result = FieldOperations::new();
    for (point, operations) in operations {
        if !lir_point(function, *point) {
            continue;
        }
        let mut zeros = BTreeSet::new();
        for operation in operations {
            let end = operation.offset + operation.size;
            match operation.operation {
                Operation::Zero => zeros.extend(operation.offset..end),
                Operation::Write => {
                    for byte in operation.offset..end {
                        zeros.remove(&byte);
                    }
                }
            }
            for field in fields {
                if field.offset >= end || field.offset + field.size <= operation.offset {
                    continue;
                }
                if operation.operation == Operation::Write
                    || (field.offset..field.offset + field.size).all(|byte| zeros.contains(&byte))
                {
                    result
                        .entry(*point)
                        .or_default()
                        .push((field.name.clone(), operation.operation));
                }
            }
        }
    }
    result
}

fn lir_point(function: &l::Function, point: Point) -> bool {
    function
        .blocks
        .get(point.0 as usize)
        .is_some_and(|block| point.1 <= block.instructions.len() + 1)
}

pub(super) fn null_assignments(function: &l::Function) -> BTreeMap<Point, BTreeSet<String>> {
    let mut null_values = function
        .values
        .iter()
        .filter(|value| matches!(value.ty, l::ValueType::Data(Type::Null)))
        .map(|value| value.id)
        .collect::<BTreeSet<_>>();
    let null = |operand: &l::Operand, values: &BTreeSet<l::ValueId>| match operand {
        l::Operand::Constant(value) => matches!(value.kind, l::ConstantKind::Null),
        l::Operand::Value(value) => values.contains(value),
    };
    loop {
        let previous = null_values.len();
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if matches!(
                instruction.kind,
                l::InstructionKind::Copy | l::InstructionKind::Cast
            ) && instruction
                .operands
                .first()
                .is_some_and(|operand| null(operand, &null_values))
            {
                if let Some(value) = instruction.result {
                    null_values.insert(value);
                }
            }
        }
        if previous == null_values.len() {
            break;
        }
    }
    let mut result = BTreeMap::<Point, BTreeSet<String>>::new();
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let point = (block.id.0, index + 1);
            if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                if instruction
                    .operands
                    .first()
                    .is_some_and(|operand| null(operand, &null_values))
                {
                    result
                        .entry(point)
                        .or_default()
                        .insert(format!("l{}", local.0));
                }
            }
            if let Some(value) = instruction
                .result
                .filter(|value| null_values.contains(value))
            {
                result
                    .entry(point)
                    .or_default()
                    .insert(format!("env_v{}", value.0));
            }
        }
        for target in block.terminator.targets() {
            for (parameter, argument) in function.blocks[target.block.0 as usize]
                .parameters
                .iter()
                .zip(&target.arguments)
            {
                if null(argument, &null_values) {
                    result
                        .entry((block.id.0, block.instructions.len() + 1))
                        .or_default()
                        .insert(format!("env_v{}", parameter.0));
                }
            }
        }
    }
    result
}

// compiler.md §338 rule 5: the final operation at a non-null assignment must be its write.
pub(super) fn ordered_clears(
    function: &l::Function,
    lifetime: &BTreeSet<String>,
    assignments: &BTreeMap<Point, BTreeSet<String>>,
    c: &FieldOperations,
    jit: &FieldOperations,
) -> Result<(), String> {
    let nulls = null_assignments(function);
    let parameters = function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .parameters
                .iter()
                .map(move |value| ((block.id.0, 0), format!("env_v{}", value.0)))
        })
        .collect::<BTreeSet<_>>();
    for (tier, operations) in [("C", c), ("JIT", jit)] {
        for (point, operations) in operations {
            if !lir_point(function, *point) {
                continue;
            }
            let last = operations
                .iter()
                .map(|(name, operation)| (name, *operation))
                .collect::<BTreeMap<_, _>>();
            for (name, operation) in operations {
                if *operation != Operation::Zero || !lifetime.contains(name) {
                    continue;
                }
                let assignment = assignments
                    .get(point)
                    .is_some_and(|fields| fields.contains(name))
                    && !(tier == "C" && parameters.contains(&(*point, name.clone())));
                let null = nulls.get(point).is_some_and(|fields| fields.contains(name));
                require!(
                    assignment && (null || last.get(name) == Some(&Operation::Write)),
                    "{tier} lifetime clear {name} at {point:?}: final operation {:?}",
                    last.get(name)
                );
            }
        }
    }
    Ok(())
}
