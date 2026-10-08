use super::*;

fn empty_module(mut functions: Vec<l::Function>) -> l::Module {
    for function in &mut functions {
        function.liveness = l::Liveness {
            generator_close: Vec::new(),
            generator_cleanup: Vec::new(),
            live_ins: vec![Vec::new(); function.blocks.len()],
            value_origins: function.values.iter().map(|value| value.id).collect(),
        };
    }
    l::Module {
        host_entries: Vec::new(),
        entry: Some(l::FunctionId(0)),
        async_roots: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
        string_aliases: Vec::new(),
        globals: Vec::new(),
        foreign_functions: Vec::new(),
        functions,
        worker_entries: Vec::new(),
        intrinsic_operations: Vec::new(),
        initializer: None,
    }
}

/// §94.1: a scheduled await resume whose handle carries no completion
/// is an internal protocol defect. The interpreter reports the same
/// kind, message, and suspension position as the two tiers, and it does
/// not poll, re-register, or fabricate a result.
#[test]
fn an_await_resume_without_a_completion_reports_the_internal_defect() {
    let pos = Pos::new("invalid-protocol.ts", 4, 11);
    let function = l::Function {
        id: l::FunctionId(0),
        source_name: "waiting".to_string(),
        kind: l::FunctionKind::Free,
        exported: false,
        is_generator: false,
        is_async: true,
        creation_traps: Vec::new(),
        host_entry_traps: None,
        can_raise: false,
        parameters: Vec::new(),
        return_type: Type::Void,
        locals: Vec::new(),
        values: Vec::new(),
        liveness: l::Liveness::default(),
        blocks: vec![l::BasicBlock {
            id: l::BlockId(0),
            source_name: Some("entry".to_string()),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: l::Terminator::Return {
                value: None,
                pos: pos.clone(),
            },
        }],
        entry: l::BlockId(0),
        pos: pos.clone(),
    };
    let module = empty_module(vec![function]);
    let mut interpreter = Interpreter::new(&module).expect("module is valid");
    let Value::Coroutine(waiting) = interpreter
        .call_function(l::FunctionId(0), Vec::new())
        .expect("async frame")
    else {
        panic!("an async function produces a coroutine");
    };
    let Value::Coroutine(handle) = interpreter
        .call_function(l::FunctionId(0), Vec::new())
        .expect("async frame")
    else {
        panic!("an async function produces a coroutine");
    };
    // The state a correct scheduler never builds: a ready continuation
    // whose awaited frame has not completed.
    waiting.borrow_mut().awaiting = Some(AwaitedHandle {
        handle: Rc::clone(&handle),
        pos: pos.clone(),
    });
    assert!(!handle.borrow().completed);
    let error = interpreter
        .async_resume(&waiting)
        .expect_err("the resume reports the defect");
    assert_eq!(
        error,
        InterpretError::Trap {
            kind: subscript_runtime::TrapKind::Internal.rule().to_string(),
            runtime_kind: Some(subscript_runtime::TrapKind::Internal),
            pos,
            message: "async resume without completion".to_string(),
        }
    );
}

/// compiler.md §116.2 rule 5: an exception resume requires its raise site.
#[test]
fn an_exception_resume_without_await_raise_reports_the_internal_defect() {
    let (mut module, ()) = interpreter_for(
        "async function fail(): Promise<i32> { throw new Error(\"late\"); }\n\
             export async function main(): Promise<void> {\n\
             try { await fail(); } catch { print(\"caught\"); }\n\
             }\n",
    );
    assert_eq!(interpret(&module).expect("valid raise site"), b"caught\n");
    let main = module
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .expect("main function");
    let pos = main
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            l::Terminator::Suspend { pos, .. } => Some(pos.clone()),
            _ => None,
        })
        .expect("await position");
    let successor = main
        .blocks
        .iter_mut()
        .find(|block| {
            block.instructions.first().is_some_and(|instruction| {
                matches!(instruction.kind, l::InstructionKind::AwaitRaise)
            })
        })
        .expect("resume successor");
    successor.instructions.remove(0);
    let error = interpret(&module).expect_err("missing raise site");
    let InterpretError::Execution { output, source } = error else {
        panic!("expected execution error: {error:?}");
    };
    assert!(output.is_empty());
    assert_eq!(
        *source,
        InterpretError::Trap {
            kind: subscript_runtime::TrapKind::Internal.rule().to_string(),
            runtime_kind: Some(subscript_runtime::TrapKind::Internal),
            pos,
            message: "async exception resume without AwaitRaise".to_string(),
        }
    );
}

