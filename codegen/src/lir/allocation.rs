//! The allocation fact of instructions, blocks, and functions
//! (`compiler.md` §342 rule 10).
//!
//! The operation table states the fact of each intrinsic operation, and
//! [`l::BuiltinMethod::can_allocate`] states the fact of each built-in
//! method. [`instruction_allocation`] gives the fact of one instruction by a
//! total match over [`l::InstructionKind`]. [`terminator_can_allocate`] does
//! the same over [`l::Terminator`]. [`AllocationFacts::of_module`] then
//! gives the fact of each function by a fixpoint over the call graph.
//!
//! The facts are an analysis result, not a stored field. A consumer
//! computes them from the module that it executes or changes, so no fact
//! can describe an older form of the instructions.

use super::*;

/// The fact of one instruction before the call graph is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstructionAllocation {
    /// The instruction cannot allocate in the Context.
    Never,
    /// The instruction can allocate in the Context.
    Can,
    /// The instruction can allocate if the script function can allocate.
    Callee(l::FunctionId),
}

/// The method bodies of a module, for the fact of a method call.
#[derive(Debug, Clone, Default)]
pub struct MethodBodies(HashMap<l::MethodId, l::FunctionId>);

impl MethodBodies {
    /// Collects the constructor and method bodies of each class.
    #[must_use]
    pub fn of_module(module: &l::Module) -> Self {
        Self(
            module
                .classes
                .iter()
                .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
                .map(|method| (method.id, method.function))
                .collect(),
        )
    }
}

/// True if a value of this type has no Context storage of its own, so an
/// operation that only makes it does not allocate.
fn scalar(ty: &Type) -> bool {
    ty.is_numeric()
        || matches!(
            ty,
            Type::Bool | Type::Enum(_) | Type::Date | Type::Void | Type::StringAlias(_)
        )
}

/// True if the instruction has no result, or a scalar or address result.
fn result_is_scalar(function: &l::Function, instruction: &l::Instruction) -> bool {
    let Some(id) = instruction.result else {
        return true;
    };
    let value = function
        .values
        .get(id.0 as usize)
        .filter(|value| value.id == id)
        .or_else(|| function.values.iter().find(|value| value.id == id));
    match value.map(|value| &value.ty) {
        Some(l::ValueType::Data(ty)) => scalar(ty),
        Some(l::ValueType::Address(_)) => true,
        Some(l::ValueType::Iterator(_)) | None => false,
    }
}

/// The type of the result of an instruction, if it has one.
fn result_type<'f>(
    function: &'f l::Function,
    instruction: &l::Instruction,
) -> Option<&'f l::ValueType> {
    let id = instruction.result?;
    function
        .values
        .get(id.0 as usize)
        .filter(|value| value.id == id)
        .or_else(|| function.values.iter().find(|value| value.id == id))
        .map(|value| &value.ty)
}

/// True if an `ArrayLiteral` makes a `FixedArray` value. Each tier writes
/// such a literal in place and does not allocate in the Context
/// (`compiler.md` §344 rule 7).
fn fixed_array_literal(function: &l::Function, instruction: &l::Instruction) -> bool {
    matches!(
        result_type(function, instruction),
        Some(l::ValueType::Data(Type::FixedArray(..)))
    )
}

/// True if an `AllocateClass` makes a value class at an address. Each tier
/// zeroes local or frame storage for a value class and does not allocate
/// in the Context (`compiler.md` §344 rule 7).
fn value_class_at_address(
    module: &l::Module,
    function: &l::Function,
    instruction: &l::Instruction,
    class: ClassId,
) -> bool {
    module
        .classes
        .get(class.0)
        .is_some_and(|definition| definition.is_value)
        && matches!(
            result_type(function, instruction),
            Some(l::ValueType::Address(_))
        )
}

