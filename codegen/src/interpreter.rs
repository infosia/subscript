//! Specification-driven reference interpreter for ordered LIR.
//!
//! This module intentionally consumes only [`subscript_compiler::lir`].  It
//! is a test oracle for the shared lowering, not a shipped execution tier.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::ops::Range;
use std::rc::{Rc, Weak};

use subscript_compiler::lir as l;
use subscript_compiler::types::scalar_size_align;
use subscript_compiler::{ClassId, Pos, Type};
use subscript_runtime::context::Context;
use subscript_runtime::ffi;
use subscript_runtime::trap::TrapKind as RuntimeTrapKind;

use crate::lir_types::{runtime_trap_kind, runtime_trap_site, TrapSite};
use crate::position_table::no_script_site;

/// A failure observed while executing a verified LIR module.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InterpretError {
    /// The LIR asks the interpreter to use a fact the module does not carry.
    InvalidLir {
        /// Exact instruction/control-flow problem.
        message: String,
        /// Source position, when an instruction owns the problem.
        pos: Option<Pos>,
    },
    /// A semantic trap ended the program.
    Trap {
        /// Stable runtime trap rule.
        kind: String,
        /// Runtime trap identity, when the LIR site maps to a runtime trap.
        runtime_kind: Option<RuntimeTrapKind>,
        /// Source position carried by LIR.
        pos: Pos,
        /// Runtime detail, when supplied by the shared runtime.
        message: String,
    },
    /// The oracle deliberately cannot link a program dependency.
    Unsupported {
        /// The unsupported dependency or operation.
        reason: String,
    },
    /// An address was used after its dynamic-array provenance was invalidated.
    PoisonedAddress {
        /// Instruction attempting the use.
        instruction: String,
        /// Instruction that invalidated the address's base.
        invalidated_by: String,
        /// Source position of the invalidation.
        invalidated_at: Pos,
    },
    /// An exception propagates (`compiler.md` §115.6 rule 5). The host
    /// entry converts one that no handler catches into the
    /// uncaught-exception trap, so [`interpret`] never returns it.
    Exception {
        /// Address of the Error object.
        object: usize,
        /// The Error's report text.
        message: String,
        /// Position of the last `throw`.
        pos: Pos,
    },
    /// Execution stopped after already producing observable output.
    Execution {
        /// Bytes written before the failure.
        output: Vec<u8>,
        /// The semantic failure that stopped execution.
        source: Box<InterpretError>,
    },
}

impl fmt::Display for InterpretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InterpretError::InvalidLir { message, pos } => {
                if let Some(pos) = pos {
                    write!(f, "{pos}: invalid LIR: {message}")
                } else {
                    write!(f, "invalid LIR: {message}")
                }
            }
            InterpretError::Trap {
                kind, pos, message, ..
            } => {
                write!(f, "{pos}: trap {kind}: {message}")
            }
            InterpretError::Unsupported { reason } => write!(f, "unsupported: {reason}"),
            InterpretError::Exception { message, pos, .. } => {
                write!(f, "{pos}: exception: {message}")
            }
            InterpretError::PoisonedAddress {
                instruction,
                invalidated_by,
                invalidated_at,
            } => write!(
                f,
                "{instruction} used an address poisoned by {invalidated_by} at {invalidated_at}"
            ),
            InterpretError::Execution { source, .. } => source.fmt(f),
        }
    }
}

impl Error for InterpretError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            InterpretError::Execution { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl InterpretError {
    /// Converts a propagating exception into the uncaught-exception trap
    /// at a boundary (`compiler.md` §115.4). Every other error passes
    /// unchanged.
    pub(crate) fn settled(self) -> Self {
        match self {
            InterpretError::Exception { message, pos, .. } => InterpretError::Trap {
                kind: RuntimeTrapKind::UncaughtException.rule().to_string(),
                runtime_kind: Some(RuntimeTrapKind::UncaughtException),
                pos,
                message,
            },
            other => other,
        }
    }

    /// Bytes written before this error stopped execution.
    #[must_use]
    pub fn output(&self) -> &[u8] {
        match self {
            InterpretError::Execution { output, .. } => output,
            _ => &[],
        }
    }
}

/// Executes a complete LIR module and returns the program's captured stdout.
///
/// The synthetic initializer, when present, runs before the exported
/// zero-argument `main`. Every other exported zero-argument async function is
/// then kicked in first entry export-site order, and host checkpoints run until no work
/// can advance (`compiler.md` §26.3 and §94). Runtime-owned strings, arrays,
/// maps, sets, JSON state, dates, regular expressions, and formatting all go
/// through `subscript-runtime`.
///
/// # Errors
///
/// Returns a semantic trap, malformed-LIR finding, provenance error, or an
/// explicitly unsupported external dependency.
pub fn interpret(module: &l::Module) -> Result<Vec<u8>, InterpretError> {
    let mut interpreter = Interpreter::new(module)?;
    match interpreter.run() {
        Ok(output) => Ok(output),
        Err(source) => Err(InterpretError::Execution {
            output: interpreter.context.take_stdout(),
            source: Box::new(source),
        }),
    }
}

/// Pushes every coroutine handle one value owns: directly, through a
/// closure capture, or as an iteration subject. Every other variant owns
/// none, and an address is a borrow of a slot this walk already clears.
fn collect_coroutines(value: &Value, out: &mut Vec<Rc<RefCell<Coroutine>>>) {
    match value {
        Value::Coroutine(frame) => out.push(Rc::clone(frame)),
        Value::Callable(callable) => {
            for capture in &callable.captures {
                collect_coroutines(capture, out);
            }
        }
        Value::Iterator(cursor) => collect_coroutines(&cursor.subject, out),
        _ => {}
    }
}

impl Drop for Interpreter<'_> {
    fn drop(&mut self) {
        // §94.2: Context release discards all work without resumption.
        self.release_scheduler_storage();
    }
}

#[derive(Clone)]
enum Value {
    I(i64),
    U(u64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Handle(*mut u8),
    Blob(Vec<u8>),
    Callable(Rc<Callable>),
    Coroutine(Rc<RefCell<Coroutine>>),
    Iterator(Rc<IteratorCursor>),
    Address(Address),
    Null,
    Void,
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I(v) => write!(f, "I({v})"),
            Value::U(v) => write!(f, "U({v})"),
            Value::F32(v) => write!(f, "F32({v:?})"),
            Value::F64(v) => write!(f, "F64({v:?})"),
            Value::Bool(v) => write!(f, "Bool({v})"),
            Value::Handle(v) => write!(f, "Handle({v:p})"),
            Value::Blob(v) => write!(f, "Blob({} bytes)", v.len()),
            Value::Callable(v) => write!(f, "Callable(f{})", v.function.0),
            Value::Coroutine(_) => f.write_str("Coroutine"),
            Value::Iterator(v) => write!(f, "Iterator({:?})", v.kind),
            Value::Address(_) => f.write_str("Address"),
            Value::Null => f.write_str("Null"),
            Value::Void => f.write_str("Void"),
        }
    }
}

type Slot = Rc<RefCell<Value>>;

#[derive(Clone)]
struct Callable {
    function: l::FunctionId,
    captures: Vec<Value>,
}

#[derive(Clone)]
struct Address {
    target: AddressTarget,
    pointee: Type,
    poison: Rc<RefCell<Option<Invalidation>>>,
}

#[derive(Clone)]
enum AddressTarget {
    Slot(Slot),
    SlotBytes { slot: Slot, offset: usize },
    Pointer(*mut u8),
}

#[derive(Clone)]
struct Invalidation {
    instruction: String,
    pos: Pos,
}

struct IteratorCursor {
    kind: l::ForOfKind,
    bound_kind: l::IteratorBoundKind,
    subject: Value,
    /// Storage position bound captured when the cursor is created.
    bound: i64,
    position: i64,
    assoc_probe_size: Option<usize>,
}

