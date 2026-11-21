//! Placement of collection polls (`compiler.md` §342 rule 11).
//!
//! The pass runs once per module, after local storage classification and
//! before the verifier. It reads the allocation facts of
//! [`AllocationFacts::of_module`] and inserts
//! [`l::InstructionKind::CollectionPoll`] at two kinds of site:
//!
//! - **Backedge (rule 11a).** A depth-first search from the entry block
//!   finds each retreating edge. The natural loop of the edge is its header
//!   and each block that reaches the source of the edge without passing the
//!   header. If a block of the natural loop can allocate, the source block
//!   gets one poll before its terminator.
//! - **Entry (rule 11b).** A function that can allocate gets one poll in its
//!   entry block if it is in a call-graph cycle, has its address taken, is
//!   async, or is a generator. The poll is after the leading stores of
//!   parameter values to their storage. The native tiers store each
//!   parameter value to its address-taken storage in the prologue, before
//!   the jump to the entry block (`lower/func.rs`, the parameter loop before
//!   the entry jump; `cemit/body.rs`, the parameter copies before the first
//!   block), so the poll is after those stores too.
//!
//! A poll cannot allocate, so the allocation facts of a module are the same
//! before and after the pass.
//!
//! # Bound (rule 11d)
//!
//! The scope of the bound is LIR control flow (`compiler.md` §345 rule 3).
//! Each path through LIR control flow reaches a poll, or the outermost exit,
//! after a bounded count of LIR instructions that can allocate:
//!
//! 1. **One activation, no call.** Each cycle of a control-flow graph has a
//!    retreating edge of any depth-first search from the entry, and the
//!    natural loop of that edge, as this pass computes it, contains each
//!    block of the cycle. So a cycle with a block that can allocate has a
//!    poll in the source block of its retreating edge, and that block is on
//!    the cycle. A path with no poll is then a path with no repeated
//!    allocating block. Its count of instructions that can allocate is at
//!    most the size of the function.
//! 2. **Calls.** The activations that run with no poll form a call tree.
//!    A call to a function that cannot allocate adds no allocation. A chain
//!    of that tree has no allocating function twice: a repeated function is
//!    a call-graph cycle, and each function of a cycle that can allocate has
//!    an entry poll. An indirect call reaches only functions whose address is
//!    taken, and each of those that can allocate has an entry poll. So the
//!    depth of the tree is at most the count of functions, and by point 1
//!    each activation makes at most its size of calls. The count of
//!    instructions that can allocate with no poll has a bound that only the
//!    module sets.
//! 3. **Runtime operations.** The bound counts instructions, not
//!    allocations. One runtime operation can allocate in proportion to its
//!    input, and no poll is in a runtime loop: a poll there needs a root for
//!    each input, partial result, and temporary of the operation. The flag
//!    that the operation sets is served at the first poll after it returns,
//!    or at the outermost exit (`compiler.md` §345 rule 3). A runtime
//!    operation with a function-value callback calls a function whose
//!    address is taken. That function has an entry poll if it can allocate.
//!
//! # Resume (rule 11c)
//!
//! A resume of an async function or a generator enters the function at the
//! successor block of a suspension. The suspension edge is a control-flow
//! edge of the LIR, so a loop that contains an `await` or a `yield` is a
//! cycle of the graph, and point 1 above gives it a poll. A first start
//! enters at the entry block, which has an entry poll. Between two
//! suspensions of one activation, the resumed code is bounded by point 1 and
//! point 2.
//!
//! The scheduler drain (`Context::async_step`) resumes each ready task in one
//! script entry. That loop is in the runtime, not in the LIR, and it has no
//! poll. Each resume of a task runs bounded code by the argument above, and
//! each task resumes once for each suspension it reaches. A task that
//! suspends again in a loop passes the backedge poll of that loop. So one
//! drain allocates at most the bound above for each task that is ready in
//! that drain, and rule 8 serves the pending flag at the exit of the drain.
//! The count of ready tasks is bounded by the live tasks, and each task is a
//! live allocation that passed its entry poll when it started.

use super::allocation::AllocationFacts;
use super::*;

