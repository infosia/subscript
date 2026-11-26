//! The script entry fact of instructions and terminators (`compiler.md`
//! §352 rule 1).
//!
//! [`instruction_script_entry`] states, by a total match over
//! [`l::InstructionKind`], which script functions of the module an
//! instruction can run before it returns. [`terminator_script_entry`] does
//! the same over [`l::Terminator`]. The placement of collection polls and
//! its verifier each build their call graph from this fact, with their own
//! graph code (§352 rule 2, §351 rule 8).
//!
//! A foreign call runs host code, and the host can call a script callback
//! that the program registered with it, before the foreign call returns.
//! Such a callback is a function value, so its address is taken: the fact
//! of a foreign call is [`ScriptEntry::AddressTaken`], also with no
//! function-value operand (a host keeps a callback from an earlier
//! registration). A host completion with a foreign target calls its bound C
//! function inside the instruction, so its fact is
//! [`ScriptEntry::AddressTaken`] too. A host completion with a standard
//! target starts a request on the runtime provider of the Context, which
//! runs no script code: its fact is [`ScriptEntry::None`]. Each completion
//! delivers its result at a host step, after the instruction returns.
//! The fact does not cover an export that the host calls from inside a
//! foreign call: the ship tier traps a host entry during a host entry
//! (`compiler.md` §223), and that recursion is not in the call graph
//! (§351 rule 7, §351.4 item 1).
//!
//! A direct call names its callee, also when the callee is a generator or
//! an async function. A call of a generator creates its frame and runs no
//! body; an async handle creation runs the callee to its first await. The
//! body of a generator runs at a resume or a close of the generator, whose
//! fact is [`ScriptEntry::GeneratorBody`] (§352 rule 3).

use super::*;

/// The script functions of the module that one instruction or terminator
/// can run before it returns (`compiler.md` §352 rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScriptEntry {
    /// The operation runs no script function of the module.
    None,
    /// A direct call: the operation runs this function.
    Function(l::FunctionId),
    /// An indirect call, a runtime operation with a function-value
    /// operand, a foreign call, or a host completion with a foreign target:
    /// the operation can run each function whose address is taken.
    AddressTaken,
    /// A resume or a close of a generator: the operation can run the body
    /// of each generator function of the module. The handle does not name
    /// its body.
    GeneratorBody,
    /// An await of a direct async call: the suspension creates this async
    /// function and runs it to its first await or return.
    AsyncFunction(l::FunctionId),
}

/// The method bodies of a module, for the fact of a method call.
#[derive(Debug, Clone, Default)]
pub struct ScriptMethods(HashMap<l::MethodId, l::FunctionId>);