struct Coroutine {
    task_id: u64,
    #[cfg(test)]
    function_pos: Pos,
    #[cfg(test)]
    create_pos: Pos,
    #[cfg(test)]
    suspension_pos: Pos,
    #[cfg(test)]
    active: bool,
    // Execution storage has its own allocation and borrow domain.
    // Handle ownership and waiter registration never borrow this cell.
    kind: CoroutineKind,
    completed: bool,
    completion: Option<Completion>,
    owners: u32,
    /// A host-kicked export root has no script holder (`compiler.md`
    /// §116.1 rule 5).
    host_root: bool,
    /// Continuations registered on this frame, in registration order
    /// (`compiler.md` §94.1 rule 5). Completion moves them to the ready
    /// queue's tail in that order.
    waiters: Vec<AsyncJob>,
    /// The awaited frame this suspension registered on, with the position
    /// that reports a resume without a completion. The registration owns one count.
    awaiting: Option<AwaitedHandle>,
}

/// The completion of an async handle (`compiler.md` §116.2 rules 1 and 5).
enum Completion {
    Value(Value),
    /// The exception that left the body, and whether an `await` raised it
    /// (§116.1 rule 4).
    Exception(Box<ExceptionCompletion>),
}

struct ExceptionCompletion {
    exception: (usize, String, Pos),
    observed: bool,
}

/// One outstanding await registration.
struct AwaitedHandle {
    handle: Rc<RefCell<Coroutine>>,
    pos: Pos,
}

/// What a suspension asks the scheduler to do (`compiler.md` §94.1).
enum AsyncRequest {
    /// `SuspendKind::Async`: wait for the next host checkpoint.
    Park(Pos),
    /// `SuspendKind::AsyncCall`: create and start the child, then register.
    Call(l::CallTarget, Vec<Value>, Pos),
    /// `SuspendKind::AsyncHandle`: register on the named handle.
    Handle(Rc<RefCell<Coroutine>>, Pos, bool),
}

struct Frame {
    function: l::FunctionId,
    block: l::BlockId,
    values: Vec<Option<Value>>,
    locals: Vec<InterpreterLocal>,
    /// A value supplied by the completed child operation.
    resume: Option<Value>,
    /// The exact successor parameter that receives `resume`.
    resume_target: Option<l::ValueId>,
    /// The exception of an exception completion, which the `AwaitRaise`
    /// of the resume successor raises (`compiler.md` §116.2 rule 3).
    delivered: Option<(usize, String, Pos)>,
}

struct InterpreterLocal {
    storage: l::LocalStorageClass,
    slot: Slot,
    poisoned_at: Option<(l::BlockId, Pos)>,
}

impl InterpreterLocal {
    fn slot(&self) -> &Slot {
        match self.storage {
            l::LocalStorageClass::Activation | l::LocalStorageClass::Frame => &self.slot,
        }
    }
}

enum Flow {
    Returned(Value),
    /// An exception left an async body with a script holder
    /// (`compiler.md` §116.1 rule 1).
    Raised((usize, String, Pos)),
    Suspended {
        yielded: Option<Value>,
        /// `None` for a generator yield, which keeps its own protocol.
        request: Option<AsyncRequest>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrapPhase {
    Before,
    After,
}

#[derive(Clone, Copy)]
struct Layout {
    size: usize,
    align: usize,
}

struct CallbackState {
    interpreter: *mut (),
    callable: Rc<Callable>,
    first_ty: Type,
    second_ty: Option<Type>,
    error: Option<InterpretError>,
}

struct Interpreter<'m> {
    module: &'m l::Module,
    context: Box<Context>,
    globals: Vec<Slot>,
    // `Context` retains raw addresses to these root slots. Indirection keeps
    // every address stable when this Vec grows; `Vec<usize>` would move its
    // elements on reallocation and leave the runtime holding dangling roots.
    roots: Vec<Rc<Cell<usize>>>,
    field_layouts: HashMap<l::FieldId, (usize, Type)>,
    class_layouts: HashMap<ClassId, Layout>,
    layout_cache: HashMap<String, Layout>,
    padding_cache: HashMap<String, Vec<Range<usize>>>,
    poison_registry: HashMap<l::ValueId, Vec<Weak<RefCell<Option<Invalidation>>>>>,
    async_handles: RefCell<HashMap<usize, Rc<RefCell<Coroutine>>>>,
    next_async_task_id: u64,
    async_registry: RefCell<HashMap<u64, Weak<RefCell<Coroutine>>>>,
    // The generator registry. §106.3 rules 1 and 5 own it.
    generator_handles: RefCell<HashMap<usize, Rc<RefCell<Coroutine>>>>,
    // §94 scheduler state: runnable continuations in FIFO order, and the
    // frames that wait for the next host checkpoint.
    async_ready: std::collections::VecDeque<AsyncJob>,
    async_parked: std::collections::VecDeque<Rc<RefCell<Coroutine>>>,
    async_trapping: Option<InterpretError>,
    async_stopped: Vec<Rc<RefCell<Coroutine>>>,
    // Runtime helpers can report while an instruction is still executing.
    // Keep the enclosing LIR sites here so that even those reports use the
    // checker-owned source position rather than the instruction's broad span.
    active_traps: Vec<l::Trap>,
    // The exception a handler edge carries to its catch entry or its park.
    caught: Option<(usize, String, Pos)>,
    // The exceptions that wait while the hooks of an exception exit run,
    // innermost last (compiler.md §115.5 rule 7).
    parked: Vec<(usize, String, Pos)>,
}

impl<'m> Interpreter<'m> {
    fn new(module: &'m l::Module) -> Result<Self, InterpretError> {
        let mut interpreter = Self {
            module,
            context: Context::new(),
            globals: Vec::new(),
            roots: Vec::new(),
            field_layouts: HashMap::new(),
            class_layouts: HashMap::new(),
            layout_cache: HashMap::new(),
            padding_cache: HashMap::new(),
            poison_registry: HashMap::new(),
            async_handles: RefCell::new(HashMap::new()),
            next_async_task_id: 1,
            async_registry: RefCell::new(HashMap::new()),
            generator_handles: RefCell::new(HashMap::new()),
            async_ready: std::collections::VecDeque::new(),
            async_parked: std::collections::VecDeque::new(),
            async_trapping: None,
            async_stopped: Vec::new(),
            active_traps: Vec::new(),
            caught: None,
            parked: Vec::new(),
        };
        interpreter.compute_class_layouts()?;
        interpreter.globals = module
            .globals
            .iter()
            .map(|global| Rc::new(RefCell::new(interpreter.zero(&global.ty))))
            .collect();
        Ok(interpreter)
    }

    fn run(&mut self) -> Result<Vec<u8>, InterpretError> {
        self.clear_trap();
        self.context.enter_script();
        let outcome = self.run_entries();
        self.context.exit_script();
        // compiler.md §115.4 item 1: the host entry converts an exception
        // that no handler catches.
        outcome.map_err(InterpretError::settled)
    }

    fn run_entries(&mut self) -> Result<Vec<u8>, InterpretError> {
        if let Some(initializer) = self.module.initializer {
            let _ = self.call_function(initializer, Vec::new())?;
        }
        let entry = self
            .module
            .entry
            .ok_or_else(|| InterpretError::InvalidLir {
                message: "module has no executable entry".to_string(),
                pos: None,
            })?;
        let entry = self.function(entry)?;
        if !entry.parameters.is_empty() {
            return Err(InterpretError::Unsupported {
                reason: "exported main requires host-supplied parameters".to_string(),
            });
        }
        let entry_id = entry.id;
        let entry_pos = entry.pos.clone();
        // §26.3 and §94.1 rule 10: the standard runner kicks `main`, then
        // each other async target in first entry export-site order, and then
        // steps while pending work remains. An export kick starts a body and
        // never drains ready work (rule 7).
        let result = self.call_function(entry_id, Vec::new())?;
        if let Value::Coroutine(coroutine) = result {
            self.async_kick(&coroutine)?;
        }
        for function in self.module.async_roots.clone() {
            let Value::Coroutine(coroutine) = self.call_function(function, Vec::new())? else {
                return Err(self.invalid(None, "async export did not create a coroutine"));
            };
            self.async_kick(&coroutine)?;
        }
        while self.async_pending() != 0 {
            self.async_step()?;
        }
        self.check_runtime(&entry_pos)?;
        Ok(self.context.take_stdout())
    }