// ----- §94.2 teardown: scheduler storage, including blocked cycles -----

/// Lowers one program and builds an interpreter for it.
fn interpreter_for(source: &str) -> (l::Module, ()) {
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "teardown.ts",
        source,
    )])
    .expect("the program checks");
    (
        crate::lir::lower_module(&hir).expect("the program lowers"),
        (),
    )
}

/// Every frame the scheduler can still reach: the ready queue, the parked
/// list, the handle table, and, transitively, each frame's waiter list
/// and its awaited handle. A frame released from the handle table is
/// still reachable this way, so the closure sees it.
fn reachable_frames(interpreter: &Interpreter<'_>) -> Vec<Rc<RefCell<Coroutine>>> {
    let mut work: Vec<Rc<RefCell<Coroutine>>> = interpreter
        .async_ready
        .iter()
        .filter_map(AsyncJob::handle)
        .chain(interpreter.async_parked.iter().cloned())
        .collect();
    work.extend(interpreter.async_handles.borrow().values().cloned());
    let mut seen: HashMap<usize, Rc<RefCell<Coroutine>>> = HashMap::new();
    while let Some(frame) = work.pop() {
        if seen
            .insert(Rc::as_ptr(&frame) as usize, Rc::clone(&frame))
            .is_some()
        {
            continue;
        }
        let state = frame.borrow();
        work.extend(state.waiters.iter().filter_map(AsyncJob::handle));
        if let Some(awaited) = state.awaiting.as_ref() {
            work.push(Rc::clone(&awaited.handle));
        }
    }
    seen.into_values().collect()
}

/// Runs `source` to quiescence, then reports how many frames outlive the
/// interpreter. `Weak` is the lifetime evidence: a Context payload
/// counter cannot see a leaked Rust allocation.
fn frames_surviving_teardown(source: &str) -> (usize, usize, usize, usize) {
    let (module, ()) = interpreter_for(source);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    let _ = interpreter.run();
    let pending = interpreter.async_pending();
    let reachable = reachable_frames(&interpreter);
    let weak: Vec<Weak<RefCell<Coroutine>>> = reachable.iter().map(Rc::downgrade).collect();
    let captured = weak.len();
    drop(reachable);
    drop(interpreter);
    let survivors = weak
        .iter()
        .filter(|frame| frame.upgrade().is_some())
        .count();
    (captured, survivors, pending, weak.len())
}

const MUTUAL_AWAIT: &str =
    "async function waiter(id: i32, others: Promise<i32>[]): Promise<i32> {\n\
                                \x20 await Context.suspend();\n\
                                \x20 const other: Promise<i32> = others[1 - id];\n\
                                \x20 return await other;\n\
                                }\n\
                                export async function main(): Promise<void> {\n\
                                \x20 const handles: Promise<i32>[] = [];\n\
                                \x20 const a: Promise<i32> = waiter(0, handles);\n\
                                \x20 const b: Promise<i32> = waiter(1, handles);\n\
                                \x20 handles.push(a);\n\
                                \x20 handles.push(b);\n\
                                \x20 const x: i32 = await a;\n\
                                \x20 const y: i32 = await b;\n\
                                }\n";

const SELF_AWAIT: &str = "async function selfish(others: Promise<i32>[]): Promise<i32> {\n\
                              \x20 await Context.suspend();\n\
                              \x20 const mine: Promise<i32> = others[0];\n\
                              \x20 return await mine;\n\
                              }\n\
                              export async function main(): Promise<void> {\n\
                              \x20 const handles: Promise<i32>[] = [];\n\
                              \x20 const h: Promise<i32> = selfish(handles);\n\
                              \x20 handles.push(h);\n\
                              \x20 const v: i32 = await h;\n\
                              }\n";

