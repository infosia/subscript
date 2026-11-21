//! The poll property of a finished module (`compiler.md` §342 rule 11).
//!
//! This check derives the property from the finished functions and does not
//! use the decisions of the placement pass (core principle 9). It uses the
//! strongly connected components of each graph, and the pass uses a
//! depth-first search with natural loops and a reachability walk.
//!
//! 1. In the control-flow graph of a function, an edge that leaves a block
//!    after a poll of that block passes the poll, so the check removes it.
//!    No cycle of the remaining graph contains a block that can allocate.
//! 2. A function that can allocate and has its address taken, is async, is
//!    a generator, or is in a call-graph cycle has a poll in its entry
//!    block before each instruction of that block that can allocate.
//! 3. A function that cannot allocate has no poll.

use super::allocation::AllocationFacts;
use super::*;

/// Records each violation of the poll property (`compiler.md` §342 rule 11).
pub(super) fn verify(module: &l::Module, errors: &mut Vec<VerifyError>) {
    let facts = AllocationFacts::of_module(module);
    let required = entry_poll_required(module);
    for (function, required) in module.functions.iter().zip(required) {
        let polls = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| instruction.kind == l::InstructionKind::CollectionPoll)
            .count();
        if !facts.function_can_allocate(function.id) {
            if polls != 0 {
                errors.push(VerifyError {
                    message: format!(
                        "function {} cannot allocate and has {polls} collection poll(s)",
                        function.id.0
                    ),
                });
            }
            continue;
        }
        verify_cycles(function, &facts, errors);
        if required {
            verify_entry_poll(function, &facts, errors);
        }
    }
}

fn first_poll(block: &l::BasicBlock) -> Option<usize> {
    block
        .instructions
        .iter()
        .position(|instruction| instruction.kind == l::InstructionKind::CollectionPoll)
}