/// True if a `Coerce` with no trap takes a value of a reference class to
/// the nullable type of the same class (`compiler.md` §351 rule 9). The
/// value is a pointer to the object in both types. The interpreter copies
/// the handle, the dev JIT keeps the pointer, and the emitted C casts the
/// pointer, so no tier allocates in the Context.
fn reference_to_nullable(
    module: &l::Module,
    function: &l::Function,
    instruction: &l::Instruction,
) -> bool {
    let source = match instruction.operands.as_slice() {
        [l::Operand::Value(value)] => function
            .values
            .get(value.0 as usize)
            .filter(|data| data.id == *value)
            .or_else(|| function.values.iter().find(|data| data.id == *value))
            .map(|data| &data.ty),
        _ => None,
    };
    let Some(l::ValueType::Data(Type::Class(source))) = source else {
        return false;
    };
    let reference = module
        .classes
        .get(source.0)
        .is_some_and(|class| !class.is_value);
    instruction.traps.is_empty()
        && reference
        && matches!(
            result_type(function, instruction),
            Some(l::ValueType::Data(Type::Nullable(target, _)))
                if matches!(target.as_ref(), Type::Class(target) if target == source)
        )
}

/// The table fact of an intrinsic operation. An operation that the module
/// table does not list can allocate.
fn intrinsic_can_allocate(module: &l::Module, intrinsic: &l::Intrinsic) -> bool {
    module
        .intrinsic_operations
        .iter()
        .find(|row| row.family == intrinsic.family && row.operation == intrinsic.operation)
        .is_none_or(|row| row.can_allocate)
}

/// The fact of a call to a script function. A call that creates a
/// generator or an async frame allocates that frame.
fn script_call(module: &l::Module, callee: l::FunctionId) -> InstructionAllocation {
    match module
        .functions
        .get(callee.0 as usize)
        .filter(|function| function.id == callee)
    {
        Some(function) if !function.is_generator && !function.is_async => {
            InstructionAllocation::Callee(callee)
        }
        _ => InstructionAllocation::Can,
    }
}

/// The fact of a call target (`compiler.md` §342 rule 10).
fn call_allocation(
    module: &l::Module,
    methods: &MethodBodies,
    target: &l::CallTarget,
) -> InstructionAllocation {
    use InstructionAllocation::{Can, Never};
    match &target.kind {
        l::CallTargetKind::Function(id)
        | l::CallTargetKind::LambdaSelf(id)
        | l::CallTargetKind::StaticClosure(id) => script_call(module, *id),
        l::CallTargetKind::ClosureDefault { helper, .. } => script_call(module, *helper),
        l::CallTargetKind::Method(id) => methods
            .0
            .get(id)
            .map_or(Can, |function| script_call(module, *function)),
        l::CallTargetKind::Foreign(_) | l::CallTargetKind::Indirect => Can,
        l::CallTargetKind::Intrinsic(intrinsic) => {
            if intrinsic_can_allocate(module, intrinsic) {
                Can
            } else {
                Never
            }
        }
        l::CallTargetKind::BuiltinMethod(method) => {
            if method.can_allocate() {
                Can
            } else {
                Never
            }
        }
    }
}

/// Returns the fact of one instruction (`compiler.md` §342 rule 10).
///
/// The match is total, so a new instruction kind cannot exist without its
/// fact. A conversion, a binary operation, and an iterator read allocate
/// only for a result that is not scalar: a string concatenation, a
/// formatted number, a code point, or a boxed value. A coercion of a
/// reference class to its own nullable type cannot allocate (`compiler.md`
/// §351 rule 9).
#[must_use]
pub fn instruction_allocation(
    module: &l::Module,
    methods: &MethodBodies,
    function: &l::Function,
    instruction: &l::Instruction,
) -> InstructionAllocation {
    use l::InstructionKind as K;
    use InstructionAllocation::{Can, Never};
    match &instruction.kind {
        K::Copy
        | K::LoadLocal(_)
        | K::StoreLocal(_)
        | K::AddressOfLocal(_)
        | K::LoadGlobal(_)
        | K::StoreGlobal(_)
        | K::AddressOfGlobal(_)
        | K::FunctionRef(_)
        | K::Unary(_)
        | K::StringOperand
        | K::NarrowNonNull(_)
        | K::AddressOfValue
        | K::AddressOfField(_)
        | K::AddressOfIndex { .. }
        | K::LoadAddress
        | K::StoreAddress
        | K::LoadField(_)
        | K::Length
        | K::Zero
        | K::IteratorCreate { .. }
        | K::IteratorHasNext
        | K::IteratorBound
        | K::IteratorAdvance
        | K::GeneratorIsClosing
        | K::GeneratorFinalizer(_)
        | K::AsyncHandleRetain
        | K::AsyncHandleArrayRetain
        | K::CollectionPoll => Never,
        K::Coerce if reference_to_nullable(module, function, instruction) => Never,
        K::Binary(_) | K::Cast | K::Coerce | K::IteratorValue => {
            if result_is_scalar(function, instruction) {
                Never
            } else {
                Can
            }
        }
        K::Call(target) => call_allocation(module, methods, target),
        K::ArrayLiteral if fixed_array_literal(function, instruction) => Never,
        K::AllocateClass(class)
            if value_class_at_address(module, function, instruction, *class) =>
        {
            Never
        }
        K::StringLiteral(_)
        | K::AliasString(_)
        | K::AllocateClass(_)
        | K::BoxBoundaryValue { .. }
        | K::ArrayLiteral
        | K::ArrayWithCapacity
        | K::ArraySpreadLiteral(_)
        | K::SetFromSource(_)
        | K::MapFromSource
        | K::Template(_)
        | K::MakeClosure(_)
        | K::AsyncHandleCreate(_)
        | K::AsyncAll
        | K::AsyncRace
        | K::HostCompletion { .. }
        | K::TaskGroup(_)
        | K::AsyncHandleRelease
        | K::AsyncHandleArrayRelease
        | K::Throw
        | K::CatchEntry
        | K::ExceptionPark
        | K::ExceptionResume
        | K::ExceptionMessage
        | K::ExceptionPosition
        | K::ExceptionRestore
        | K::FinalizerEnter(_)
        | K::GeneratorClose
        | K::AwaitRaise => Can,
    }
}