impl ScriptMethods {
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

/// True if a value of the function has a function type.
fn is_function_value(function: &l::Function, value: l::ValueId) -> bool {
    function
        .values
        .get(value.0 as usize)
        .filter(|data| data.id == value)
        .or_else(|| function.values.iter().find(|data| data.id == value))
        .is_some_and(
            |data| matches!(&data.ty, l::ValueType::Data(ty) if ty.function_type().is_some()),
        )
}

/// True if an operand of the instruction has a function type.
fn has_function_operand(function: &l::Function, operands: &[l::Operand]) -> bool {
    operands.iter().any(|operand| match operand {
        l::Operand::Value(value) => is_function_value(function, *value),
        l::Operand::Constant(constant) => constant.ty.function_type().is_some(),
    })
}

/// The fact of a runtime operation: it runs script code only through a
/// function-value operand.
fn runtime_operation(function_operand: bool) -> ScriptEntry {
    if function_operand {
        ScriptEntry::AddressTaken
    } else {
        ScriptEntry::None
    }
}

/// The fact of a call target. `function_operand` is true if an operand of
/// the call has a function type.
fn call_target_entry(
    methods: &ScriptMethods,
    target: &l::CallTarget,
    function_operand: bool,
) -> ScriptEntry {
    match &target.kind {
        l::CallTargetKind::Function(id)
        | l::CallTargetKind::LambdaSelf(id)
        | l::CallTargetKind::StaticClosure(id)
        | l::CallTargetKind::ClosureDefault { helper: id, .. } => ScriptEntry::Function(*id),
        // A method id with no body names no function of the module.
        l::CallTargetKind::Method(id) => methods
            .0
            .get(id)
            .map_or(ScriptEntry::None, |body| ScriptEntry::Function(*body)),
        // A host can call a registered script callback before it returns.
        l::CallTargetKind::Foreign(_) => ScriptEntry::AddressTaken,
        l::CallTargetKind::Indirect => ScriptEntry::AddressTaken,
        l::CallTargetKind::Intrinsic(_) => runtime_operation(function_operand),
        l::CallTargetKind::BuiltinMethod(method) => match method {
            l::BuiltinMethod::GeneratorNext => ScriptEntry::GeneratorBody,
            l::BuiltinMethod::ArrayPush
            | l::BuiltinMethod::ArrayPushReserved
            | l::BuiltinMethod::ArrayStoreKept
            | l::BuiltinMethod::ArrayTruncateKept
            | l::BuiltinMethod::ArrayPop
            | l::BuiltinMethod::ArrayClear
            | l::BuiltinMethod::StringSlice => runtime_operation(function_operand),
        },
    }
}

/// Returns the fact of one instruction (`compiler.md` §352 rule 1).
///
/// The match is total, so a new instruction kind cannot exist without its
/// fact. Each kind with the fact [`ScriptEntry::None`] runs runtime code,
/// host code, or generated code with no script statement: a template
/// formats scalars and strings, a host completion with a standard target
/// starts a request on the runtime provider, a release of an async frame or a generator
/// frame frees it and runs no body, and a lexical finalizer of a `using` or
/// a `finally` is in the LIR of the function itself.
#[must_use]
pub fn instruction_script_entry(
    methods: &ScriptMethods,
    function: &l::Function,
    instruction: &l::Instruction,
) -> ScriptEntry {
    use l::InstructionKind as K;
    match &instruction.kind {
        K::Copy
        | K::StringLiteral(_)
        | K::AliasString(_)
        | K::LoadLocal(_)
        | K::StoreLocal(_)
        | K::AddressOfLocal(_)
        | K::LoadGlobal(_)
        | K::StoreGlobal(_)
        | K::AddressOfGlobal(_)
        | K::FunctionRef(_)
        | K::Unary(_)
        | K::Binary(_)
        | K::Cast
        | K::Coerce
        | K::StringOperand
        | K::NarrowNonNull(_)
        | K::AllocateClass(_)
        | K::BoxBoundaryValue { .. }
        | K::AddressOfValue
        | K::AddressOfField(_)
        | K::AddressOfIndex { .. }
        | K::LoadAddress
        | K::StoreAddress
        | K::LoadField(_)
        | K::Length
        | K::ArrayLiteral
        | K::ArrayWithCapacity
        | K::ArraySpreadLiteral(_)
        | K::SetFromSource(_)
        | K::MapFromSource
        | K::Template(_)
        | K::MakeClosure(_)
        | K::AsyncAll
        | K::AsyncRace
        | K::HostCompletion {
            target: l::HostCompletionTarget::Standard(_),
            ..
        }
        | K::TaskGroup(_)
        | K::AsyncHandleRetain
        | K::AsyncHandleRelease
        | K::AsyncHandleArrayRetain
        | K::AsyncHandleArrayRelease
        | K::IteratorCreate { .. }
        | K::IteratorHasNext
        | K::IteratorValue
        | K::IteratorBound
        | K::IteratorAdvance
        | K::Zero
        | K::Throw
        | K::CatchEntry
        | K::ExceptionPark
        | K::ExceptionResume
        | K::ExceptionMessage
        | K::ExceptionPosition
        | K::ExceptionRestore
        | K::FinalizerEnter(_)
        | K::GeneratorIsClosing
        | K::GeneratorFinalizer(_)
        | K::AwaitRaise
        | K::CollectionPoll => ScriptEntry::None,
        K::Call(target) | K::AsyncHandleCreate(target) => call_target_entry(
            methods,
            target,
            has_function_operand(function, &instruction.operands),
        ),
        // The bound C function runs inside the instruction, and the host
        // can call a registered script callback before it returns.
        K::HostCompletion {
            target: l::HostCompletionTarget::Foreign(_),
            ..
        } => ScriptEntry::AddressTaken,
        // `HostCompletionTarget` is non-exhaustive outside its crate. A
        // target with no arm above takes the fact that covers a host call.
        K::HostCompletion { .. } => ScriptEntry::AddressTaken,
        // The close continuation of a generator runs its `finally` code.
        K::GeneratorClose => ScriptEntry::GeneratorBody,
    }
}

/// Returns the fact of one terminator (`compiler.md` §352 rule 1).
///
/// A yield returns to the resumer, and an await of a handle registers the
/// caller on a completion: neither runs script code. An await of a direct
/// async call creates its callee and runs it to its first await or return.
#[must_use]
pub fn terminator_script_entry(
    methods: &ScriptMethods,
    function: &l::Function,
    terminator: &l::Terminator,
) -> ScriptEntry {
    match terminator {
        l::Terminator::Branch(_)
        | l::Terminator::ConditionalBranch { .. }
        | l::Terminator::Switch { .. }
        | l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_) => ScriptEntry::None,
        l::Terminator::Suspend { kind, .. } => match kind {
            l::SuspendKind::Yield(_)
            | l::SuspendKind::Async
            | l::SuspendKind::AsyncHandle { .. } => ScriptEntry::None,
            l::SuspendKind::AsyncCall { target, operands } => {
                let function_operand = operands
                    .iter()
                    .any(|value| is_function_value(function, *value));
                match call_target_entry(methods, target, function_operand) {
                    ScriptEntry::Function(id) => ScriptEntry::AsyncFunction(id),
                    other => other,
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    fn lower(files: &[SourceFile]) -> l::Module {
        let hir = check_program(files).expect("checks");
        super::super::lower_module(&hir).expect("lowers")
    }

    fn function<'m>(module: &'m l::Module, name: &str) -> &'m l::Function {
        module
            .functions
            .iter()
            .find(|function| function.source_name == name)
            .unwrap_or_else(|| panic!("function {name} is missing"))
    }

    fn id(module: &l::Module, name: &str) -> l::FunctionId {
        function(module, name).id
    }

    /// The facts of the instructions of `name` whose kind `select` accepts.
    fn facts(
        module: &l::Module,
        name: &str,
        select: fn(&l::InstructionKind) -> bool,
    ) -> Vec<ScriptEntry> {
        let methods = ScriptMethods::of_module(module);
        let function = function(module, name);
        let facts: Vec<_> = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| select(&instruction.kind))
            .map(|instruction| instruction_script_entry(&methods, function, instruction))
            .collect();
        assert!(!facts.is_empty(), "{name} has no selected instruction");
        facts
    }

    /// The facts of the instructions of `name` that are not `None`.
    fn script_facts(module: &l::Module, name: &str) -> Vec<ScriptEntry> {
        facts(module, name, |_| true)
            .into_iter()
            .filter(|fact| *fact != ScriptEntry::None)
            .collect()
    }

    fn is_call(kind: &l::InstructionKind) -> bool {
        matches!(kind, l::InstructionKind::Call(_))
    }

    const SAMPLE: &str = r#"
class P {
  x: i32 = 0;
  constructor(x: i32) { this.x = x; }
  get(): i32 { return this.x; }
}
function add(a: i32, b: i32): i32 { return a + b; }
function direct(): i32 { return add(1, 2); }
function method(p: P): i32 { return p.get(); }
function construct(): P { return new P(1); }
function indirect(f: (x: i32) => i32): i32 { return f(1); }
function callback(xs: i32[], f: (x: i32) => void): void { xs.forEach(f); }
function plain(xs: i32[]): i32 { return xs.indexOf(1); }
function grow(xs: i32[]): void { xs.push(1); }
function local(xs: i32[]): i32[] { return xs.map((x: i32): i32 => x * 2); }
function selfish(): i32 {
  const down = (n: i32): i32 => { if (n > 0) return down(n - 1); return 0; };
  return down(3);
}
function defaults(base: i32): i32 {
  const b: i32 = base;
  const scale = (x: i32, k: i32 = b): i32 => x * k;
  return scale(3);
}
async function child(): Promise<i32> { return 1; }
async function created(): Promise<i32> {
  const h: Promise<i32> = child();
  return await h;
}
async function awaited(): Promise<i32> { return await child(); }
function* items(): Generator<i32> {
  try { yield 1; } finally { print("closed"); }
}
function resume(it: Generator<i32>): i32 { return it.next().value; }
function closes(): i32 {
  let s: i32 = 0;
  for (const x of items()) { s += x; break; }
  return s;
}
export async function main(): Promise<void> {
  const xs: i32[] = [1];
  callback(xs, (x: i32): void => { print(`${x}`); });
  grow(xs);
  print(`${direct()} ${method(new P(2))} ${construct().x} ${plain(xs)}`);
  print(`${indirect((x: i32): i32 => x + 1)} ${local(xs).length} ${selfish()} ${defaults(2)}`);
  print(`${await created()} ${await awaited()} ${resume(items())} ${closes()}`);
}
"#;

    fn sample() -> l::Module {
        lower(&[SourceFile::new("script-entry.ts", SAMPLE)])
    }

    #[test]
    fn a_direct_call_a_method_call_and_a_constructor_run_one_function() {
        let module = sample();
        assert_eq!(
            facts(&module, "direct", is_call),
            vec![ScriptEntry::Function(id(&module, "add"))]
        );
        assert_eq!(
            facts(&module, "method", is_call),
            vec![ScriptEntry::Function(id(&module, "get"))]
        );
        let constructor = module
            .classes
            .iter()
            .find(|class| class.source_name == "P")
            .and_then(|class| class.constructor.as_ref())
            .map(|method| method.function)
            .expect("P has a constructor");
        assert_eq!(
            script_facts(&module, "construct"),
            vec![ScriptEntry::Function(constructor)]
        );
    }

    /// Each lambda call form in the module, with the function that its
    /// target names.
    #[test]
    fn a_lambda_call_runs_its_body() {
        let module = sample();
        let methods = ScriptMethods::of_module(&module);
        let mut seen = BTreeSet::new();
        for function in &module.functions {
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                let l::InstructionKind::Call(target) = &instruction.kind else {
                    continue;
                };
                let (form, callee) = match &target.kind {
                    l::CallTargetKind::StaticClosure(callee) => ("static", *callee),
                    l::CallTargetKind::LambdaSelf(callee) => ("self", *callee),
                    l::CallTargetKind::ClosureDefault { helper, .. } => ("default", *helper),
                    _ => continue,
                };
                seen.insert(form);
                assert_eq!(
                    instruction_script_entry(&methods, function, instruction),
                    ScriptEntry::Function(callee),
                    "{form}"
                );
            }
        }
        assert_eq!(
            seen,
            BTreeSet::from(["default", "self", "static"]),
            "each lambda call form is in the sample"
        );
    }

