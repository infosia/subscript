//! The run-time check of the script entry fact (`compiler.md` §352 rule
//! 10).
//!
//! The fact of an instruction or a terminator comes from the LIR
//! ([`crate::lir::script_entry`]). The interpreter knows which function it
//! enters and which operation of the calling frame enters it. The two are
//! separate derivations, so a wrong fact is an `Internal` runtime error
//! here. The check reads one fact for each entry of a frame.
//!
//! [`Interpreter::execute_frame`] runs each body of a script function: a
//! call, the first start of an async function, a resume and a close of a
//! generator, and a job of the drain. Its first statement calls
//! [`Interpreter::check_script_entry`]. The calling operation is one of
//! these:
//!
//! - The await site of the parent, for the callee of an await of a direct
//!   async call. The parent suspends and leaves the frame stack before the
//!   scheduler starts the callee ([`Interpreter::start_awaited_call`]).
//! - Otherwise the instruction that the innermost active frame executes: a
//!   call, a constructor, a lambda call, a default-parameter helper, an
//!   async handle creation, a callback of a runtime operation, a resume
//!   with `next()`, and a close of a generator. Each of them runs the body
//!   while the calling frame is on the frame stack.
//! - No script frame. The frame stack is empty only in a host call of the
//!   driver ([`Interpreter::run`]): the module initializer, the entry call
//!   of `main`, the kick of each other async export, and a job of a
//!   checkpoint drain. Each of these is a call from the host, so no fact
//!   of the module covers it, and the check does not apply.

use super::*;
use crate::lir::script_entry::{
    instruction_script_entry, terminator_script_entry, ScriptEntry, ScriptMethods,
};

/// The input of the check: the facts of the module, and the await site of
/// a pending start.
pub(super) struct ScriptEntryCheck {
    methods: ScriptMethods,
    /// The functions whose address is taken: the operand of a
    /// `FunctionRef` or a `MakeClosure`.
    taken: std::collections::HashSet<l::FunctionId>,
    /// The function and the block of the await of a direct async call whose
    /// callee the scheduler starts next.
    await_site: Option<(l::FunctionId, l::BlockId)>,
    /// A map of each fact that the check reads, for the firing controls.
    #[cfg(test)]
    pub(super) map: Option<fn(ScriptEntry) -> ScriptEntry>,
}

