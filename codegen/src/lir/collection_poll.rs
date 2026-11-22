//! Placement of collection polls (`compiler.md` §342 rule 11, §351).
//!
//! The pass runs once per module, after local storage classification and
//! before the verifier. It reads the allocation facts of
//! [`AllocationFacts::of_module`] and inserts
//! [`l::InstructionKind::CollectionPoll`] at four kinds of site:
//!
//! - **Backedge (rule 11a).** A depth-first search from the entry block
//!   finds each retreating edge. The natural loop of the edge is its header
//!   and each block that reaches the source of the edge without passing the
//!   header. If a block of the natural loop can allocate, the source block
//!   gets one poll before its terminator.
//! - **Entry (rule 11b, §351 rule 5).** A function that can allocate gets
//!   one poll in its entry block if it is in a cycle of the call graph with
//!   indirect edges, has its address taken, is async, or is a generator.
//!   The poll is after the leading stores of parameter values to their
//!   storage. The native tiers store each parameter value to its
//!   address-taken storage in the prologue, before the jump to the entry
//!   block (`lower/func.rs`, the parameter loop before the entry jump;
//!   `cemit/body.rs`, the parameter copies before the first block), so the
//!   poll is after those stores too.
//! - **After a call into the cycle (rule 11e, §351 rule 1).** In a
//!   function of a cycle, a call whose possible callee is in that cycle
//!   gets one poll directly after it if its continuation can reach an
//!   instruction that can allocate, or a suspension, before a poll.
//! - **Handler (rule 11f, §351 rule 2).** In a function of a cycle, a
//!   handler block whose path can allocate before a poll gets one poll
//!   after its leading instructions that take the exception.
//!
//! The call graph with indirect edges has an edge for each direct call, and
//! an edge from each function with an indirect call, or with a runtime
//! operation that has an operand of a function type, to each function whose
//! address is taken. The language has no other dispatch: an interface is a
//! constraint, and each instance calls the member of its type argument
//! directly (`compiler.md` §208).
//!
//! The scan of a continuation follows the handler edges of the instructions
//! it passes and ends at a return, a trap, or an unreachable point. A poll
//! and a direct call to a function with an entry poll end a path of the
//! scan (§351 rule 3). A function outside each cycle gets no poll of the
//! last two kinds (§351 rule 4).
//!
//! A poll cannot allocate, so the allocation facts of a module are the same
//! before and after the pass. The pass computes them again after the first
//! insertions, because they are indexed by instruction position.
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
//! 2. **Calls and returns (§351 rule 6).** A path with no poll enters
//!    activations by calls and leaves them by normal or exceptional returns.
//!    Each call goes to a function of the cycle of the caller, or to a
//!    function that does not reach the caller. So the functions of one stack, in stack order, pass through the
//!    cycles of the call graph with indirect edges in one order, and a
//!    function outside each cycle has one frame on the stack at most.
//!    - *Descent.* A chain of calls with no poll has no allocating function
//!      twice: a repeated function is in a cycle, and each function of a
//!      cycle that can allocate has an entry poll. An indirect call and a
//!      runtime operation with a function-value operand reach only
//!      functions whose address is taken, and each of those that can
//!      allocate has an entry poll.
//!    - *Normal return.* A return from a callee in the cycle of the caller
//!      reaches the poll directly after the call, or a continuation that
//!      reaches no instruction that can allocate and no suspension before a
//!      poll or its own return (§351 rules 1 and 3). Such a continuation
//!      makes no call that can allocate. So a chain of returns inside one
//!      cycle allocates nothing with no poll, at any depth. A return to a
//!      caller outside the cycle of the callee crosses from one cycle, or
//!      one function outside each cycle, to another. A chain of returns
//!      makes at most one such crossing for each function of the module.
//!    - *Exceptional return.* An exception that leaves a frame enters a
//!      handler of the frame, or leaves the frame with no instruction. In a
//!      function of a cycle, a handler whose path can allocate before a
//!      poll has a poll after the leading instructions that take the
//!      exception (§351 rule 2). So each frame of a cycle on the path of
//!      an exception adds at most those leading instructions before a poll.
//!
//!    Between two polls a path thus runs, for each crossing, at most the
//!    instructions of one activation by point 1, and at most the leading
//!    instructions of the handlers of one function. The count of
//!    instructions that can allocate with no poll has a bound that only the
//!    module sets, for direct calls, indirect calls, callback operations,
//!    normal returns, and exceptional returns.
//!
//!    Not in the bound (§351 rule 7): a recursion that passes through the
//!    host, which the call graph of the module does not show; a value that
//!    passes up through nested generators, one suspension for each
//!    generator with no poll between them; and the allocations of one
//!    runtime operation (point 3).
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
//! The scheduler drain (`Context::async_step`) resumes each ready job in one
//! script entry. Each resume runs the bounded code of one activation by the
//! points above. The drain serves the flag between two jobs (`compiler.md`
//! §351 rule 12, §342 rule 11g). So a chain of resumes in one drain, such as
//! the return chain of an async recursion, reaches a poll after each job.
//! The interpreter's drain has the same poll.

