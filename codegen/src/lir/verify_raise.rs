//! Verification of raise sites and handler edges (`compiler.md` §115.6
//! rule 2).
//!
//! A raise site is decided here from LIR facts alone: a `throw`, a call
//! to a target whose function carries `can_raise`, an indirect call, a
//! built-in call that calls a script callback, and an `await` of a held
//! handle or of a direct call to a target that carries `can_raise`. The
//! lowering decides the same question from HIR trap sites, so the two
//! derivations are separate (CLAUDE.md core principle 9).

use super::verify::finding;
use super::*;

pub(super) fn verify_raise_edges(
    module: &l::Module,
    function: &l::Function,
    errors: &mut Vec<VerifyError>,
) {
    let mut handler_sources = HashMap::<l::BlockId, Vec<(l::BlockId, usize)>>::new();
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let site = format!("block {} instruction {index}", block.id.0);
            let edges = instruction
                .traps
                .iter()
                .filter(|trap| matches!(trap.kind, l::TrapKind::Raise(_)))
                .count();
            if edges > 1 {
                errors.push(finding(
                    function,
                    format!("{site} carries {edges} raise edges"),
                ));
            }
            if edges == 0 && is_raise_site(module, function, instruction) {
                errors.push(finding(
                    function,
                    format!(
                        "{site} is a raise site with no raise edge: kind={:?}",
                        instruction.kind
                    ),
                ));
            }
            // A generator body takes the propagate exit to its own
            // boundary, which converts the exception into a trap
            // (compiler.md §115.4 item 3). An `async` body completes its
            // handle, and an `await` raises it (§116.1 rules 1 and 2), so an
            // async function propagates as every other function does and
            // must carry `can_raise`.
            if matches!(instruction.raise_edge(), Some(l::RaiseEdge::Propagate))
                && !function.can_raise
                && !function.is_generator
            {
                errors.push(finding(
                    function,
                    format!("{site} propagates an exception from a function that cannot raise"),
                ));
            }
            if starts_handler(instruction) && index != 0 {
                errors.push(finding(
                    function,
                    format!("{site} is a catch entry that does not start its block"),
                ));
            }
            if let Some(handler) = instruction.handler() {
                handler_sources
                    .entry(handler)
                    .or_default()
                    .push((block.id, index));
            }
        }
    }
    for block in &function.blocks {
        for target in block.terminator.successors() {
            let catches = function
                .blocks
                .get(target.0 as usize)
                .and_then(|target| target.instructions.first())
                .is_some_and(starts_handler);
            if catches {
                errors.push(finding(
                    function,
                    format!(
                        "block {} reaches catch-entry block {} by an ordinary edge",
                        block.id.0, target.0
                    ),
                ));
            }
        }
    }
    for (handler, sources) in handler_sources {
        verify_handler(function, handler, &sources, errors);
    }
    verify_await_raises(module, function, errors);
    // compiler.md §115.5 rule 2: only the raise edge of an exit hook
    // reaches the trap for a hook that raises during an exit.
    for block in &function.blocks {
        let l::Terminator::Trap(trap) = &block.terminator else {
            continue;
        };
        let catches = block
            .instructions
            .first()
            .is_some_and(|first| matches!(first.kind, l::InstructionKind::CatchEntry));
        if trap.kind == l::TrapKind::DisposeRaisedDuringExit && !catches {
            errors.push(finding(
                function,
                format!(
                    "block {} traps with dispose-raised-during-exit without a catch entry",
                    block.id.0
                ),
            ));
        }
    }
}