/// Inserts the collection polls of each function of the module
/// (`compiler.md` §342 rule 11).
pub(super) fn place_collection_polls(module: &mut l::Module) {
    let facts = AllocationFacts::of_module(module);
    let entry = entry_polls(module, &facts);
    for (function, entry_poll) in module.functions.iter_mut().zip(entry) {
        if !facts.function_can_allocate(function.id) {
            continue;
        }
        let sites = backedge_poll_sites(function, &facts);
        for (source, index) in sites {
            let block = &mut function.blocks[source];
            let pos = match block.instructions.get(index) {
                Some(instruction) => instruction.pos.clone(),
                None => terminator_pos(block, &function.pos),
            };
            block.instructions.insert(index, poll(pos));
        }
        if entry_poll {
            insert_entry_poll(function);
        }
    }
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

/// The position of the terminator, for the poll before it.
fn terminator_pos(block: &l::BasicBlock, fallback: &Pos) -> Pos {
    match &block.terminator {
        l::Terminator::Return { pos, .. }
        | l::Terminator::Unreachable { pos }
        | l::Terminator::Suspend { pos, .. } => pos.clone(),
        l::Terminator::Trap(trap) => trap.pos.clone(),
        l::Terminator::Branch(_)
        | l::Terminator::ConditionalBranch { .. }
        | l::Terminator::Switch { .. } => block
            .instructions
            .last()
            .map_or_else(|| fallback.clone(), |instruction| instruction.pos.clone()),
    }
}

/// Inserts the entry poll after the leading stores of parameter values.
fn insert_entry_poll(function: &mut l::Function) {
    let entry = function.entry.0 as usize;
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| parameter.value)
        .collect::<BTreeSet<_>>();
    let pos = function.pos.clone();
    let Some(block) = function.blocks.get_mut(entry) else {
        return;
    };
    let index = block
        .instructions
        .iter()
        .take_while(|instruction| {
            matches!(instruction.kind, l::InstructionKind::StoreLocal(_))
                && matches!(instruction.operands.as_slice(),
                    [l::Operand::Value(value)] if parameters.contains(value))
        })
        .count();
    block.instructions.insert(index, poll(pos));
}