    fn function(&self, id: l::FunctionId) -> Result<&l::Function, InterpretError> {
        self.module
            .functions
            .get(id.0 as usize)
            .filter(|function| function.id == id)
            .ok_or_else(|| self.invalid(None, format!("function f{} is missing", id.0)))
    }

    fn call_function(
        &mut self,
        id: l::FunctionId,
        arguments: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let function = self.function(id)? as *const l::Function;
        // SAFETY: `module` is immutable for the interpreter's lifetime.
        // Calling into the interpreter cannot move or mutate this function.
        let function = unsafe { &*function };
        if function.parameters.len() != arguments.len() {
            return Err(self.invalid(
                Some(function.pos.clone()),
                format!(
                    "call of f{} has {} arguments for {} parameters",
                    id.0,
                    arguments.len(),
                    function.parameters.len()
                ),
            ));
        }
        let mut values = vec![None; function.values.len()];
        let locals: Vec<InterpreterLocal> = function
            .locals
            .iter()
            .map(|local| InterpreterLocal {
                storage: local.storage,
                slot: Rc::new(RefCell::new(match &local.ty {
                    l::ValueType::Data(ty) => self.zero(ty),
                    l::ValueType::Address(_) | l::ValueType::Iterator(_) => Value::Void,
                })),
                poisoned_at: None,
            })
            .collect();
        for (parameter, argument) in function.parameters.iter().zip(arguments) {
            self.set_value(
                &mut values,
                parameter.value,
                argument.clone(),
                &function.pos,
            )?;
            if let Some(storage) = parameter.storage {
                let slot = locals.get(storage.0 as usize).ok_or_else(|| {
                    self.invalid(
                        Some(parameter.pos.clone()),
                        format!("parameter storage local {} is missing", storage.0),
                    )
                })?;
                *slot.slot().borrow_mut() = argument;
            }
        }
        let frame = Frame {
            function: id,
            block: function.entry,
            values,
            locals,
            resume: None,
            resume_target: None,
            delivered: None,
        };
        if function.is_generator || function.is_async {
            for trap in &function.creation_traps {
                if trap.kind == l::TrapKind::Allocation && self.context.trapped() {
                    return Err(self.trap_error(trap));
                }
            }
            let task_id = if function.is_async {
                self.register_task_id()?
            } else {
                0
            };
            let coroutine = Rc::new(RefCell::new(Coroutine {
                task_id,
                #[cfg(test)]
                function_pos: function.pos.clone(),
                #[cfg(test)]
                create_pos: no_script_site(),
                #[cfg(test)]
                suspension_pos: no_script_site(),
                #[cfg(test)]
                active: false,
                kind: CoroutineKind::Invocation(Rc::new(RefCell::new(frame))),
                completed: false,
                completion: None,
                owners: u32::from(function.is_async),
                host_root: false,
                waiters: Vec::new(),
                awaiting: None,
            }));
            if function.is_async {
                self.async_registry
                    .borrow_mut()
                    .insert(task_id, Rc::downgrade(&coroutine));
                self.async_handles
                    .borrow_mut()
                    .insert(Rc::as_ptr(&coroutine) as usize, Rc::clone(&coroutine));
            }
            if function.is_generator {
                // §106.3 rule 1 owns this registration.
                self.generator_handles
                    .borrow_mut()
                    .insert(Rc::as_ptr(&coroutine) as usize, Rc::clone(&coroutine));
            }
            return Ok(Value::Coroutine(coroutine));
        }
        let mut frame = frame;
        match self.execute_frame(&mut frame)? {
            Flow::Returned(value) => Ok(value),
            Flow::Raised(_) | Flow::Suspended { .. } => Err(self.invalid(
                Some(function.pos.clone()),
                "non-coroutine function suspended",
            )),
        }
    }

    // ----- §94 host-driven continuation scheduler -----

    /// Releases every scheduler registration, including the reference cycles
    /// a blocked wait forms (`compiler.md` §94.2: "It releases scheduler
    /// storage, including blocked cycles, without implicit collection").
    ///
    /// A blocked frame and the frame it waits on own each other: the waiter
    /// list owns the caller, and the caller's registration owns the awaited
    /// handle. That is the ownership §94.2 asks for while the program runs,
    /// and it is what a self-await, a mutual wait, or a longer wait ring
    /// leaves behind at quiescence. The production runtime frees such a ring
    /// with its Context arena; here the walk below breaks it instead.
    ///
    /// The walk starts at every scheduler state and at the generator registry
    /// (§106.3 rule 5), and follows the two owning edges, so it also reaches a
    /// frame the handle table has already released. It runs no continuation
    /// and invokes no collector.
    fn release_scheduler_storage(&mut self) {
        let mut work: Vec<Rc<RefCell<Coroutine>>> =
            self.async_ready.drain(..).map(|job| job.handle()).collect();
        work.extend(self.async_parked.drain(..));
        work.append(&mut self.async_stopped);
        work.extend(
            self.async_handles
                .borrow_mut()
                .drain()
                .map(|(_, frame)| frame),
        );
        // §106.3 rule 5 owns the release point.
        work.extend(
            self.generator_handles
                .borrow_mut()
                .drain()
                .map(|(_, frame)| frame),
        );
        for global in &self.globals {
            collect_coroutines(&global.borrow(), &mut work);
        }
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        while let Some(frame) = work.pop() {
            if !seen.insert(Rc::as_ptr(&frame) as usize) {
                continue;
            }
            let mut state = frame.borrow_mut();
            work.extend(
                std::mem::take(&mut state.waiters)
                    .into_iter()
                    .map(|job| job.handle()),
            );
            if let Some(awaited) = state.awaiting.take() {
                work.push(awaited.handle);
            }
            // A suspended frame's own state holds the handles the program
            // gave it, and two frames that hold each other's handle form the
            // same ring. Teardown discards the work, so the saved state goes
            // with the registration.
            let frame_cell = match &mut state.kind {
                CoroutineKind::Invocation(frame) => Rc::clone(frame),
                CoroutineKind::Aggregate(aggregate) => {
                    work.extend(aggregate.inputs.iter_mut().filter_map(Option::take));
                    state.completion = None;
                    continue;
                }
            };
            let mut saved = frame_cell.borrow_mut();
            for value in saved.values.iter().flatten() {
                collect_coroutines(value, &mut work);
            }
            for local in &saved.locals {
                collect_coroutines(&local.slot.borrow(), &mut work);
            }
            if let Some(resume) = saved.resume.as_ref() {
                collect_coroutines(resume, &mut work);
            }
            if let Some(Completion::Value(completion)) = state.completion.as_ref() {
                collect_coroutines(completion, &mut work);
            }
            saved.values.clear();
            saved.locals.clear();
            saved.resume = None;
            state.completion = None;
        }
    }

    /// Work a checkpoint can advance: ready jobs plus parked frames
    /// (`compiler.md` §94.2).
    fn async_pending(&self) -> usize {
        self.async_ready.len() + self.async_parked.len()
    }

    /// Starts an exported root and registers whatever it suspends on
    /// (§94.1 rules 1, 7 and 10). The kick drains nothing.
    fn async_kick(&mut self, coroutine: &Rc<RefCell<Coroutine>>) -> Result<(), InterpretError> {
        coroutine.borrow_mut().host_root = true;
        self.async_start(coroutine)?;
        if self.context.trapped() {
            // The existing trap policy preserves the trapping frame.
            return Ok(());
        }
        // The kick holds no scheduler reference: a suspended root registered
        // its own, and a completed root has no continuation work.
        self.release_coroutine(coroutine)
    }