/// Every `await` that can raise starts its resume successor with its raise
/// site, and an `AwaitRaise` stands nowhere else (`compiler.md` §116.2
/// rule 3). An `await` can raise when it awaits a held handle, or a direct
/// call to a target that carries `can_raise`.
fn verify_await_raises(module: &l::Module, function: &l::Function, errors: &mut Vec<VerifyError>) {
    let mut required = HashSet::new();
    for block in &function.blocks {
        let l::Terminator::Suspend {
            kind, successor, ..
        } = &block.terminator
        else {
            continue;
        };
        let raises = match kind {
            l::SuspendKind::AsyncHandle { .. } => true,
            l::SuspendKind::AsyncCall { target, .. } => target_can_raise(module, target),
            l::SuspendKind::Yield(_) | l::SuspendKind::Async => false,
        };
        if raises {
            required.insert(*successor);
        }
    }
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            if !matches!(instruction.kind, l::InstructionKind::AwaitRaise) {
                continue;
            }
            if index != 0 || !required.contains(&block.id) {
                errors.push(finding(
                    function,
                    format!(
                        "block {} instruction {index} is an await raise site outside the start of a raising await's resume successor",
                        block.id.0
                    ),
                ));
            }
        }
        let starts = block
            .instructions
            .first()
            .is_some_and(|first| matches!(first.kind, l::InstructionKind::AwaitRaise));
        if required.contains(&block.id) && !starts {
            errors.push(finding(
                function,
                format!(
                    "block {} resumes an await that can raise and does not start with its raise site",
                    block.id.0
                ),
            ));
        }
    }
}

/// Whether the function of a direct call target carries `can_raise`.
fn target_can_raise(module: &l::Module, target: &l::CallTarget) -> bool {
    let function_can_raise = |id: l::FunctionId| {
        module
            .functions
            .get(id.0 as usize)
            .is_some_and(|callee| callee.can_raise)
    };
    match &target.kind {
        l::CallTargetKind::Function(id) | l::CallTargetKind::StaticClosure(id) => {
            function_can_raise(*id)
        }
        l::CallTargetKind::Method(method) => module
            .classes
            .iter()
            .flat_map(|class| class.constructor.iter().chain(&class.methods))
            .find(|candidate| candidate.id == *method)
            .is_some_and(|candidate| function_can_raise(candidate.function)),
        l::CallTargetKind::Indirect => true,
        l::CallTargetKind::Intrinsic(_)
        | l::CallTargetKind::BuiltinMethod(_)
        | l::CallTargetKind::Foreign(_) => false,
    }
}

/// Whether `instruction` is the first instruction of a handler block: a
/// catch entry, or the park of an exception exit (`compiler.md` §115.5
/// rule 7).
fn starts_handler(instruction: &l::Instruction) -> bool {
    matches!(
        instruction.kind,
        l::InstructionKind::CatchEntry | l::InstructionKind::ExceptionPark
    )
}

fn verify_handler(
    function: &l::Function,
    handler: l::BlockId,
    sources: &[(l::BlockId, usize)],
    errors: &mut Vec<VerifyError>,
) {
    let Some(block) = function
        .blocks
        .get(handler.0 as usize)
        .filter(|block| block.id == handler)
    else {
        errors.push(finding(
            function,
            format!("raise edge names missing handler block {}", handler.0),
        ));
        return;
    };
    if !block.instructions.first().is_some_and(starts_handler) {
        errors.push(finding(
            function,
            format!(
                "handler block {} does not start with a catch entry",
                handler.0
            ),
        ));
    }
    if !block.parameters.is_empty() {
        errors.push(finding(
            function,
            format!(
                "handler block {} has block parameters; a raise edge carries none",
                handler.0
            ),
        ));
    }
    if sources.len() != 1 {
        errors.push(finding(
            function,
            format!(
                "handler block {} is the target of {} raise sites",
                handler.0,
                sources.len()
            ),
        ));
    }
    // The edge leaves after the raise site, so the handler reads no value
    // that its raising block defines at or after the site.
    let uses = block
        .instructions
        .iter()
        .flat_map(|instruction| &instruction.operands)
        .filter_map(|operand| match operand {
            l::Operand::Value(value) => Some(*value),
            l::Operand::Constant(_) => None,
        })
        .chain(block.terminator.value_uses());
    for value in uses {
        for (source, site) in sources {
            let Some(raising) = function.blocks.get(source.0 as usize) else {
                continue;
            };
            let late = raising
                .instructions
                .iter()
                .enumerate()
                .any(|(index, instruction)| index >= *site && instruction.result == Some(value));
            if late {
                errors.push(finding(
                    function,
                    format!(
                        "handler block {} reads value {} that block {} defines at or after its raise site",
                        handler.0, value.0, source.0
                    ),
                ));
            }
        }
    }
}

