//! Suspension live-in threading and local storage classification.

use super::verify_dominance::{dominators, predecessors, successors, terminator_values};
use super::*;

/// Turns every value live across a suspend edge into an explicit successor
/// definition and repairs SSA at downstream joins. The input graph is already
/// ordinary SSA; a suspension is the additional definition boundary.
pub(super) fn thread_suspension_live_ins(function: &mut l::Function) -> Result<(), LowerError> {
    let original_value_count = function.values.len();
    let live_in = lir_live_ins(function, original_value_count);
    for block in &mut function.blocks {
        let l::Terminator::Suspend {
            successor,
            invalidates,
            ..
        } = &mut block.terminator
        else {
            continue;
        };
        let successor_live_in = live_in
            .get(successor.0 as usize)
            .cloned()
            .unwrap_or_default();
        invalidates.retain(|value| successor_live_in.contains(value));
    }
    let mut value_origins = (0..original_value_count)
        .map(|index| l::ValueId(index as u32))
        .collect::<Vec<_>>();
    let block_count = function.blocks.len();
    let mut carried = vec![Vec::<(l::ValueId, l::ValueId)>::new(); block_count];
    let mut suspend_successors = BTreeSet::new();
    for block in &function.blocks {
        if let l::Terminator::Suspend { successor, .. } = block.terminator {
            suspend_successors.insert(successor);
        }
    }

    for successor in suspend_successors {
        let Some(destination) = function.blocks.get(successor.0 as usize) else {
            return Err(LowerError {
                pos: function.pos.clone(),
                message: format!("suspend successor block {} is missing", successor.0),
            });
        };
        let values = live_in
            .get(successor.0 as usize)
            .cloned()
            .unwrap_or_default();
        let destination_id = destination.id;
        for original in values {
            let definition = function
                .values
                .get(original.0 as usize)
                .cloned()
                .ok_or_else(|| LowerError {
                    pos: function.pos.clone(),
                    message: format!("live-in value {} is missing", original.0),
                })?;
            let parameter = l::ValueId(function.values.len() as u32);
            function.values.push(l::Value {
                id: parameter,
                ty: definition.ty,
                fresh_owner: definition.fresh_owner,
                source_name: definition.source_name,
            });
            value_origins.push(original);
            function.blocks[destination_id.0 as usize]
                .parameters
                .push(parameter);
            carried[destination_id.0 as usize].push((original, parameter));
        }
    }

    let origins = carried
        .iter()
        .flat_map(|values| values.iter().map(|(original, _)| *original))
        .collect::<BTreeSet<_>>();
    if origins.is_empty() {
        function.liveness = l::Liveness {
            live_ins: live_in
                .into_iter()
                .map(|values| values.into_iter().collect())
                .collect(),
            generator_cleanup: std::mem::take(&mut function.liveness.generator_cleanup),
            generator_close: std::mem::take(&mut function.liveness.generator_close),
            value_origins,
        };
        return Ok(());
    }

    let reachable = reachable_blocks(function);
    let function_parameters = function
        .parameters
        .iter()
        .map(|parameter| parameter.value)
        .collect::<HashSet<_>>();

    // compiler.md §273 rule 1: reuse the CFG dominators for pruned SSA.
    let mut preds = predecessors(function);
    // compiler.md §273 rule 6: dead edges cannot affect reachable dominators.
    for predecessors in &mut preds {
        predecessors.retain(|pred| reachable[pred.0 as usize]);
    }
    let dom = dominators(function, &preds);
    let mut children = vec![Vec::new(); block_count];
    let mut frontier = vec![BTreeSet::new(); block_count];
    for block in &function.blocks {
        let index = block.id.0 as usize;
        if !reachable[index] || block.id == function.entry {
            continue;
        }
        if let Some(parent) = dom[index]
            .iter()
            .filter(|candidate| **candidate != block.id && reachable[candidate.0 as usize])
            .max_by_key(|candidate| dom[candidate.0 as usize].len())
        {
            children[parent.0 as usize].push(index);
        }
    }
    for block in &function.blocks {
        let index = block.id.0 as usize;
        if !reachable[index] {
            continue;
        }
        for pred in &preds[index] {
            if !reachable[pred.0 as usize] {
                continue;
            }
            for candidate in &dom[pred.0 as usize] {
                if *candidate == block.id || !dom[index].contains(candidate) {
                    frontier[candidate.0 as usize].insert(index);
                }
            }
        }
    }

    for origin in origins {
        let mut special = vec![None; block_count];
        for (block_index, values) in carried.iter().enumerate() {
            special[block_index] = values
                .iter()
                .find_map(|(candidate, parameter)| (*candidate == origin).then_some(*parameter));
        }
        let mut merges = vec![None; block_count];
        let mut definitions = vec![false; block_count];
        let mut pending = VecDeque::new();
        for (index, block) in function.blocks.iter().enumerate() {
            definitions[index] = special[index].is_some()
                || block.parameters.contains(&origin)
                || (block.id == function.entry && function_parameters.contains(&origin))
                || block
                    .instructions
                    .iter()
                    .any(|instruction| instruction.result == Some(origin));
            if reachable[index] && definitions[index] {
                pending.push_back(index);
            }
        }
        // compiler.md §273 rules 1 to 3: each frontier site enters the queue once.
        while let Some(definition) = pending.pop_front() {
            for &index in &frontier[definition] {
                if !live_in[index].contains(&origin)
                    || special[index].is_some()
                    || merges[index].is_some()
                {
                    continue;
                }
                let definition = function.values[origin.0 as usize].clone();
                let parameter = l::ValueId(function.values.len() as u32);
                function.values.push(l::Value {
                    id: parameter,
                    ty: definition.ty,
                    fresh_owner: definition.fresh_owner,
                    source_name: definition.source_name,
                });
                value_origins.push(origin);
                function.blocks[index].parameters.push(parameter);
                merges[index] = Some(parameter);
                if !definitions[index] {
                    pending.push_back(index);
                }
            }
        }
        let mut incoming = vec![None; block_count];
        let mut outgoing = vec![None; block_count];
        let mut walk = vec![(function.entry.0 as usize, None)];
        while let Some((index, inherited)) = walk.pop() {
            let block = &function.blocks[index];
            incoming[index] = special[index].or(merges[index]).or(inherited);
            outgoing[index] = if block.parameters.contains(&origin)
                || (block.id == function.entry && function_parameters.contains(&origin))
                || block
                    .instructions
                    .iter()
                    .any(|instruction| instruction.result == Some(origin))
            {
                Some(origin)
            } else {
                incoming[index]
            };
            for &child in children[index].iter().rev() {
                walk.push((child, outgoing[index]));
            }
        }

        for block_index in 0..block_count {
            if !reachable[block_index] {
                continue;
            }
            let block_id = function.blocks[block_index].id;
            let block_parameter_definition = function.blocks[block_index]
                .parameters
                .iter()
                .take_while(|value| (value.0 as usize) < original_value_count)
                .any(|value| *value == origin);
            let function_parameter_definition =
                block_id == function.entry && function_parameters.contains(&origin);
            let mut current = if block_parameter_definition || function_parameter_definition {
                Some(origin)
            } else {
                incoming[block_index]
            };

            let parameters = function.blocks[block_index].parameters.clone();
            for parameter in parameters {
                if parameter != origin {
                    replace_address_base(function, parameter, origin, current);
                }
            }
            let instruction_count = function.blocks[block_index].instructions.len();
            for instruction_index in 0..instruction_count {
                let instruction = &mut function.blocks[block_index].instructions[instruction_index];
                replace_operands(&mut instruction.operands, origin, current);
                replace_ids(&mut instruction.invalidates, origin, current);
                let result = instruction.result;
                if let Some(result) = result {
                    replace_address_base(function, result, origin, current);
                }
                if result == Some(origin) {
                    current = Some(origin);
                }
            }
            replace_terminator_uses(
                &mut function.blocks[block_index].terminator,
                origin,
                current,
            );
            for cleanup in &mut function.liveness.generator_cleanup {
                if cleanup.suspension == Some(block_id) {
                    replace_ids(&mut cleanup.owners, origin, current);
                }
            }
        }

        for (destination_index, merge) in merges.iter().enumerate().take(block_count) {
            let Some(_parameter) = merge else {
                continue;
            };
            let destination = function.blocks[destination_index].id;
            for (source_index, version) in outgoing.iter().copied().enumerate().take(block_count) {
                if !reachable[source_index] {
                    continue;
                }
                if let Some(version) = version {
                    append_normal_edge_argument(
                        &mut function.blocks[source_index].terminator,
                        destination,
                        l::Operand::Value(version),
                    );
                }
            }
        }

        for (source_index, version) in outgoing.iter().copied().enumerate().take(block_count) {
            let successor = match &function.blocks[source_index].terminator {
                l::Terminator::Suspend { successor, .. } => *successor,
                _ => continue,
            };
            if carried[successor.0 as usize]
                .iter()
                .any(|(candidate, _)| *candidate == origin)
            {
                let version = version.ok_or_else(|| LowerError {
                    pos: function.pos.clone(),
                    message: format!(
                        "value {} is live at suspend in block {} but has no reaching definition",
                        origin.0, function.blocks[source_index].id.0
                    ),
                })?;
                if let l::Terminator::Suspend {
                    arguments,
                    ownership,
                    ..
                } = &mut function.blocks[source_index].terminator
                {
                    arguments.push(l::Operand::Value(version));
                    ownership.push(false);
                }
            }
        }
    }
    function.liveness = l::Liveness {
        live_ins: live_in
            .into_iter()
            .map(|values| values.into_iter().collect())
            .collect(),
        generator_cleanup: std::mem::take(&mut function.liveness.generator_cleanup),
        generator_close: std::mem::take(&mut function.liveness.generator_close),
        value_origins,
    };
    Ok(())
}

