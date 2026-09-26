//! Verification of raise sites and handler edges (`compiler.md` §115.6
//! rule 2).
//!
//! A raise site is decided here from LIR facts alone: a `throw`, a call
//! to a target whose function carries `can_raise`, an indirect call, and a
//! built-in call that receives a function value. The lowering decides the
//! same question from HIR trap sites, so the two derivations are separate
//! (CLAUDE.md core principle 9).

use super::verify::{finding, operand_type};
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
            // An `async` body and a generator body take the propagate exit
            // to their own boundary, which converts the exception into a
            // trap (compiler.md §115.4 items 2 and 3). Every other function
            // propagates to its caller, so it must carry `can_raise`.
            if matches!(instruction.raise_edge(), Some(l::RaiseEdge::Propagate))
                && !function.can_raise
                && !function.is_async
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
fn is_raise_site(module: &l::Module, function: &l::Function, instruction: &l::Instruction) -> bool {
    let target = match &instruction.kind {
        l::InstructionKind::Throw | l::InstructionKind::ExceptionResume => return true,
        l::InstructionKind::Call(target) => target,
        _ => return false,
    };
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
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_) => {
            instruction.operands.iter().any(|operand| {
                matches!(
                    operand_type(function, operand),
                    Some(l::ValueType::Data(Type::Func(_)))
                )
            })
        }
        l::CallTargetKind::Foreign(_) => false,
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
    fn a_coroutine_body_propagates_to_its_own_boundary() {
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
        let mut module = lower_module(&hir).expect("lowers and verifies");
        for name in ["load", "numbers"] {
            let function = module
                .functions
                .iter()
                .find(|function| function.source_name == name)
                .expect("coroutine");
            assert!(!function.can_raise, "{name} never raises to a caller");
            assert!(
                function
                    .blocks
                    .iter()
                    .any(
                        |block| block.instructions.iter().any(|instruction| matches!(
                            instruction.raise_edge(),
                            Some(l::RaiseEdge::Propagate)
                        ))
                    ),
                "{name} holds a propagating raise site"
            );
        }
        // The firing control: the same propagating site in a plain function
        // that cannot raise is rejected.
        let load = module
            .functions
            .iter()
            .position(|function| function.source_name == "load")
            .expect("load");
        module.functions[load].is_async = false;
        let findings = findings(&module);
        assert!(
            findings.contains("propagates an exception from a function that cannot raise"),
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