/// Whether `instruction` can leave an exception pending.
fn is_raise_site(
    module: &l::Module,
    _function: &l::Function,
    instruction: &l::Instruction,
) -> bool {
    let target = match &instruction.kind {
        l::InstructionKind::Throw
        | l::InstructionKind::ExceptionResume
        | l::InstructionKind::AwaitRaise => return true,
        l::InstructionKind::Call(target) => target,
        _ => return false,
    };
    match &target.kind {
        l::CallTargetKind::Intrinsic(intrinsic) => module
            .intrinsic_operations
            .iter()
            .find(|op| op.family == intrinsic.family && op.operation == intrinsic.operation)
            .is_some_and(|op| match op.family {
                l::IntrinsicFamily::Array => matches!(
                    op.semantic_name.as_str(),
                    "ForEach"
                        | "Map"
                        | "Filter"
                        | "Reduce"
                        | "ReduceRight"
                        | "Some"
                        | "Every"
                        | "FindIndex"
                        | "Find"
                        | "FindLast"
                        | "FindLastIndex"
                        | "FlatMap"
                        | "Sort"
                ),
                l::IntrinsicFamily::Map => {
                    matches!(op.semantic_name.as_str(), "ForEach" | "GroupBy")
                }
                l::IntrinsicFamily::Set => op.semantic_name == "ForEach",
                _ => false,
            }),
        l::CallTargetKind::BuiltinMethod(_) => false,
        _ => target_can_raise(module, target),
    }
}

#[cfg(test)]
mod tests {
    use subscript_compiler::lir as l;
    use subscript_compiler::{check_program, SourceFile};

    use crate::lir::{lower_module, verify_module};

    const PROGRAM: &str = "function fail(): void {\n\
                           \x20 throw new Error(\"x\");\n\
                           }\n\
                           function quiet(): void {\n\
                           \x20 print(\"quiet\");\n\
                           }\n\
                           export function main(): void {\n\
                           \x20 try {\n\
                           \x20   fail();\n\
                           \x20 } catch (e) {\n\
                           \x20   print(\"caught\");\n\
                           \x20 }\n\
                           \x20 quiet();\n\
                           }\n";

    fn lowered() -> l::Module {
        let hir = check_program(&[SourceFile::new("raise.ts", PROGRAM)]).expect("checks clean");
        lower_module(&hir).expect("lowers and verifies")
    }

    fn main_index(module: &l::Module) -> usize {
        module
            .functions
            .iter()
            .position(|function| function.source_name == "main")
            .expect("main")
    }

    /// The block and instruction of main's call to `fail`.
    fn raise_site(module: &l::Module) -> (usize, usize) {
        let main = &module.functions[main_index(module)];
        main.blocks
            .iter()
            .enumerate()
            .find_map(|(block, candidate)| {
                candidate
                    .instructions
                    .iter()
                    .position(|instruction| instruction.handler().is_some())
                    .map(|instruction| (block, instruction))
            })
            .expect("a raise site with a handler")
    }