const TRAP_WITH_BLOCKED_WORK: &str =
    "async function blocked(others: Promise<i32>[]): Promise<i32> {\n\
                                          \x20 await Context.suspend();\n\
                                          \x20 const other: Promise<i32> = others[0];\n\
                                          \x20 return await other;\n\
                                          }\n\
                                          async function faulty(): Promise<i32> {\n\
                                          \x20 await Context.suspend();\n\
                                          \x20 unreachable();\n\
                                          \x20 return 1;\n\
                                          }\n\
                                          export async function main(): Promise<void> {\n\
                                          \x20 const handles: Promise<i32>[] = [];\n\
                                          \x20 const bad: Promise<i32> = faulty();\n\
                                          \x20 const waiting: Promise<i32> = blocked(handles);\n\
                                          \x20 handles.push(waiting);\n\
                                          \x20 const v: i32 = await bad;\n\
                                          \x20 const w: i32 = await waiting;\n\
                                          }\n";

const COMPLETED_CONTROL: &str = "async function work(id: i32): Promise<i32> {\n\
                                     \x20 await Context.suspend();\n\
                                     \x20 return id;\n\
                                     }\n\
                                     export async function main(): Promise<void> {\n\
                                     \x20 const a: Promise<i32> = work(1);\n\
                                     \x20 const b: Promise<i32> = work(2);\n\
                                     \x20 print(`${await a},${await b}`);\n\
                                     }\n";

#[test]
fn running_frame_can_retain_its_own_handle() {
    let source = SELF_AWAIT.replace(
        "return await mine;",
        "print(\"retained\"); return await mine;",
    );
    let (module, ()) = interpreter_for(&source);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    assert_eq!(interpreter.run().expect("self retain"), b"retained\n");
    assert_eq!(interpreter.async_pending(), 0);
}

#[test]
fn child_can_register_on_its_executing_creator() {
    let source = "async function child(others: Promise<i32>[]): Promise<i32> {
  return await others[0];
}
async function creator(others: Promise<i32>[]): Promise<i32> {
  await Context.suspend();
  const waiting: Promise<i32> = child(others);
  print(\"registered\");
  return await waiting;
}
export async function main(): Promise<void> {
  const handles: Promise<i32>[] = [];
  const h: Promise<i32> = creator(handles);
  handles.push(h);
  const value: i32 = await h;
}
";
    let (module, ()) = interpreter_for(source);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    assert_eq!(
        interpreter.run().expect("child registration"),
        b"registered\n"
    );
    assert_eq!(interpreter.async_pending(), 0);
    let frames = reachable_frames(&interpreter);
    assert_eq!(frames.len(), 3);
    let weak: Vec<_> = frames.iter().map(Rc::downgrade).collect();
    drop(frames);
    drop(interpreter);
    assert!(weak.iter().all(|frame| frame.upgrade().is_none()));
}

#[test]
fn interpreter_clearance_stops_the_trapping_continuation() {
    mod programs {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/async_review.rs"
        ));
    }
    for (source, expected) in [
        (programs::BODY, b"m1\nm2\n".as_slice()),
        (programs::CALLEE, b"m1\nm2\nboom:start\n".as_slice()),
        (programs::SETTLED, b"m1\nm2\n".as_slice()),
    ] {
        let (module, ()) = interpreter_for(source);
        let mut interpreter = Interpreter::new(&module).expect("interpreter");
        let error = interpreter.run().expect_err("bounds trap");
        assert!(matches!(error, InterpretError::Trap { .. }));
        assert_eq!(interpreter.context.take_stdout(), expected);
        assert_eq!(interpreter.async_pending(), 1);
        assert_eq!(interpreter.async_step(), Err(error));
        for _ in 0..5 {
            interpreter.clear_trap();
            assert_eq!(interpreter.async_pending(), 0);
            interpreter.async_step().expect("idle checkpoint");
            assert_eq!(interpreter.context.take_stdout(), b"");
            // A callee prefix trap stops the child and, at clearance, its ready caller.
            let stopped = if source == programs::CALLEE { 2 } else { 1 };
            assert_eq!(interpreter.async_stopped.len(), stopped);
        }
        let weak = Rc::downgrade(&interpreter.async_stopped[0]);
        drop(interpreter);
        assert!(weak.upgrade().is_none());
    }
    let (module, ()) = interpreter_for(programs::CONTROL);
    let mut interpreter = Interpreter::new(&module).expect("control interpreter");
    assert_eq!(
        interpreter.run().expect("control advances"),
        b"m1\nm2\nm3\n"
    );
}

