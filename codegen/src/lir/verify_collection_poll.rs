//! The poll property of a finished module (`compiler.md` §342 rule 11,
//! §351 rule 8).
//!
//! This check derives the property from the finished functions and does not
//! use the decisions of the placement pass (core principle 9). It uses the
//! strongly connected components of each graph and a backward fixpoint over
//! block suffixes. The pass uses a depth-first search with natural loops, a
//! reachability walk, and a forward scan of each continuation.
//!
//! 1. In the control-flow graph of a function, an edge that leaves a block
//!    after a poll of that block passes the poll, so the check removes it.
//!    No cycle of the remaining graph contains a block that can allocate.
//! 2. A function that can allocate and has its address taken, is async, is
//!    a generator, or is in a cycle of the call graph with indirect edges
//!    has a poll in its entry block before each instruction of that block
//!    that can allocate (§351 rule 5).
//! 3. A function that cannot allocate has no poll.
//! 4. In a function of a cycle, no path from the instruction after a call
//!    or a generator resume into that cycle reaches an instruction that can
//!    allocate, or a suspension, before a poll (§351 rule 1, §352 rule 4).
//! 5. In a function of a cycle, no path from a handler block, after its
//!    leading instructions that take the exception, reaches an instruction
//!    that can allocate before a poll (§351 rule 2).
//! 6. In a generator of a cycle, no path from the start of a resume point
//!    (the successor block of a `yield` suspension) reaches an instruction
//!    that can allocate, or a suspension, before a poll (§352 rule 9). A
//!    resume point has no leading instruction that must come before the
//!    poll. The property holds with the poll at the start of the resume
//!    point, or at the start of each branch that is open, such as the close
//!    continuation: the test of the close request and the branch cannot
//!    allocate.
//!
//! The call graph has the edges of the script entry fact of each
//! instruction and terminator (§352 rules 1 to 3): an edge to each function
//! that a fact names, to each function whose address is taken (also from a
//! foreign call, whose host can call a registered callback), and, from a
//! resume or a close of a generator, to each generator function.
//!
//! In properties 4 to 6 a direct call to a function whose entry block has
//! a poll before each instruction that can allocate is a poll (§351 rule
//! 3). A resume of a generator is not (§352 rule 5). A path follows the
//! handler edges of the instructions it passes.

use super::allocation::{terminator_can_allocate, AllocationFacts};
use super::script_entry::{
    instruction_script_entry, terminator_script_entry, ScriptEntry, ScriptMethods,
};
use super::*;