    // §94.2: the reference driver clears at a host entry boundary.
    fn clear_trap(&mut self) {
        if self.async_trapping.take().is_some() {
            // This witness has no reload adapter or staleness exception.
            if let Some(frame) = self.async_ready.pop_front() {
                self.async_stopped.push(frame.handle());
            }
        }
        self.context.clear_trap();
    }

    /// Runs one frame to its first await or return, then applies its
    /// outcome to the scheduler.
    fn async_start(&mut self, coroutine: &Rc<RefCell<Coroutine>>) -> Result<(), InterpretError> {
        let result = self
            .execute_coroutine(coroutine)
            .and_then(|flow| self.apply_async_flow(coroutine, flow));
        if result.is_err() || self.context.trapped() {
            self.async_stopped.push(Rc::clone(coroutine));
        }
        result
    }

    /// Resumes a queued continuation. §94.1 rule 11: the resume reads the
    /// awaited frame's immutable cached completion and never polls it.
    fn async_resume(&mut self, coroutine: &Rc<RefCell<Coroutine>>) -> Result<(), InterpretError> {
        let awaited = coroutine.borrow_mut().awaiting.take();
        if let Some(awaited) = awaited {
            let completion = match awaited.handle.borrow_mut().completion.as_mut() {
                None => None,
                Some(Completion::Value(value)) => Some(Ok(value.clone())),
                // compiler.md §116.1 rules 2 and 3: each await raises the
                // same object.
                Some(Completion::Exception(payload)) => {
                    payload.observed = true;
                    Some(Err(payload.exception.clone()))
                }
            };
            let Some(completion) = completion else {
                // §94.1: an internal protocol defect, never a source trap and
                // never a reason to poll or re-register.
                return Err(InterpretError::Trap {
                    kind: subscript_runtime::TrapKind::Internal.rule().to_string(),
                    runtime_kind: Some(RuntimeTrapKind::Internal),
                    pos: awaited.pos.clone(),
                    message: "async resume without completion".to_string(),
                });
            };
            let state = coroutine
                .borrow()
                .kind
                .frame()
                .ok_or_else(|| self.invalid(None, "aggregate resumed as an invocation"))?;
            let mut frame = state.borrow_mut();
            let value = match completion {
                Ok(value) => value,
                Err(exception) => {
                    let starts_with_raise = self
                        .module
                        .functions
                        .get(frame.function.0 as usize)
                        .and_then(|function| function.blocks.get(frame.block.0 as usize))
                        .and_then(|block| block.instructions.first())
                        .is_some_and(|instruction| {
                            matches!(instruction.kind, l::InstructionKind::AwaitRaise)
                        });
                    if !starts_with_raise {
                        return Err(InterpretError::Trap {
                            kind: subscript_runtime::TrapKind::Internal.rule().to_string(),
                            runtime_kind: Some(RuntimeTrapKind::Internal),
                            pos: awaited.pos.clone(),
                            message: "async exception resume without AwaitRaise".to_string(),
                        });
                    }
                    // The successor's `AwaitRaise` takes the edge; the
                    // resume value is the zero value, as the tiers read it.
                    frame.delivered = Some(exception);
                    let ty = frame
                        .resume_target
                        .and_then(|target| {
                            self.module
                                .functions
                                .get(frame.function.0 as usize)?
                                .values
                                .get(target.0 as usize)
                        })
                        .map(|value| value.ty.clone());
                    match ty {
                        Some(l::ValueType::Data(ty)) => self.zero(&ty),
                        _ => Value::Void,
                    }
                }
            };
            frame.resume = Some(value);
            drop(frame);
            self.release_coroutine(&awaited.handle)?;
        }
        let flow = self.execute_coroutine(coroutine)?;
        self.apply_async_flow(coroutine, flow)
    }

    /// Executes one coroutine frame once. A frame already completed
    /// produces its stored completion rather than running again.
    fn execute_coroutine(
        &mut self,
        coroutine: &Rc<RefCell<Coroutine>>,
    ) -> Result<Flow, InterpretError> {
        let (frame, host_root) = {
            let state = coroutine.borrow();
            if state.completed {
                return Ok(Flow::Returned(match &state.completion {
                    Some(Completion::Value(value)) => value.clone(),
                    Some(Completion::Exception(_)) | None => Value::Void,
                }));
            }
            (
                state
                    .kind
                    .frame()
                    .ok_or_else(|| self.invalid(None, "aggregate executed as an invocation"))?,
                state.host_root,
            )
        };
        let mut frame = frame
            .try_borrow_mut()
            .map_err(|_| self.invalid(None, "coroutine frame is already executing"))?;
        let is_async = self.function(frame.function)?.is_async;
        // compiler.md §116.1 rule 1: an exception that leaves an async body
        // with a script holder completes its handle. A host-kicked root
        // (rule 5) and a generator body (§115.4 item 3) are boundaries.
        #[cfg(test)]
        {
            coroutine.borrow_mut().active = true;
        }
        let outcome = self.execute_frame(&mut frame);
        #[cfg(test)]
        {
            coroutine.borrow_mut().active = false;
        }
        match outcome {
            Err(InterpretError::Exception {
                object,
                message,
                pos,
            }) if is_async && !host_root => Ok(Flow::Raised((object, message, pos))),
            outcome => outcome.map_err(InterpretError::settled),
        }
    }

    /// Applies one execution outcome to the scheduler state.
    fn apply_async_flow(
        &mut self,
        coroutine: &Rc<RefCell<Coroutine>>,
        flow: Flow,
    ) -> Result<(), InterpretError> {
        match flow {
            Flow::Returned(value) => {
                let waiters = {
                    let mut state = coroutine.borrow_mut();
                    state.completed = true;
                    state.completion = Some(Completion::Value(value));
                    std::mem::take(&mut state.waiters)
                };
                // §94.1 rule 5: completion makes every registered
                // continuation runnable, in registration order, at the tail.
                self.async_ready.extend(waiters);
                Ok(())
            }
            Flow::Raised(exception) => {
                let (waiters, owners) = {
                    let mut state = coroutine.borrow_mut();
                    state.completed = true;
                    state.completion = Some(Completion::Exception(Box::new(ExceptionCompletion {
                        exception: exception.clone(),
                        observed: false,
                    })));
                    (std::mem::take(&mut state.waiters), state.owners)
                };
                self.async_ready.extend(waiters);
                // compiler.md §116.1 rule 4: with no holder left, the
                // exception can never be observed.
                if owners == 0 {
                    let (object, message, pos) = exception;
                    return Err(InterpretError::Exception {
                        object,
                        message,
                        pos,
                    }
                    .settled());
                }
                Ok(())
            }
            Flow::Suspended {
                request: Some(AsyncRequest::Park(_pos)),
                ..
            } => {
                #[cfg(test)]
                {
                    coroutine.borrow_mut().suspension_pos = _pos;
                }
                self.async_parked.push_back(Rc::clone(coroutine));
                Ok(())
            }
            Flow::Suspended {
                request: Some(AsyncRequest::Call(target, arguments, pos)),
                ..
            } => {
                // §94.1 rules 1 and 4: the suspension creates the child and
                // runs its prefix, then registers. It never resumes it.
                let Value::Coroutine(child) =
                    self.invoke_target(&target, arguments, None, Some(&pos))?
                else {
                    return Err(self.invalid(Some(pos), "async call did not create a coroutine"));
                };
                #[cfg(test)]
                {
                    child.borrow_mut().create_pos = pos.clone();
                }
                self.async_start(&child)?;
                if self.context.trapped() {
                    return Ok(());
                }
                self.register_continuation(coroutine, &child, pos, true);
                Ok(())
            }
            Flow::Suspended {
                request: Some(AsyncRequest::Handle(handle, pos, owned)),
                ..
            } => {
                self.register_continuation(coroutine, &handle, pos, owned);
                Ok(())
            }
            Flow::Suspended { request: None, .. } => {
                Err(self.invalid(None, "a generator suspension reached the async scheduler"))
            }
        }
    }