/// Computes the value live-ins with the lowering's single graph fixed point.
fn lir_live_ins(function: &l::Function, original_value_count: usize) -> Vec<BTreeSet<l::ValueId>> {
    let mut uses = vec![BTreeSet::new(); function.blocks.len()];
    let mut definitions = vec![BTreeSet::new(); function.blocks.len()];
    for block in &function.blocks {
        let index = block.id.0 as usize;
        definitions[index].extend(
            block
                .parameters
                .iter()
                .copied()
                .filter(|value| (value.0 as usize) < original_value_count),
        );
        for instruction in &block.instructions {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    if !definitions[index].contains(value) {
                        uses[index].insert(*value);
                    }
                }
            }
            if let Some(result) = instruction.result {
                definitions[index].insert(result);
            }
        }
        for value in terminator_values(&block.terminator) {
            if !definitions[index].contains(&value) {
                uses[index].insert(value);
            }
        }
    }

    let mut live_in = vec![BTreeSet::new(); function.blocks.len()];
    let mut live_out = vec![BTreeSet::new(); function.blocks.len()];
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let index = block.id.0 as usize;
            let next_out = successors(block)
                .into_iter()
                .filter_map(|successor| live_in.get(successor.0 as usize))
                .flat_map(|values| values.iter().copied())
                .collect::<BTreeSet<_>>();
            let mut next_in = uses[index].clone();
            next_in.extend(
                next_out
                    .iter()
                    .filter(|value| !definitions[index].contains(value))
                    .copied(),
            );
            if live_out[index] != next_out {
                live_out[index] = next_out;
                changed = true;
            }
            if live_in[index] != next_in {
                live_in[index] = next_in;
                changed = true;
            }
        }
        if !changed {
            return live_in;
        }
    }
}