/// Returns the fact of one terminator (`compiler.md` §342 rule 10). A
/// suspension can allocate: the scheduler can create and start a child.
#[must_use]
pub fn terminator_can_allocate(terminator: &l::Terminator) -> bool {
    match terminator {
        l::Terminator::Branch(_)
        | l::Terminator::ConditionalBranch { .. }
        | l::Terminator::Switch { .. }
        | l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_) => false,
        l::Terminator::Suspend { .. } => true,
    }
}

/// The allocation facts of one module (`compiler.md` §342 rule 10).
#[derive(Debug, Clone, Default)]
pub struct AllocationFacts {
    /// Per function, in function order.
    functions: Vec<bool>,
    /// Per function, per block in block order, per instruction.
    instructions: Vec<Vec<Vec<bool>>>,
    /// Per function, per block in block order: the terminator.
    terminators: Vec<Vec<bool>>,
}

impl AllocationFacts {
    /// Computes the facts of each instruction and each function.
    ///
    /// A function can allocate if it has an instruction or a terminator
    /// that can allocate, or a call to a function that can allocate.
    #[must_use]
    pub fn of_module(module: &l::Module) -> Self {
        let methods = MethodBodies::of_module(module);
        let count = module.functions.len();
        let local: Vec<Vec<Vec<InstructionAllocation>>> = module
            .functions
            .iter()
            .map(|function| {
                function
                    .blocks
                    .iter()
                    .map(|block| {
                        block
                            .instructions
                            .iter()
                            .map(|instruction| {
                                instruction_allocation(module, &methods, function, instruction)
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        let terminators: Vec<Vec<bool>> = module
            .functions
            .iter()
            .map(|function| {
                function
                    .blocks
                    .iter()
                    .map(|block| terminator_can_allocate(&block.terminator))
                    .collect()
            })
            .collect();
        // The least fixpoint: a function can allocate if it allocates
        // itself, or if a callee can allocate.
        let mut functions = vec![false; count];
        let mut callers: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut work = Vec::new();
        for (index, blocks) in local.iter().enumerate() {
            let mut itself = terminators[index].iter().any(|fact| *fact);
            for fact in blocks.iter().flatten() {
                match fact {
                    InstructionAllocation::Never => {}
                    InstructionAllocation::Can => itself = true,
                    InstructionAllocation::Callee(callee) => {
                        match callers.get_mut(callee.0 as usize) {
                            Some(list) => list.push(index),
                            None => itself = true,
                        }
                    }
                }
            }
            if itself {
                functions[index] = true;
                work.push(index);
            }
        }
        while let Some(callee) = work.pop() {
            for &caller in &callers[callee] {
                if !functions[caller] {
                    functions[caller] = true;
                    work.push(caller);
                }
            }
        }
        let instructions = local
            .iter()
            .map(|blocks| {
                blocks
                    .iter()
                    .map(|facts| {
                        facts
                            .iter()
                            .map(|fact| match fact {
                                InstructionAllocation::Never => false,
                                InstructionAllocation::Can => true,
                                InstructionAllocation::Callee(callee) => {
                                    functions.get(callee.0 as usize).copied().unwrap_or(true)
                                }
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        Self {
            functions,
            instructions,
            terminators,
        }
    }

    /// Returns true if a call to the function can allocate. An unknown
    /// function can allocate.
    #[must_use]
    pub fn function_can_allocate(&self, function: l::FunctionId) -> bool {
        self.functions
            .get(function.0 as usize)
            .copied()
            .unwrap_or(true)
    }

    /// Returns true if the instruction at `index` of the block can allocate,
    /// with the facts of its callees. An unknown position can allocate.
    #[must_use]
    pub fn instruction_can_allocate(
        &self,
        function: l::FunctionId,
        block: l::BlockId,
        index: usize,
    ) -> bool {
        self.instructions
            .get(function.0 as usize)
            .and_then(|blocks| blocks.get(block.0 as usize))
            .and_then(|facts| facts.get(index))
            .copied()
            .unwrap_or(true)
    }

    /// Returns true if an instruction or the terminator of the block can
    /// allocate. An unknown block can allocate.
    #[must_use]
    pub fn block_can_allocate(&self, function: l::FunctionId, block: l::BlockId) -> bool {
        let (function, block) = (function.0 as usize, block.0 as usize);
        let instructions = self
            .instructions
            .get(function)
            .and_then(|blocks| blocks.get(block));
        let terminator = self
            .terminators
            .get(function)
            .and_then(|blocks| blocks.get(block));
        match (instructions, terminator) {
            (Some(facts), Some(terminator)) => *terminator || facts.iter().any(|fact| *fact),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    fn lower(source: &str) -> l::Module {
        let hir = check_program(&[SourceFile::new("allocation.ts", source)]).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    fn function<'m>(module: &'m l::Module, name: &str) -> &'m l::Function {
        module
            .functions
            .iter()
            .find(|function| function.source_name == name)
            .unwrap_or_else(|| panic!("function {name} is missing"))
    }

    /// The facts of the instructions of `name` whose kind `select` accepts.
    fn facts(
        module: &l::Module,
        name: &str,
        select: fn(&l::InstructionKind) -> bool,
    ) -> Vec<InstructionAllocation> {
        let methods = MethodBodies::of_module(module);
        let function = function(module, name);
        let facts: Vec<_> = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| select(&instruction.kind))
            .map(|instruction| instruction_allocation(module, &methods, function, instruction))
            .collect();
        assert!(!facts.is_empty(), "{name} has no selected instruction");
        facts
    }

    const SAMPLE: &str = r#"
class Point { x: i32 = 0; }
function add(a: i32, b: i32): i32 { return a + b; }
function same(a: string, b: string): boolean { return a === b; }
function join(a: string, b: string): string { return a + b; }
function make(): Point { return new Point(); }
function count(xs: i32[]): i32 { return xs.length; }
function find(s: string): i32 { return s.indexOf("a"); }
function upper(s: string): string { return s.toUpperCase(); }
function last(xs: i32[]): i32 { xs.pop(); return 0; }
function grow(xs: i32[]): void { xs.push(1); }
function outer(s: string): i32 { return join(s, s).length; }
function even(n: i32): boolean { if (n == 0) { return true; } return odd(n - 1); }
function odd(n: i32): boolean { if (n == 0) { return false; } return even(n - 1); }
function ping(n: i32): i32 { if (n == 0) { return 0; } return pong(n - 1); }
function pong(n: i32): i32 { const xs: i32[] = [n]; return ping(n - 1) + xs.length; }
function apply(f: (x: i32) => i32): i32 { return f(1); }
export function main(): void {
  print(`${add(1, 2)} ${same("a", "b")} ${make().x} ${count([1])} ${find("ab")}`);
  print(`${upper("a")} ${last([1])} ${outer("a")} ${even(4)} ${ping(3)}`);
  grow([1]);
  print(`${apply((x: i32): i32 => x + 1)}`);
}
"#;

    #[test]
    fn instruction_facts_cover_both_sides() {
        use InstructionAllocation::{Can, Never};
        let module = lower(SAMPLE);
        let binary = |kind: &l::InstructionKind| matches!(kind, l::InstructionKind::Binary(_));
        let call = |kind: &l::InstructionKind| matches!(kind, l::InstructionKind::Call(_));
        assert_eq!(facts(&module, "add", binary), vec![Never]);
        assert_eq!(facts(&module, "same", binary), vec![Never]);
        assert_eq!(facts(&module, "join", binary), vec![Can]);
        assert!(facts(&module, "make", |kind| {
            matches!(kind, l::InstructionKind::AllocateClass(_))
        })
        .iter()
        .all(|fact| *fact == Can));
        assert!(facts(&module, "count", |kind| matches!(
            kind,
            l::InstructionKind::Length
        ))
        .iter()
        .all(|fact| *fact == Never));
        assert_eq!(facts(&module, "find", call), vec![Never]);
        assert_eq!(facts(&module, "upper", call), vec![Can]);
        assert_eq!(facts(&module, "last", call), vec![Never]);
        assert_eq!(facts(&module, "grow", call), vec![Can]);
        assert_eq!(facts(&module, "apply", call), vec![Can]);
        let join = function(&module, "join").id;
        assert_eq!(
            facts(&module, "outer", call),
            vec![InstructionAllocation::Callee(join)]
        );
    }

    #[test]
    fn a_reference_class_to_its_nullable_type_cannot_allocate() {
        use InstructionAllocation::{Can, Never};
        let module = lower(
            "class P { x: i32 = 0; }\n\
             function wrap(p: P): P | null { return p; }\n\
             function text(s: string): string | null { return s; }\n\
             export function main(): void {\n\
               const p: P | null = wrap(new P());\n\
               const s: string | null = text(\"a\");\n\
               print(`${p !== null} ${s !== null}`);\n\
             }",
        );
        let coerce = |kind: &l::InstructionKind| *kind == l::InstructionKind::Coerce;
        assert_eq!(facts(&module, "wrap", coerce), vec![Never]);
        // The rule names a reference class only: a string keeps the fact
        // of a result that is not scalar.
        assert_eq!(facts(&module, "text", coerce), vec![Can]);
    }

    #[test]
    fn terminator_facts_cover_both_sides() {
        let pos = Pos::new("t.ts", 1, 1);
        assert!(!terminator_can_allocate(&l::Terminator::Unreachable {
            pos: pos.clone()
        }));
        assert!(!terminator_can_allocate(&l::Terminator::Trap(l::Trap {
            kind: l::TrapKind::Call,
            pos,
        })));
    }

    #[test]
    fn suspensions_can_allocate() {
        let module = lower(
            "async function child(): Promise<i32> { return 1; }\n\
             async function parent(): Promise<i32> { return await child(); }\n\
             export async function main(): Promise<void> { print(`${await parent()}`); }",
        );
        let suspends = function(&module, "parent")
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
            .count();
        assert!(suspends > 0);
        assert!(function(&module, "parent")
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
            .all(|block| terminator_can_allocate(&block.terminator)));
    }

    #[test]
    fn function_facts_follow_the_call_graph() {
        let module = lower(SAMPLE);
        let facts = AllocationFacts::of_module(&module);
        let can = |name: &str| facts.function_can_allocate(function(&module, name).id);
        // No instruction allocates.
        assert!(!can("add"));
        assert!(!can("last"));
        // Only the callee allocates.
        assert!(can("outer"));
        // A recursive pair with no allocation, and one with an allocation
        // in one member.
        assert!(!can("even"));
        assert!(!can("odd"));
        assert!(can("ping"));
        assert!(can("pong"));
        // An indirect call can allocate.
        assert!(can("apply"));
        // An unknown function can allocate.
        assert!(facts.function_can_allocate(l::FunctionId(u32::MAX)));
    }

    #[test]
    fn block_and_instruction_facts_include_callee_facts() {
        let module = lower(SAMPLE);
        let facts = AllocationFacts::of_module(&module);
        for name in ["outer", "even"] {
            let function = function(&module, name);
            let expected = facts.function_can_allocate(function.id);
            let any_block = function
                .blocks
                .iter()
                .any(|block| facts.block_can_allocate(function.id, block.id));
            assert_eq!(any_block, expected, "{name}");
            let any_instruction = function.blocks.iter().any(|block| {
                (0..block.instructions.len())
                    .any(|index| facts.instruction_can_allocate(function.id, block.id, index))
            });
            assert_eq!(any_instruction, expected, "{name}");
        }
    }
}