    /// §94.1 rules 4 to 6: an unfinished handle keeps the continuation in
    /// its registration-ordered list; a completed handle appends it to the
    /// ready queue's tail. Neither path resumes the handle.
    fn register_continuation(
        &mut self,
        frame: &Rc<RefCell<Coroutine>>,
        handle: &Rc<RefCell<Coroutine>>,
        pos: Pos,
        owned: bool,
    ) {
        // compiler.md §116.1 rule 4a: the registration owns one handle count.
        if !owned {
            let mut state = handle.borrow_mut();
            state.owners = state.owners.saturating_add(1);
        }
        #[cfg(test)]
        {
            frame.borrow_mut().suspension_pos = pos.clone();
        }
        frame.borrow_mut().awaiting = Some(AwaitedHandle {
            handle: Rc::clone(handle),
            pos,
        });
        if handle.borrow().completed {
            self.async_ready
                .push_back(AsyncJob::Invocation(Rc::clone(frame)));
        } else {
            handle
                .borrow_mut()
                .waiters
                .push(AsyncJob::Invocation(Rc::clone(frame)));
        }
    }

    /// Ends one holder's ownership of a handle. The last release drops the
    /// interpreter's handle table entry; the completion cache and the values
    /// reachable from it live as long as some owner holds them (§94.2).
    ///
    /// The last release of a handle that holds an exception that no `await`
    /// raised is the uncaught-exception trap (`compiler.md` §116.1 rule 4).
    fn release_coroutine(&self, coroutine: &Rc<RefCell<Coroutine>>) -> Result<(), InterpretError> {
        let key = Rc::as_ptr(coroutine) as usize;
        let mut state = coroutine.borrow_mut();
        if state.owners != 0 {
            state.owners -= 1;
        }
        if state.owners == 0 {
            let mut unobserved = match &state.completion {
                Some(Completion::Exception(payload)) if !payload.observed => {
                    Some(payload.exception.clone())
                }
                _ => None,
            };
            if let CoroutineKind::Aggregate(aggregate) = &mut state.kind {
                if aggregate.reported {
                    unobserved = None;
                }
                if unobserved.is_some() {
                    aggregate.reported = true;
                }
            }
            let unread = matches!(&state.kind, CoroutineKind::Aggregate(a) if a.inputs.iter().any(Option::is_some));
            if state.completed && !unread {
                self.async_registry.borrow_mut().remove(&state.task_id);
            }
            drop(state);
            self.async_handles.borrow_mut().remove(&key);
            if let Some((object, message, pos)) = unobserved {
                return Err(InterpretError::Exception {
                    object,
                    message,
                    pos,
                }
                .settled());
            }
        }
        Ok(())
    }

    fn resume_generator(
        &mut self,
        coroutine: &Rc<RefCell<Coroutine>>,
        value_ty: &Type,
    ) -> Result<Value, InterpretError> {
        if coroutine.borrow().completed {
            return self.iter_result(true, self.zero(value_ty), value_ty);
        }
        let flow = self.execute_coroutine(coroutine)?;
        match flow {
            Flow::Raised(_) => Err(self.invalid(None, "a generator body raised to its consumer")),
            Flow::Returned(_) => {
                coroutine.borrow_mut().completed = true;
                self.iter_result(true, self.zero(value_ty), value_ty)
            }
            Flow::Suspended {
                yielded: Some(value),
                request: None,
            } => Ok(self.iter_result(false, value, value_ty)?),
            Flow::Suspended {
                yielded: None,
                request: None,
            } if *value_ty == Type::Void => self.iter_result(false, Value::Void, value_ty),
            // A generator body holds no `await`: async generators are outside
            // the decided surface, so an async suspension here is invalid LIR
            // rather than work for the §94 scheduler.
            Flow::Suspended { .. } => Err(self.invalid(
                None,
                "a generator suspension does not match its element type",
            )),
        }
    }