fn reachable_blocks(function: &l::Function) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    let mut queue = VecDeque::from([function.entry]);
    while let Some(block) = queue.pop_front() {
        let Some(mark) = reachable.get_mut(block.0 as usize) else {
            continue;
        };
        if *mark {
            continue;
        }
        *mark = true;
        if let Some(block) = function.blocks.get(block.0 as usize) {
            queue.extend(successors(block));
        }
    }
    reachable
}

/// Marks storage that a resumed activation must read before any redefinition.
pub(super) fn classify_local_storage(function: &mut l::Function) {
    let predecessors = predecessors(function);
    let dominators = dominators(function, &predecessors);
    let mut store_blocks = vec![BTreeSet::new(); function.locals.len()];
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                if let Some(stores) = store_blocks.get_mut(local.0 as usize) {
                    stores.insert(block.id);
                }
            }
        }
    }
    let required = function
        .locals
        .iter()
        .filter(|local| local_requires_frame(function, local.id, &store_blocks, &dominators))
        .map(|local| local.id)
        .collect::<HashSet<_>>();
    for local in &mut function.locals {
        local.storage = if required.contains(&local.id) {
            l::LocalStorageClass::Frame
        } else {
            l::LocalStorageClass::Activation
        };
    }
}

fn local_requires_frame(
    function: &l::Function,
    local: l::LocalId,
    store_blocks: &[BTreeSet<l::BlockId>],
    dominators: &[BTreeSet<l::BlockId>],
) -> bool {
    let Some(store_blocks) = store_blocks.get(local.0 as usize) else {
        return false;
    };
    function.blocks.iter().any(|suspend| {
        let l::Terminator::Suspend { successor, .. } = suspend.terminator else {
            return false;
        };
        let definition_dominates = store_blocks.iter().any(|definition| {
            *definition == suspend.id
                || dominators
                    .get(suspend.id.0 as usize)
                    .is_some_and(|blocks| blocks.contains(definition))
        });
        definition_dominates && local_read_before_redefinition(function, successor, local)
    })
}

