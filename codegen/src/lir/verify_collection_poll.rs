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
//!    into that cycle reaches an instruction that can allocate, or a
//!    suspension, before a poll (§351 rule 1).
//! 5. In a function of a cycle, no path from a handler block, after its
//!    leading instructions that take the exception, reaches an instruction
//!    that can allocate before a poll (§351 rule 2).
//!
//! In properties 4 and 5 a direct call to a function whose entry block has
//! a poll before each instruction that can allocate is a poll (§351 rule
//! 3). A path follows the handler edges of the instructions it passes.

use super::allocation::{terminator_can_allocate, AllocationFacts};
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
/// §351 "Terms") and its cycles.
struct Graph {
    /// The method bodies, for the callee of a method call.
    methods: HashMap<l::MethodId, usize>,
    /// True for each function whose address is taken.
    taken: Vec<bool>,
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
        let methods = module
            .classes
            .iter()
            .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
            .map(|method| (method.id, method.function.0 as usize))
            .collect::<HashMap<_, _>>();
        let mut taken = vec![false; count];
        let mut edges = vec![Vec::new(); count];
        let mut value_callers = Vec::new();
        for (index, function) in module.functions.iter().enumerate() {
            let mut value_caller = false;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                if let l::InstructionKind::FunctionRef(id) | l::InstructionKind::MakeClosure(id) =
                    &instruction.kind
                {
                    if let Some(slot) = taken.get_mut(id.0 as usize) {
                        *slot = true;
                    }
                    continue;
                }
                if let Some(callee) = direct_callee(&methods, instruction) {
                    if callee < count {
                        edges[index].push(callee);
                    }
                }
                value_caller |= takes_function_value(function, instruction);
            }
            // An await of a direct async call is an edge to its callee.
            for block in &function.blocks {
                let l::Terminator::Suspend {
                    kind: l::SuspendKind::AsyncCall { target, .. },
                    ..
                } = &block.terminator
                else {
                    continue;
                };
                match callee_of_target(&methods, target) {
                    Some(callee) if callee < count => edges[index].push(callee),
                    Some(_) => {}
                    None => value_caller |= matches!(target.kind, l::CallTargetKind::Indirect),
                }
            }
            if value_caller {
                value_callers.push(index);
            }
        }
        for caller in value_callers {
            edges[caller].extend((0..count).filter(|target| taken[*target]));
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

    /// True if a possible callee of the instruction is in the component of
    /// the function `caller`.
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
        if direct_callee(&self.methods, instruction).is_some_and(same) {
            return true;
        }
        takes_function_value(function, instruction)
            && self
                .taken
                .iter()
                .enumerate()
                .any(|(target, taken)| *taken && same(target))
    }
}

/// The script function that a call or a task creation names directly.
fn direct_callee(
    methods: &HashMap<l::MethodId, usize>,
    instruction: &l::Instruction,
) -> Option<usize> {
    match &instruction.kind {
        l::InstructionKind::Call(target) | l::InstructionKind::AsyncHandleCreate(target) => {
            callee_of_target(methods, target)
        }
        _ => None,
    }
}

/// The script function that a call target names directly.
fn callee_of_target(
    methods: &HashMap<l::MethodId, usize>,
    target: &l::CallTarget,
) -> Option<usize> {
    match &target.kind {
        l::CallTargetKind::Function(id)
        | l::CallTargetKind::LambdaSelf(id)
        | l::CallTargetKind::StaticClosure(id)
        | l::CallTargetKind::ClosureDefault { helper: id, .. } => Some(id.0 as usize),
        l::CallTargetKind::Method(id) => methods.get(id).copied(),
        _ => None,
    }
}

/// True if the instruction calls a function value: an indirect call, or a
/// runtime operation with an operand of a function type.
fn takes_function_value(function: &l::Function, instruction: &l::Instruction) -> bool {
    let target = match &instruction.kind {
        l::InstructionKind::Call(target) | l::InstructionKind::AsyncHandleCreate(target) => target,
        _ => return false,
    };
    let function_operand = || {
        instruction.operands.iter().any(|operand| {
            let ty = match operand {
                l::Operand::Constant(constant) => Some(&constant.ty),
                l::Operand::Value(id) => function
                    .values
                    .iter()
                    .find(|value| value.id == *id)
                    .and_then(|value| match &value.ty {
                        l::ValueType::Data(ty) => Some(ty),
                        _ => None,
                    }),
            };
            ty.is_some_and(|ty| ty.function_type().is_some())
        })
    };
    match &target.kind {
        l::CallTargetKind::Indirect => true,
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_) => function_operand(),
        _ => false,
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
    /// or an async function creates its frame and runs no entry poll.
    fn closes(&self, instruction: &l::Instruction) -> bool {
        if instruction.kind == l::InstructionKind::CollectionPoll {
            return true;
        }
        matches!(instruction.kind, l::InstructionKind::Call(_))
            && direct_callee(&self.graph.methods, instruction).is_some_and(|callee| {
                self.entry_polled.get(callee).copied().unwrap_or(false)
                    && !self
                        .graph
                        .creates_a_frame
                        .get(callee)
                        .copied()
                        .unwrap_or(true)
            })
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
}