#[test]
fn teardown_releases_a_mutual_await_cycle() {
    let (captured, survivors, pending, _) = frames_surviving_teardown(MUTUAL_AWAIT);
    assert_eq!(pending, 0, "the program reaches quiescence");
    assert!(
        captured >= 3,
        "the scheduler still reaches the blocked frames"
    );
    assert_eq!(survivors, 0, "frames retained after interpreter teardown");
}

#[test]
fn teardown_releases_a_self_await_cycle() {
    let (captured, survivors, pending, _) = frames_surviving_teardown(SELF_AWAIT);
    assert_eq!(pending, 0, "the program reaches quiescence");
    assert!(
        captured >= 2,
        "the scheduler still reaches the blocked frames"
    );
    assert_eq!(survivors, 0, "frames retained after interpreter teardown");
}

#[test]
fn teardown_releases_frames_after_a_trap_with_blocked_work() {
    let (captured, survivors, _, _) = frames_surviving_teardown(TRAP_WITH_BLOCKED_WORK);
    assert!(captured >= 2, "a blocked registration outlives the trap");
    assert_eq!(survivors, 0, "frames retained after interpreter teardown");
}

/// The control that separates a cycle from ordinary completion: a
/// program whose awaits all complete leaves the scheduler reaching no
/// frame at all, so the three tests above measure a cycle rather than
/// the handle table.
#[test]
fn a_completed_program_leaves_the_scheduler_empty() {
    let (captured, survivors, pending, _) = frames_surviving_teardown(COMPLETED_CONTROL);
    assert_eq!(pending, 0);
    assert_eq!(
        captured, 0,
        "no ready job, parked frame, handle, waiter, or awaited handle remains"
    );
    assert_eq!(survivors, 0);
}

/// Teardown must not free work the program is still using. `twice`
/// parks twice, so one checkpoint leaves queued work, and the frames the
/// scheduler holds are still alive at that boundary.
#[test]
fn queued_work_survives_until_the_interpreter_is_dropped() {
    let source = "async function twice(id: i32): Promise<i32> {\n\
                      \x20 await Context.suspend();\n\
                      \x20 await Context.suspend();\n\
                      \x20 return id;\n\
                      }\n\
                      export async function main(): Promise<void> {\n\
                      \x20 const a: Promise<i32> = twice(1);\n\
                      \x20 const b: Promise<i32> = twice(2);\n\
                      \x20 print(`${await a},${await b}`);\n\
                      }\n";
    let (module, ()) = interpreter_for(source);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    let entry = module.entry.expect("entry");
    let Value::Coroutine(root) = interpreter
        .call_function(entry, Vec::new())
        .expect("root frame")
    else {
        panic!("an async export produces a coroutine");
    };
    interpreter.async_kick(&root).expect("kick");
    drop(root);
    assert!(
        interpreter.async_pending() > 0,
        "the kick leaves queued work"
    );
    let queued: Vec<Weak<RefCell<Coroutine>>> = reachable_frames(&interpreter)
        .iter()
        .map(Rc::downgrade)
        .collect();
    assert_eq!(queued.len(), 3, "two children and the blocked holder");

    interpreter.async_step().expect("first checkpoint");
    assert!(
        interpreter.async_pending() > 0,
        "the children park again, so work remains"
    );
    assert_eq!(
        queued
            .iter()
            .filter(|frame| frame.upgrade().is_some())
            .count(),
        3,
        "a checkpoint must not free the work it is still running"
    );

    while interpreter.async_pending() != 0 {
        interpreter.async_step().expect("checkpoint");
    }
    assert_eq!(interpreter.context.take_stdout(), b"1,2\n");
    drop(interpreter);
    assert_eq!(
        queued
            .iter()
            .filter(|frame| frame.upgrade().is_some())
            .count(),
        0,
        "frames retained after interpreter teardown"
    );
}