fn local_read_before_redefinition(
    function: &l::Function,
    start: l::BlockId,
    local: l::LocalId,
) -> bool {
    let mut pending = VecDeque::from([start]);
    let mut visited = HashSet::new();
    while let Some(block) = pending.pop_front() {
        if !visited.insert(block) {
            continue;
        }
        let Some(block) = function.blocks.get(block.0 as usize) else {
            continue;
        };
        let mut redefined = false;
        for instruction in &block.instructions {
            match instruction.kind {
                l::InstructionKind::LoadLocal(id) | l::InstructionKind::AddressOfLocal(id)
                    if id == local =>
                {
                    return true;
                }
                l::InstructionKind::StoreLocal(id) if id == local => {
                    redefined = true;
                    break;
                }
                _ => {}
            }
        }
        if !redefined {
            pending.extend(successors(block));
        }
    }
    false
}

fn replace_address_base(
    function: &mut l::Function,
    value: l::ValueId,
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    let Some(replacement) = replacement else {
        return;
    };
    if let Some(l::Value {
        ty: l::ValueType::Address(address),
        ..
    }) = function.values.get_mut(value.0 as usize)
    {
        if address.array_base == Some(original) {
            address.array_base = Some(replacement);
        }
    }
}

fn replace_operands(
    operands: &mut [l::Operand],
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    let Some(replacement) = replacement else {
        return;
    };
    for operand in operands {
        if matches!(operand, l::Operand::Value(value) if *value == original) {
            *operand = l::Operand::Value(replacement);
        }
    }
}

fn replace_ids(values: &mut [l::ValueId], original: l::ValueId, replacement: Option<l::ValueId>) {
    let Some(replacement) = replacement else {
        return;
    };
    for value in values {
        if *value == original {
            *value = replacement;
        }
    }
}

fn replace_terminator_uses(
    terminator: &mut l::Terminator,
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    if let Some(replacement) = replacement {
        // A replacement must update invalidation mentions with the read uses.
        terminator.map_values(|value| {
            if value == original {
                replacement
            } else {
                value
            }
        });
    }
}