    fn execute_frame(&mut self, frame: &mut Frame) -> Result<Flow, InterpretError> {
        loop {
            let function = self.function(frame.function)? as *const l::Function;
            // SAFETY: `module` is immutable for the interpreter's lifetime.
            // Taking `&mut self` while executing an instruction cannot move or
            // mutate the referenced LIR function.
            let function = unsafe { &*function };
            let block = function
                .blocks
                .get(frame.block.0 as usize)
                .filter(|block| block.id == frame.block)
                .ok_or_else(|| {
                    self.invalid(
                        Some(function.pos.clone()),
                        format!("block b{} is missing", frame.block.0),
                    )
                })?;

            if let Some(resume) = frame.resume.take() {
                if let Some(target) = frame.resume_target.take() {
                    self.set_value(&mut frame.values, target, resume, &function.pos)?;
                }
            }
            let mut handler = None;
            for instruction in &block.instructions {
                let outcome = self.execute_instruction(frame, function, instruction);
                // compiler.md §115.6 rule 2: a raise site with a handler
                // edge takes it for an exception.
                match (outcome, instruction.handler()) {
                    (Ok(()), _) => {}
                    (
                        Err(InterpretError::Exception {
                            object,
                            message,
                            pos,
                        }),
                        Some(block),
                    ) => {
                        self.caught = Some((object, message, pos));
                        handler = Some(block);
                    }
                    (Err(error), _) => return Err(error),
                }
                self.invalidate(&instruction.invalidates, &instruction.pos, || {
                    format!("{:?}", instruction.kind)
                });
                if handler.is_some() {
                    break;
                }
            }
            if let Some(handler) = handler {
                frame.block = handler;
                continue;
            }
            match &block.terminator {
                l::Terminator::Branch(target) => {
                    self.take_edge(frame, function, target)?;
                }
                l::Terminator::ConditionalBranch {
                    condition,
                    then_target,
                    else_target,
                } => {
                    let condition = self.operand(frame, condition, &function.pos)?.as_bool()?;
                    self.take_edge(
                        frame,
                        function,
                        if condition { then_target } else { else_target },
                    )?;
                }
                l::Terminator::Switch {
                    value,
                    arms,
                    default,
                } => {
                    let value = self.operand(frame, value, &function.pos)?;
                    let mut selected = default;
                    for arm in arms {
                        let constant = self.constant(&arm.value)?;
                        if self.equal(&value, &constant, &arm.value.ty)? {
                            selected = &arm.target;
                            break;
                        }
                    }
                    self.take_edge(frame, function, selected)?;
                }
                l::Terminator::Return { value, .. } => {
                    return Ok(Flow::Returned(match value {
                        Some(value) => self.operand(frame, value, &function.pos)?,
                        None => Value::Void,
                    }));
                }
                l::Terminator::Unreachable { pos } => {
                    return Err(self.invalid(
                        Some(pos.clone()),
                        "reached a structurally unreachable LIR block",
                    ));
                }
                l::Terminator::Trap(trap) => return Err(self.trap_error(trap)),
                l::Terminator::Suspend {
                    kind,
                    pos,
                    successor,
                    resume_value,
                    arguments,
                    invalidates,
                    traps: _,
                } => {
                    self.invalidate(invalidates, pos, || format!("Suspend({kind:?})"));
                    let destination =
                        function.blocks.get(successor.0 as usize).ok_or_else(|| {
                            self.invalid(
                                Some(function.pos.clone()),
                                format!("suspend successor b{} is missing", successor.0),
                            )
                        })?;
                    // Read every terminator operand before changing the frame.
                    // §68.7.4 and §94.1: the suspension kind decides which
                    // registration the scheduler makes.
                    let pending: (Option<Value>, Option<AsyncRequest>) = match kind {
                        l::SuspendKind::Yield(value) => (
                            value
                                .map(|value| self.get_value(frame, value, &function.pos))
                                .transpose()?,
                            None,
                        ),
                        l::SuspendKind::Async => (None, Some(AsyncRequest::Park(pos.clone()))),
                        l::SuspendKind::AsyncCall { target, operands } => {
                            let arguments = operands
                                .iter()
                                .map(|value| self.get_value(frame, *value, &function.pos))
                                .collect::<Result<Vec<_>, _>>()?;
                            (
                                None,
                                Some(AsyncRequest::Call(target.clone(), arguments, pos.clone())),
                            )
                        }
                        l::SuspendKind::AsyncHandle { handle, owned } => {
                            let Value::Coroutine(handle) =
                                self.get_value(frame, *handle, &function.pos)?
                            else {
                                return Err(self.invalid(
                                    Some(pos.clone()),
                                    "held await operand is not an async handle",
                                ));
                            };
                            (
                                None,
                                Some(AsyncRequest::Handle(handle, pos.clone(), *owned)),
                            )
                        }
                    };
                    let parameters = &destination.parameters[usize::from(resume_value.is_some())..];
                    if arguments.len() != parameters.len() {
                        return Err(self.invalid(
                            Some(function.pos.clone()),
                            format!(
                                "suspend to b{} has {} arguments for {} live-in parameters",
                                successor.0,
                                arguments.len(),
                                parameters.len()
                            ),
                        ));
                    }
                    let saved = arguments
                        .iter()
                        .zip(parameters)
                        .map(|(argument, parameter)| {
                            self.operand(frame, argument, &function.pos)
                                .map(|value| (*parameter, value))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    for local in &mut frame.locals {
                        if local.storage == l::LocalStorageClass::Activation {
                            local.poisoned_at = Some((block.id, pos.clone()));
                        }
                    }
                    // The successor parameters are the entire live-in set. Nothing
                    // else is retained in the suspended frame.
                    frame.values.fill(None);
                    for (parameter, value) in saved {
                        self.set_value(&mut frame.values, parameter, value, &function.pos)?;
                    }
                    frame.block = *successor;
                    frame.resume_target = *resume_value;
                    let (yielded, request) = pending;
                    return Ok(Flow::Suspended { yielded, request });
                }
            }
        }
    }

    fn take_edge(
        &mut self,
        frame: &mut Frame,
        function: &l::Function,
        target: &l::BlockTarget,
    ) -> Result<(), InterpretError> {
        let destination = function
            .blocks
            .get(target.block.0 as usize)
            .ok_or_else(|| {
                self.invalid(
                    Some(function.pos.clone()),
                    format!("edge destination b{} is missing", target.block.0),
                )
            })?;
        if target.arguments.len() != destination.parameters.len() {
            return Err(self.invalid(
                Some(function.pos.clone()),
                format!(
                    "edge to b{} has {} arguments for {} parameters",
                    target.block.0,
                    target.arguments.len(),
                    destination.parameters.len()
                ),
            ));
        }
        let arguments = target
            .arguments
            .iter()
            .map(|operand| self.operand(frame, operand, &function.pos))
            .collect::<Result<Vec<_>, _>>()?;
        for (parameter, value) in destination.parameters.iter().zip(arguments) {
            self.set_value(&mut frame.values, *parameter, value, &function.pos)?;
        }
        frame.block = target.block;
        Ok(())
    }

    fn invoke_target(
        &mut self,
        target: &l::CallTarget,
        mut operands: Vec<Value>,
        operand_types: Option<&[l::ValueType]>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        match &target.kind {
            l::CallTargetKind::Function(function) => self.call_function(*function, operands),
            l::CallTargetKind::StaticClosure(function) => {
                let callable = operands.first().cloned().ok_or_else(|| {
                    self.invalid(pos.cloned(), "static closure call has no callable")
                })?;
                operands.remove(0);
                let Value::Callable(callable) = callable else {
                    return Err(type_error("callable", &callable));
                };
                if callable.function != *function {
                    return Err(self.invalid(
                        pos.cloned(),
                        "static closure callable and direct target disagree",
                    ));
                }
                self.invoke_callable(&callable, operands, pos)
            }
            l::CallTargetKind::Method(method) => {
                let function = self
                    .module
                    .classes
                    .iter()
                    .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
                    .find(|candidate| candidate.id == *method)
                    .map(|method| method.function)
                    .ok_or_else(|| self.invalid(None, format!("method {} is missing", method.0)))?;
                self.call_function(function, operands)
            }
            l::CallTargetKind::Indirect => {
                let callable = operands
                    .first()
                    .cloned()
                    .ok_or_else(|| self.invalid(None, "indirect call has no callable operand"))?;
                operands.remove(0);
                let Value::Callable(callable) = callable else {
                    return Err(type_error("callable", &callable));
                };
                self.invoke_callable(&callable, operands, pos)
            }
            l::CallTargetKind::Foreign(id) => {
                let foreign = self
                    .module
                    .foreign_functions
                    .get(id.0 as usize)
                    .map_or_else(
                        || format!("foreign function {}", id.0),
                        |foreign| foreign.source_name.clone(),
                    );
                Err(InterpretError::Unsupported {
                    reason: format!("{foreign} requires a native library"),
                })
            }
            l::CallTargetKind::Intrinsic(intrinsic) => {
                let operation = self
                    .module
                    .intrinsic_operations
                    .iter()
                    .find(|operation| {
                        operation.family == intrinsic.family
                            && operation.operation == intrinsic.operation
                    })
                    .ok_or_else(|| {
                        self.invalid(
                            None,
                            format!(
                                "intrinsic {:?}.{} is absent from the module table",
                                intrinsic.family, intrinsic.operation
                            ),
                        )
                    })?
                    .semantic_name
                    .clone();
                self.invoke_intrinsic(
                    intrinsic,
                    &operation,
                    operands,
                    operand_types.ok_or_else(|| {
                        self.invalid(pos.cloned(), "intrinsic call has no operand types")
                    })?,
                    target.return_type.as_ref(),
                    pos,
                )
            }
            l::CallTargetKind::BuiltinMethod(method) => self.invoke_builtin(
                *method,
                operands,
                operand_types.ok_or_else(|| {
                    self.invalid(pos.cloned(), "built-in call has no operand types")
                })?,
                target.return_type.as_ref(),
            ),
        }
    }

    fn invoke_callable(
        &mut self,
        callable: &Rc<Callable>,
        arguments: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        #[cfg(not(test))]
        let _ = pos;
        let mut operands = callable.captures.clone();
        operands.extend(arguments);
        let value = self.call_function(callable.function, operands)?;
        if self
            .module
            .functions
            .get(callable.function.0 as usize)
            .is_some_and(|function| function.is_async)
        {
            let Value::Coroutine(handle) = &value else {
                return Err(self.invalid(None, "async callable returns no handle"));
            };
            #[cfg(test)]
            {
                handle.borrow_mut().create_pos = pos.cloned().unwrap_or_else(no_script_site);
            }
            self.async_start(&Rc::clone(handle))?;
        }
        Ok(value)
    }

    fn callable_operand(
        &self,
        value: Option<&Value>,
        operation: &str,
    ) -> Result<Rc<Callable>, InterpretError> {
        match value {
            Some(Value::Callable(callable)) => Ok(Rc::clone(callable)),
            Some(other) => Err(type_error("callable", other)),
            None => Err(self.invalid(None, format!("{operation} has no callback"))),
        }
    }

    fn invoke_builtin(
        &mut self,
        method: l::BuiltinMethod,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        match method {
            l::BuiltinMethod::ArrayPush => {
                let array = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Array.push has no receiver"))?
                    .as_handle()?;
                let value = operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, "Array.push has no value"))?;
                let element_ty = match parameter_types.get(1) {
                    Some(l::ValueType::Data(ty)) => ty,
                    _ => {
                        return Err(
                            self.invalid(None, "Array.push has no declared element parameter type")
                        );
                    }
                };
                let bytes = self.pack(element_ty, value)?;
                // SAFETY: live array and readable element scratch.
                let length = unsafe {
                    ffi::subscript_rt_array_push(&mut *self.context, array, bytes.as_ptr(), 0)
                };
                self.check_runtime(&Pos::new("<builtin>", 1, 1))?;
                Ok(Value::I(length as i64))
            }
            l::BuiltinMethod::ArrayPop => {
                let array = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Array.pop has no receiver"))?
                    .as_handle()?;
                let ty = match result_ty {
                    Some(l::ValueType::Data(ty)) => ty,
                    _ => return Err(self.invalid(None, "Array.pop has no result type")),
                };
                let layout = self
                    .layout_cached(ty)
                    .ok_or_else(|| self.invalid(None, "Array.pop type has no layout"))?;
                let mut bytes = vec![0; layout.size];
                // SAFETY: live array and writable element storage.
                unsafe {
                    ffi::subscript_rt_array_pop(&mut *self.context, array, bytes.as_mut_ptr(), 0)
                };
                self.check_runtime(&Pos::new("<builtin>", 1, 1))?;
                self.unpack(ty, &bytes)
            }
            l::BuiltinMethod::StringSlice => {
                let string = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "string.slice has no receiver"))?
                    .as_handle()?;
                let start = operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, "string.slice has no start"))?
                    .as_i64()? as i32;
                let end = operands
                    .get(2)
                    .ok_or_else(|| self.invalid(None, "string.slice has no end"))?
                    .as_i64()? as i32;
                // SAFETY: live string handle; runtime owns byte-boundary rules.
                let value = unsafe {
                    ffi::subscript_rt_str_slice(&mut *self.context, string, start, end, 0)
                };
                self.check_runtime(&Pos::new("<builtin>", 1, 1))?;
                self.root_handle(value);
                Ok(Value::Handle(value))
            }
            l::BuiltinMethod::GeneratorNext => {
                let Value::Coroutine(coroutine) = operands
                    .first()
                    .cloned()
                    .ok_or_else(|| self.invalid(None, "generator.next has no receiver"))?
                else {
                    return Err(self.invalid(None, "generator.next receiver is not a coroutine"));
                };
                let value_ty = match result_ty {
                    Some(l::ValueType::Data(Type::IterResult(value))) => value,
                    _ => return Err(self.invalid(None, "generator.next result is not IterResult")),
                };
                self.resume_generator(&coroutine, value_ty)
            }
        }
    }

    fn invoke_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        result_ty: Option<&l::ValueType>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        match intrinsic.family {
            l::IntrinsicFamily::Ambient => self.intrinsic_ambient(operation, operands),
            l::IntrinsicFamily::Math => self.intrinsic_math(operation, operands),
            l::IntrinsicFamily::Number => self.intrinsic_number(operation, operands),
            l::IntrinsicFamily::Date => self.intrinsic_date(operation, operands),
            l::IntrinsicFamily::String => self.intrinsic_string(operation, operands),
            l::IntrinsicFamily::Regex => self.intrinsic_regex(operation, operands),
            l::IntrinsicFamily::Text => self.intrinsic_text(operation, operands),
            l::IntrinsicFamily::Json => self.intrinsic_json(operation, operands, result_ty),
            l::IntrinsicFamily::Array => {
                self.intrinsic_array(operation, operands, parameter_types, result_ty)
            }
            l::IntrinsicFamily::Map => self.intrinsic_map(
                operation,
                operands,
                parameter_types,
                intrinsic.type_argument.as_ref(),
                result_ty,
            ),
            l::IntrinsicFamily::Set => self.intrinsic_set(
                operation,
                operands,
                parameter_types,
                intrinsic.type_argument.as_ref(),
                result_ty,
            ),
            l::IntrinsicFamily::ContextBytes => {
                self.intrinsic_context_bytes(intrinsic, operation, operands, pos)
            }
            l::IntrinsicFamily::Worker => Err(InterpretError::Unsupported {
                reason: format!("Worker.{operation} requires a runtime worker adapter"),
            }),
        }
    }
}