// ----- §106.3: a generator packs and unpacks through its own registry -----

/// One generator function and one class field that holds a generator, so
/// the module carries the `Generator<i32>` layout the pack path needs.
const GENERATOR_STORAGE: &str = "function* upTo(first: i32, last: i32): Generator<i32> {\n\
                                     \x20 for (let value: i32 = first; value <= last; value += 1) {\n\
                                     \x20   yield value;\n\
                                     \x20 }\n\
                                     }\n\
                                     class Holder {\n\
                                     \x20 source: Generator<i32>;\n\
                                     \x20 constructor(source: Generator<i32>) {\n\
                                     \x20   this.source = source;\n\
                                     \x20 }\n\
                                     }\n\
                                     export function main(): void {\n\
                                     \x20 const holder: Holder = new Holder(upTo(1, 2));\n\
                                     \x20 const result = holder.source.next();\n\
                                     \x20 print(`${result.value}`);\n\
                                     }\n";

/// The id of the one generator function in `GENERATOR_STORAGE`.
fn generator_function(module: &l::Module) -> l::FunctionId {
    let generators: Vec<l::FunctionId> = module
        .functions
        .iter()
        .filter(|function| function.is_generator)
        .map(|function| function.id)
        .collect();
    assert_eq!(generators.len(), 1, "the program declares one generator");
    generators[0]
}

/// A `Generator<i32>` frame from `GENERATOR_STORAGE`, bounded by `last`
/// so that two calls give two frames with different progress.
fn generator_frame(
    interpreter: &mut Interpreter<'_>,
    id: l::FunctionId,
    last: i64,
) -> Rc<RefCell<Coroutine>> {
    let value = interpreter
        .call_function(id, vec![Value::I(1), Value::I(last)])
        .expect("the generator call creates a frame");
    let Value::Coroutine(frame) = value else {
        panic!("a generator call produces a coroutine");
    };
    frame
}

fn generator_type() -> Type {
    Type::Generator(Box::new(Type::I32))
}

/// A generator that reads its own handle back out of storage and keeps it
/// in a local across a suspend. The frame then owns itself.
const SELF_NAMING_GENERATOR: &str = "function* selfish(box: Generator<i32>[]): Generator<i32> {\n\
         \x20 yield 1;\n\
         \x20 const mine: Generator<i32> = box[0];\n\
         \x20 yield 2;\n\
         \x20 box.push(mine);\n\
         \x20 yield 3;\n\
         }\n\
         export function main(): void {\n\
         \x20 const box: Generator<i32>[] = [];\n\
         \x20 const source: Generator<i32> = selfish(box);\n\
         \x20 box.push(source);\n\
         \x20 const first = source.next();\n\
         \x20 const second = source.next();\n\
         \x20 print(`${first.value},${second.value}`);\n\
         }\n";

/// Every frame a suspended frame's own saved state names.
fn frames_named_by(frame: &Rc<RefCell<Coroutine>>) -> Vec<Rc<RefCell<Coroutine>>> {
    let mut out = Vec::new();
    let state = frame.borrow();
    let frame = state.kind.frame().expect("invocation frame");
    let saved = frame.borrow();
    for value in saved.values.iter().flatten() {
        collect_coroutines(value, &mut out);
    }
    for local in &saved.locals {
        collect_coroutines(&local.slot.borrow(), &mut out);
    }
    out
}