impl ScriptEntryCheck {
    pub(super) fn of_module(module: &l::Module) -> Self {
        let taken = module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match &instruction.kind {
                l::InstructionKind::FunctionRef(id) | l::InstructionKind::MakeClosure(id) => {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        Self {
            methods: ScriptMethods::of_module(module),
            taken,
            await_site: None,
            #[cfg(test)]
            map: None,
        }
    }
}

/// The operation of the calling frame that enters a function.
enum Site<'m> {
    Instruction(&'m l::Instruction),
    Terminator(&'m l::Terminator, &'m Pos),
}

impl Interpreter<'_> {
    /// Checks that the operation of the calling frame states a fact that
    /// covers the entry of `entered` (`compiler.md` §352 rule 10). A fact
    /// that does not cover the entry is an `Internal` runtime error that
    /// names the operation, the calling function, and the entered function.
    #[inline(never)]
    pub(super) fn check_script_entry(
        &mut self,
        entered: l::FunctionId,
    ) -> Result<(), InterpretError> {
        let (caller, block, index) = match self.script_entry.await_site.take() {
            Some((caller, block)) => (caller, block, None),
            None => {
                let Some(&frame) = self.active_frames.last() else {
                    return Ok(());
                };
                // SAFETY: execute_frame registers its frame for the call's
                // duration, and the frame is not moved while it runs. The
                // fields are read once, and no reference outlives them.
                let (caller, block, index) =
                    unsafe { ((*frame).function, (*frame).block, (*frame).instruction) };
                (caller, block, Some(index))
            }
        };
        let module = self.module;
        let function = self.function(caller)?;
        let body = function
            .blocks
            .get(block.0 as usize)
            .filter(|body| body.id == block)
            .ok_or_else(|| {
                self.invalid(None, format!("entry site block b{} is missing", block.0))
            })?;
        // The frame index of an executing instruction is its index plus 1;
        // the index after the last instruction is the terminator.
        let site = match index.and_then(|index| index.checked_sub(1)) {
            Some(index) if index < body.instructions.len() => {
                Site::Instruction(&body.instructions[index])
            }
            _ => Site::Terminator(&body.terminator, &function.pos),
        };
        let fact = match site {
            Site::Instruction(instruction) => {
                instruction_script_entry(&self.script_entry.methods, function, instruction)
            }
            Site::Terminator(terminator, _) => {
                terminator_script_entry(&self.script_entry.methods, function, terminator)
            }
        };
        #[cfg(test)]
        let fact = self.script_entry.map.map_or(fact, |map| map(fact));
        let callee = module.functions.get(entered.0 as usize);
        let covered = match fact {
            ScriptEntry::Function(id) | ScriptEntry::AsyncFunction(id) => id == entered,
            ScriptEntry::AddressTaken => self.script_entry.taken.contains(&entered),
            ScriptEntry::GeneratorBody => callee.is_some_and(|callee| callee.is_generator),
            ScriptEntry::None => false,
        };
        if covered {
            return Ok(());
        }
        let (operation, pos) = match site {
            Site::Instruction(instruction) => (
                format!("instruction {}", kind_name(&instruction.kind)),
                instruction.pos.clone(),
            ),
            Site::Terminator(terminator, pos) => {
                (format!("terminator {}", kind_name(terminator)), pos.clone())
            }
        };
        let name = |id: l::FunctionId| {
            module
                .functions
                .get(id.0 as usize)
                .map_or("<missing>", |function| function.source_name.as_str())
                .to_string()
        };
        Err(InterpretError::Trap {
            kind: RuntimeTrapKind::Internal.rule().to_string(),
            runtime_kind: Some(RuntimeTrapKind::Internal),
            pos,
            message: format!(
                "{operation} of f{} `{}` states the script entry {fact:?} and entered f{} `{}` \
                 (compiler.md §352 rule 10)",
                caller.0,
                name(caller),
                entered.0,
                name(entered)
            ),
        })
    }

    /// Starts the callee of an await of a direct async call. The parent has
    /// suspended and left the frame stack, so the entry site of the callee
    /// is the await of the parent: the suspension whose successor is the
    /// current block of the parent.
    pub(super) fn start_awaited_call(
        &mut self,
        parent: &Rc<RefCell<Coroutine>>,
        child: &Rc<RefCell<Coroutine>>,
    ) -> Result<(), InterpretError> {
        let resumed = parent
            .borrow()
            .kind
            .frame()
            .map(|frame| {
                let frame = frame.borrow();
                (frame.function, frame.block)
            })
            .ok_or_else(|| self.invalid(None, "an awaiting parent is not an invocation"))?;
        let (function, successor) = resumed;
        let block = self
            .function(function)?
            .blocks
            .iter()
            .find(|block| {
                matches!(&block.terminator, l::Terminator::Suspend {
                    kind: l::SuspendKind::AsyncCall { .. },
                    successor: target,
                    ..
                } if *target == successor)
            })
            .map(|block| block.id)
            .ok_or_else(|| self.invalid(None, "an awaiting parent has no await of a call"))?;
        self.script_entry.await_site = Some((function, block));
        let outcome = self.async_start(child);
        self.script_entry.await_site = None;
        outcome
    }
}

/// The name of an instruction kind or a terminator kind, with no operand.
fn kind_name(kind: &impl fmt::Debug) -> String {
    let text = format!("{kind:?}");
    text.split(['(', ' ', '{']).next().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> l::Module {
        let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "script-entry.ts",
            source,
        )])
        .expect("the program checks");
        crate::lir::lower_module(&hir).expect("the program lowers")
    }

    /// Runs the module with each fact read through `map`.
    fn run(
        module: &l::Module,
        map: Option<fn(ScriptEntry) -> ScriptEntry>,
    ) -> Result<Vec<u8>, InterpretError> {
        let mut interpreter = Interpreter::new(module)?;
        interpreter.script_entry.map = map;
        interpreter.run()
    }

    /// The message of the `Internal` error of the check.
    fn internal(outcome: Result<Vec<u8>, InterpretError>) -> String {
        match outcome {
            Err(InterpretError::Trap {
                runtime_kind: Some(RuntimeTrapKind::Internal),
                message,
                ..
            }) if message.contains("§352 rule 10") => message,
            other => panic!("expected the Internal error of the check, got {other:?}"),
        }
    }

    const DIRECT: &str = r#"