use super::allocation::AllocationFacts;
use super::*;

/// Inserts the collection polls of each function of the module
/// (`compiler.md` §342 rule 11, §351 rules 1 to 5).
pub(super) fn place_collection_polls(module: &mut l::Module) {
    let facts = AllocationFacts::of_module(module);
    let graph = CallGraph::of_module(module);
    let entry = entry_polls(module, &facts, &graph);
    for (function, entry_poll) in module.functions.iter_mut().zip(entry.iter().copied()) {
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
    // The facts are indexed by instruction position, so they are computed
    // again after the first insertions. A poll cannot allocate, so each
    // fact is the same.
    let facts = AllocationFacts::of_module(module);
    let sites = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| return_path_sites(module, function, index, &facts, &graph, &entry))
        .collect::<Vec<_>>();
    for (function, sites) in module.functions.iter_mut().zip(sites) {
        for (block, index) in sites.into_iter().rev() {
            let Some(body) = function.blocks.get_mut(block) else {
                continue;
            };
            let pos = match body.instructions.get(index) {
                Some(instruction) => instruction.pos.clone(),
                None => terminator_pos(body, &function.pos),
            };
            body.instructions.insert(index, poll(pos));
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
    match &instruction.kind {
        l::InstructionKind::Call(target) | l::InstructionKind::AsyncHandleCreate(target) => {
            target_callee(methods, target)
        }
        _ => None,
    }
}

/// The async call of a suspension: an await of a direct async call
/// creates its callee and runs it (`compiler.md` §351 "Terms").
fn suspension_call(terminator: &l::Terminator) -> Option<&l::CallTarget> {
    match terminator {
        l::Terminator::Suspend {
            kind: l::SuspendKind::AsyncCall { target, .. },
            ..
        } => Some(target),
        _ => None,
    }
}

/// The script function that a call target names directly.
fn target_callee(
    methods: &HashMap<l::MethodId, l::FunctionId>,
    target: &l::CallTarget,
) -> Option<l::FunctionId> {
    match &target.kind {
        l::CallTargetKind::Function(id)
        | l::CallTargetKind::LambdaSelf(id)
        | l::CallTargetKind::StaticClosure(id) => Some(*id),
        l::CallTargetKind::ClosureDefault { helper, .. } => Some(*helper),
        l::CallTargetKind::Method(id) => methods.get(id).copied(),
        _ => None,
    }
}

/// The call graph with indirect edges (`compiler.md` §351 "Terms").
///
/// A function has an edge to each direct callee. A function with an
/// indirect call, or with a runtime operation that takes a function value,
/// has an edge to each function whose address is taken. The pass decides
/// cycles by reachability; the verifier builds its own graph and uses
/// strongly connected components.
struct CallGraph {
    /// The functions whose address is taken.
    taken: Vec<usize>,
    /// The functions that each function reaches by one edge or more.
    reaches: Vec<Vec<bool>>,
    /// The method bodies, for the direct callee of a method call.
    methods: HashMap<l::MethodId, l::FunctionId>,
}

impl CallGraph {
    fn of_module(module: &l::Module) -> Self {
        let count = module.functions.len();
        let methods = module
            .classes
            .iter()
            .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
            .map(|method| (method.id, method.function))
            .collect::<HashMap<_, _>>();
        let mut callees = vec![Vec::new(); count];
        let mut taken = BTreeSet::new();
        let mut indirect = vec![false; count];
        for (index, function) in module.functions.iter().enumerate() {
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                if let l::InstructionKind::FunctionRef(id) | l::InstructionKind::MakeClosure(id) =
                    &instruction.kind
                {
                    if (id.0 as usize) < count {
                        taken.insert(id.0 as usize);
                    }
                }
                if let Some(callee) = direct_callees(&methods, instruction) {
                    if (callee.0 as usize) < count {
                        callees[index].push(callee.0 as usize);
                    }
                }
                if calls_a_function_value(function, instruction) {
                    indirect[index] = true;
                }
            }
            // An await of a direct async call is an edge to its callee.
            for target in function
                .blocks
                .iter()
                .filter_map(|block| suspension_call(&block.terminator))
            {
                match target_callee(&methods, target) {
                    Some(callee) if (callee.0 as usize) < count => {
                        callees[index].push(callee.0 as usize);
                    }
                    Some(_) => {}
                    None => {
                        if matches!(target.kind, l::CallTargetKind::Indirect) {
                            indirect[index] = true;
                        }
                    }
                }
            }
        }
        let taken = taken.into_iter().collect::<Vec<_>>();
        for (index, row) in callees.iter_mut().enumerate() {
            if indirect[index] {
                row.extend(taken.iter().copied());
            }
        }
        let reaches = (0..count)
            .map(|start| {
                let mut seen = vec![false; count];
                let mut work = callees[start].clone();
                while let Some(next) = work.pop() {
                    if !std::mem::replace(&mut seen[next], true) {
                        work.extend(callees[next].iter().copied());
                    }
                }
                seen
            })
            .collect();
        Self {
            taken,
            reaches,
            methods,
        }
    }

    /// True if the function reaches itself: it is in a cycle.
    fn in_cycle(&self, function: usize) -> bool {
        self.reaches
            .get(function)
            .and_then(|row| row.get(function))
            .copied()
            .unwrap_or(false)
    }

    /// True if `callee` is in the cycle of `caller`. The caller has an edge
    /// to the callee, so the two are in one cycle if the callee reaches
    /// the caller.
    fn same_cycle(&self, caller: usize, callee: usize) -> bool {
        self.reaches
            .get(callee)
            .and_then(|row| row.get(caller))
            .copied()
            .unwrap_or(false)
    }

    /// True if a possible callee of the instruction is in the cycle of
    /// `caller` (`compiler.md` §351 rule 1).
    fn calls_into_cycle(
        &self,
        function: &l::Function,
        caller: usize,
        instruction: &l::Instruction,
    ) -> bool {
        if let Some(callee) = direct_callees(&self.methods, instruction) {
            if self.same_cycle(caller, callee.0 as usize) {
                return true;
            }
        }
        calls_a_function_value(function, instruction)
            && self
                .taken
                .iter()
                .any(|&target| self.same_cycle(caller, target))
    }
}

/// True if the instruction is an indirect call, or a runtime operation
/// with an operand of a function type (`compiler.md` §351 "Terms").
fn calls_a_function_value(function: &l::Function, instruction: &l::Instruction) -> bool {
    let target = match &instruction.kind {
        l::InstructionKind::Call(target) | l::InstructionKind::AsyncHandleCreate(target) => target,
        _ => return false,
    };
    match &target.kind {
        l::CallTargetKind::Indirect => true,
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_) => {
            instruction.operands.iter().any(|operand| {
                match operand {
                l::Operand::Value(value) => function
                    .values
                    .get(value.0 as usize)
                    .filter(|data| data.id == *value)
                    .or_else(|| function.values.iter().find(|data| data.id == *value))
                    .is_some_and(|data| {
                        matches!(&data.ty, l::ValueType::Data(ty) if ty.function_type().is_some())
                    }),
                l::Operand::Constant(constant) => constant.ty.function_type().is_some(),
            }
            })
        }
        _ => false,
    }
}