/// Pins §106.3 rules 2 and 3. The loop is the control, and it is total:
/// an unpack that answers a registered frame the key does not name passes
/// for at most one of the three, whatever order the registry iterates.
#[test]
fn a_packed_generator_unpacks_to_the_same_frame() {
    let (module, ()) = interpreter_for(GENERATOR_STORAGE);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    let id = generator_function(&module);
    let frames: Vec<Rc<RefCell<Coroutine>>> = (2..5)
        .map(|last| generator_frame(&mut interpreter, id, last))
        .collect();
    assert_eq!(
        interpreter.generator_handles.borrow().len(),
        frames.len(),
        "each call registers its own frame"
    );

    let ty = generator_type();
    let mut keys: Vec<Vec<u8>> = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let packed = interpreter
            .pack(&ty, &Value::Coroutine(Rc::clone(frame)))
            .expect("the generator packs");
        let Value::Coroutine(restored) = interpreter.unpack(&ty, &packed).expect("the key unpacks")
        else {
            panic!("a generator key unpacks to a coroutine");
        };
        assert!(
            Rc::ptr_eq(frame, &restored),
            "frame {index} unpacks to another frame"
        );
        keys.push(packed);
    }
    let distinct: std::collections::HashSet<&Vec<u8>> = keys.iter().collect();
    assert_eq!(distinct.len(), frames.len(), "each frame packs to one key");
}

/// Pins §106.3 rule 4's null case, in both directions. The registered
/// key is the control: the same call answers a coroutine for it.
#[test]
fn a_zero_generator_key_unpacks_to_null() {
    let (module, ()) = interpreter_for(GENERATOR_STORAGE);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    let id = generator_function(&module);
    let frame = generator_frame(&mut interpreter, id, 2);
    let ty = generator_type();

    let zero = [0u8; 8];
    assert!(
        matches!(
            interpreter.unpack(&ty, &zero).expect("a zero key unpacks"),
            Value::Null
        ),
        "a zero key gives null"
    );
    assert_eq!(
        interpreter.pack(&ty, &Value::Null).expect("null packs"),
        zero.to_vec(),
        "null packs to a zero key"
    );

    let packed = interpreter
        .pack(&ty, &Value::Coroutine(frame))
        .expect("the generator packs");
    assert_ne!(packed, zero.to_vec(), "a live frame has a non-zero key");
    assert!(
        matches!(
            interpreter
                .unpack(&ty, &packed)
                .expect("a registered key unpacks"),
            Value::Coroutine(_)
        ),
        "a registered key gives the frame, so the null answer is the zero key's"
    );
}

/// Pins §106.3 rule 4's unknown key. The registered key is the control:
/// the same call still answers a coroutine for it.
#[test]
fn an_unknown_generator_key_reports_invalid_lir() {
    let (module, ()) = interpreter_for(GENERATOR_STORAGE);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    let id = generator_function(&module);
    let frame = generator_frame(&mut interpreter, id, 2);
    let ty = generator_type();
    let packed = interpreter
        .pack(&ty, &Value::Coroutine(frame))
        .expect("the generator packs");

    let key = u64::from_ne_bytes(packed.clone().try_into().expect("a key is eight bytes"));
    let stray = key.wrapping_add(0x1000);
    assert!(
        !interpreter
            .generator_handles
            .borrow()
            .contains_key(&(stray as usize)),
        "the probe key names no registered generator"
    );
    let error = interpreter
        .unpack(&ty, &stray.to_ne_bytes())
        .expect_err("an unknown key is invalid LIR");
    assert_eq!(
        error,
        InterpretError::InvalidLir {
            message: "unknown packed generator".to_string(),
            pos: None,
        }
    );
    assert!(
        interpreter.unpack(&ty, &packed).is_ok(),
        "the registered key still unpacks, so the error is the stray key's"
    );
}