class P { x: i32 = 0; }
function make(n: i32): P { const p: P = new P(); p.x = n; return p; }
export function main(): void { print(`${make(3).x}`); }
"#;

    const CALLBACK: &str = r#"
function each(xs: i32[], f: (x: i32) => void): void { xs.forEach(f); }
export function main(): void {
  each([1, 2], (x: i32): void => { print(`${x}`); });
}
"#;

    const RESUME: &str = r#"
function* items(): Generator<i32> { yield 1; yield 2; }
export function main(): void {
  const it: Generator<i32> = items();
  print(`${it.next().value} ${it.next().value}`);
}
"#;

    /// The control: each program runs with the facts of the module.
    #[test]
    fn true_facts_run_without_an_internal_error() {
        assert_eq!(run(&lower(DIRECT), None).expect("runs"), b"3\n");
        assert_eq!(run(&lower(CALLBACK), None).expect("runs"), b"1\n2\n");
        assert_eq!(run(&lower(RESUME), None).expect("runs"), b"1 2\n");
    }

    /// The firing control of a direct call: the fact "one function" read as
    /// "none".
    #[test]
    fn a_direct_call_with_no_fact_is_internal() {
        let message = internal(run(
            &lower(DIRECT),
            Some(|fact| match fact {
                ScriptEntry::Function(_) => ScriptEntry::None,
                other => other,
            }),
        ));
        assert!(
            message.contains("instruction Call of f")
                && message.contains("`main` states the script entry None")
                && message.contains("`make`"),
            "{message}"
        );
    }

    /// The firing control of a callback operation: `forEach` with a
    /// variable callback, with the fact "each function whose address is
    /// taken" read as "none".
    #[test]
    fn a_callback_operation_with_no_fact_is_internal() {
        let message = internal(run(
            &lower(CALLBACK),
            Some(|fact| match fact {
                ScriptEntry::AddressTaken => ScriptEntry::None,
                other => other,
            }),
        ));
        assert!(
            message.contains("instruction Call of f")
                && message.contains("`each` states the script entry None"),
            "{message}"
        );
    }

    /// The firing control of a generator resume: the fact "each generator
    /// body" read as "none".
    #[test]
    fn a_generator_resume_with_no_fact_is_internal() {
        let message = internal(run(
            &lower(RESUME),
            Some(|fact| match fact {
                ScriptEntry::GeneratorBody => ScriptEntry::None,
                other => other,
            }),
        ));
        assert!(
            message.contains("`main` states the script entry None") && message.contains("`items`"),
            "{message}"
        );
    }

    /// A fact that names another function is a wrong value too.
    #[test]
    fn a_fact_that_names_another_function_is_internal() {
        let message = internal(run(
            &lower(DIRECT),
            Some(|fact| match fact {
                ScriptEntry::Function(id) => ScriptEntry::Function(l::FunctionId(id.0 + 1000)),
                other => other,
            }),
        ));
        assert!(message.contains("`make`"), "{message}");
    }

    /// The await of a direct async call: the callee starts after the parent
    /// left the frame stack, and the check reads the await of the parent.
    #[test]
    fn an_await_of_a_direct_call_reads_the_await_site() {
        const AWAIT: &str = r#"
async function child(): Promise<i32> { return 4; }
export async function main(): Promise<void> { print(`${await child()}`); }
"#;
        let module = lower(AWAIT);
        assert_eq!(run(&module, None).expect("runs"), b"4\n");
        let message = internal(run(
            &module,
            Some(|fact| match fact {
                ScriptEntry::AsyncFunction(_) => ScriptEntry::None,
                other => other,
            }),
        ));
        assert!(
            message.contains("terminator Suspend of f")
                && message.contains("`main` states the script entry None")
                && message.contains("`child`"),
            "{message}"
        );
    }
}