unsafe fn callback_state<'a>(env: *const u8) -> &'a mut CallbackState {
    // SAFETY: every runtime callback using these bridges receives the address
    // of a live stack-owned `CallbackState` as its environment.
    unsafe { &mut *env.cast_mut().cast::<CallbackState>() }
}

unsafe fn callback_interpreter(state: &mut CallbackState) -> &mut Interpreter<'static> {
    // SAFETY: the erased pointer was made from the currently executing
    // interpreter. The runtime call is synchronous, so it cannot outlive it.
    unsafe { &mut *state.interpreter.cast::<Interpreter<'static>>() }
}

unsafe fn callback_failed(ctx: *mut Context, state: &mut CallbackState, error: InterpretError) {
    let message = error.to_string();
    // The runtime uses its nonzero word to stop the traversal immediately.
    // The interpreter returns the more precise saved error after the FFI
    // call. An exception stops it with the exception state, which the
    // catch entry clears (compiler.md §115.4). An exception that left an
    // inner runtime loop is already pending, so it is not raised again.
    match &error {
        InterpretError::Exception { .. } if unsafe { (*ctx).exception_pending() } => {}
        InterpretError::Exception {
            object,
            message: text,
            ..
        } => unsafe { (*ctx).raise_exception(*object as *mut u8, text.clone(), 0) },
        _ => unsafe { (*ctx).trap(RuntimeTrapKind::Internal, message, 0) },
    }
    state.error = Some(error);
}

unsafe extern "C" fn map_callback_bridge(
    ctx: *mut Context,
    _code: *const u8,
    env: *const u8,
    value: *const u8,
    key: *const u8,
) {
    let state = unsafe { callback_state(env) };
    if state.error.is_some() {
        return;
    }
    let first_ty = state.first_ty.clone();
    let second_ty = state.second_ty.clone();
    let callable = Rc::clone(&state.callable);
    let interpreter = unsafe { callback_interpreter(state) };
    let result = (|| {
        let value_layout = interpreter
            .layout_cached(&first_ty)
            .ok_or_else(|| interpreter.invalid(None, "Map.forEach value has no layout"))?;
        let key_ty = second_ty
            .as_ref()
            .ok_or_else(|| interpreter.invalid(None, "Map.forEach key type is missing"))?;
        let key_layout = interpreter
            .layout_cached(key_ty)
            .ok_or_else(|| interpreter.invalid(None, "Map.forEach key has no layout"))?;
        let value_bytes = unsafe { std::slice::from_raw_parts(value, value_layout.size) };
        let key_bytes = unsafe { std::slice::from_raw_parts(key, key_layout.size) };
        let value = interpreter.unpack(&first_ty, value_bytes)?;
        let key = interpreter.unpack(key_ty, key_bytes)?;
        let _ = interpreter.invoke_callable(&callable, vec![value, key], None)?;
        Ok::<(), InterpretError>(())
    })();
    if let Err(error) = result {
        unsafe { callback_failed(ctx, state, error) };
    }
}

unsafe extern "C" fn set_callback_bridge(
    ctx: *mut Context,
    _code: *const u8,
    env: *const u8,
    key: *const u8,
) {
    let state = unsafe { callback_state(env) };
    if state.error.is_some() {
        return;
    }
    let first_ty = state.first_ty.clone();
    let callable = Rc::clone(&state.callable);
    let interpreter = unsafe { callback_interpreter(state) };
    let result = (|| {
        let layout = interpreter
            .layout_cached(&first_ty)
            .ok_or_else(|| interpreter.invalid(None, "Set.forEach key has no layout"))?;
        let bytes = unsafe { std::slice::from_raw_parts(key, layout.size) };
        let key = interpreter.unpack(&first_ty, bytes)?;
        let _ = interpreter.invoke_callable(&callable, vec![key], None)?;
        Ok::<(), InterpretError>(())
    })();
    if let Err(error) = result {
        unsafe { callback_failed(ctx, state, error) };
    }
}