    #[test]
    fn an_indirect_call_and_a_callback_operation_run_a_function_whose_address_is_taken() {
        let module = sample();
        assert_eq!(
            facts(&module, "indirect", is_call),
            vec![ScriptEntry::AddressTaken]
        );
        assert_eq!(
            facts(&module, "callback", is_call),
            vec![ScriptEntry::AddressTaken]
        );
    }

    #[test]
    fn a_runtime_operation_with_no_function_operand_runs_no_script() {
        let module = sample();
        // An intrinsic and a built-in method.
        assert_eq!(facts(&module, "plain", is_call), vec![ScriptEntry::None]);
        assert_eq!(facts(&module, "grow", is_call), vec![ScriptEntry::None]);
    }

    #[test]
    fn an_async_handle_creation_and_an_await_of_a_direct_call_run_the_callee() {
        let module = sample();
        let child = id(&module, "child");
        assert_eq!(
            facts(&module, "created", |kind| matches!(
                kind,
                l::InstructionKind::AsyncHandleCreate(_)
            )),
            vec![ScriptEntry::Function(child)]
        );
        let methods = ScriptMethods::of_module(&module);
        let awaited = function(&module, "awaited");
        let suspensions = awaited
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.terminator,
                    l::Terminator::Suspend {
                        kind: l::SuspendKind::AsyncCall { .. },
                        ..
                    }
                )
            })
            .map(|block| terminator_script_entry(&methods, awaited, &block.terminator))
            .collect::<Vec<_>>();
        assert_eq!(suspensions, vec![ScriptEntry::AsyncFunction(child)]);
        // An await of a held handle runs no script code.
        let created = function(&module, "created");
        let handles = created
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.terminator,
                    l::Terminator::Suspend {
                        kind: l::SuspendKind::AsyncHandle { .. },
                        ..
                    }
                )
            })
            .map(|block| terminator_script_entry(&methods, created, &block.terminator))
            .collect::<Vec<_>>();
        assert!(!handles.is_empty());
        assert!(handles.iter().all(|fact| *fact == ScriptEntry::None));
    }

    #[test]
    fn a_resume_and_a_close_of_a_generator_run_a_generator_body() {
        let module = sample();
        assert_eq!(
            facts(&module, "resume", is_call),
            vec![ScriptEntry::GeneratorBody]
        );
        // `for…of` resumes with `next()` and closes at the `break`.
        assert!(facts(&module, "closes", is_call).contains(&ScriptEntry::GeneratorBody));
        assert!(facts(&module, "closes", |kind| {
            *kind == l::InstructionKind::GeneratorClose
        })
        .iter()
        .all(|fact| *fact == ScriptEntry::GeneratorBody));
        // The close path inside the generator, and its yield, run no script
        // code of their own.
        assert!(facts(&module, "items", |kind| {
            matches!(
                kind,
                l::InstructionKind::GeneratorIsClosing | l::InstructionKind::GeneratorFinalizer(_)
            )
        })
        .iter()
        .all(|fact| *fact == ScriptEntry::None));
        let methods = ScriptMethods::of_module(&module);
        let items = function(&module, "items");
        assert!(items
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
            .all(|block| {
                terminator_script_entry(&methods, items, &block.terminator) == ScriptEntry::None
            }));
    }

    /// A foreign call can run each function whose address is taken: the
    /// host can call a callback that the program registered earlier, so a
    /// call with no function-value operand has the fact too.
    #[test]
    fn a_foreign_call_runs_a_function_whose_address_is_taken() {
        let module = lower(&[
            SourceFile::ambient(
                "host.d.ts",
                "// @subscript-c-header include=\"host.h\"\ndeclare function hostTick(): void;",
            ),
            SourceFile::new(
                "foreign.ts",
                "function tick(): void { hostTick(); }\nexport function main(): void { tick(); }",
            ),
        ]);
        let calls = facts(&module, "tick", |kind| {
            matches!(
                kind,
                l::InstructionKind::Call(l::CallTarget {
                    kind: l::CallTargetKind::Foreign(_),
                    ..
                })
            )
        });
        assert_eq!(calls, vec![ScriptEntry::AddressTaken]);
    }

    fn is_host_completion(kind: &l::InstructionKind) -> bool {
        matches!(kind, l::InstructionKind::HostCompletion { .. })
    }

    /// A host completion with a foreign target calls its C function inside
    /// the instruction, as a foreign call does. A standard target starts a
    /// request on the runtime provider, which runs no script code.
    #[test]
    fn a_host_completion_runs_script_only_through_a_foreign_target() {
        let module = lower(&[
            SourceFile::ambient(
                "host.d.ts",
                "// @subscript-c-header include=\"host.h\"\n\
                 // @subscript-c-completion function=\"hostRead\" result=\"int32_t\"\n\
                 declare function hostRead(): Promise<i32>;",
            ),
            SourceFile::new(
                "foreign.ts",
                "async function read(): Promise<i32> { return await hostRead(); }\n\
                 export async function main(): Promise<void> { print(`${await read()}`); }",
            ),
        ]);
        assert!(function(&module, "read").blocks.iter().any(|block| block
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction.kind,
                l::InstructionKind::HostCompletion {
                    target: l::HostCompletionTarget::Foreign(_),
                    ..
                }
            ))));
        assert_eq!(
            facts(&module, "read", is_host_completion),
            vec![ScriptEntry::AddressTaken]
        );

        let mut options = subscript_compiler::CheckOptions::default();
        options.enabled_modules.push("node:fs/promises".into());
        let hir = subscript_compiler::check_program_with(
            &[SourceFile::entry(
                "main.ts",
                "import {readFile} from 'node:fs/promises';\n\
                 async function load(): Promise<string> { return await readFile('a.txt', 'utf8'); }\n\
                 export async function main(): Promise<void> { print(await load()); }",
            )],
            &options,
        )
        .expect("checks");
        let module = super::super::lower_module(&hir).expect("lowers");
        assert!(function(&module, "load").blocks.iter().any(|block| block
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction.kind,
                l::InstructionKind::HostCompletion {
                    target: l::HostCompletionTarget::Standard(_),
                    ..
                }
            ))));
        assert_eq!(
            facts(&module, "load", is_host_completion),
            vec![ScriptEntry::None]
        );
    }

    #[test]
    fn plain_terminators_run_no_script() {
        let methods = ScriptMethods::default();
        let module = sample();
        let function = function(&module, "add");
        let pos = Pos::new("t.ts", 1, 1);
        for terminator in [
            l::Terminator::Unreachable { pos: pos.clone() },
            l::Terminator::Trap(l::Trap {
                kind: l::TrapKind::Call,
                pos: pos.clone(),
            }),
            l::Terminator::Return { value: None, pos },
        ] {
            assert_eq!(
                terminator_script_entry(&methods, function, &terminator),
                ScriptEntry::None
            );
        }
    }
}
