//! The return-path and resume-point poll sites of the placement
//! (`compiler.md` §351 rules 1 to 4, §352 rules 4, 5, and 9): the forward
//! scan of a continuation, and the sites of a function of a cycle. The
//! module documentation of the parent states the rules and the bound.

use super::*;

/// The leading instructions of a handler block that take the exception.
/// A poll goes after them (`compiler.md` §351 rule 2).
pub(super) fn takes_the_exception(instruction: &l::Instruction) -> bool {
    matches!(
        instruction.kind,
        l::InstructionKind::CatchEntry
            | l::InstructionKind::ExceptionPark
            | l::InstructionKind::ExceptionMessage
            | l::InstructionKind::ExceptionPosition
    )
}

/// The resume points of a generator: the successor block of each `yield`
/// suspension (`compiler.md` §352 rule 9). A resume point has no required
/// leading instruction: its `GeneratorIsClosing` reads the close request of
/// the resume, which a poll does not change, so a poll can go first.
pub(super) fn resume_points(function: &l::Function) -> BTreeSet<usize> {
    function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            l::Terminator::Suspend {
                kind: l::SuspendKind::Yield(_),
                successor,
                ..
            } => Some(successor.0 as usize),
            _ => None,
        })
        .collect()
}

/// The close continuation and the normal continuation of a resume point
/// that is a plain dispatch: a block with only the test of the close
/// request, which ends with a branch on that test (`compiler.md` §352 rule
/// 9). Neither part can allocate.
pub(super) fn resume_dispatch(function: &l::Function, resume: usize) -> Option<(usize, usize)> {
    let block = function.blocks.get(resume)?;
    let [test] = block.instructions.as_slice() else {
        return None;
    };
    let l::Terminator::ConditionalBranch {
        condition: l::Operand::Value(condition),
        then_target,
        else_target,
    } = &block.terminator
    else {
        return None;
    };
    let close = then_target.block.0 as usize;
    let next = else_target.block.0 as usize;
    (test.kind == l::InstructionKind::GeneratorIsClosing
        && test.result == Some(*condition)
        && close != next
        && close < function.blocks.len()
        && next < function.blocks.len())
    .then_some((close, next))
}

/// The return-path and resume-point poll sites of one function, sorted, as
/// (block, instruction index) pairs (`compiler.md` §351 rules 1, 2, and 4,
/// §352 rule 9). The poll goes before the instruction at the index.
pub(super) fn return_path_sites(
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
            // Rule 1: a poll directly after a call or a resume into the
            // cycle (§352 rule 4).
            if graph.calls_into_cycle(function, index, instruction)
                && scan.allocates_before_a_poll(block_index, position + 1)
            {
                sites.insert((block_index, position + 1));
            }
        }
    }
    // §352 rule 9: a poll on the open path of each resume point of a
    // generator. A later resume enters the generator there, past its entry
    // poll.
    if function.is_generator {
        for resume in resume_points(function) {
            if let Some(site) = scan.resume_point_site(resume) {
                sites.insert(site);
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
pub(super) struct ContinuationScan<'a> {
    pub(super) module: &'a l::Module,
    pub(super) function: &'a l::Function,
    pub(super) facts: &'a AllocationFacts,
    pub(super) graph: &'a CallGraph,
    pub(super) entry: &'a [bool],
}

impl ContinuationScan<'_> {
    /// The poll site of a resume point, if its path is open (`compiler.md`
    /// §352 rule 9). At a plain dispatch the scan reads each branch: an open
    /// normal branch puts the poll at the start of the resume point, where
    /// both branches pass it; an open close branch alone puts it at the
    /// start of the close continuation.
    pub(super) fn resume_point_site(&self, resume: usize) -> Option<(usize, usize)> {
        let Some((close, next)) = resume_dispatch(self.function, resume) else {
            return self
                .allocates_before_a_poll(resume, 0)
                .then_some((resume, 0));
        };
        if self.allocates_before_a_poll(next, 0) {
            Some((resume, 0))
        } else if self.allocates_before_a_poll(close, 0) {
            Some((close, 0))
        } else {
            None
        }
    }

    /// True if a direct call reaches the entry poll of its callee before
    /// the callee can allocate. Rule 3 counts it as a poll. A call to a
    /// generator or an async function creates its frame and does not run
    /// the entry block at the call, so it is not a poll. A resume of a
    /// generator is not a poll: the generator can resume at a suspension
    /// point, past its entry poll (§352 rule 5).
    pub(super) fn is_entry_polled_call(&self, instruction: &l::Instruction) -> bool {
        let l::InstructionKind::Call(_) = &instruction.kind else {
            return false;
        };
        let ScriptEntry::Function(callee) = self.graph.fact(self.function, instruction) else {
            return false;
        };
        self.module
            .functions
            .get(callee.0 as usize)
            .is_some_and(|function| {
                function.id == callee && !function.is_generator && !function.is_async
            })
            && self.entry.get(callee.0 as usize).copied().unwrap_or(false)
    }

    /// True if a path from instruction `start` of `block` reaches an
    /// instruction that can allocate, or a suspension, before a poll. A
    /// path follows the handler edges of the instructions it passes, and
    /// ends at a return, a trap, or an unreachable point.
    pub(super) fn allocates_before_a_poll(&self, block: usize, start: usize) -> bool {
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