unsafe extern "C" fn group_by_callback_bridge(
    ctx: *mut Context,
    _code: *const u8,
    env: *const u8,
    element: *const u8,
    key_out: *mut u8,
) {
    let state = unsafe { callback_state(env) };
    if state.error.is_some() {
        return;
    }
    let first_ty = state.first_ty.clone();
    let second_ty = state.second_ty.clone();
    let callable = Rc::clone(&state.callable);
    let interpreter = unsafe { callback_interpreter(state) };
    let result = (|| {
        let element_layout = interpreter
            .layout_cached(&first_ty)
            .ok_or_else(|| interpreter.invalid(None, "Map.groupBy element has no layout"))?;
        let key_ty = second_ty
            .as_ref()
            .ok_or_else(|| interpreter.invalid(None, "Map.groupBy key type is missing"))?;
        let key_layout = interpreter
            .layout_cached(key_ty)
            .ok_or_else(|| interpreter.invalid(None, "Map.groupBy key has no layout"))?;
        let bytes = unsafe { std::slice::from_raw_parts(element, element_layout.size) };
        let element = interpreter.unpack(&first_ty, bytes)?;
        let key = interpreter.invoke_callable(&callable, vec![element], None)?;
        let packed = interpreter.pack(key_ty, &key)?;
        unsafe { std::ptr::copy_nonoverlapping(packed.as_ptr(), key_out, key_layout.size) };
        Ok::<(), InterpretError>(())
    })();
    if let Err(error) = result {
        unsafe { callback_failed(ctx, state, error) };
    }
}

impl Value {
    fn as_i64(&self) -> Result<i64, InterpretError> {
        match self {
            Value::I(v) => Ok(*v),
            Value::U(v) => Ok(*v as i64),
            other => Err(type_error("integer", other)),
        }
    }

    fn as_u64(&self) -> Result<u64, InterpretError> {
        match self {
            Value::I(v) => Ok(*v as u64),
            Value::U(v) => Ok(*v),
            other => Err(type_error("integer", other)),
        }
    }

    fn as_f64(&self) -> Result<f64, InterpretError> {
        match self {
            Value::F32(v) => Ok(f64::from(*v)),
            Value::F64(v) => Ok(*v),
            Value::I(v) => Ok(*v as f64),
            Value::U(v) => Ok(*v as f64),
            other => Err(type_error("number", other)),
        }
    }

    fn as_bool(&self) -> Result<bool, InterpretError> {
        match self {
            Value::Bool(v) => Ok(*v),
            other => Err(type_error("boolean", other)),
        }
    }

    fn as_handle(&self) -> Result<*mut u8, InterpretError> {
        match self {
            Value::Handle(v) => Ok(*v),
            Value::Null => Ok(std::ptr::null_mut()),
            other => Err(type_error("runtime handle", other)),
        }
    }

    fn as_address(&self) -> Result<&Address, InterpretError> {
        match self {
            Value::Address(value) => Ok(value),
            other => Err(type_error("address", other)),
        }
    }

    fn as_iterator(&self) -> Result<&Rc<IteratorCursor>, InterpretError> {
        match self {
            Value::Iterator(value) => Ok(value),
            other => Err(type_error("iterator cursor", other)),
        }
    }
}

impl Address {
    fn check(&self, instruction: &l::InstructionKind) -> Result<(), InterpretError> {
        if let Some(invalidation) = self.poison.borrow().clone() {
            return Err(InterpretError::PoisonedAddress {
                instruction: format!("{instruction:?}"),
                invalidated_by: invalidation.instruction,
                invalidated_at: invalidation.pos,
            });
        }
        Ok(())
    }
}

fn type_error(expected: &str, actual: &Value) -> InterpretError {
    InterpretError::InvalidLir {
        message: format!("expected {expected}, found {actual:?}"),
        pos: None,
    }
}

fn align_up(value: usize, align: usize) -> usize {
    value.saturating_add(align.saturating_sub(1)) & !align.saturating_sub(1)
}

fn integer_bits(ty: &Type) -> Option<u32> {
    Some(match ty {
        Type::I8 | Type::U8 | Type::Bool => 8,
        Type::I16 | Type::U16 | Type::F16 => 16,
        Type::I32 | Type::U32 | Type::Enum(_) | Type::StringAlias(_) => 32,
        Type::I64 | Type::U64 | Type::Date => 64,
        _ => return None,
    })
}

fn is_signed(ty: &Type) -> bool {
    matches!(
        ty,
        Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Enum(_)
            | Type::StringAlias(_)
            | Type::Date
    )
}

fn mask_bits(value: u64, bits: u32) -> u64 {
    if bits == 64 {
        value
    } else {
        value & ((1u64 << bits) - 1)
    }
}

fn sign_extend(value: u64, bits: u32) -> i64 {
    if bits == 64 {
        value as i64
    } else {
        let shift = 64 - bits;
        ((value << shift) as i64) >> shift
    }
}

fn compare_i64(left: i64, right: i64, operation: l::BinaryOp) -> bool {
    match operation {
        l::BinaryOp::Lt => left < right,
        l::BinaryOp::Le => left <= right,
        l::BinaryOp::Gt => left > right,
        l::BinaryOp::Ge => left >= right,
        _ => false,
    }
}

fn compare_u64(left: u64, right: u64, operation: l::BinaryOp) -> bool {
    match operation {
        l::BinaryOp::Lt => left < right,
        l::BinaryOp::Le => left <= right,
        l::BinaryOp::Gt => left > right,
        l::BinaryOp::Ge => left >= right,
        _ => false,
    }
}

fn compare_f64(left: f64, right: f64, operation: l::BinaryOp) -> bool {
    match operation {
        l::BinaryOp::Lt => left < right,
        l::BinaryOp::Le => left <= right,
        l::BinaryOp::Gt => left > right,
        l::BinaryOp::Ge => left >= right,
        _ => false,
    }
}

fn ffi_len_string(context: &Context, handle: *mut u8) -> i32 {
    if handle.is_null() {
        return 0;
    }
    // SAFETY: interpreter string handles belong to this Context.
    unsafe { context.str_bytes(handle) }.len() as i32
}

fn array_elem_kind(ty: &Type, module: &l::Module) -> u32 {
    match ty {
        Type::F32 => 1,
        Type::F64 => 2,
        Type::Str => 3,
        Type::F16 => 4,
        Type::I8 | Type::I16 | Type::I32 | Type::I64 => 5,
        Type::Class(id)
            if module
                .classes
                .get(id.0)
                .is_some_and(|class| !class.is_value) =>
        {
            0
        }
        _ => 0,
    }
}

fn array_fmt_kind(ty: &Type) -> u32 {
    match ty {
        Type::I32 | Type::Enum(_) | Type::StringAlias(_) => 0,
        Type::U32 => 1,
        Type::I64 | Type::Date => 2,
        Type::U64 => 3,
        Type::F32 => 4,
        Type::F64 => 5,
        Type::Bool => 6,
        Type::Str => 7,
        Type::I8 => 8,
        Type::U8 => 9,
        Type::I16 => 10,
        Type::U16 => 11,
        Type::F16 => 12,
        _ => 0,
    }
}

#[cfg(test)]
mod tests;

fn assoc_key_kind(ty: &Type, module: &l::Module) -> u32 {
    match ty {
        Type::F32 => 1,
        Type::F64 => 2,
        Type::Str => 3,
        Type::Class(id)
            if module
                .classes
                .get(id.0)
                .is_some_and(|class| !class.is_value) =>
        {
            4
        }
        _ => 0,
    }
}

#[cfg(test)]
#[path = "interpreter/completion_tests.rs"]
mod completion_tests;

mod text;

mod collections;
mod instruction;
mod intrinsics;
mod memory;
mod operations;

mod async_all;
use async_all::{AsyncJob, CoroutineKind};

#[cfg(test)]
mod budget_tests;

mod checkpoint;

#[cfg(test)]
mod inspection_tests;

mod async_inspection;