/// The poll site of each source block of a retreating edge whose natural
/// loop has a block that can allocate (`compiler.md` §342 rule 11a). The
/// site is the index of the instruction that the poll goes before: the end
/// of the block for a terminator edge, or the raise site of a handler edge.
fn backedge_poll_sites(
    function: &l::Function,
    facts: &AllocationFacts,
) -> std::collections::BTreeMap<usize, usize> {
    let count = function.blocks.len();
    let entry = function.entry.0 as usize;
    let mut sites = std::collections::BTreeMap::new();
    if entry >= count {
        return sites;
    }
    let successors = function
        .blocks
        .iter()
        .map(|block| {
            block
                .successors()
                .into_iter()
                .map(|id| id.0 as usize)
                .filter(|index| *index < count)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut predecessors = vec![Vec::new(); count];
    for (index, row) in successors.iter().enumerate() {
        for &successor in row {
            predecessors[successor].push(index);
        }
    }
    // Depth-first search: 0 is unvisited, 1 is on the stack, 2 is done.
    let mut state = vec![0u8; count];
    let mut retreating = Vec::new();
    let mut stack = vec![(entry, 0usize)];
    state[entry] = 1;
    while let Some((block, next)) = stack.last_mut() {
        let block = *block;
        if let Some(&successor) = successors[block].get(*next) {
            *next += 1;
            match state[successor] {
                0 => {
                    state[successor] = 1;
                    stack.push((successor, 0));
                }
                1 => retreating.push((block, successor)),
                _ => {}
            }
        } else {
            state[block] = 2;
            stack.pop();
        }
    }
    for (source, header) in retreating {
        let mut body = vec![false; count];
        body[header] = true;
        let mut work = vec![source];
        while let Some(block) = work.pop() {
            if std::mem::replace(&mut body[block], true) {
                continue;
            }
            work.extend(predecessors[block].iter().copied());
        }
        let allocates =
            function.blocks.iter().enumerate().any(|(index, block)| {
                body[index] && facts.block_can_allocate(function.id, block.id)
            });
        if allocates {
            let block = &function.blocks[source];
            let index = block
                .instructions
                .iter()
                .position(|instruction| {
                    instruction
                        .handler()
                        .is_some_and(|id| id.0 as usize == header)
                })
                .unwrap_or(block.instructions.len());
            let site = sites.entry(source).or_insert(index);
            *site = (*site).min(index);
        }
    }
    sites
}

/// The direct callees of an instruction, for the call graph.
fn direct_callees(
    methods: &HashMap<l::MethodId, l::FunctionId>,
    instruction: &l::Instruction,
) -> Option<l::FunctionId> {
    let target = match &instruction.kind {
        l::InstructionKind::Call(target) | l::InstructionKind::AsyncHandleCreate(target) => target,
        _ => return None,
    };
    match &target.kind {
        l::CallTargetKind::Function(id)
        | l::CallTargetKind::LambdaSelf(id)
        | l::CallTargetKind::StaticClosure(id) => Some(*id),
        l::CallTargetKind::ClosureDefault { helper, .. } => Some(*helper),
        l::CallTargetKind::Method(id) => methods.get(id).copied(),
        _ => None,
    }
}

/// The entry-poll decision of each function (`compiler.md` §342 rule 11b).
fn entry_polls(module: &l::Module, facts: &AllocationFacts) -> Vec<bool> {
    let count = module.functions.len();
    let methods = module
        .classes
        .iter()
        .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
        .map(|method| (method.id, method.function))
        .collect::<HashMap<_, _>>();
    let mut needed = module
        .functions
        .iter()
        .map(|function| function.is_async || function.is_generator)
        .collect::<Vec<_>>();
    let mut callees = vec![Vec::new(); count];
    for (index, function) in module.functions.iter().enumerate() {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let l::InstructionKind::FunctionRef(id) | l::InstructionKind::MakeClosure(id) =
                &instruction.kind
            {
                if let Some(slot) = needed.get_mut(id.0 as usize) {
                    *slot = true;
                }
            }
            if let Some(callee) = direct_callees(&methods, instruction) {
                if (callee.0 as usize) < count {
                    callees[index].push(callee.0 as usize);
                }
            }
        }
    }
    // A function is in a call-graph cycle if it reaches itself.
    for start in 0..count {
        if needed[start] {
            continue;
        }
        let mut seen = vec![false; count];
        let mut work = callees[start].clone();
        while let Some(next) = work.pop() {
            if next == start {
                needed[start] = true;
                break;
            }
            if !std::mem::replace(&mut seen[next], true) {
                work.extend(callees[next].iter().copied());
            }
        }
    }
    module
        .functions
        .iter()
        .zip(needed)
        .map(|(function, needed)| needed && facts.function_can_allocate(function.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    fn lower(source: &str) -> l::Module {
        let hir = check_program(&[SourceFile::new("poll.ts", source)]).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    fn function<'m>(module: &'m l::Module, name: &str) -> &'m l::Function {
        module
            .functions
            .iter()
            .find(|function| function.source_name == name)
            .unwrap_or_else(|| panic!("function {name} is missing"))
    }

    /// The count of entry polls and of polls before a terminator.
    fn polls(module: &l::Module, name: &str) -> (usize, usize) {
        let function = function(module, name);
        let mut entry = 0;
        let mut backedge = 0;
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if instruction.kind != l::InstructionKind::CollectionPoll {
                    continue;
                }
                if block.id == function.entry {
                    entry += 1;
                } else {
                    assert_eq!(index + 1, block.instructions.len(), "{name}: poll position");
                    backedge += 1;
                }
            }
        }
        (entry, backedge)
    }

    const SAMPLE: &str = r#"
class P { x: i32 = 0; }
function make(): P { return new P(); }
function allocating_loop(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { const p: P = new P(); s += p.x; }
  return s;
}
function plain_loop(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { s += i; }
  return s;
}
function callee_loop(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) { s += make().x; }
  return s;
}
function inner_allocates(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) {
    for (let j: i32 = 0; j < n; j += 1) { s += make().x; }
  }
  return s;
}
function outer_allocates(n: i32): i32 {
  let s: i32 = 0;
  for (let i: i32 = 0; i < n; i += 1) {
    s += make().x;
    for (let j: i32 = 0; j < n; j += 1) { s += j; }
  }
  return s;
}
function build(depth: i32): P {
  if (depth === 0) { return new P(); }
  build(depth - 1);
  return new P();
}
function count(depth: i32): i32 {
  if (depth === 0) { return 0; }
  return 1 + count(depth - 1);
}
function callback(x: i32): P { const p: P = new P(); p.x = x; return p; }
function straight(): P { const a: P = new P(); return make(); }
async function task(): Promise<i32> { return make().x; }
function* items(): Generator<i32> { yield make().x; }
function map_plain(a: i32[]): i32[] { return a.map((x: i32): i32 => x * 2); }
function filter_plain(a: i32[]): i32[] { return a.filter((x: i32): boolean => x > 0); }
function map_allocating(a: i32[]): P[] {
  return a.map((x: i32): P => { const p: P = new P(); p.x = x; return p; });
}
export async function main(): Promise<void> {
  const f: (x: i32) => P = callback;
  let total: i32 = allocating_loop(2) + plain_loop(2) + callee_loop(2) + f(1).x;
  total += inner_allocates(2) + outer_allocates(2) + build(2).x + count(2) + straight().x;
  for (const item of items()) { total += item; }
  const values: i32[] = [1, 2];
  total += map_plain(values).length + filter_plain(values).length;
  total += map_allocating(values).length;
  total += await task();
  print(`${total}`);
}
"#;

    #[test]
    fn backedge_polls_follow_the_allocation_of_the_natural_loop() {
        let module = lower(SAMPLE);
        assert_eq!(polls(&module, "allocating_loop"), (0, 1));
        assert_eq!(polls(&module, "plain_loop"), (0, 0));
        assert_eq!(polls(&module, "callee_loop"), (0, 1));
        // The natural loop of the outer backedge contains the inner loop.
        assert_eq!(polls(&module, "inner_allocates"), (0, 2));
        // The inner loop cannot allocate, so its backedge has no poll.
        assert_eq!(polls(&module, "outer_allocates"), (0, 1));
        // A static-callback push stores into reserved capacity, so only
        // the callback decides (compiler.md §342 rule 10b).
        assert_eq!(polls(&module, "map_plain"), (0, 0));
        assert_eq!(polls(&module, "filter_plain"), (0, 0));
        assert_eq!(polls(&module, "map_allocating"), (0, 1));
    }

    #[test]
    fn entry_polls_follow_cycles_address_taken_and_coroutines() {
        let module = lower(SAMPLE);
        assert_eq!(polls(&module, "build"), (1, 0));
        assert_eq!(polls(&module, "count"), (0, 0));
        assert_eq!(polls(&module, "callback"), (1, 0));
        assert_eq!(polls(&module, "straight"), (0, 0));
        assert_eq!(polls(&module, "make"), (0, 0));
        assert_eq!(polls(&module, "task").0, 1);
        assert_eq!(polls(&module, "items").0, 1);
    }

    #[test]
    fn the_pass_keeps_the_allocation_facts() {
        let module = lower(SAMPLE);
        let mut stripped = module.clone();
        for function in &mut stripped.functions {
            for block in &mut function.blocks {
                block
                    .instructions
                    .retain(|instruction| instruction.kind != l::InstructionKind::CollectionPoll);
            }
        }
        let before = AllocationFacts::of_module(&stripped);
        let after = AllocationFacts::of_module(&module);
        for (stripped, function) in stripped.functions.iter().zip(&module.functions) {
            assert_eq!(
                before.function_can_allocate(stripped.id),
                after.function_can_allocate(function.id),
                "{}",
                function.source_name
            );
            for block in &function.blocks {
                assert_eq!(
                    before.block_can_allocate(stripped.id, block.id),
                    after.block_can_allocate(function.id, block.id),
                    "{} block {}",
                    function.source_name,
                    block.id.0
                );
            }
        }
    }
}