/// The entry-poll decision of each function (`compiler.md` §342 rule 11b,
/// §351 rule 5): a function that can allocate has an entry poll if it is
/// in a cycle of the call graph with indirect edges, has its address
/// taken, is async, or is a generator.
fn entry_polls(module: &l::Module, facts: &AllocationFacts, graph: &CallGraph) -> Vec<bool> {
    let mut needed = module
        .functions
        .iter()
        .map(|function| function.is_async || function.is_generator)
        .collect::<Vec<_>>();
    for &target in &graph.taken {
        if let Some(slot) = needed.get_mut(target) {
            *slot = true;
        }
    }
    for (index, slot) in needed.iter_mut().enumerate() {
        if graph.in_cycle(index) {
            *slot = true;
        }
    }
    module
        .functions
        .iter()
        .zip(needed)
        .map(|(function, needed)| needed && facts.function_can_allocate(function.id))
        .collect()
}

/// The leading instructions of a handler block that take the exception.
/// A poll goes after them (`compiler.md` §351 rule 2).
fn takes_the_exception(instruction: &l::Instruction) -> bool {
    matches!(
        instruction.kind,
        l::InstructionKind::CatchEntry
            | l::InstructionKind::ExceptionPark
            | l::InstructionKind::ExceptionMessage
            | l::InstructionKind::ExceptionPosition
    )
}