/// Pins §106.7 gate item 3's drain, which §106.3 rule 5 owns. The test
/// builds a frame that the registry names and that also names itself.
/// `Weak` is the lifetime evidence, because no Context payload counter
/// sees a leaked Rust allocation.
#[test]
fn teardown_releases_a_generator_that_names_itself() {
    let (module, ()) = interpreter_for(SELF_NAMING_GENERATOR);
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    assert_eq!(interpreter.run().expect("the program runs"), b"1,2\n");

    let registered: Vec<Rc<RefCell<Coroutine>>> = interpreter
        .generator_handles
        .borrow()
        .values()
        .cloned()
        .collect();
    assert_eq!(registered.len(), 1, "the program creates one generator");
    assert!(
        frames_named_by(&registered[0])
            .iter()
            .any(|named| Rc::ptr_eq(named, &registered[0])),
        "the frame's own state names it, so the cycle is present"
    );

    let weak: Vec<Weak<RefCell<Coroutine>>> = registered.iter().map(Rc::downgrade).collect();
    drop(registered);
    drop(interpreter);
    assert!(
        weak.iter().all(|frame| frame.upgrade().is_none()),
        "a generator frame outlived the interpreter"
    );
}

#[test]
fn poisoned_address_names_use_and_invalidation() {
    let module = empty_module(Vec::new());
    let mut interpreter = Interpreter::new(&module).expect("empty module is valid");
    let poison = Rc::new(RefCell::new(None));
    interpreter
        .poison_registry
        .insert(l::ValueId(3), vec![Rc::downgrade(&poison)]);
    interpreter.invalidate(&[l::ValueId(3)], &Pos::new("poison.ts", 4, 7), || {
        "Call(Array.Push)".to_string()
    });
    let address = Address {
        target: AddressTarget::Slot(Rc::new(RefCell::new(Value::I(1)))),
        pointee: Type::I32,
        poison,
    };
    let error = address
        .check(&l::InstructionKind::LoadAddress)
        .expect_err("poisoned load must fail");
    assert!(matches!(
        error,
        InterpretError::PoisonedAddress {
            instruction,
            invalidated_by,
            invalidated_at,
        } if instruction == "LoadAddress"
            && invalidated_by == "Call(Array.Push)"
            && invalidated_at == Pos::new("poison.ts", 4, 7)
    ));
}