/// Property 1: no cycle without a poll contains a block that can allocate.
fn verify_cycles(function: &l::Function, facts: &AllocationFacts, errors: &mut Vec<VerifyError>) {
    let count = function.blocks.len();
    let edges = function
        .blocks
        .iter()
        .map(|block| {
            let limit = first_poll(block);
            let mut targets = block
                .instructions
                .iter()
                .enumerate()
                .filter(|(index, _)| limit.is_none_or(|limit| *index < limit))
                .filter_map(|(_, instruction)| instruction.handler())
                .collect::<Vec<_>>();
            if limit.is_none() {
                targets.extend(block.terminator.successors());
            }
            targets
                .into_iter()
                .map(|id| id.0 as usize)
                .filter(|index| *index < count)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let components = strongly_connected_components(&edges);
    let mut size = vec![0usize; count];
    for component in &components {
        size[*component] += 1;
    }
    for (index, block) in function.blocks.iter().enumerate() {
        let cyclic = size[components[index]] > 1 || edges[index].contains(&index);
        if cyclic && facts.block_can_allocate(function.id, block.id) {
            errors.push(VerifyError {
                message: format!(
                    "function {} block {} can allocate in a cycle with no collection poll",
                    function.id.0, block.id.0
                ),
            });
        }
    }
}

/// Property 2: the entry poll is before each allocation of the entry block.
fn verify_entry_poll(
    function: &l::Function,
    facts: &AllocationFacts,
    errors: &mut Vec<VerifyError>,
) {
    let entry = function.entry;
    let Some(block) = function.blocks.get(entry.0 as usize) else {
        return;
    };
    let ordered = first_poll(block).is_some_and(|poll| {
        (0..poll).all(|index| !facts.instruction_can_allocate(function.id, entry, index))
    });
    if !ordered {
        errors.push(VerifyError {
            message: format!(
                "function {} needs an entry collection poll before its first allocation",
                function.id.0
            ),
        });
    }
}

/// The functions that need an entry poll if they can allocate: address
/// taken, async, generator, or in a call-graph cycle.
fn entry_poll_required(module: &l::Module) -> Vec<bool> {
    let count = module.functions.len();
    let methods = module
        .classes
        .iter()
        .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
        .map(|method| (method.id, method.function.0 as usize))
        .collect::<HashMap<_, _>>();
    let mut required = module
        .functions
        .iter()
        .map(|function| function.is_async || function.is_generator)
        .collect::<Vec<_>>();
    let mut edges = vec![Vec::new(); count];
    for (index, function) in module.functions.iter().enumerate() {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            let target = match &instruction.kind {
                l::InstructionKind::FunctionRef(id) | l::InstructionKind::MakeClosure(id) => {
                    if let Some(slot) = required.get_mut(id.0 as usize) {
                        *slot = true;
                    }
                    continue;
                }
                l::InstructionKind::Call(target)
                | l::InstructionKind::AsyncHandleCreate(target) => target,
                _ => continue,
            };
            let callee = match &target.kind {
                l::CallTargetKind::Function(id)
                | l::CallTargetKind::LambdaSelf(id)
                | l::CallTargetKind::StaticClosure(id)
                | l::CallTargetKind::ClosureDefault { helper: id, .. } => Some(id.0 as usize),
                l::CallTargetKind::Method(id) => methods.get(id).copied(),
                _ => None,
            };
            if let Some(callee) = callee.filter(|callee| *callee < count) {
                edges[index].push(callee);
            }
        }
    }
    let components = strongly_connected_components(&edges);
    let mut size = vec![0usize; count];
    for component in &components {
        size[*component] += 1;
    }
    for index in 0..count {
        if size[components[index]] > 1 || edges[index].contains(&index) {
            required[index] = true;
        }
    }
    required
}

/// Tarjan's algorithm without recursion. Returns the component index of
/// each node.
fn strongly_connected_components(edges: &[Vec<usize>]) -> Vec<usize> {
    const NONE: usize = usize::MAX;
    let count = edges.len();
    let mut order = vec![NONE; count];
    let mut low = vec![0usize; count];
    let mut on_stack = vec![false; count];
    let mut component = vec![NONE; count];
    let mut stack = Vec::new();
    let mut next_order = 0usize;
    let mut next_component = 0usize;
    for root in 0..count {
        if order[root] != NONE {
            continue;
        }
        let mut calls = vec![(root, 0usize)];
        order[root] = next_order;
        low[root] = next_order;
        next_order += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (node, ref mut next)) = calls.last_mut() {
            if let Some(&target) = edges[node].get(*next) {
                *next += 1;
                if order[target] == NONE {
                    order[target] = next_order;
                    low[target] = next_order;
                    next_order += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    calls.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(order[target]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == order[node] {
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component[member] = next_component;
                    if member == node {
                        break;
                    }
                }
                next_component += 1;
            }
        }
    }
    component
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    const SOURCE: &str = r#"
class P { x: i32 = 0; }
function fill(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const p: P = new P(); s += p.x; }
  return s;
}
function build(depth: i32): P {
  const p: P = new P();
  if (depth > 0) { build(depth - 1); }
  return p;
}
function sum(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { s += i; }
  return s;
}
export function main(): void { print(`${fill(2) + build(2).x + sum(2)}`); }
"#;

    fn lower() -> l::Module {
        let hir = check_program(&[SourceFile::new("verify-poll.ts", SOURCE)]).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    fn function_mut<'m>(module: &'m mut l::Module, name: &str) -> &'m mut l::Function {
        module
            .functions
            .iter_mut()
            .find(|function| function.source_name == name)
            .unwrap_or_else(|| panic!("function {name} is missing"))
    }

    fn remove_polls(function: &mut l::Function) -> usize {
        let mut removed = 0;
        for block in &mut function.blocks {
            let before = block.instructions.len();
            block
                .instructions
                .retain(|instruction| instruction.kind != l::InstructionKind::CollectionPoll);
            removed += before - block.instructions.len();
        }
        removed
    }

    fn messages(module: &l::Module) -> Vec<String> {
        let mut errors = Vec::new();
        verify(module, &mut errors);
        errors.into_iter().map(|error| error.message).collect()
    }

    fn poll(pos: Pos) -> l::Instruction {
        l::Instruction {
            result: None,
            kind: l::InstructionKind::CollectionPoll,
            count_action: None,
            operands: Vec::new(),
            invalidates: Vec::new(),
            traps: Vec::new(),
            pos,
        }
    }

    #[test]
    fn the_placed_module_satisfies_the_property() {
        assert_eq!(messages(&lower()), Vec::<String>::new());
    }

    #[test]
    fn an_allocating_cycle_with_no_poll_is_an_error() {
        let mut module = lower();
        assert_eq!(remove_polls(function_mut(&mut module, "fill")), 1);
        let messages = messages(&module);
        assert!(
            messages
                .iter()
                .any(|message| message.contains("can allocate in a cycle")),
            "{messages:?}"
        );
    }

    #[test]
    fn a_missing_or_late_entry_poll_is_an_error() {
        let mut module = lower();
        assert_eq!(remove_polls(function_mut(&mut module, "build")), 1);
        let messages_missing = messages(&module);
        assert!(
            messages_missing
                .iter()
                .any(|message| message.contains("needs an entry collection poll")),
            "{messages_missing:?}"
        );
        // A poll after the first allocation of the entry block.
        let build = function_mut(&mut module, "build");
        let pos = build.pos.clone();
        let entry = build.entry.0 as usize;
        let block = &mut build.blocks[entry];
        let allocation = block
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.kind, l::InstructionKind::AllocateClass(_))
            })
            .expect("build allocates in its entry block");
        block.instructions.insert(allocation + 1, poll(pos));
        let messages_late = messages(&module);
        assert!(
            messages_late
                .iter()
                .any(|message| message.contains("needs an entry collection poll")),
            "{messages_late:?}"
        );
    }

    #[test]
    fn a_poll_in_a_function_that_cannot_allocate_is_an_error() {
        let mut module = lower();
        let sum = function_mut(&mut module, "sum");
        assert_eq!(remove_polls(sum), 0);
        let pos = sum.pos.clone();
        let entry = sum.entry.0 as usize;
        sum.blocks[entry].instructions.insert(0, poll(pos));
        let messages = messages(&module);
        assert!(
            messages
                .iter()
                .any(|message| message.contains("cannot allocate and has 1 collection poll")),
            "{messages:?}"
        );
    }

    #[test]
    fn components_find_cycles_and_self_edges() {
        let components = strongly_connected_components(&[vec![1], vec![0], vec![2], vec![]]);
        assert_eq!(components[0], components[1]);
        assert_ne!(components[0], components[2]);
        assert_ne!(components[2], components[3]);
    }
}