/// Records each violation of the poll property (`compiler.md` §342 rule 11,
/// §351 rule 8).
pub(super) fn verify(module: &l::Module, errors: &mut Vec<VerifyError>) {
    let facts = AllocationFacts::of_module(module);
    let graph = Graph::of_module(module);
    let entry_polled = module
        .functions
        .iter()
        .map(|function| has_entry_poll(function, &facts))
        .collect::<Vec<_>>();
    for (index, function) in module.functions.iter().enumerate() {
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
        if graph.entry_poll_required(module, index) {
            verify_entry_poll(function, &facts, errors);
        }
        if graph.cyclic[index] {
            let suffixes = Suffixes::of_function(function, &facts, &graph, &entry_polled);
            verify_return_paths(function, index, &graph, &suffixes, errors);
            if function.is_generator {
                verify_resume_points(function, &suffixes, errors);
            }
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

/// The call graph with indirect edges of a finished module (`compiler.md`
/// §351 "Terms", §352 rules 2 and 3) and its cycles. The edges come from
/// the script entry fact of each instruction and each terminator.
struct Graph {
    /// The method bodies, for the fact of a method call.
    methods: ScriptMethods,
    /// True for each function whose address is taken.
    taken: Vec<bool>,
    /// True for each generator function.
    generator: Vec<bool>,
    /// The strongly connected component of each function.
    components: Vec<usize>,
    /// True for each function in a component with an edge inside it.
    cyclic: Vec<bool>,
    /// True for each generator and each async function: a call creates its
    /// frame and does not run its entry block.
    creates_a_frame: Vec<bool>,
}

impl Graph {
    fn of_module(module: &l::Module) -> Self {
        let count = module.functions.len();
        let methods = ScriptMethods::of_module(module);
        let mut taken = vec![false; count];
        let mut edges = vec![Vec::new(); count];
        let mut value_callers = Vec::new();
        let mut resumers = Vec::new();
        for (index, function) in module.functions.iter().enumerate() {
            let mut value_caller = false;
            let mut resumer = false;
            let mut facts = Vec::new();
            for block in &function.blocks {
                for instruction in &block.instructions {
                    if let l::InstructionKind::FunctionRef(id)
                    | l::InstructionKind::MakeClosure(id) = &instruction.kind
                    {
                        if let Some(slot) = taken.get_mut(id.0 as usize) {
                            *slot = true;
                        }
                    }
                    facts.push(instruction_script_entry(&methods, function, instruction));
                }
                facts.push(terminator_script_entry(
                    &methods,
                    function,
                    &block.terminator,
                ));
            }
            for fact in facts {
                match fact {
                    ScriptEntry::Function(callee) | ScriptEntry::AsyncFunction(callee) => {
                        if (callee.0 as usize) < count {
                            edges[index].push(callee.0 as usize);
                        }
                    }
                    ScriptEntry::AddressTaken => value_caller = true,
                    ScriptEntry::GeneratorBody => resumer = true,
                    ScriptEntry::None => {}
                }
            }
            if value_caller {
                value_callers.push(index);
            }
            if resumer {
                resumers.push(index);
            }
        }
        let generator = module
            .functions
            .iter()
            .map(|function| function.is_generator)
            .collect::<Vec<_>>();
        for caller in value_callers {
            edges[caller].extend((0..count).filter(|target| taken[*target]));
        }
        for caller in resumers {
            edges[caller].extend((0..count).filter(|target| generator[*target]));
        }
        let components = strongly_connected_components(&edges);
        let mut size = vec![0usize; count];
        for component in &components {
            size[*component] += 1;
        }
        let cyclic = (0..count)
            .map(|index| size[components[index]] > 1 || edges[index].contains(&index))
            .collect();
        let creates_a_frame = module
            .functions
            .iter()
            .map(|function| function.is_generator || function.is_async)
            .collect();
        Self {
            methods,
            taken,
            generator,
            components,
            cyclic,
            creates_a_frame,
        }
    }

    /// Property 2: the function needs an entry poll if it can allocate.
    fn entry_poll_required(&self, module: &l::Module, index: usize) -> bool {
        module
            .functions
            .get(index)
            .is_some_and(|function| function.is_async || function.is_generator)
            || self.taken.get(index).copied().unwrap_or(false)
            || self.cyclic.get(index).copied().unwrap_or(false)
    }

    /// True if a function that the instruction can run is in the component
    /// of the function `caller`.
    fn enters_component(
        &self,
        function: &l::Function,
        caller: usize,
        instruction: &l::Instruction,
    ) -> bool {
        let component = self.components.get(caller).copied();
        let same = |callee: usize| {
            component.is_some() && self.components.get(callee).copied() == component
        };
        let any = |set: &[bool]| {
            set.iter()
                .enumerate()
                .any(|(target, member)| *member && same(target))
        };
        match instruction_script_entry(&self.methods, function, instruction) {
            ScriptEntry::Function(callee) | ScriptEntry::AsyncFunction(callee) => {
                same(callee.0 as usize)
            }
            ScriptEntry::AddressTaken => any(&self.taken),
            ScriptEntry::GeneratorBody => any(&self.generator),
            ScriptEntry::None => false,
        }
    }
}

/// True if the entry block has a poll before each instruction of that
/// block that can allocate.
fn has_entry_poll(function: &l::Function, facts: &AllocationFacts) -> bool {
    let entry = function.entry;
    function.blocks.get(entry.0 as usize).is_some_and(|block| {
        first_poll(block).is_some_and(|poll| {
            (0..poll).all(|index| !facts.instruction_can_allocate(function.id, entry, index))
        })
    })
}

/// The leading instructions of a handler block that take the exception.
fn takes_the_exception(instruction: &l::Instruction) -> bool {
    matches!(
        instruction.kind,
        l::InstructionKind::CatchEntry
            | l::InstructionKind::ExceptionPark
            | l::InstructionKind::ExceptionMessage
            | l::InstructionKind::ExceptionPosition
    )
}

/// For each block of a function: true if a path from its first
/// instruction reaches an instruction that can allocate, or a suspension,
/// before a poll. The least fixpoint over the block suffixes.
struct Suffixes<'a> {
    function: &'a l::Function,
    facts: &'a AllocationFacts,
    graph: &'a Graph,
    entry_polled: &'a [bool],
    open: Vec<bool>,
}

impl<'a> Suffixes<'a> {
    fn of_function(
        function: &'a l::Function,
        facts: &'a AllocationFacts,
        graph: &'a Graph,
        entry_polled: &'a [bool],
    ) -> Self {
        let mut suffixes = Self {
            function,
            facts,
            graph,
            entry_polled,
            open: vec![false; function.blocks.len()],
        };
        loop {
            let mut changed = false;
            for block in (0..function.blocks.len()).rev() {
                if !suffixes.open[block] && suffixes.open_from(block, 0) {
                    suffixes.open[block] = true;
                    changed = true;
                }
            }
            if !changed {
                return suffixes;
            }
        }
    }

    /// True if the instruction is a poll, or a direct call to a function
    /// with an entry poll (`compiler.md` §351 rule 3). A call to a generator
    /// or an async function creates its frame and runs no entry poll. A
    /// resume of a generator is not a direct call (§352 rule 5).
    fn closes(&self, instruction: &l::Instruction) -> bool {
        if instruction.kind == l::InstructionKind::CollectionPoll {
            return true;
        }
        if !matches!(instruction.kind, l::InstructionKind::Call(_)) {
            return false;
        }
        let ScriptEntry::Function(callee) =
            instruction_script_entry(&self.graph.methods, self.function, instruction)
        else {
            return false;
        };
        let callee = callee.0 as usize;
        self.entry_polled.get(callee).copied().unwrap_or(false)
            && !self
                .graph
                .creates_a_frame
                .get(callee)
                .copied()
                .unwrap_or(true)
    }

    /// The suffix of `block` from instruction `start`, with the present
    /// values of the other blocks. An unknown block is open.
    fn open_from(&self, block: usize, start: usize) -> bool {
        let Some(body) = self.function.blocks.get(block) else {
            return true;
        };
        let target_open = |id: l::BlockId| self.open.get(id.0 as usize).copied().unwrap_or(true);
        for (index, instruction) in body.instructions.iter().enumerate().skip(start) {
            if self.closes(instruction) {
                return false;
            }
            if self
                .facts
                .instruction_can_allocate(self.function.id, body.id, index)
                || instruction.handler().is_some_and(target_open)
            {
                return true;
            }
        }
        terminator_can_allocate(&body.terminator)
            || body.terminator.successors().into_iter().any(target_open)
    }
}

/// Properties 4 and 5 for one function of a cycle.
fn verify_return_paths(
    function: &l::Function,
    index: usize,
    graph: &Graph,
    suffixes: &Suffixes<'_>,
    errors: &mut Vec<VerifyError>,
) {
    let mut handlers = BTreeSet::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (position, instruction) in block.instructions.iter().enumerate() {
            if let Some(handler) = instruction.handler() {
                handlers.insert(handler.0 as usize);
            }
            if graph.enters_component(function, index, instruction)
                && suffixes.open_from(block_index, position + 1)
            {
                errors.push(VerifyError {
                    message: format!(
                        "function {} block {} instruction {position}: a return into the cycle \
                         reaches an allocation with no collection poll",
                        function.id.0, block.id.0
                    ),
                });
            }
        }
    }
    for handler in handlers {
        let Some(block) = function.blocks.get(handler) else {
            continue;
        };
        let start = block
            .instructions
            .iter()
            .take_while(|instruction| takes_the_exception(instruction))
            .count();
        if suffixes.open_from(handler, start) {
            errors.push(VerifyError {
                message: format!(
                    "function {} handler block {} reaches an allocation with no collection poll",
                    function.id.0, block.id.0
                ),
            });
        }
    }
}

/// Property 6 for one generator of a cycle.
fn verify_resume_points(
    function: &l::Function,
    suffixes: &Suffixes<'_>,
    errors: &mut Vec<VerifyError>,
) {
    let resumes = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            l::Terminator::Suspend {
                kind: l::SuspendKind::Yield(_),
                successor,
                ..
            } => Some(*successor),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for resume in resumes {
        if suffixes.open_from(resume.0 as usize, 0) {
            errors.push(VerifyError {
                message: format!(
                    "function {} resume point block {} reaches an allocation with no collection \
                     poll",
                    function.id.0, resume.0
                ),
            });
        }
    }
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

    /// The shapes of `compiler.md` §351 rules 1 to 3 and 5.
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
function count(depth: i32): i32 {
  if (depth === 0) { return 0; }
  return 1 + count(depth - 1);
}
export function main(): void {
  let total: i32 = build(2).x + ping(2).x + next(2).x + f(2).x + twice(2).x + count(2);
  try { dive(2); } catch (e) { total += 1; }
  print(`${total}`);
}
"#;

    fn lower_return_paths() -> l::Module {
        let hir =
            check_program(&[SourceFile::new("return-paths.ts", RETURN_PATHS)]).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    /// Removes each poll of the function except the entry poll: the first
    /// poll of the entry block, with no call before it. Returns the count.
    fn remove_return_path_polls(function: &mut l::Function) -> usize {
        let entry = function.entry;
        let mut removed = 0;
        for block in &mut function.blocks {
            let mut kept_entry = block.id != entry;
            let mut index = 0;
            while index < block.instructions.len() {
                let instruction = &block.instructions[index];
                if matches!(
                    instruction.kind,
                    l::InstructionKind::Call(_) | l::InstructionKind::AsyncHandleCreate(_)
                ) {
                    kept_entry = true;
                }
                if instruction.kind == l::InstructionKind::CollectionPoll {
                    if kept_entry {
                        block.instructions.remove(index);
                        removed += 1;
                        continue;
                    }
                    kept_entry = true;
                }
                index += 1;
            }
        }
        removed
    }

    #[test]
    fn the_return_path_shapes_satisfy_the_property() {
        assert_eq!(messages(&lower_return_paths()), Vec::<String>::new());
    }

    #[test]
    fn a_shape_with_its_return_path_poll_removed_is_rejected() {
        for (name, expected) in [
            ("build", "a return into the cycle"),
            ("ping", "a return into the cycle"),
            ("pong", "a return into the cycle"),
            ("step", "a return into the cycle"),
            ("g", "a return into the cycle"),
            ("twice", "a return into the cycle"),
            ("dive", "handler block"),
        ] {
            let mut module = lower_return_paths();
            let function = function_mut(&mut module, name);
            assert_eq!(remove_return_path_polls(function), 1, "{name}");
            let id = function.id.0;
            let messages = messages(&module);
            assert!(
                messages
                    .iter()
                    .any(|message| message.starts_with(&format!("function {id} "))
                        && message.contains(expected)),
                "{name}: {messages:?}"
            );
            // The entry poll is present.
            assert!(
                !messages
                    .iter()
                    .any(|message| message.contains("needs an entry collection poll")),
                "{name}: {messages:?}"
            );
        }
    }

    #[test]
    fn a_function_in_a_cycle_through_an_indirect_edge_needs_an_entry_poll() {
        let mut module = lower_return_paths();
        let f = function_mut(&mut module, "f");
        assert_eq!(remove_polls(f), 1);
        let id = f.id.0;
        let messages = messages(&module);
        assert!(
            messages.contains(&format!(
                "function {id} needs an entry collection poll before its first allocation"
            )),
            "{messages:?}"
        );
    }

    #[test]
    fn an_allocation_after_a_recursive_call_with_no_poll_is_rejected() {
        let module = lower_return_paths();
        // The recursion with no allocation has no poll and passes.
        let mut control = module.clone();
        assert_eq!(remove_polls(function_mut(&mut control, "count")), 0);
        assert_eq!(messages(&control), Vec::<String>::new());
        // The same recursion with one allocation after the call.
        let mut module = module;
        let class = module
            .classes
            .iter()
            .find(|class| class.source_name == "P")
            .map(|class| class.id)
            .expect("class P");
        let count = function_mut(&mut module, "count");
        let id = count.id.0;
        let self_call = count
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block, body)| {
                body.instructions
                    .iter()
                    .position(|instruction| matches!(instruction.kind, l::InstructionKind::Call(_)))
                    .map(|index| (block, index))
            })
            .expect("count calls itself");
        let pos = count.pos.clone();
        count.blocks[self_call.0].instructions.insert(
            self_call.1 + 1,
            l::Instruction {
                result: None,
                kind: l::InstructionKind::AllocateClass(class),
                count_action: None,
                operands: Vec::new(),
                invalidates: Vec::new(),
                traps: Vec::new(),
                pos,
            },
        );
        let messages = messages(&module);
        assert!(
            messages
                .iter()
                .any(|message| message.starts_with(&format!("function {id} "))
                    && message.contains("a return into the cycle")),
            "{messages:?}"
        );
    }

    /// A suspension, a callback operation, a frame creation, and an await
    /// of a direct async call (`compiler.md` §351 rules 1, 3, and "Terms").
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
  print(`${sum(2)} ${creates(2).x} ${(await awaits(2)).x}`);
}
"#;

    fn lower_suspensions() -> l::Module {
        let hir = check_program(&[SourceFile::new("suspensions.ts", SUSPENSIONS)]).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    /// `down`: the continuation of an indirect call reaches a yield before
    /// it can allocate. `visit`: the cycle passes only through `forEach`
    /// with a callback in a variable. `creates`: a call that creates a
    /// generator is not a poll.
    #[test]
    fn a_suspension_shape_with_its_return_path_poll_removed_is_rejected() {
        assert_eq!(messages(&lower_suspensions()), Vec::<String>::new());
        for name in ["down", "visit", "creates"] {
            let mut module = lower_suspensions();
            let function = function_mut(&mut module, name);
            assert_eq!(remove_return_path_polls(function), 1, "{name}");
            let id = function.id.0;
            let messages = messages(&module);
            assert!(
                messages
                    .iter()
                    .any(|message| message.starts_with(&format!("function {id} "))
                        && message.contains("a return into the cycle")),
                "{name}: {messages:?}"
            );
            assert!(
                !messages
                    .iter()
                    .any(|message| message.contains("needs an entry collection poll")),
                "{name}: {messages:?}"
            );
        }
    }

    /// An await of a direct async call is an edge: an async function that
    /// awaits itself is in a cycle of the graph.
    #[test]
    fn an_await_of_a_direct_async_call_is_an_edge_of_the_graph() {
        let module = lower_suspensions();
        let index = module
            .functions
            .iter()
            .position(|function| function.source_name == "awaits")
            .expect("awaits");
        let graph = Graph::of_module(&module);
        assert!(graph.cyclic[index]);
        assert!(graph.creates_a_frame[index]);
    }

    /// The shapes of `compiler.md` §352 rules 3 to 5: a function resumes a
    /// generator whose body calls back into it.
    const GENERATORS: &str = r#"
class P { a: i64 = 0; }
function* child(d: i32): Generator<i32> {
  if (d >= 0) { step(d); return; }
  yield 1;
}
function step(d: i32): void {
  if (d > 0) {
    const it = child(d - 1);
    it.next();
  }
  new P(); new P();
}
function* other(d: i32): Generator<i32> {
  if (d >= 0) { start(d); return; }
  yield 1;
}
function drive(it: Generator<i32>): void {
  it.next();
  new P();
}
function start(d: i32): void {
  if (d > 0) { drive(other(d - 1)); }
}
function* walk(d: i32): Generator<i32> {
  if (d > 0) { for (const x of walk(d - 1)) { } }
  new P();
  if (d < 0) { yield 1; }
}
function* pass(d: i32): Generator<i32> {
  if (d === 0) { yield 1; return; }
  for (const v of pass(d - 1)) { yield v; }
  new P();
}
function* up(d: i32): Generator<i32> {
  if (d === 0) { yield 1; return; }
  const it: Generator<i32> = up(d - 1);
  const r = it.next();
  yield r.value + 1;
}
export function main(): void {
  step(2);
  start(2);
  for (const x of walk(2)) { }
  for (const v of pass(2)) { print(`${v}`); }
  print(`${up(2).next().value}`);
}
"#;

    fn lower_generators() -> l::Module {
        let hir = check_program(&[SourceFile::new("generators.ts", GENERATORS)]).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    /// Removes each poll directly after a resume of a generator and returns
    /// the count. Each other poll stays: the entry poll and the poll after
    /// the creation of the generator.
    fn remove_polls_after_resumes(module: &l::Module, name: &str) -> (l::Module, usize) {
        let mut module = module.clone();
        let methods = ScriptMethods::of_module(&module);
        let function = function_mut(&mut module, name);
        let snapshot = function.clone();
        let mut removed = 0;
        for (block, original) in function.blocks.iter_mut().zip(&snapshot.blocks) {
            let resumes = original
                .instructions
                .iter()
                .enumerate()
                .filter(|(index, instruction)| {
                    matches!(instruction.kind, l::InstructionKind::Call(_))
                        && instruction_script_entry(&methods, &snapshot, instruction)
                            == ScriptEntry::GeneratorBody
                        && original.instructions.get(index + 1).map(|next| &next.kind)
                            == Some(&l::InstructionKind::CollectionPoll)
                })
                .map(|(index, _)| index + 1)
                .collect::<Vec<_>>();
            for index in resumes.into_iter().rev() {
                block.instructions.remove(index);
                removed += 1;
            }
        }
        (module, removed)
    }

    fn poll_count(function: &l::Function) -> usize {
        function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| instruction.kind == l::InstructionKind::CollectionPoll)
            .count()
    }

    /// For each shape of §352 acceptance 1 and 2, the module with only the
    /// poll after the resume removed is rejected, with each other poll
    /// present (acceptance 3).
    #[test]
    fn a_generator_shape_with_its_resume_poll_removed_is_rejected() {
        let module = lower_generators();
        assert_eq!(messages(&module), Vec::<String>::new());
        for name in ["step", "drive", "walk", "pass", "up"] {
            let (removed, count) = remove_polls_after_resumes(&module, name);
            assert_eq!(count, 1, "{name}");
            let before = module
                .functions
                .iter()
                .find(|function| function.source_name == name)
                .map(poll_count)
                .expect("the function");
            let function = removed
                .functions
                .iter()
                .find(|function| function.source_name == name)
                .expect("the function");
            // Only the poll after the resume is gone.
            assert_eq!(poll_count(function) + 1, before, "{name}");
            let id = function.id.0;
            let messages = messages(&removed);
            assert!(
                messages
                    .iter()
                    .any(|message| message.starts_with(&format!("function {id} "))
                        && message.contains("a return into the cycle")),
                "{name}: {messages:?}"
            );
            assert!(
                !messages
                    .iter()
                    .any(|message| message.contains("needs an entry collection poll")),
                "{name}: {messages:?}"
            );
        }
    }

    /// The resume is an edge to each generator function: `drive` is in a
    /// cycle only through it.
    #[test]
    fn a_resume_is_an_edge_of_the_graph() {
        let module = lower_generators();
        let index = |name: &str| {
            module
                .functions
                .iter()
                .position(|function| function.source_name == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        let graph = Graph::of_module(&module);
        assert!(graph.cyclic[index("drive")]);
        assert_eq!(
            graph.components[index("drive")],
            graph.components[index("other")]
        );
        assert!(graph.entry_poll_required(&module, index("drive")));
    }

    /// The shapes of `compiler.md` §352 rule 9: `stage` resumes its child
    /// from the code after a `yield`, `walk` walks a tree, and `plain`
    /// resumes no generator, so it is in no cycle.
    const RESUME_POINTS: &str = r#"
class P { a: i64 = 0; }
class Node { value: i32 = 0; children: Node[] = []; }
function* stage(d: i32): Generator<i32> {
  if (d === 0) { yield 0; yield 0; return; }
  const child: Generator<i32> = stage(d - 1);
  child.next(); yield 1;
  new P(); child.next(); yield 2;
}
function* walk(n: Node): Generator<i32> {
  yield n.value;
  for (const c of n.children) { for (const x of walk(c)) { yield x; } }
}
function* plain(): Generator<i32> { yield 1; new P(); yield 2; }
export function main(): void {
  const a: Generator<i32> = stage(2);
  a.next();
  a.next();
  const root: Node = new Node();
  root.children.push(new Node());
  for (const x of walk(root)) { }
  for (const x of plain()) { }
}
"#;

    fn lower_resume_points() -> l::Module {
        let hir =
            check_program(&[SourceFile::new("resume-points.ts", RESUME_POINTS)]).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    /// The poll of each resume point of a function, as the resume point
    /// and the block whose first instruction is that poll: the resume point
    /// itself, or the close continuation that its branch on the close
    /// request names.
    fn polled_resume_points(function: &l::Function) -> Vec<(usize, usize)> {
        let first_is_poll = |block: usize| {
            function.blocks[block]
                .instructions
                .first()
                .map(|first| &first.kind)
                == Some(&l::InstructionKind::CollectionPoll)
        };
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
            .filter_map(|resume| {
                if first_is_poll(resume) {
                    return Some((resume, resume));
                }
                let l::Terminator::ConditionalBranch { then_target, .. } =
                    &function.blocks[resume].terminator
                else {
                    return None;
                };
                let close = then_target.block.0 as usize;
                first_is_poll(close).then_some((resume, close))
            })
            .collect()
    }

    /// Property 6: for `stage` and `walk`, the module with only the poll of
    /// one resume point removed is rejected, with the entry poll and each
    /// poll after a resume present. `plain` has no such poll and passes.
    /// The poll of the resume point after `yield x` in `walk` is at the
    /// start of its close continuation, and the property accepts it there.
    #[test]
    fn a_resume_point_with_its_poll_removed_is_rejected() {
        let module = lower_resume_points();
        assert_eq!(messages(&module), Vec::<String>::new());
        let plain = module
            .functions
            .iter()
            .find(|function| function.source_name == "plain")
            .expect("plain");
        assert_eq!(polled_resume_points(plain), Vec::<(usize, usize)>::new());
        for name in ["stage", "walk"] {
            let function = module
                .functions
                .iter()
                .find(|function| function.source_name == name)
                .expect("the function");
            let resumes = polled_resume_points(function);
            assert!(!resumes.is_empty(), "{name}");
            if name == "walk" {
                assert!(
                    resumes.iter().any(|(resume, block)| resume != block),
                    "{name}: {resumes:?}"
                );
            }
            for (resume, block) in resumes {
                let mut removed = module.clone();
                let target = function_mut(&mut removed, name);
                let polls = poll_count(target);
                target.blocks[block].instructions.remove(0);
                assert_eq!(poll_count(target) + 1, polls, "{name}");
                let id = target.id.0;
                let messages = messages(&removed);
                assert_eq!(
                    messages,
                    vec![format!(
                        "function {id} resume point block {resume} reaches an allocation with no \
                         collection poll"
                    )],
                    "{name}"
                );
            }
        }
    }

    /// A foreign call is an edge to each function whose address is taken
    /// (`compiler.md` §352 rule 1): `visit` calls the host, which can call
    /// `visit` back, and allocates after the foreign call. The module with
    /// only the poll after the foreign call removed is rejected, with the
    /// entry poll present.
    #[test]
    fn a_foreign_call_shape_with_its_poll_removed_is_rejected() {
        let files = [
            SourceFile::ambient(
                "host.d.ts",
                "// @subscript-c-header include=\"host.h\"\ndeclare function hostWalk(n: i32): void;",
            ),
            SourceFile::new(
                "foreign.ts",
                "class P { x: i32 = 0; }\n\
                 let keep: (n: i32) => void = visit;\n\
                 function visit(n: i32): void { if (n > 0) { hostWalk(n - 1); } new P(); new P(); }\n\
                 export function main(): void { visit(2); }\n",
            ),
        ];
        let module =
            super::super::lower_module(&check_program(&files).expect("checks")).expect("lowers");
        assert_eq!(messages(&module), Vec::<String>::new());
        let mut removed = module.clone();
        let visit = function_mut(&mut removed, "visit");
        let polls = poll_count(visit);
        let mut taken = 0;
        for block in &mut visit.blocks {
            let after = block
                .instructions
                .iter()
                .position(|instruction| {
                    matches!(
                        &instruction.kind,
                        l::InstructionKind::Call(l::CallTarget {
                            kind: l::CallTargetKind::Foreign(_),
                            ..
                        })
                    )
                })
                .map(|index| index + 1);
            if let Some(index) = after {
                assert_eq!(
                    block.instructions[index].kind,
                    l::InstructionKind::CollectionPoll
                );
                block.instructions.remove(index);
                taken += 1;
            }
        }
        assert_eq!(taken, 1);
        assert_eq!(poll_count(visit) + 1, polls);
        let id = visit.id.0;
        let messages = messages(&removed);
        assert!(
            messages
                .iter()
                .any(|message| message.starts_with(&format!("function {id} "))
                    && message.contains("a return into the cycle")),
            "{messages:?}"
        );
        assert!(
            !messages
                .iter()
                .any(|message| message.contains("needs an entry collection poll")),
            "{messages:?}"
        );
    }
}