#[test]
fn suspend_restores_resume_value_then_remaining_live_ins() {
    let pos = Pos::new("suspend.ts", 1, 1);
    let child = l::Function {
        id: l::FunctionId(1),
        source_name: "child".to_string(),
        kind: l::FunctionKind::Free,
        exported: false,
        is_generator: false,
        // §94.1 rule 4: an `AsyncCall` target is an async function, so
        // the suspension creates a frame and registers on it.
        is_async: true,
        creation_traps: Vec::new(),
        host_entry_traps: None,
        can_raise: false,
        parameters: Vec::new(),
        return_type: Type::I32,
        locals: Vec::new(),
        values: Vec::new(),
        liveness: l::Liveness::default(),
        blocks: vec![l::BasicBlock {
            id: l::BlockId(0),
            source_name: Some("entry".to_string()),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: l::Terminator::Return {
                value: Some(l::Operand::Constant(l::Constant {
                    ty: Type::I32,
                    kind: l::ConstantKind::Integer(9),
                })),
                pos: pos.clone(),
            },
        }],
        entry: l::BlockId(0),
        pos: pos.clone(),
    };
    let main = l::Function {
        id: l::FunctionId(0),
        source_name: "main".to_string(),
        kind: l::FunctionKind::Free,
        exported: true,
        is_generator: false,
        is_async: true,
        creation_traps: Vec::new(),
        host_entry_traps: None,
        can_raise: false,
        parameters: Vec::new(),
        return_type: Type::I32,
        locals: Vec::new(),
        values: vec![
            l::Value {
                id: l::ValueId(0),
                ty: l::ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("resume".to_string()),
            },
            l::Value {
                id: l::ValueId(1),
                ty: l::ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("live".to_string()),
            },
            l::Value {
                id: l::ValueId(2),
                ty: l::ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("live.resume".to_string()),
            },
            l::Value {
                id: l::ValueId(3),
                ty: l::ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: None,
            },
        ],
        liveness: l::Liveness::default(),
        blocks: vec![
            l::BasicBlock {
                id: l::BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: vec![l::Instruction {
                    count_action: None,
                    result: Some(l::ValueId(1)),
                    kind: l::InstructionKind::Copy,
                    operands: vec![l::Operand::Constant(l::Constant {
                        ty: Type::I32,
                        kind: l::ConstantKind::Integer(7),
                    })],
                    invalidates: Vec::new(),
                    traps: Vec::new(),
                    pos: pos.clone(),
                }],
                terminator: l::Terminator::Suspend {
                    ownership: Vec::new(),
                    kind: l::SuspendKind::AsyncCall {
                        target: l::CallTarget {
                            kind: l::CallTargetKind::Function(l::FunctionId(1)),
                            parameter_types: Vec::new(),
                            return_type: Some(l::ValueType::Data(Type::I32)),
                        },
                        operands: Vec::new(),
                    },
                    pos: pos.clone(),
                    successor: l::BlockId(1),
                    resume_value: Some(l::ValueId(0)),
                    arguments: vec![l::Operand::Value(l::ValueId(1))],
                    invalidates: Vec::new(),
                    traps: Vec::new(),
                },
            },
            l::BasicBlock {
                id: l::BlockId(1),
                source_name: Some("resume".to_string()),
                parameters: vec![l::ValueId(0), l::ValueId(2)],
                instructions: vec![
                    l::Instruction {
                        count_action: Some(l::CountAction::Uncounted),
                        result: None,
                        kind: l::InstructionKind::AwaitRaise,
                        operands: Vec::new(),
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                        pos: pos.clone(),
                    },
                    l::Instruction {
                        count_action: None,
                        result: Some(l::ValueId(3)),
                        kind: l::InstructionKind::Binary(l::BinaryOp::Add),
                        operands: vec![
                            l::Operand::Value(l::ValueId(0)),
                            l::Operand::Value(l::ValueId(2)),
                        ],
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                        pos: pos.clone(),
                    },
                ],
                terminator: l::Terminator::Return {
                    value: Some(l::Operand::Value(l::ValueId(3))),
                    pos: pos.clone(),
                },
            },
        ],
        entry: l::BlockId(0),
        pos,
    };
    let module = empty_module(vec![main, child]);
    assert_eq!(interpret(&module), Ok(Vec::new()));
}

#[test]
fn s180_interpreter_suspended_finalizers_have_distinct_frame_slots() {
    let (module, ()) = interpreter_for(crate::finally_tests::FRAME_SOURCE);
    let id = module
        .functions
        .iter()
        .find(|f| f.source_name == "f")
        .unwrap()
        .id;
    for count in 1..=2 {
        let mut interpreter = Interpreter::new(&module).unwrap();
        let mut handles = Vec::new();
        for tag in ["A", "B"].into_iter().take(count) {
            let pos = Pos::new("slots.ts", 1, 1);
            let tag = interpreter.alloc_string(tag.as_bytes(), &pos).unwrap();
            let Value::Coroutine(handle) = interpreter
                .call_function(id, vec![Value::Handle(tag)])
                .unwrap()
            else {
                panic!("frame handle");
            };
            interpreter.async_start(&handle).unwrap();
            handles.push(handle);
        }
        let mut objects = Vec::new();
        for (handle, expected) in handles.iter().zip(["Error: A", "Error: B"]) {
            let state = handle.borrow();
            let CoroutineKind::Invocation(frame) = &state.kind else {
                panic!("invocation");
            };
            let frame = frame.borrow();
            let values = &frame.locals;
            let object = values[0].slot.borrow().as_handle().unwrap();
            let text = values[1].slot.borrow().as_handle().unwrap();
            let position = values[2].slot.borrow().as_u64().unwrap();
            assert!(!object.is_null());
            assert_eq!(interpreter.string_bytes(text).unwrap(), expected.as_bytes());
            assert_eq!(interpreter.exception_positions[position as usize].line, 3);
            objects.push(object);
        }
        if count == 2 {
            assert_ne!(objects[0], objects[1]);
        }
        assert!(interpreter.parked.is_empty());
    }
}