/// The return-path poll sites of one function, sorted, as (block,
/// instruction index) pairs (`compiler.md` §351 rules 1, 2, and 4). The
/// poll goes before the instruction at the index.
fn return_path_sites(
    module: &l::Module,
    function: &l::Function,
    index: usize,
    facts: &AllocationFacts,
    graph: &CallGraph,
    entry: &[bool],
) -> Vec<(usize, usize)> {
    // Rule 4: a function outside each cycle gets no new poll.
    if !graph.in_cycle(index) || !facts.function_can_allocate(function.id) {
        return Vec::new();
    }
    let scan = ContinuationScan {
        module,
        function,
        facts,
        graph,
        entry,
    };
    let mut sites = BTreeSet::new();
    let mut handlers = BTreeSet::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (position, instruction) in block.instructions.iter().enumerate() {
            if let Some(handler) = instruction.handler() {
                handlers.insert(handler.0 as usize);
            }
            // Rule 1: a poll directly after a call into the cycle.
            if graph.calls_into_cycle(function, index, instruction)
                && scan.allocates_before_a_poll(block_index, position + 1)
            {
                sites.insert((block_index, position + 1));
            }
        }
    }
    // Rule 2: a poll in a handler, after the instructions that take the
    // exception.
    for handler in handlers {
        let Some(block) = function.blocks.get(handler) else {
            continue;
        };
        let start = block
            .instructions
            .iter()
            .take_while(|instruction| takes_the_exception(instruction))
            .count();
        if scan.allocates_before_a_poll(handler, start) {
            sites.insert((handler, start));
        }
    }
    sites.into_iter().collect()
}

/// The forward scan of a continuation (`compiler.md` §351 rules 1 to 3).
struct ContinuationScan<'a> {
    module: &'a l::Module,
    function: &'a l::Function,
    facts: &'a AllocationFacts,
    graph: &'a CallGraph,
    entry: &'a [bool],
}