    fn findings(module: &l::Module) -> String {
        verify_module(module)
            .expect_err("the violating form fails verification")
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_lowered_form_verifies() {
        let module = lowered();
        assert!(verify_module(&module).is_ok());
        let (block, instruction) = raise_site(&module);
        let main = &module.functions[main_index(&module)];
        let handler = main.blocks[block].instructions[instruction]
            .handler()
            .expect("handler");
        assert!(matches!(
            main.blocks[handler.0 as usize].instructions[0].kind,
            l::InstructionKind::CatchEntry
        ));
    }

    #[test]
    fn a_raise_site_with_no_edge_is_rejected() {
        let mut module = lowered();
        let main = main_index(&module);
        let (block, instruction) = raise_site(&module);
        module.functions[main].blocks[block].instructions[instruction]
            .traps
            .retain(|trap| !matches!(trap.kind, l::TrapKind::Raise(_)));
        let findings = findings(&module);
        assert!(
            findings.contains("is a raise site with no raise edge"),
            "{findings}"
        );
    }

    #[test]
    fn a_throw_with_no_edge_is_rejected() {
        let mut module = lowered();
        let fail = module
            .functions
            .iter()
            .position(|function| function.source_name == "fail")
            .expect("fail");
        for block in &mut module.functions[fail].blocks {
            for instruction in &mut block.instructions {
                if matches!(instruction.kind, l::InstructionKind::Throw) {
                    instruction.traps.clear();
                }
            }
        }
        let findings = findings(&module);
        assert!(
            findings.contains("is a raise site with no raise edge: kind=Throw"),
            "{findings}"
        );
    }

    #[test]
    fn a_handler_without_a_catch_entry_is_rejected() {
        let mut module = lowered();
        let main = main_index(&module);
        let (block, instruction) = raise_site(&module);
        let handler = module.functions[main].blocks[block].instructions[instruction]
            .handler()
            .expect("handler");
        module.functions[main].blocks[handler.0 as usize]
            .instructions
            .remove(0);
        let findings = findings(&module);
        assert!(
            findings.contains("does not start with a catch entry"),
            "{findings}"
        );
    }

    #[test]
    fn an_edge_to_a_missing_or_ordinary_block_is_rejected() {
        let mut module = lowered();
        let main = main_index(&module);
        let (block, instruction) = raise_site(&module);
        let missing = l::BlockId(module.functions[main].blocks.len() as u32);
        for trap in &mut module.functions[main].blocks[block].instructions[instruction].traps {
            if matches!(trap.kind, l::TrapKind::Raise(_)) {
                trap.kind = l::TrapKind::Raise(l::RaiseEdge::Handler(missing));
            }
        }
        let missing_findings = findings(&module);
        assert!(
            missing_findings.contains(&format!(
                "raise edge names missing handler block {}",
                missing.0
            )),
            "{missing_findings}"
        );

        let mut module = lowered();
        let entry = module.functions[main].entry;
        let (block, instruction) = raise_site(&module);
        for trap in &mut module.functions[main].blocks[block].instructions[instruction].traps {
            if matches!(trap.kind, l::TrapKind::Raise(_)) {
                trap.kind = l::TrapKind::Raise(l::RaiseEdge::Handler(entry));
            }
        }
        let findings = findings(&module);
        assert!(
            findings.contains("does not start with a catch entry"),
            "{findings}"
        );
    }

    #[test]
    fn a_catch_entry_reached_by_an_ordinary_edge_is_rejected() {
        let mut module = lowered();
        let main = main_index(&module);
        let (block, instruction) = raise_site(&module);
        let handler = module.functions[main].blocks[block].instructions[instruction]
            .handler()
            .expect("handler");
        let entry = module.functions[main].entry.0 as usize;
        module.functions[main].blocks[entry].terminator = l::Terminator::Branch(l::BlockTarget {
            block: handler,
            arguments: Vec::new(),
        });
        let findings = findings(&module);
        assert!(
            findings.contains(&format!(
                "reaches catch-entry block {} by an ordinary edge",
                handler.0
            )),
            "{findings}"
        );
    }

    #[test]
    fn a_propagating_site_in_a_function_that_cannot_raise_is_rejected() {
        let mut module = lowered();
        let quiet = module
            .functions
            .iter()
            .position(|function| function.source_name == "quiet")
            .expect("quiet");
        assert!(!module.functions[quiet].can_raise);
        let call = module.functions[quiet]
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction.kind, l::InstructionKind::Call(_)))
            .expect("the print call");
        let pos = call.pos.clone();
        call.traps.push(l::Trap {
            kind: l::TrapKind::Raise(l::RaiseEdge::Propagate),
            pos,
        });
        let findings = findings(&module);
        assert!(
            findings.contains("propagates an exception from a function that cannot raise"),
            "{findings}"
        );
    }

    #[test]
    fn a_generator_propagates_to_its_boundary_and_an_async_body_to_its_handle() {
        let hir = check_program(&[SourceFile::new(
            "boundary.ts",
            "function fail(): void { throw new Error(\"x\"); }\n\
             async function load(): Promise<i32> { fail(); return 1; }\n\
             function* numbers(): Generator<i32> { fail(); yield 1; }\n\
             export async function main(): Promise<void> {\n\
             \x20 for (const n of numbers()) { print(`${n}`); }\n\
             \x20 print(`${await load()}`);\n\
             }\n",
        )])
        .expect("checks clean");
        let module = lower_module(&hir).expect("lowers and verifies");
        let find = |name: &str| {
            module
                .functions
                .iter()
                .position(|function| function.source_name == name)
                .expect("coroutine")
        };
        let propagates = |function: &l::Function| {
            function.blocks.iter().any(|block| {
                block.instructions.iter().any(|instruction| {
                    matches!(instruction.raise_edge(), Some(l::RaiseEdge::Propagate))
                })
            })
        };
        let (load, numbers) = (find("load"), find("numbers"));
        assert!(
            module.functions[load].can_raise,
            "compiler.md §116.1 rule 1: an async body raises to its awaiter"
        );
        assert!(
            !module.functions[numbers].can_raise,
            "a generator never raises to its consumer"
        );
        assert!(propagates(&module.functions[load]));
        assert!(propagates(&module.functions[numbers]));
        // The firing controls: the same propagating site in an async
        // function without the fact, or in a plain function, is rejected.
        let mut changed = module.clone();
        changed.functions[load].can_raise = false;
        let async_findings = findings(&changed);
        assert!(
            async_findings.contains("propagates an exception from a function that cannot raise"),
            "{async_findings}"
        );
        let mut changed = module;
        changed.functions[numbers].is_generator = false;
        let plain_findings = findings(&changed);
        assert!(
            plain_findings.contains("propagates an exception from a function that cannot raise"),
            "{plain_findings}"
        );
    }

    const AWAIT_PROGRAM: &str = "async function fail(): Promise<i32> { throw new Error(\"x\"); }\n\
                                 async function quiet(): Promise<i32> { return 1; }\n\
                                 export async function main(): Promise<void> {\n\
                                 \x20 const held: Promise<i32> = quiet();\n\
                                 \x20 try {\n\
                                 \x20   print(`${await fail()}`);\n\
                                 \x20 } catch {\n\
                                 \x20   print(\"caught\");\n\
                                 \x20 }\n\
                                 \x20 print(`${await quiet()} ${await held}`);\n\
                                 }\n";

    fn await_lowered() -> (l::Module, usize) {
        let hir =
            check_program(&[SourceFile::new("await.ts", AWAIT_PROGRAM)]).expect("checks clean");
        let module = lower_module(&hir).expect("lowers and verifies");
        let main = main_index(&module);
        (module, main)
    }

    /// The resume successors of main's three awaits, in block order, with
    /// whether each starts with an `AwaitRaise`.
    fn await_successors(function: &l::Function) -> Vec<(l::BlockId, bool)> {
        function
            .blocks
            .iter()
            .filter_map(|block| match &block.terminator {
                l::Terminator::Suspend {
                    kind: l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. },
                    successor,
                    ..
                } => Some(*successor),
                _ => None,
            })
            .map(|successor| {
                let starts = function.blocks[successor.0 as usize]
                    .instructions
                    .first()
                    .is_some_and(|first| matches!(first.kind, l::InstructionKind::AwaitRaise));
                (successor, starts)
            })
            .collect()
    }

    #[test]
    fn an_await_that_can_raise_starts_its_resume_with_its_raise_site() {
        let (module, main) = await_lowered();
        let function = &module.functions[main];
        let successors = await_successors(function);
        assert_eq!(
            successors
                .iter()
                .map(|(_, starts)| *starts)
                .collect::<Vec<_>>(),
            [true, false, true],
            "a raising direct await, a quiet direct await, and a held await"
        );
        let first = &function.blocks[successors[0].0 .0 as usize].instructions[0];
        assert!(
            matches!(first.raise_edge(), Some(l::RaiseEdge::Handler(_))),
            "the await inside the `try` block takes its handler edge"
        );
        let held = &function.blocks[successors[2].0 .0 as usize].instructions[0];
        let Some(l::RaiseEdge::Handler(landing)) = held.raise_edge() else {
            panic!("the held await must release its owner on the exception edge");
        };
        let cleanup = &function.blocks[landing.0 as usize];
        assert!(cleanup
            .instructions
            .iter()
            .any(|i| matches!(i.kind, l::InstructionKind::AsyncHandleRelease)));
        assert_eq!(
            cleanup.instructions.last().and_then(|i| i.raise_edge()),
            Some(&l::RaiseEdge::Propagate)
        );
        assert!(function.can_raise);
    }

    #[test]
    fn an_await_without_its_raise_site_is_rejected() {
        let (mut module, main) = await_lowered();
        let (successor, _) = await_successors(&module.functions[main])[2];
        module.functions[main].blocks[successor.0 as usize]
            .instructions
            .remove(0);
        let findings = findings(&module);
        assert!(
            findings.contains(&format!(
                "block {} resumes an await that can raise and does not start with its raise site",
                successor.0
            )),
            "{findings}"
        );
    }

    #[test]
    fn an_await_raise_site_elsewhere_is_rejected() {
        let (mut module, main) = await_lowered();
        let (quiet, _) = await_successors(&module.functions[main])[1];
        let pos = module.functions[main].pos.clone();
        let pos_copy = pos.clone();
        module.functions[main].blocks[quiet.0 as usize]
            .instructions
            .insert(
                0,
                l::Instruction {
                    result: None,
                    kind: l::InstructionKind::AwaitRaise,
                    operands: Vec::new(),
                    invalidates: Vec::new(),
                    traps: vec![l::Trap {
                        kind: l::TrapKind::Raise(l::RaiseEdge::Propagate),
                        pos,
                    }],
                    pos: pos_copy,
                },
            );
        let findings = findings(&module);
        assert!(
            findings.contains(&format!(
                "block {} instruction 0 is an await raise site outside the start of a raising await's resume successor",
                quiet.0
            )),
            "{findings}"
        );
    }

    #[test]
    fn a_handle_release_without_its_check_is_rejected() {
        let (mut module, main) = await_lowered();
        let release = module.functions[main]
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction.kind, l::InstructionKind::AsyncHandleRelease))
            .expect("the release of the held handle");
        assert!(
            matches!(
                release.traps.as_slice(),
                [l::Trap {
                    kind: l::TrapKind::Call,
                    ..
                }]
            ),
            "compiler.md §116.1 rule 4: the lowered release checks the word"
        );
        release.traps.clear();
        let findings = findings(&module);
        assert!(
            findings.contains("async handle release does not carry one Call trap"),
            "{findings}"
        );
    }

    const USING_PROGRAM: &str = "class R {\n\
                                 \x20 fails: boolean;\n\
                                 \x20 constructor(fails: boolean) { this.fails = fails; }\n\
                                 \x20 [Symbol.dispose](): void {\n\
                                 \x20   if (this.fails) { throw new Error(\"hook\"); }\n\
                                 \x20 }\n\
                                 }\n\
                                 function fail(): void { throw new Error(\"x\"); }\n\
                                 function run(): void {\n\
                                 \x20 using r = new R(true);\n\
                                 \x20 fail();\n\
                                 }\n\
                                 export function main(): void {\n\
                                 \x20 try { run(); } catch { print(\"caught\"); }\n\
                                 }\n";

    fn using_lowered() -> (l::Module, usize) {
        let hir =
            check_program(&[SourceFile::new("using.ts", USING_PROGRAM)]).expect("checks clean");
        let module = lower_module(&hir).expect("lowers and verifies");
        let run = module
            .functions
            .iter()
            .position(|function| function.source_name == "run")
            .expect("run");
        (module, run)
    }

    fn block_index(
        module: &l::Module,
        function: usize,
        test: impl Fn(&l::BasicBlock) -> bool,
    ) -> usize {
        module.functions[function]
            .blocks
            .iter()
            .position(test)
            .expect("block")
    }

    #[test]
    fn an_exception_exit_parks_resumes_and_traps_a_raising_hook() {
        let (module, run) = using_lowered();
        let blocks = &module.functions[run].blocks;
        let park = blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.instructions.first().map(|first| &first.kind),
                    Some(l::InstructionKind::ExceptionPark)
                )
            })
            .count();
        assert_eq!(park, 1, "the call to `fail` lands in one park block");
        let resume = blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| matches!(instruction.kind, l::InstructionKind::ExceptionResume))
            .collect::<Vec<_>>();
        assert_eq!(resume.len(), 1);
        assert_eq!(
            resume[0].raise_edge(),
            Some(&l::RaiseEdge::Propagate),
            "the exception continues to the caller"
        );
        let traps = blocks
            .iter()
            .filter(|block| {
                matches!(&block.terminator, l::Terminator::Trap(trap)
                    if trap.kind == l::TrapKind::DisposeRaisedDuringExit)
            })
            .count();
        assert_eq!(
            traps, 1,
            "the hook call on the exit lands in one trap block"
        );
    }

    #[test]
    fn a_resume_with_no_edge_is_rejected() {
        let (mut module, run) = using_lowered();
        for block in &mut module.functions[run].blocks {
            for instruction in &mut block.instructions {
                if matches!(instruction.kind, l::InstructionKind::ExceptionResume) {
                    instruction.traps.clear();
                }
            }
        }
        let findings = findings(&module);
        assert!(
            findings.contains("is a raise site with no raise edge: kind=ExceptionResume"),
            "{findings}"
        );
    }

    #[test]
    fn a_park_block_is_a_handler_and_loses_its_status_without_the_park() {
        let (mut module, run) = using_lowered();
        let park = block_index(&module, run, |block| {
            matches!(
                block.instructions.first().map(|first| &first.kind),
                Some(l::InstructionKind::ExceptionPark)
            )
        });
        module.functions[run].blocks[park].instructions.remove(0);
        let findings = findings(&module);
        assert!(
            findings.contains(&format!(
                "handler block {park} does not start with a catch entry"
            )),
            "{findings}"
        );
    }

    #[test]
    fn a_dispose_trap_without_a_catch_entry_is_rejected() {
        let (mut module, run) = using_lowered();
        let trap = block_index(&module, run, |block| {
            matches!(&block.terminator, l::Terminator::Trap(trap)
                if trap.kind == l::TrapKind::DisposeRaisedDuringExit)
        });
        // The firing control: the lowered form verifies.
        assert!(verify_module(&module).is_ok());
        module.functions[run].blocks[trap].instructions.clear();
        let findings = findings(&module);
        assert!(
            findings.contains(&format!(
                "block {trap} traps with dispose-raised-during-exit without a catch entry"
            )),
            "{findings}"
        );
    }

    #[test]
    fn a_handler_with_a_parameter_is_rejected() {
        let mut module = lowered();
        let main = main_index(&module);
        let (block, instruction) = raise_site(&module);
        let handler = module.functions[main].blocks[block].instructions[instruction]
            .handler()
            .expect("handler");
        let function = &mut module.functions[main];
        let value = l::ValueId(function.values.len() as u32);
        function.values.push(l::Value {
            id: value,
            ty: l::ValueType::Data(subscript_compiler::Type::I32),
            fresh_owner: false,
            source_name: None,
        });
        function.blocks[handler.0 as usize].parameters.push(value);
        let findings = findings(&module);
        assert!(
            findings.contains("has block parameters; a raise edge carries none"),
            "{findings}"
        );
    }
}