fn append_normal_edge_argument(
    terminator: &mut l::Terminator,
    destination: l::BlockId,
    argument: l::Operand,
) {
    let append = |target: &mut l::BlockTarget| {
        if target.block == destination {
            target.arguments.push(argument.clone());
            target.ownership.push(false);
        }
    };
    match terminator {
        l::Terminator::Branch(target) => append(target),
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => {
            append(then_target);
            append(else_target);
        }
        l::Terminator::Switch { arms, default, .. } => {
            for arm in arms {
                append(&mut arm.target);
            }
            append(default);
        }
        l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_)
        | l::Terminator::Suspend { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::SourceFile;

    fn source(condition: &str) -> String {
        format!(
            r#"
function* gen(): Generator<i32> {{ yield 1; yield 2; }}
export async function main(): Promise<void> {{
  let n: i32 = 0;
  for (let r: i32 = 0; r < 1; r++) {{
    for (const j of [0, 1]) {{ {condition} }}
    for (const x of gen()) {{ await Context.suspend(); n++; }}
  }}
  print(`end ${{n}}`);
}}
"#
        )
    }

    #[test]
    fn suspension_live_in_fixed_point() {
        // compiler.md §273 acceptance 1 and 3: a regression must fail in time.
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let files = [SourceFile::new("a445.ts", include_str!(
                "../../../corpus/accept/a445-the-suspension-live-in-analysis-reaches-a-fixed-point.ts"
            ))];
            let hir = subscript_compiler::check_program(&files).expect("a445 checks");
            let lir = super::super::lower_module(&hir).expect("a445 analysis ends");
            assert_eq!(crate::interpreter::interpret(&lir).expect("a445 runs"),
                include_bytes!("../../../corpus/accept/a445-the-suspension-live-in-analysis-reaches-a-fixed-point.expected"));
            for condition in ["if (j > 0) { break; }", "if (j > 0) {}"] {
                let files = [SourceFile::new("fixed-point.ts", source(condition))];
                let hir = subscript_compiler::check_program(&files).expect("source checks");
                let lir = super::super::lower_module(&hir).expect("suspension analysis ends");
                assert_eq!(
                    crate::interpreter::interpret(&lir).expect("LIR runs"),
                    b"end 2\n"
                );
            }
            send.send(()).unwrap();
        });
        receive
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("suspension analysis must finish within 10 seconds");
    }

    #[test]
    fn suspension_live_in_control_matches_pin() {
        let files = [SourceFile::new("fixed-point.ts", source(""))];
        let hir = subscript_compiler::check_program(&files).expect("source checks");
        let lir = super::super::lower_module(&hir).expect("control lowers");
        let main = lir
            .functions
            .iter()
            .find(|f| f.source_name == "main")
            .unwrap();
        // compiler.md §273 acceptance 3: pin control blocks and merge origins.
        let shape = main
            .blocks
            .iter()
            .map(|b| (b.source_name.as_deref(), b.parameters.len()))
            .collect::<Vec<_>>();
        assert_eq!(
            shape,
            vec![
                (Some("entry"), 0),
                (Some("for.cond"), 2),
                (Some("for.body"), 0),
                (Some("for.step"), 2),
                (Some("for.exit"), 2),
                (Some("for-of.cond"), 5),
                (Some("for-of.body"), 0),
                (Some("for-of.step"), 5),
                (Some("for-of.exit"), 2),
                (Some("generator-for-of.next"), 3),
                (Some("generator-for-of.body"), 0),
                (Some("generator-for-of.exhausted"), 2),
                (Some("generator-for-of.consumer-exit"), 2),
                (Some("generator-for-of.exit"), 2),
                (Some("async.resume"), 3),
            ]
        );
        let header = &main.blocks[9];
        let resume = &main.blocks[14];
        assert_eq!(
            main.liveness.value_origins[header.parameters[2].0 as usize],
            l::ValueId(26)
        );
        assert_eq!(
            resume
                .parameters
                .iter()
                .map(|v| main.liveness.value_origins[v.0 as usize])
                .collect::<Vec<_>>(),
            vec![l::ValueId(26), l::ValueId(27), l::ValueId(28)]
        );
    }

    #[test]
    fn unreachable_predecessor_preserves_dominance_and_renaming() {
        let files = [SourceFile::new("fixed-point.ts", source(""))];
        let hir = subscript_compiler::check_program(&files).unwrap();
        let mut lir = super::super::lower_module(&hir).unwrap();
        let main = lir
            .functions
            .iter_mut()
            .find(|f| f.source_name == "main")
            .unwrap();
        let origin = l::ValueId(26);
        let mut entry = main.blocks[8].clone();
        entry.id = l::BlockId(0);
        entry.parameters.clear();
        entry.terminator = main.blocks[10].terminator.clone();
        if let l::Terminator::Suspend {
            successor,
            arguments,
            ownership,
            ..
        } = &mut entry.terminator
        {
            *successor = l::BlockId(1);
            arguments.clear();
            ownership.clear();
        }
        let mut resume = main.blocks[6].clone();
        resume.id = l::BlockId(1);
        resume.instructions.clear();
        resume.terminator = l::Terminator::Branch(l::BlockTarget {
            block: l::BlockId(2),
            arguments: vec![],
            ownership: vec![],
        });
        let mut use_block = main.blocks[13].clone();
        use_block.id = l::BlockId(2);
        use_block.parameters.clear();
        use_block.instructions[0].operands = vec![l::Operand::Value(origin)];
        use_block.terminator = main.blocks[4].terminator.clone();
        let mut dead = resume.clone();
        dead.id = l::BlockId(3);
        main.blocks = vec![entry, resume, use_block, dead];
        main.entry = l::BlockId(0);
        let reachable = reachable_blocks(main);
        let mut preds = predecessors(main);
        for predecessors in &mut preds {
            predecessors.retain(|p| reachable[p.0 as usize]);
        }
        assert!(dominators(main, &preds)[2].contains(&l::BlockId(1)));
        thread_suspension_live_ins(main).unwrap();
        let release = &main.blocks[2].instructions[0];
        let l::Operand::Value(version) = release.operands[0] else {
            panic!("release value")
        };
        assert_ne!(version, origin);
        assert_eq!(main.liveness.value_origins[version.0 as usize], origin);
    }
}