impl ContinuationScan<'_> {
    /// True if a direct call reaches the entry poll of its callee before
    /// the callee can allocate. Rule 3 counts it as a poll. A call to a
    /// generator or an async function creates its frame and does not run
    /// the entry block at the call, so it is not a poll.
    fn is_entry_polled_call(&self, instruction: &l::Instruction) -> bool {
        let l::InstructionKind::Call(_) = &instruction.kind else {
            return false;
        };
        direct_callees(&self.graph.methods, instruction).is_some_and(|callee| {
            self.module
                .functions
                .get(callee.0 as usize)
                .is_some_and(|function| {
                    function.id == callee && !function.is_generator && !function.is_async
                })
                && self.entry.get(callee.0 as usize).copied().unwrap_or(false)
        })
    }

    /// True if a path from instruction `start` of `block` reaches an
    /// instruction that can allocate, or a suspension, before a poll. A
    /// path follows the handler edges of the instructions it passes, and
    /// ends at a return, a trap, or an unreachable point.
    fn allocates_before_a_poll(&self, block: usize, start: usize) -> bool {
        let function = self.function;
        let count = function.blocks.len();
        let mut seen = vec![false; count];
        let mut work = vec![(block, start)];
        while let Some((block, start)) = work.pop() {
            let Some(body) = function.blocks.get(block) else {
                return true;
            };
            let mut stopped = false;
            for (offset, instruction) in body.instructions.iter().enumerate().skip(start) {
                if instruction.kind == l::InstructionKind::CollectionPoll
                    || self.is_entry_polled_call(instruction)
                {
                    stopped = true;
                    break;
                }
                if self
                    .facts
                    .instruction_can_allocate(function.id, body.id, offset)
                {
                    return true;
                }
                if let Some(handler) = instruction.handler() {
                    let handler = handler.0 as usize;
                    if handler >= count {
                        return true;
                    }
                    if !std::mem::replace(&mut seen[handler], true) {
                        work.push((handler, 0));
                    }
                }
            }
            if stopped {
                continue;
            }
            if super::allocation::terminator_can_allocate(&body.terminator) {
                return true;
            }
            for successor in body.terminator.successors() {
                let successor = successor.0 as usize;
                if successor >= count {
                    return true;
                }
                if !std::mem::replace(&mut seen[successor], true) {
                    work.push((successor, 0));
                }
            }
        }
        false
    }
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

    /// The kind of each poll of a function.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct Kinds {
        /// Polls in the entry block that no call precedes.
        entry: usize,
        /// Polls before a terminator.
        backedge: usize,
        /// Polls directly after a call (`compiler.md` §351 rule 1).
        call: usize,
        /// Polls after the leading instructions of a handler (rule 2).
        handler: usize,
    }

    fn kinds(module: &l::Module, name: &str) -> Kinds {
        let function = function(module, name);
        let handlers = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(l::Instruction::handler)
            .collect::<BTreeSet<_>>();
        let mut kinds = Kinds::default();
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if instruction.kind != l::InstructionKind::CollectionPoll {
                    continue;
                }
                let previous = index.checked_sub(1).map(|at| &block.instructions[at].kind);
                if matches!(
                    previous,
                    Some(l::InstructionKind::Call(_) | l::InstructionKind::AsyncHandleCreate(_))
                ) {
                    kinds.call += 1;
                } else if handlers.contains(&block.id)
                    && block.instructions[..index].iter().all(takes_the_exception)
                {
                    kinds.handler += 1;
                } else if block.id == function.entry {
                    kinds.entry += 1;
                } else {
                    assert_eq!(index + 1, block.instructions.len(), "{name}: poll position");
                    kinds.backedge += 1;
                }
            }
        }
        kinds
    }

    /// The count of entry polls and of polls before a terminator.
    fn polls(module: &l::Module, name: &str) -> (usize, usize) {
        let kinds = kinds(module, name);
        (kinds.entry, kinds.backedge)
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

    /// The shapes of `compiler.md` §351 rules 1 to 5.
    const RETURN_PATHS: &str = r#"
class P { x: i32 = 0; }
const p0: P = new P();
function build(depth: i32): P {
  if (depth === 0) { return new P(); }
  build(depth - 1);
  return new P();
}
function ping(depth: i32): P {
  if (depth === 0) { return new P(); }
  pong(depth - 1);
  return new P();
}
function pong(depth: i32): P {
  if (depth === 0) { return new P(); }
  ping(depth - 1);
  return new P();
}
let next: (depth: i32) => P = step;
function step(depth: i32): P {
  if (depth === 0) { return new P(); }
  next(depth - 1);
  return new P();
}
let hook: (depth: i32) => P = g;
function f(depth: i32): P {
  if (depth === 0) { return new P(); }
  hook(depth - 1);
  return p0;
}
function g(depth: i32): P {
  if (depth === 0) { return new P(); }
  f(depth - 1);
  return new P();
}
function dive(depth: i32): P {
  if (depth === 0) { throw new Error("bottom"); }
  try {
    return dive(depth - 1);
  } catch (e) {
    const p: P = new P();
    p.x = depth;
    throw e;
  }
}
function twice(depth: i32): P {
  if (depth === 0) { return new P(); }
  twice(depth - 1);
  twice(depth - 1);
  return new P();
}
function outer(): P {
  build(3);
  return new P();
}
function count(depth: i32): i32 {
  if (depth === 0) { return 0; }
  return 1 + count(depth - 1);
}
function keep(depth: i32): P {
  if (depth === 0) { return new P(); }
  keep(depth - 1);
  return p0;
}
export function main(): void {
  let total: i32 = build(2).x + ping(2).x + next(2).x + f(2).x + twice(2).x + outer().x;
  total += count(2) + keep(2).x;
  try { dive(2); } catch (e) { total += 1; }
  print(`${total}`);
}
"#;

    fn expect(entry: usize, call: usize, handler: usize) -> Kinds {
        Kinds {
            entry,
            backedge: 0,
            call,
            handler,
        }
    }

    #[test]
    fn a_call_into_the_cycle_gets_a_poll_if_its_continuation_allocates() {
        let module = lower(RETURN_PATHS);
        // The fixture and two functions that call each other.
        assert_eq!(kinds(&module, "build"), expect(1, 1, 0));
        assert_eq!(kinds(&module, "ping"), expect(1, 1, 0));
        assert_eq!(kinds(&module, "pong"), expect(1, 1, 0));
        // A recursion through a function value.
        assert_eq!(kinds(&module, "step"), expect(1, 1, 0));
        // The continuation of `keep` reads a global and cannot allocate.
        assert_eq!(kinds(&module, "keep"), expect(1, 0, 0));
    }

    #[test]
    fn a_direct_call_closes_a_cycle_after_an_indirect_call() {
        let module = lower(RETURN_PATHS);
        // Only the continuation of the direct call in `g` allocates. `f` is
        // in the cycle only through the indirect edge, so its entry poll
        // comes from rule 5.
        assert_eq!(kinds(&module, "f"), expect(1, 0, 0));
        assert_eq!(kinds(&module, "g"), expect(1, 1, 0));
    }

    #[test]
    fn a_handler_of_a_function_of_a_cycle_gets_a_poll() {
        let module = lower(RETURN_PATHS);
        let dive = kinds(&module, "dive");
        assert_eq!((dive.entry, dive.handler, dive.backedge), (1, 1, 0));
    }

    #[test]
    fn a_call_to_a_function_with_an_entry_poll_counts_as_a_poll() {
        let module = lower(RETURN_PATHS);
        assert_eq!(kinds(&module, "twice"), expect(1, 1, 0));
        // The one poll follows the second call.
        let twice = function(&module, "twice");
        let calls = twice
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, instruction)| {
                        matches!(instruction.kind, l::InstructionKind::Call(_))
                    })
                    .map(|(index, _)| block.instructions.get(index + 1).map(|next| &next.kind))
            })
            .collect::<Vec<_>>();
        let followed = calls
            .iter()
            .map(|next| *next == Some(&l::InstructionKind::CollectionPoll))
            .collect::<Vec<_>>();
        assert_eq!(followed, vec![false, true], "{calls:?}");
    }

    #[test]
    fn no_return_path_poll_outside_a_cycle_or_without_allocation() {
        let module = lower(RETURN_PATHS);
        // Rule 4: a caller outside each cycle.
        assert_eq!(kinds(&module, "outer"), expect(0, 0, 0));
        // A recursion that cannot allocate has no poll.
        assert_eq!(kinds(&module, "count"), expect(0, 0, 0));
    }

    /// The shapes of a suspension, a callback operation, a frame creation,
    /// and an await of a direct async call (`compiler.md` §351 rules 1, 3,
    /// and "Terms").
    const SUSPENSIONS: &str = r#"
class P { x: i32 = 0; }
let below: (depth: i32) => i32 = sum;
function* down(depth: i32): Generator<i32> {
  if (depth === 0) { return; }
  const n: i32 = below(depth - 1);
  yield n;
}
function sum(depth: i32): i32 {
  let total: i32 = 0;
  for (const x of down(depth)) { total += x; }
  return total;
}
let each: (depth: i32) => void = visit;
function visit(depth: i32): void {
  if (depth === 0) { return; }
  const xs: i32[] = [depth - 1];
  xs.forEach(each);
  const p: P = new P();
  p.x = depth;
}
function* items(): Generator<i32> { yield new P().x; }
function creates(depth: i32): P {
  if (depth === 0) { return new P(); }
  creates(depth - 1);
  const it: Generator<i32> = items();
  return new P();
}
async function awaits(depth: i32): Promise<P> {
  if (depth === 0) { return new P(); }
  await awaits(depth - 1);
  return new P();
}
export async function main(): Promise<void> {
  visit(2);
  for (const x of down(2)) { print(`${x}`); }
  print(`${creates(2).x} ${(await awaits(2)).x}`);
}
"#;

    /// Rule 1 "or a suspension": the continuation of the indirect call
    /// into the cycle reaches the yield before it can allocate, and the
    /// resume returns with no allocation. An await does not isolate the
    /// suspension: its resume starts with instructions that can allocate.
    #[test]
    fn a_continuation_that_suspends_gets_a_poll() {
        let module = lower(SUSPENSIONS);
        assert_eq!(kinds(&module, "down"), expect(1, 1, 0));
    }

    /// The cycle of `visit` passes only through `forEach` with a callback
    /// in a variable: a runtime operation that takes a function value.
    #[test]
    fn a_callback_operation_closes_a_cycle() {
        let module = lower(SUSPENSIONS);
        let visit = function(&module, "visit");
        let operation = visit
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| {
                matches!(
                    &instruction.kind,
                    l::InstructionKind::Call(target) if matches!(
                        target.kind,
                        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
                    )
                ) && calls_a_function_value(visit, instruction)
            });
        assert!(operation, "visit calls a runtime operation with a callback");
        let graph = CallGraph::of_module(&module);
        let index = module
            .functions
            .iter()
            .position(|function| function.source_name == "visit")
            .expect("visit");
        assert!(graph.in_cycle(index));
        assert_eq!(kinds(&module, "visit"), expect(1, 1, 0));
    }

    /// Rule 3 counts only a call that runs the entry block of its callee.
    /// A call to a generator creates its frame, so the poll follows the
    /// recursive call.
    #[test]
    fn a_call_that_creates_a_generator_is_not_a_poll() {
        let module = lower(SUSPENSIONS);
        assert_eq!(kinds(&module, "creates"), expect(1, 1, 0));
    }

    /// An await of a direct async call is an edge of the graph: an async
    /// function that awaits itself is in a cycle. Its entry poll comes from
    /// the async rule, and the await is a terminator, not a call
    /// instruction, so rule 1 adds no poll.
    #[test]
    fn an_await_of_a_direct_async_call_is_an_edge() {
        let module = lower(SUSPENSIONS);
        let index = module
            .functions
            .iter()
            .position(|function| function.source_name == "awaits")
            .expect("awaits");
        let awaits = &module.functions[index];
        let graph = CallGraph::of_module(&module);
        assert!(awaits
            .blocks
            .iter()
            .any(|block| suspension_call(&block.terminator).is_some()));
        // No call instruction of `awaits` names itself: the edge is the
        // await.
        assert!(awaits
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| direct_callees(&graph.methods, instruction))
            .all(|callee| callee.0 as usize != index));
        assert!(graph.in_cycle(index));
        assert_eq!(kinds(&module, "awaits"), expect(1, 0, 0));
    }
}
