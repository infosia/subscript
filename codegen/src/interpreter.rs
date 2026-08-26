//! Specification-driven reference interpreter for ordered LIR.
//!
//! This module intentionally consumes only [`subscript_compiler::lir`].  It
//! is a test oracle for the shared lowering, not a shipped execution tier.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::rc::{Rc, Weak};

use subscript_compiler::lir as l;
use subscript_compiler::types::scalar_size_align;
use subscript_compiler::{ClassId, Pos, Type};
use subscript_runtime::context::Context;
use subscript_runtime::ffi;
use subscript_runtime::trap::TrapKind as RuntimeTrapKind;

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
            InterpretError::Trap { kind, pos, message } => {
                write!(f, "{pos}: trap {kind}: {message}")
            }
            InterpretError::Unsupported { reason } => write!(f, "unsupported: {reason}"),
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
/// then kicked in declaration order, and the pending roots are stepped in kick
/// order to quiescence. Runtime-owned strings, arrays, maps, sets, JSON state,
/// dates, regular expressions, and formatting all go through
/// `subscript-runtime`.
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
    subject: Value,
    /// Captured Map/Set storage bound returned by the runtime. Array/string
    /// cursors have no independent iterator object, per stdlib section 14.
    assoc_bound: Option<u64>,
    position: i64,
    next_position: Cell<i64>,
    fixed_bound: Option<i32>,
    assoc_probe_size: Option<usize>,
}

struct Coroutine {
    state: Frame,
    completed: bool,
}

struct CoroutineRoot {
    stack: Vec<Rc<RefCell<Coroutine>>>,
}

struct Frame {
    function: l::FunctionId,
    block: l::BlockId,
    values: Vec<Option<Value>>,
    locals: Vec<Slot>,
    /// A value supplied to the first successor parameter on resume.
    resume: Option<Value>,
}

enum Flow {
    Returned(Value),
    Suspended {
        yielded: Option<Value>,
        async_call: Option<(l::CallTarget, Vec<Value>)>,
    },
}

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
    poison_registry: HashMap<l::ValueId, Vec<Weak<RefCell<Option<Invalidation>>>>>,
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
            poison_registry: HashMap::new(),
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
        let mut pending = Vec::new();
        let result = self.call_function(entry_id, Vec::new())?;
        if let Value::Coroutine(coroutine) = result {
            let mut root = CoroutineRoot {
                stack: vec![coroutine],
            };
            if self.step_coroutine_root(&mut root)?.is_none() {
                pending.push(root);
            }
        }
        for function in self.module.async_roots.clone() {
            let Value::Coroutine(coroutine) = self.call_function(function, Vec::new())? else {
                return Err(self.invalid(None, "async export did not create a coroutine"));
            };
            let mut root = CoroutineRoot {
                stack: vec![coroutine],
            };
            if self.step_coroutine_root(&mut root)?.is_none() {
                pending.push(root);
            }
        }
        while !pending.is_empty() {
            let mut remaining = Vec::with_capacity(pending.len());
            for mut root in pending {
                if self.step_coroutine_root(&mut root)?.is_none() {
                    remaining.push(root);
                }
            }
            pending = remaining;
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
        let locals: Vec<Slot> = function
            .locals
            .iter()
            .map(|local| {
                Rc::new(RefCell::new(match &local.ty {
                    l::ValueType::Data(ty) => self.zero(ty),
                    l::ValueType::Address(_) | l::ValueType::Iterator(_) => Value::Void,
                }))
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
                *slot.borrow_mut() = argument;
            }
        }
        let frame = Frame {
            function: id,
            block: function.entry,
            values,
            locals,
            resume: None,
        };
        if function.is_generator || function.is_async {
            for trap in &function.creation_traps {
                if trap.kind == l::TrapKind::Allocation && self.context.trapped() {
                    return Err(self.trap_error(trap));
                }
            }
            return Ok(Value::Coroutine(Rc::new(RefCell::new(Coroutine {
                state: frame,
                completed: false,
            }))));
        }
        let mut frame = frame;
        match self.execute_frame(&mut frame)? {
            Flow::Returned(value) => Ok(value),
            Flow::Suspended { .. } => Err(self.invalid(
                Some(function.pos.clone()),
                "non-coroutine function suspended",
            )),
        }
    }

    fn drive_coroutine(
        &mut self,
        coroutine: &Rc<RefCell<Coroutine>>,
    ) -> Result<Value, InterpretError> {
        let mut root = CoroutineRoot {
            stack: vec![Rc::clone(coroutine)],
        };
        loop {
            if let Some(value) = self.step_coroutine_root(&mut root)? {
                return Ok(value);
            }
        }
    }

    /// Advances the innermost suspended frame once, then runs through
    /// completed awaited calls until the root suspends again or completes.
    fn step_coroutine_root(
        &mut self,
        root: &mut CoroutineRoot,
    ) -> Result<Option<Value>, InterpretError> {
        loop {
            let Some(coroutine) = root.stack.last().cloned() else {
                return Ok(Some(Value::Void));
            };
            let flow = {
                let mut coroutine = coroutine.borrow_mut();
                if coroutine.completed {
                    Flow::Returned(Value::Void)
                } else {
                    self.execute_frame(&mut coroutine.state)?
                }
            };
            match flow {
                Flow::Returned(value) => {
                    coroutine.borrow_mut().completed = true;
                    root.stack.pop();
                    if let Some(parent) = root.stack.last() {
                        parent.borrow_mut().state.resume = Some(value);
                    } else {
                        return Ok(Some(value));
                    }
                }
                Flow::Suspended {
                    yielded: _,
                    async_call: Some((target, arguments)),
                } => match self.invoke_target(&target, arguments, None)? {
                    Value::Coroutine(child) => root.stack.push(child),
                    value => coroutine.borrow_mut().state.resume = Some(value),
                },
                Flow::Suspended {
                    yielded: _,
                    async_call: None,
                } => return Ok(None),
            }
        }
    }

    fn resume_generator(
        &mut self,
        coroutine: &Rc<RefCell<Coroutine>>,
        value_ty: &Type,
    ) -> Result<Value, InterpretError> {
        let flow = {
            let mut coroutine = coroutine.borrow_mut();
            if coroutine.completed {
                return self.iter_result(true, self.zero(value_ty), value_ty);
            }
            self.execute_frame(&mut coroutine.state)?
        };
        match flow {
            Flow::Returned(_) => {
                coroutine.borrow_mut().completed = true;
                self.iter_result(true, self.zero(value_ty), value_ty)
            }
            Flow::Suspended {
                yielded: Some(value),
                async_call: None,
            } => Ok(self.iter_result(false, value, value_ty)?),
            Flow::Suspended { async_call, .. } => {
                if let Some((target, arguments)) = async_call {
                    let called = self.invoke_target(&target, arguments, None)?;
                    let resume = match called {
                        Value::Coroutine(child) => self.drive_coroutine(&child)?,
                        other => other,
                    };
                    coroutine.borrow_mut().state.resume = Some(resume);
                }
                self.resume_generator(coroutine, value_ty)
            }
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

            if !block.parameters.is_empty() {
                if let Some(resume) = frame.resume.take() {
                    self.set_value(
                        &mut frame.values,
                        block.parameters[0],
                        resume,
                        &function.pos,
                    )?;
                }
            }
            for instruction in &block.instructions {
                self.execute_instruction(frame, function, instruction)?;
                self.invalidate(&instruction.invalidates, &instruction.pos, || {
                    format!("{:?}", instruction.kind)
                });
                self.check_runtime(&instruction.pos)?;
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
                    let pending = match kind {
                        l::SuspendKind::Yield(value) => (
                            value
                                .map(|value| self.get_value(frame, value, &function.pos))
                                .transpose()?,
                            None,
                        ),
                        l::SuspendKind::Async => (None, None),
                        l::SuspendKind::AsyncCall { target, operands } => {
                            let arguments = operands
                                .iter()
                                .map(|value| self.get_value(frame, *value, &function.pos))
                                .collect::<Result<Vec<_>, _>>()?;
                            (None, Some((target.clone(), arguments)))
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
                    // The successor parameters are the entire live-in set. Nothing
                    // else is retained in the suspended frame.
                    frame.values.fill(None);
                    for (parameter, value) in saved {
                        self.set_value(&mut frame.values, parameter, value, &function.pos)?;
                    }
                    frame.block = *successor;
                    let (yielded, async_call) = pending;
                    return Ok(Flow::Suspended {
                        yielded,
                        async_call,
                    });
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

    fn execute_instruction(
        &mut self,
        frame: &mut Frame,
        function: &l::Function,
        instruction: &l::Instruction,
    ) -> Result<(), InterpretError> {
        let operands = instruction
            .operands
            .iter()
            .map(|operand| self.operand(frame, operand, &instruction.pos))
            .collect::<Result<Vec<_>, _>>()?;
        let result_ty = instruction
            .result
            .and_then(|value| function.values.get(value.0 as usize))
            .map(|value| &value.ty);
        let result = match &instruction.kind {
            l::InstructionKind::Copy => Some(
                self.copy_value(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    result_ty,
                )?,
            ),
            l::InstructionKind::StringLiteral(text) => Some(Value::Handle(
                self.alloc_string(text.as_bytes(), &instruction.pos)?,
            )),
            l::InstructionKind::Zero => Some(match result_ty {
                Some(l::ValueType::Data(ty)) => self.zero(ty),
                _ => {
                    return Err(
                        self.invalid(Some(instruction.pos.clone()), "Zero has no data result")
                    );
                }
            }),
            l::InstructionKind::LoadLocal(local) => Some(
                frame
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("local {} is missing", local.0),
                        )
                    })?
                    .borrow()
                    .clone(),
            ),
            l::InstructionKind::StoreLocal(local) => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                *frame
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("local {} is missing", local.0),
                        )
                    })?
                    .borrow_mut() = value.clone();
                None
            }
            l::InstructionKind::AddressOfLocal(local) => Some(Value::Address(Address {
                target: AddressTarget::Slot(
                    frame
                        .locals
                        .get(local.0 as usize)
                        .ok_or_else(|| {
                            self.invalid(
                                Some(instruction.pos.clone()),
                                format!("local {} is missing", local.0),
                            )
                        })?
                        .clone(),
                ),
                pointee: self.address_pointee(result_ty, instruction)?,
                poison: Rc::new(RefCell::new(None)),
            })),
            l::InstructionKind::LoadGlobal(global) => Some(
                self.globals
                    .get(global.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("global {} is missing", global.0),
                        )
                    })?
                    .borrow()
                    .clone(),
            ),
            l::InstructionKind::StoreGlobal(global) => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                *self
                    .globals
                    .get(global.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("global {} is missing", global.0),
                        )
                    })?
                    .borrow_mut() = value.clone();
                None
            }
            l::InstructionKind::AddressOfGlobal(global) => Some(Value::Address(Address {
                target: AddressTarget::Slot(
                    self.globals
                        .get(global.0 as usize)
                        .ok_or_else(|| {
                            self.invalid(
                                Some(instruction.pos.clone()),
                                format!("global {} is missing", global.0),
                            )
                        })?
                        .clone(),
                ),
                pointee: self.address_pointee(result_ty, instruction)?,
                poison: Rc::new(RefCell::new(None)),
            })),
            l::InstructionKind::FunctionRef(function) => Some(Value::Callable(Rc::new(Callable {
                function: *function,
                captures: Vec::new(),
            }))),
            l::InstructionKind::MakeClosure(function) => Some(Value::Callable(Rc::new(Callable {
                function: *function,
                captures: operands,
            }))),
            l::InstructionKind::Unary(operator) => Some(
                self.unary(
                    *operator,
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    result_ty,
                )?,
            ),
            l::InstructionKind::Binary(operator) => Some(
                self.binary(
                    *operator,
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.missing_operand(instruction, 1))?,
                    result_ty,
                    function,
                    instruction,
                )?,
            ),
            l::InstructionKind::Cast | l::InstructionKind::Coerce => Some(
                self.convert(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    result_ty,
                    self.instruction_operand_type(function, instruction, 0),
                    instruction,
                )?,
            ),
            l::InstructionKind::AllocateClass(class) => {
                Some(self.allocate_class(*class, result_ty, &instruction.pos)?)
            }
            l::InstructionKind::AddressOfValue => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .clone();
                Some(Value::Address(Address {
                    target: AddressTarget::Slot(Rc::new(RefCell::new(value))),
                    pointee: self.address_pointee(result_ty, instruction)?,
                    poison: Rc::new(RefCell::new(None)),
                }))
            }
            l::InstructionKind::AddressOfField(field) => {
                let base = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                Some(Value::Address(self.address_field(
                    base,
                    *field,
                    result_ty,
                    instruction,
                )?))
            }
            l::InstructionKind::AddressOfIndex { checked: _ } => {
                let base = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                let index = operands
                    .get(1)
                    .ok_or_else(|| self.missing_operand(instruction, 1))?
                    .as_i64()?;
                Some(Value::Address(self.address_index(
                    base,
                    index,
                    result_ty,
                    instruction,
                )?))
            }
            l::InstructionKind::LoadAddress => {
                let address = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_address()?;
                address.check(&instruction.kind)?;
                Some(self.load_address(address)?)
            }
            l::InstructionKind::StoreAddress => {
                let address = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_address()?;
                address.check(&instruction.kind)?;
                let value = operands
                    .get(1)
                    .ok_or_else(|| self.missing_operand(instruction, 1))?;
                self.store_address(address, value)?;
                None
            }
            l::InstructionKind::LoadField(field) => {
                let base = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                Some(match base {
                    Value::Blob(bytes) => {
                        let pointee = self.data_result_type(result_ty, instruction)?;
                        let offset = self.field_offset(*field, pointee, instruction)?;
                        self.unpack(pointee, bytes.get(offset..).unwrap_or_default())?
                    }
                    _ => {
                        let address = self.address_field(base, *field, result_ty, instruction)?;
                        self.load_address(&address)?
                    }
                })
            }
            l::InstructionKind::Length => Some(Value::I(
                self.length(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    self.instruction_operand_type(function, instruction, 0),
                )? as i64,
            )),
            l::InstructionKind::ForeignArrayData => {
                let handle = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_handle()?;
                // SAFETY: verified LIR supplies a live dynamic-array handle.
                let data = unsafe { ffi::subscript_rt_array_data(&*self.context, handle) };
                Some(Value::Address(Address {
                    target: AddressTarget::Pointer(data.cast_mut()),
                    pointee: self.address_pointee(result_ty, instruction)?,
                    poison: Rc::new(RefCell::new(None)),
                }))
            }
            l::InstructionKind::ArrayLiteral => {
                let ty = self.data_result_type(result_ty, instruction)?;
                Some(self.array_literal(ty, &operands, &instruction.pos)?)
            }
            l::InstructionKind::ArraySpreadLiteral(parts) => {
                let ty = self.data_result_type(result_ty, instruction)?;
                Some(self.array_spread_literal(ty, parts, &operands, &instruction.pos)?)
            }
            l::InstructionKind::Template(parts) => Some(Value::Handle(self.template(
                parts,
                &operands,
                function,
                instruction,
            )?)),
            l::InstructionKind::Call(target) => {
                Some(self.invoke_target(target, operands, Some(&instruction.pos))?)
            }
            l::InstructionKind::IteratorCreate(kind) => {
                let subject_ty = self
                    .instruction_operand_type(function, instruction, 0)
                    .cloned();
                Some(
                    self.iterator_create(
                        *kind,
                        operands
                            .first()
                            .ok_or_else(|| self.missing_operand(instruction, 0))?
                            .clone(),
                        subject_ty.as_ref(),
                        result_ty,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorBound => Some(Value::I(
                self.iterator_bound(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                )? as i64,
            )),
            l::InstructionKind::IteratorHasNext => {
                let index = operands
                    .get(1)
                    .ok_or_else(|| self.missing_operand(instruction, 1))?
                    .as_i64()?;
                let bound = operands
                    .get(2)
                    .ok_or_else(|| self.missing_operand(instruction, 2))?
                    .as_i64()?;
                let cursor = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_iterator()?;
                Some(Value::Bool(self.iterator_has_next(
                    cursor,
                    index,
                    bound,
                    &instruction.pos,
                )?))
            }
            l::InstructionKind::IteratorValue => Some(
                self.iterator_value(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.missing_operand(instruction, 1))?
                        .as_i64()?,
                    result_ty,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::IteratorAdvance => Some(
                self.iterator_advance(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                )?,
            ),
        };
        if let Some(id) = instruction.result {
            let result = result.ok_or_else(|| {
                self.invalid(
                    Some(instruction.pos.clone()),
                    format!("{:?} declares a result but produced none", instruction.kind),
                )
            })?;
            if let Some(l::ValueType::Address(address_ty)) = result_ty {
                if let (Some(base), Value::Address(address)) = (address_ty.array_base, &result) {
                    self.poison_registry
                        .entry(base)
                        .or_default()
                        .push(Rc::downgrade(&address.poison));
                }
            }
            self.set_value(&mut frame.values, id, result, &instruction.pos)?;
        }
        Ok(())
    }

    fn operand(
        &self,
        frame: &Frame,
        operand: &l::Operand,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        match operand {
            l::Operand::Value(value) => self.get_value(frame, *value, pos),
            l::Operand::Constant(value) => self.constant(value),
        }
    }

    fn instruction_operand_type<'a>(
        &'a self,
        function: &'a l::Function,
        instruction: &'a l::Instruction,
        index: usize,
    ) -> Option<&'a Type> {
        match instruction.operands.get(index)? {
            l::Operand::Constant(constant) => Some(&constant.ty),
            l::Operand::Value(value) => match &function.values.get(value.0 as usize)?.ty {
                l::ValueType::Data(ty) => Some(ty),
                l::ValueType::Address(address) => Some(&address.pointee),
                l::ValueType::Iterator(_) => None,
            },
        }
    }

    fn constant(&self, constant: &l::Constant) -> Result<Value, InterpretError> {
        Ok(match (&constant.ty, &constant.kind) {
            (
                Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::Enum(_)
                | Type::StringAlias(_)
                | Type::Date,
                l::ConstantKind::Integer(v),
            ) => Value::I(*v),
            (Type::U8 | Type::U16 | Type::U32 | Type::U64, l::ConstantKind::Integer(v)) => {
                Value::U(*v as u64)
            }
            (Type::F32, l::ConstantKind::FloatBits(v)) => Value::F32(f32::from_bits(*v as u32)),
            (Type::F64, l::ConstantKind::FloatBits(v)) => Value::F64(f64::from_bits(*v)),
            (Type::F16, l::ConstantKind::FloatBits(v)) => {
                Value::U(ffi::subscript_rt_f16_from_f64(f64::from_bits(*v)) as u64)
            }
            (Type::Bool, l::ConstantKind::Boolean(v)) => Value::Bool(*v),
            (_, l::ConstantKind::Null) => Value::Null,
            _ => {
                return Err(self.invalid(
                    None,
                    format!("constant payload disagrees with type {:?}", constant.ty),
                ));
            }
        })
    }

    fn get_value(&self, frame: &Frame, id: l::ValueId, pos: &Pos) -> Result<Value, InterpretError> {
        frame
            .values
            .get(id.0 as usize)
            .and_then(Clone::clone)
            .ok_or_else(|| {
                self.invalid(
                    Some(pos.clone()),
                    format!("value %{} is not present in the current live-in set", id.0),
                )
            })
    }

    fn set_value(
        &self,
        values: &mut [Option<Value>],
        id: l::ValueId,
        value: Value,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let slot = values.get_mut(id.0 as usize).ok_or_else(|| {
            self.invalid(Some(pos.clone()), format!("value %{} is missing", id.0))
        })?;
        *slot = Some(value);
        Ok(())
    }

    fn invalidate(
        &mut self,
        bases: &[l::ValueId],
        pos: &Pos,
        instruction: impl FnOnce() -> String,
    ) {
        let mut instruction = Some(instruction);
        let mut description = None;
        for base in bases {
            if let Some(addresses) = self.poison_registry.get_mut(base) {
                addresses.retain(|address| {
                    if let Some(address) = address.upgrade() {
                        let mut poison = address.borrow_mut();
                        if poison.is_none() {
                            let description = description.get_or_insert_with(|| {
                                instruction
                                    .take()
                                    .expect("invalidation description used once")(
                                )
                            });
                            *poison = Some(Invalidation {
                                instruction: description.clone(),
                                pos: pos.clone(),
                            });
                        }
                        true
                    } else {
                        false
                    }
                });
            }
        }
    }

    fn invalid(&self, pos: Option<Pos>, message: impl Into<String>) -> InterpretError {
        InterpretError::InvalidLir {
            message: message.into(),
            pos,
        }
    }

    fn missing_operand(&self, instruction: &l::Instruction, index: usize) -> InterpretError {
        self.invalid(
            Some(instruction.pos.clone()),
            format!("{:?} is missing operand {index}", instruction.kind),
        )
    }

    fn trap_error(&self, trap: &l::Trap) -> InterpretError {
        InterpretError::Trap {
            kind: format!("{:?}", trap.kind),
            pos: trap.pos.clone(),
            message: "LIR trap terminator/check fired".to_string(),
        }
    }

    fn check_runtime(&self, pos: &Pos) -> Result<(), InterpretError> {
        let Some(trap) = self.context.trap_record() else {
            return Ok(());
        };
        Err(InterpretError::Trap {
            kind: trap.kind.rule().to_string(),
            pos: pos.clone(),
            message: trap.message.clone(),
        })
    }

    fn invoke_target(
        &mut self,
        target: &l::CallTarget,
        mut operands: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        match &target.kind {
            l::CallTargetKind::Function(function) => self.call_function(*function, operands),
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
                let mut arguments = callable.captures.clone();
                arguments.extend(operands);
                self.call_function(callable.function, arguments)
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
                    &target.parameter_types,
                    target.return_type.as_ref(),
                    pos,
                )
            }
            l::CallTargetKind::BuiltinMethod(method) => self.invoke_builtin(
                *method,
                operands,
                &target.parameter_types,
                target.return_type.as_ref(),
            ),
        }
    }

    fn invoke_callable(
        &mut self,
        callable: &Rc<Callable>,
        arguments: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let mut operands = callable.captures.clone();
        operands.extend(arguments);
        self.call_function(callable.function, operands)
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

    fn intrinsic_ambient(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        match operation {
            "Print" => {
                let string = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "print has no argument"))?
                    .as_handle()?;
                // SAFETY: live runtime string.
                unsafe { ffi::subscript_rt_print(&mut *self.context, string) };
                Ok(Value::Void)
            }
            "Collect" => {
                // SAFETY: this interpreter exclusively owns the Context.
                unsafe { ffi::subscript_rt_collect(&mut *self.context) };
                Ok(Value::Void)
            }
            "UnsafeDelete" => {
                let handle = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Context.free has no argument"))?
                    .as_handle()?;
                self.context.delete(handle as usize, 0);
                Ok(Value::Void)
            }
            "Unreachable" => {
                self.context.trap(
                    RuntimeTrapKind::UnreachableReached,
                    "execution reached unreachable()",
                    0,
                );
                Ok(Value::Void)
            }
            _ => Err(self.invalid(None, format!("unknown Ambient intrinsic {operation}"))),
        }
    }

    fn intrinsic_context_bytes(
        &mut self,
        intrinsic: &l::Intrinsic,
        operation: &str,
        operands: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        let ty = intrinsic.type_argument.as_ref().ok_or_else(|| {
            self.invalid(
                pos.cloned(),
                format!("Context.{operation} has no type argument"),
            )
        })?;
        let layout = self.layout_cached(ty).ok_or_else(|| {
            self.invalid(
                pos.cloned(),
                format!("Context.{operation} type has no storage layout"),
            )
        })?;
        let size = u32::try_from(layout.size).map_err(|_| {
            self.invalid(
                pos.cloned(),
                format!("Context.{operation} storage size exceeds u32"),
            )
        })?;
        let trap_pos = pos
            .cloned()
            .unwrap_or_else(|| Pos::new("<Context bytes>", 1, 1));
        match operation {
            "BytesOf" => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.BytesOf has no value"))?;
                let mut bytes = self.pack(ty, value)?;
                self.zero_padding(ty, &mut bytes, 0)?;
                let handle = unsafe {
                    ffi::subscript_rt_array_from_bytes(&mut *self.context, bytes.as_ptr(), size, 0)
                };
                self.check_runtime(&trap_pos)?;
                self.root_handle(handle);
                Ok(Value::Handle(handle))
            }
            "BytesInto" => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.BytesInto has no value"))?;
                let target = operands
                    .get(1)
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.BytesInto has no target"))?
                    .as_handle()?;
                let offset = u32::try_from(
                    operands
                        .get(2)
                        .ok_or_else(|| {
                            self.invalid(pos.cloned(), "Context.BytesInto has no offset")
                        })?
                        .as_u64()?,
                )
                .map_err(|_| self.invalid(pos.cloned(), "Context.BytesInto offset exceeds u32"))?;
                let mut bytes = self.pack(ty, value)?;
                self.zero_padding(ty, &mut bytes, 0)?;
                let range = unsafe {
                    ffi::subscript_rt_array_byte_range(&mut *self.context, target, offset, size, 0)
                };
                self.check_runtime(&trap_pos)?;
                if size != 0 {
                    if range.is_null() {
                        return Err(self.invalid(pos.cloned(), "Context.BytesInto returned null"));
                    }
                    unsafe {
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), range, layout.size);
                    }
                }
                Ok(Value::Void)
            }
            "FromBytes" => {
                let source = operands
                    .first()
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.FromBytes has no source"))?
                    .as_handle()?;
                let offset = u32::try_from(
                    operands
                        .get(1)
                        .ok_or_else(|| {
                            self.invalid(pos.cloned(), "Context.FromBytes has no offset")
                        })?
                        .as_u64()?,
                )
                .map_err(|_| self.invalid(pos.cloned(), "Context.FromBytes offset exceeds u32"))?;
                let range = unsafe {
                    ffi::subscript_rt_array_byte_range(&mut *self.context, source, offset, size, 0)
                };
                self.check_runtime(&trap_pos)?;
                if size != 0 && range.is_null() {
                    return Err(self.invalid(pos.cloned(), "Context.FromBytes returned null"));
                }
                if layout.size == 0 {
                    return self.unpack(ty, &[]);
                }
                let bytes = unsafe { std::slice::from_raw_parts(range, layout.size) };
                self.unpack(ty, bytes)
            }
            _ => Err(self.invalid(
                pos.cloned(),
                format!("unknown Context byte intrinsic {operation}"),
            )),
        }
    }

    fn intrinsic_math(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let unary = |function: fn(f64) -> f64| -> Result<Value, InterpretError> {
            Ok(Value::F64(function(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, format!("Math.{operation} has no operand")))?
                    .as_f64()?,
            )))
        };
        let binary = |function: fn(f64, f64) -> f64| -> Result<Value, InterpretError> {
            Ok(Value::F64(function(
                operands
                    .first()
                    .ok_or_else(|| {
                        self.invalid(None, format!("Math.{operation} has no left operand"))
                    })?
                    .as_f64()?,
                operands
                    .get(1)
                    .ok_or_else(|| {
                        self.invalid(None, format!("Math.{operation} has no right operand"))
                    })?
                    .as_f64()?,
            )))
        };
        match operation {
            "Abs" => unary(subscript_runtime::math::abs),
            "Acos" => unary(subscript_runtime::math::acos),
            "Acosh" => unary(subscript_runtime::math::acosh),
            "Asin" => unary(subscript_runtime::math::asin),
            "Asinh" => unary(subscript_runtime::math::asinh),
            "Atan" => unary(subscript_runtime::math::atan),
            "Atanh" => unary(subscript_runtime::math::atanh),
            "Cbrt" => unary(subscript_runtime::math::cbrt),
            "Ceil" => unary(subscript_runtime::math::ceil),
            "Cos" => unary(subscript_runtime::math::cos),
            "Cosh" => unary(subscript_runtime::math::cosh),
            "Exp" => unary(subscript_runtime::math::exp),
            "Expm1" => unary(subscript_runtime::math::expm1),
            "Floor" => unary(subscript_runtime::math::floor),
            "Log" => unary(subscript_runtime::math::log),
            "Log1p" => unary(subscript_runtime::math::log1p),
            "Log10" => unary(subscript_runtime::math::log10),
            "Log2" => unary(subscript_runtime::math::log2),
            "Round" => unary(subscript_runtime::math::round),
            "Sign" => unary(subscript_runtime::math::sign),
            "Sin" => unary(subscript_runtime::math::sin),
            "Sinh" => unary(subscript_runtime::math::sinh),
            "Sqrt" => unary(subscript_runtime::math::sqrt),
            "Tan" => unary(subscript_runtime::math::tan),
            "Tanh" => unary(subscript_runtime::math::tanh),
            "Trunc" => unary(subscript_runtime::math::trunc),
            "Atan2" => binary(subscript_runtime::math::atan2),
            "Hypot" => binary(subscript_runtime::math::hypot),
            "Pow" => binary(subscript_runtime::math::pow),
            "Max" => binary(subscript_runtime::math::max),
            "Min" => binary(subscript_runtime::math::min),
            "Random" => {
                // SAFETY: exclusively owned runtime Context.
                Ok(Value::F64(unsafe {
                    ffi::subscript_rt_math_random(&mut *self.context)
                }))
            }
            "Clz32" => Ok(Value::I(subscript_runtime::math::clz32(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.clz32 has no operand"))?
                    .as_u64()? as u32,
            ) as i64)),
            "Imul" => Ok(Value::I(subscript_runtime::math::imul(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.imul has no left operand"))?
                    .as_i64()? as i32,
                operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, "Math.imul has no right operand"))?
                    .as_i64()? as i32,
            ) as i64)),
            "Fround" => Ok(Value::F64(subscript_runtime::math::fround(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.fround has no operand"))?
                    .as_f64()?,
            ))),
            "F32ToBits" => Ok(Value::U(subscript_runtime::math::f32_to_bits(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.f32ToBits has no operand"))?
                    .as_f64()?,
            ) as u64)),
            "F32FromBits" => Ok(Value::F64(subscript_runtime::math::f32_from_bits(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.f32FromBits has no operand"))?
                    .as_u64()? as u32,
            ))),
            _ => Err(self.invalid(None, format!("unknown Math intrinsic {operation}"))),
        }
    }

    fn intrinsic_number(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let first = || {
            operands.first().ok_or_else(|| {
                self.invalid(None, format!("Number.{operation} has no first operand"))
            })
        };
        let second = || {
            operands.get(1).ok_or_else(|| {
                self.invalid(None, format!("Number.{operation} has no second operand"))
            })
        };
        let value = match operation {
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsNaN" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_nan(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsFinite" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_finite(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsInteger" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_integer(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsSafeInteger" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_safe_integer(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: live runtime string and explicit radix.
            "ParseInt" => Value::F64(unsafe {
                ffi::subscript_rt_num_parse_int(
                    context,
                    first()?.as_handle()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: live runtime string.
            "ParseFloat" => Value::F64(unsafe {
                ffi::subscript_rt_num_parse_float(context, first()?.as_handle()?, 0)
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToFixed" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_fixed(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToStringF32" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_string_f32(
                    context,
                    first()?.as_f64()? as f32,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToStringF64" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_string_f64(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToExponential" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_exponential(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToPrecision" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_precision(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            _ => return Err(self.invalid(None, format!("unknown Number intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<number>", 1, 1))?;
        if let Value::Handle(handle) = value {
            self.root_handle(handle);
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    fn intrinsic_date(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let first = || {
            operands
                .first()
                .ok_or_else(|| self.invalid(None, format!("Date.{operation} has no receiver")))
        };
        let value =
            match operation {
                // SAFETY: scalar Date representation and owned Context.
                "New" => {
                    Value::I(unsafe { ffi::subscript_rt_date_new(context, first()?.as_i64()?, 0) })
                }
                "Utc" => {
                    if operands.len() != 7 {
                        return Err(self
                            .invalid(None, format!("Date.UTC has {} arguments", operands.len())));
                    }
                    // SAFETY: seven checker-normalized scalar components.
                    Value::I(unsafe {
                        ffi::subscript_rt_date_utc(
                            context,
                            operands[0].as_i64()? as i32,
                            operands[1].as_i64()? as i32,
                            operands[2].as_i64()? as i32,
                            operands[3].as_i64()? as i32,
                            operands[4].as_i64()? as i32,
                            operands[5].as_i64()? as i32,
                            operands[6].as_i64()? as i32,
                            0,
                        )
                    })
                }
                // SAFETY: owned Context clock.
                "Now" => Value::I(unsafe { ffi::subscript_rt_date_now(context) }),
                "GetUtcFullYear" | "GetUtcMonth" | "GetUtcDate" | "GetUtcDay" | "GetUtcHours"
                | "GetUtcMinutes" | "GetUtcSeconds" | "GetUtcMilliseconds" => {
                    let field = match operation {
                        "GetUtcFullYear" => 0,
                        "GetUtcMonth" => 1,
                        "GetUtcDate" => 2,
                        "GetUtcDay" => 3,
                        "GetUtcHours" => 4,
                        "GetUtcMinutes" => 5,
                        "GetUtcSeconds" => 6,
                        _ => 7,
                    };
                    // SAFETY: valid Date field code.
                    Value::I(unsafe {
                        ffi::subscript_rt_date_get(context, first()?.as_i64()?, field)
                    } as i64)
                }
                // SAFETY: scalar Date; runtime owns ISO formatting/range checks.
                "ToIso" => Value::Handle(unsafe {
                    ffi::subscript_rt_date_to_iso(context, first()?.as_i64()?, 0)
                }),
                _ => return Err(self.invalid(None, format!("unknown Date intrinsic {operation}"))),
            };
        self.check_runtime(&Pos::new("<date>", 1, 1))?;
        if let Value::Handle(handle) = value {
            self.root_handle(handle);
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    fn intrinsic_string(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let receiver = operands
            .first()
            .ok_or_else(|| self.invalid(None, format!("String.{operation} has no receiver")))?
            .as_handle()?;
        let context = &mut *self.context as *mut Context;
        let handle = |index: usize| -> Result<*mut u8, InterpretError> {
            operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("String.{operation} has no operand {index}"))
                })?
                .as_handle()
        };
        let integer = |index: usize| -> Result<i32, InterpretError> {
            Ok(operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("String.{operation} has no operand {index}"))
                })?
                .as_i64()? as i32)
        };
        let value = match operation {
            // SAFETY: live strings and checker-normalized scalar arguments.
            "Slice" => Value::Handle(unsafe {
                ffi::subscript_rt_str_slice(context, receiver, integer(1)?, integer(2)?, 0)
            }),
            // SAFETY: live strings and scalar byte position.
            "IndexOf" => Value::I(unsafe {
                ffi::subscript_rt_str_index_of(context, receiver, handle(1)?, integer(2)?)
            } as i64),
            // SAFETY: live strings.
            "LastIndexOf" => Value::I(unsafe {
                ffi::subscript_rt_str_last_index_of(context, receiver, handle(1)?)
            } as i64),
            // SAFETY: live strings and scalar byte position.
            "Includes" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_str_includes(context, receiver, handle(1)?, integer(2)?)
                } != 0,
            ),
            // SAFETY: live strings and scalar byte position.
            "StartsWith" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_str_starts_with(context, receiver, handle(1)?, integer(2)?)
                } != 0,
            ),
            // SAFETY: live strings and scalar byte position.
            "EndsWith" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_str_ends_with(context, receiver, handle(1)?, integer(2)?)
                } != 0,
            ),
            // SAFETY: live string; runtime owns range check.
            "CharCodeAt" => Value::I(unsafe {
                ffi::subscript_rt_str_char_code_at(context, receiver, integer(1)?, 0)
            } as i64),
            // SAFETY: live strings; runtime owns split allocation.
            "Split" => Value::Handle(unsafe {
                ffi::subscript_rt_str_split(context, receiver, handle(1)?, 0)
            }),
            // SAFETY: live string; runtime owns Unicode trimming.
            "Trim" => Value::Handle(unsafe { ffi::subscript_rt_str_trim(context, receiver, 0) }),
            // SAFETY: live string; runtime owns Unicode trimming.
            "TrimStart" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_trim_start(context, receiver, 0) })
            }
            // SAFETY: live string; runtime owns Unicode trimming.
            "TrimEnd" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_trim_end(context, receiver, 0) })
            }
            // SAFETY: live string; runtime owns range/allocation.
            "Repeat" => Value::Handle(unsafe {
                ffi::subscript_rt_str_repeat(context, receiver, integer(1)?, 0)
            }),
            // SAFETY: live strings; runtime owns byte padding.
            "PadStart" => Value::Handle(unsafe {
                ffi::subscript_rt_str_pad_start(context, receiver, integer(1)?, handle(2)?, 0)
            }),
            // SAFETY: live strings; runtime owns byte padding.
            "PadEnd" => Value::Handle(unsafe {
                ffi::subscript_rt_str_pad_end(context, receiver, integer(1)?, handle(2)?, 0)
            }),
            // SAFETY: live string; runtime owns Unicode conversion.
            "ToUpperCase" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_to_upper(context, receiver, 0) })
            }
            // SAFETY: live string; runtime owns Unicode conversion.
            "ToLowerCase" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_to_lower(context, receiver, 0) })
            }
            // SAFETY: live strings; runtime owns replacement semantics.
            "Replace" => Value::Handle(unsafe {
                ffi::subscript_rt_str_replace(context, receiver, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: live strings; runtime owns replacement semantics.
            "ReplaceAll" => Value::Handle(unsafe {
                ffi::subscript_rt_str_replace_all(context, receiver, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "Substring" => Value::Handle(unsafe {
                ffi::subscript_rt_str_substring(context, receiver, integer(1)?, integer(2)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "Substr" => Value::Handle(unsafe {
                ffi::subscript_rt_str_substr(context, receiver, integer(1)?, integer(2)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "CharAt" => Value::Handle(unsafe {
                ffi::subscript_rt_str_char_at(context, receiver, integer(1)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "CodePointAt" => Value::I(unsafe {
                ffi::subscript_rt_str_code_point_at(context, receiver, integer(1)?, 0)
            } as i64),
            // SAFETY: live strings; shared concat implementation.
            "Concat" => Value::Handle(unsafe {
                ffi::subscript_rt_str_method_concat(context, receiver, handle(1)?, 0)
            }),
            _ => return Err(self.invalid(None, format!("unknown String intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<string>", 1, 1))?;
        if let Value::Handle(handle) = value {
            self.root_handle(handle);
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    fn intrinsic_regex(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let handle = |index: usize| -> Result<*mut u8, InterpretError> {
            operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("RegExp.{operation} has no operand {index}"))
                })?
                .as_handle()
        };
        let integer = |index: usize| -> Result<i32, InterpretError> {
            Ok(operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("RegExp.{operation} has no operand {index}"))
                })?
                .as_i64()? as i32)
        };
        let value = match operation {
            // SAFETY: operands are live runtime strings.
            "New" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_new(context, handle(0)?, handle(1)?, 0)
            }),
            // SAFETY: operands are a live regex and string.
            "Test" => Value::Bool(
                unsafe { ffi::subscript_rt_regex_test(context, handle(0)?, handle(1)?, 0) } != 0,
            ),
            // SAFETY: operand is a live regex.
            "Source" => {
                Value::Handle(unsafe { ffi::subscript_rt_regex_source(context, handle(0)?, 0) })
            }
            // SAFETY: operand is a live regex.
            "Flags" => {
                Value::Handle(unsafe { ffi::subscript_rt_regex_flags(context, handle(0)?, 0) })
            }
            // SAFETY: operands are a live subject string and regex.
            "Search" => Value::I(unsafe {
                ffi::subscript_rt_regex_search(context, handle(0)?, handle(1)?, 0)
            } as i64),
            // SAFETY: operands are live subject, regex, and replacement handles.
            "Replace" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_replace(context, handle(0)?, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: operands are live subject, regex, and replacement handles.
            "ReplaceAll" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_replace_all(context, handle(0)?, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: operands are a live subject string and regex.
            "Split" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_split(context, handle(0)?, handle(1)?, 0)
            }),
            // SAFETY: operand is a live regex and the group is an integer.
            "MatchStart" => Value::I(unsafe {
                ffi::subscript_rt_regex_match_start(context, handle(0)?, integer(1)?, 0)
            } as i64),
            // SAFETY: operand is a live regex and the group is an integer.
            "MatchEnd" => Value::I(unsafe {
                ffi::subscript_rt_regex_match_end(context, handle(0)?, integer(1)?, 0)
            } as i64),
            _ => return Err(self.invalid(None, format!("unknown RegExp intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<regexp>", 1, 1))?;
        if let Value::Handle(handle) = value {
            self.root_handle(handle);
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    fn intrinsic_json(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let operand = |index: usize| -> Result<&Value, InterpretError> {
            operands.get(index).ok_or_else(|| {
                self.invalid(None, format!("JSON.{operation} has no operand {index}"))
            })
        };
        let id = |index: usize| -> Result<u64, InterpretError> { operand(index)?.as_u64() };
        let integer = |index: usize| -> Result<i64, InterpretError> { operand(index)?.as_i64() };
        let handle =
            |index: usize| -> Result<*mut u8, InterpretError> { operand(index)?.as_handle() };
        let value = match operation {
            // SAFETY: the Context owns the transient builder table.
            "Begin" => Value::U(unsafe { ffi::subscript_rt_json_begin(context, 0) }),
            // SAFETY: the Context owns the transient builder table.
            "BeginTracked" => Value::U(unsafe { ffi::subscript_rt_json_begin_tracked(context, 0) }),
            // SAFETY: builder was obtained from this Context.
            "Finish" => Value::Handle(unsafe { ffi::subscript_rt_json_finish(context, id(0)?, 0) }),
            // SAFETY: builder and string operands are live.
            "Raw" => {
                unsafe { ffi::subscript_rt_json_raw(context, id(0)?, handle(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder and string operands are live.
            "Str" => {
                unsafe { ffi::subscript_rt_json_str(context, id(0)?, handle(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "I32" => {
                unsafe { ffi::subscript_rt_json_i32(context, id(0)?, integer(1)? as i32, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "U32" => {
                unsafe {
                    ffi::subscript_rt_json_u32(context, id(0)?, operand(1)?.as_u64()? as u32, 0)
                };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "I64" => {
                unsafe { ffi::subscript_rt_json_i64(context, id(0)?, integer(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "U64" => {
                unsafe { ffi::subscript_rt_json_u64(context, id(0)?, operand(1)?.as_u64()?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "F32" => {
                unsafe {
                    ffi::subscript_rt_json_f32(context, id(0)?, operand(1)?.as_f64()? as f32, 0)
                };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "F64" => {
                unsafe { ffi::subscript_rt_json_f64(context, id(0)?, operand(1)?.as_f64()?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "Bool" => {
                unsafe {
                    ffi::subscript_rt_json_bool(
                        context,
                        id(0)?,
                        u8::from(operand(1)?.as_bool()?),
                        0,
                    )
                };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "Date" => {
                unsafe { ffi::subscript_rt_json_date(context, id(0)?, integer(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "Null" => {
                unsafe { ffi::subscript_rt_json_null(context, id(0)?, 0) };
                Value::Void
            }
            // SAFETY: builder and reference are live.
            "Visit" => Value::Bool(
                unsafe { ffi::subscript_rt_json_visit(context, id(0)?, handle(1)?, 0) } != 0,
            ),
            // SAFETY: builder and reference are live.
            "Leave" => {
                unsafe { ffi::subscript_rt_json_leave(context, id(0)?, handle(1)?, 0) };
                Value::Void
            }
            // SAFETY: input is a live string.
            "ParseBegin" => {
                Value::U(unsafe { ffi::subscript_rt_json_parse_begin(context, handle(0)?, 0) })
            }
            // SAFETY: parser was obtained from this Context.
            "ParseEnd" => {
                unsafe { ffi::subscript_rt_json_parse_end(context, id(0)?, 0) };
                Value::Void
            }
            // SAFETY: parser was obtained from this Context.
            "ParseRoot" => {
                Value::U(unsafe { ffi::subscript_rt_json_parse_root(context, id(0)?, 0) })
            }
            // SAFETY: parser/node handles and discriminator are LIR integers.
            "ParseIsKind" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_json_parse_is_kind(context, id(0)?, id(1)?, id(2)? as u32, 0)
                } != 0,
            ),
            // SAFETY: parser/node handles and discriminator are LIR integers.
            "ParseNumberFits" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_json_parse_number_fits(
                        context,
                        id(0)?,
                        id(1)?,
                        id(2)? as u32,
                        0,
                    )
                } != 0,
            ),
            // SAFETY: parser/node handles are live.
            "ParseNumber" => {
                let number =
                    unsafe { ffi::subscript_rt_json_parse_number(context, id(0)?, id(1)?, 0) };
                if matches!(result_ty, Some(l::ValueType::Data(Type::F32))) {
                    Value::F32(number as f32)
                } else {
                    Value::F64(number)
                }
            }
            // SAFETY: parser/node handles and discriminator are LIR integers.
            "ParseInteger" => {
                let bits = unsafe {
                    ffi::subscript_rt_json_parse_integer(context, id(0)?, id(1)?, id(2)? as u32, 0)
                };
                match result_ty {
                    Some(l::ValueType::Data(ty)) => self.integer_result(ty, bits)?,
                    _ => Value::U(bits),
                }
            }
            // SAFETY: parser/node handles are live.
            "ParseBool" => Value::Bool(
                unsafe { ffi::subscript_rt_json_parse_bool(context, id(0)?, id(1)?, 0) } != 0,
            ),
            // SAFETY: parser/node handles are live.
            "ParseString" => Value::Handle(unsafe {
                ffi::subscript_rt_json_parse_string(context, id(0)?, id(1)?, 0)
            }),
            // SAFETY: parser/node handles are live.
            "ParseArrayLen" => Value::I(unsafe {
                ffi::subscript_rt_json_parse_array_len(context, id(0)?, id(1)?, 0)
            } as i64),
            // SAFETY: parser/node handles are live and index is an integer.
            "ParseArrayGet" => Value::U(unsafe {
                ffi::subscript_rt_json_parse_array_get(
                    context,
                    id(0)?,
                    id(1)?,
                    integer(2)? as i32,
                    0,
                )
            }),
            // SAFETY: parser/node handles and key string are live.
            "ParseObjectGet" => Value::U(unsafe {
                ffi::subscript_rt_json_parse_object_get(context, id(0)?, id(1)?, handle(2)?, 0)
            }),
            _ => return Err(self.invalid(None, format!("unknown JSON intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<json>", 1, 1))?;
        if let Value::Handle(handle) = value {
            self.root_handle(handle);
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    fn intrinsic_array(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let receiver_ty = match parameter_types.first() {
            Some(l::ValueType::Data(ty @ (Type::Array(_) | Type::FixedArray(_, _)))) => ty,
            _ => {
                return Err(
                    self.invalid(None, format!("Array.{operation} receiver type is missing"))
                );
            }
        };
        let element_ty = match receiver_ty {
            Type::Array(element) | Type::FixedArray(element, _) => element.as_ref(),
            _ => {
                return Err(
                    self.invalid(None, format!("Array.{operation} receiver type is invalid"))
                );
            }
        };
        if matches!(
            operation,
            "ForEach"
                | "Map"
                | "Filter"
                | "Reduce"
                | "Some"
                | "Every"
                | "FindIndex"
                | "Sort"
                | "ReduceRight"
        ) {
            return self.intrinsic_array_callback(
                operation,
                operands,
                receiver_ty,
                element_ty,
                result_ty,
            );
        }
        let array = operands
            .first()
            .ok_or_else(|| self.invalid(None, format!("Array.{operation} has no receiver")))?
            .as_handle()?;
        let context = &mut *self.context as *mut Context;
        let integer = |index: usize| -> Result<i32, InterpretError> {
            Ok(operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no operand {index}"))
                })?
                .as_i64()? as i32)
        };
        let handle = |index: usize| -> Result<*mut u8, InterpretError> {
            operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no operand {index}"))
                })?
                .as_handle()
        };
        let packed = |index: usize| -> Result<Vec<u8>, InterpretError> {
            self.pack(
                element_ty,
                operands.get(index).ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no operand {index}"))
                })?,
            )
        };
        let kind = array_elem_kind(element_ty, self.module);
        let value = match operation {
            "IndexOf" => {
                let needle = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::I(unsafe {
                    ffi::subscript_rt_arr_index_of(context, array, needle.as_ptr(), kind)
                } as i64)
            }
            "LastIndexOf" => {
                let needle = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::I(unsafe {
                    ffi::subscript_rt_arr_last_index_of(context, array, needle.as_ptr(), kind)
                } as i64)
            }
            "Includes" => {
                let needle = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::Bool(
                    unsafe {
                        ffi::subscript_rt_arr_includes(context, array, needle.as_ptr(), kind)
                    } != 0,
                )
            }
            "Join" => {
                // SAFETY: live array/string; runtime owns Q14 formatting.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_join(
                        context,
                        array,
                        handle(1)?,
                        array_fmt_kind(element_ty),
                        0,
                    )
                })
            }
            "Slice" => {
                // SAFETY: live array; runtime owns clamp/allocation.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_slice(context, array, integer(1)?, integer(2)?, 0)
                })
            }
            "Fill" => {
                let fill = packed(1)?;
                // SAFETY: live array and correctly packed element.
                unsafe {
                    ffi::subscript_rt_arr_fill(
                        context,
                        array,
                        fill.as_ptr(),
                        integer(2)?,
                        integer(3)?,
                    )
                };
                Value::Handle(array)
            }
            "Reverse" => {
                // SAFETY: live array.
                unsafe { ffi::subscript_rt_arr_reverse(context, array) };
                Value::Handle(array)
            }
            "Concat" => {
                // SAFETY: live equal-width arrays.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_concat(context, array, handle(1)?, 0)
                })
            }
            "Splice" => {
                // SAFETY: live array; runtime owns structural mutation.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_splice(context, array, integer(1)?, integer(2)?, 0)
                })
            }
            "Shift" => {
                let layout = self
                    .layout_cached(element_ty)
                    .ok_or_else(|| self.invalid(None, "shift element has no layout"))?;
                let mut bytes = vec![0; layout.size];
                // SAFETY: live array and writable element storage.
                unsafe { ffi::subscript_rt_arr_shift(context, array, bytes.as_mut_ptr(), 0) };
                self.unpack(element_ty, &bytes)?
            }
            "Unshift" => {
                let value = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::I(
                    unsafe { ffi::subscript_rt_arr_unshift(context, array, value.as_ptr(), 0) }
                        as i64,
                )
            }
            "CopyWithin" => {
                // SAFETY: live array; runtime owns overlapping copy/clamps.
                unsafe {
                    ffi::subscript_rt_arr_copy_within(
                        context,
                        array,
                        integer(1)?,
                        integer(2)?,
                        integer(3)?,
                    )
                };
                Value::Handle(array)
            }
            _ => return Err(self.invalid(None, format!("unknown Array intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<array>", 1, 1))?;
        if let Value::Handle(handle) = value {
            if handle != array || matches!(operation, "Slice" | "Concat" | "Splice") {
                self.root_handle(handle);
            }
            Ok(Value::Handle(handle))
        } else {
            let _ = result_ty;
            Ok(value)
        }
    }

    fn intrinsic_array_callback(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        receiver_ty: &Type,
        element_ty: &Type,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let receiver = operands
            .first()
            .cloned()
            .ok_or_else(|| self.invalid(None, format!("Array.{operation} has no receiver")))?;
        let callable = self.callable_operand(operands.get(1), &format!("Array.{operation}"))?;
        let arity = self
            .function(callable.function)?
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == l::ParameterKind::Explicit)
            .count();
        let indexed = match operation {
            "Reduce" | "ReduceRight" => arity == 3,
            "Sort" => false,
            _ => arity == 2,
        };
        let initial_len = self.array_subject_len(&receiver, receiver_ty)?;
        let callback_arguments = |value: Value, index: usize| {
            if indexed {
                vec![value, Value::I(index as i64)]
            } else {
                vec![value]
            }
        };
        match operation {
            "ForEach" => {
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    let _ = self.invoke_callable(&callable, callback_arguments(value, index))?;
                }
                Ok(Value::Void)
            }
            "Map" => {
                let result_element = match result_ty {
                    Some(l::ValueType::Data(Type::Array(element))) => element.as_ref(),
                    _ => return Err(self.invalid(None, "Array.map result is not a dynamic array")),
                };
                let out = self.new_array(result_element)?;
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    let mapped =
                        self.invoke_callable(&callable, callback_arguments(value, index))?;
                    self.array_push_value(out, result_element, &mapped)?;
                }
                Ok(Value::Handle(out))
            }
            "Filter" => {
                let out = self.new_array(element_ty)?;
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    let keep = self
                        .invoke_callable(&callable, callback_arguments(value.clone(), index))?
                        .as_bool()?;
                    if keep {
                        self.array_push_value(out, element_ty, &value)?;
                    }
                }
                Ok(Value::Handle(out))
            }
            "Reduce" | "ReduceRight" => {
                let mut accumulator = operands.get(2).cloned().ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no initial value"))
                })?;
                for step in 0..initial_len {
                    let index = if operation == "Reduce" {
                        step
                    } else {
                        initial_len - 1 - step
                    };
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        if operation == "Reduce" {
                            break;
                        }
                        continue;
                    };
                    let mut arguments = vec![accumulator, value];
                    if indexed {
                        arguments.push(Value::I(index as i64));
                    }
                    accumulator = self.invoke_callable(&callable, arguments)?;
                }
                Ok(accumulator)
            }
            "Some" | "Every" | "FindIndex" => {
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    let matched = self
                        .invoke_callable(&callable, callback_arguments(value, index))?
                        .as_bool()?;
                    if operation == "Some" && matched {
                        return Ok(Value::Bool(true));
                    }
                    if operation == "Every" && !matched {
                        return Ok(Value::Bool(false));
                    }
                    if operation == "FindIndex" && matched {
                        return Ok(Value::I(index as i64));
                    }
                }
                Ok(match operation {
                    "Every" => Value::Bool(true),
                    "FindIndex" => Value::I(-1),
                    _ => Value::Bool(false),
                })
            }
            "Sort" => {
                if !matches!(receiver_ty, Type::Array(_)) {
                    return Err(self.invalid(None, "FixedArray.sort is not an accepted operation"));
                }
                let mut sorted = Vec::with_capacity(initial_len);
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    sorted.push(value);
                }
                // Stable insertion into scratch storage. No receiver bytes are
                // changed until every comparator invocation has succeeded.
                for index in 1..sorted.len() {
                    let mut cursor = index;
                    while cursor > 0 {
                        let comparison = self
                            .invoke_callable(
                                &callable,
                                vec![sorted[cursor - 1].clone(), sorted[cursor].clone()],
                            )?
                            .as_i64()?;
                        if comparison <= 0 {
                            break;
                        }
                        sorted.swap(cursor - 1, cursor);
                        cursor -= 1;
                    }
                }
                let handle = receiver.as_handle()?;
                for (index, value) in sorted.iter().enumerate() {
                    self.array_store_value(handle, element_ty, index, value)?;
                }
                Ok(Value::Handle(handle))
            }
            _ => Err(self.invalid(
                None,
                format!("unknown Array callback intrinsic {operation}"),
            )),
        }
    }

    fn array_subject_len(
        &self,
        receiver: &Value,
        receiver_ty: &Type,
    ) -> Result<usize, InterpretError> {
        match (receiver_ty, receiver) {
            (Type::Array(_), Value::Handle(handle)) => {
                // SAFETY: receiver is a live runtime array.
                let len = unsafe { self.context.array_len(*handle) };
                usize::try_from(len).map_err(|_| self.invalid(None, "array length is negative"))
            }
            (Type::FixedArray(_, count), Value::Blob(_)) => usize::try_from(*count)
                .map_err(|_| self.invalid(None, "fixed-array count does not fit usize")),
            (Type::Array(_), other) => Err(type_error("runtime array handle", other)),
            (Type::FixedArray(_, _), other) => Err(type_error("fixed array", other)),
            _ => Err(self.invalid(None, "array callback receiver has a non-array type")),
        }
    }

    fn array_subject_value(
        &mut self,
        receiver: &Value,
        receiver_ty: &Type,
        element_ty: &Type,
        index: usize,
    ) -> Result<Option<Value>, InterpretError> {
        let layout = self
            .layout_cached(element_ty)
            .ok_or_else(|| self.invalid(None, "array callback element has no layout"))?;
        match (receiver_ty, receiver) {
            (Type::Array(_), Value::Handle(handle)) => {
                // Callbacks may shorten the receiver. Appends never extend the
                // captured `initial_len`, but a removed suffix ends traversal.
                let current_len = unsafe { self.context.array_len(*handle) };
                if i32::try_from(index).map_or(true, |index| index >= current_len) {
                    return Ok(None);
                }
                let pointer = unsafe { self.context.array_elem_ptr(*handle, index as i32, 0) };
                self.check_runtime(&Pos::new("<array-callback>", 1, 1))?;
                let bytes = unsafe { std::slice::from_raw_parts(pointer, layout.size) };
                self.unpack(element_ty, bytes).map(Some)
            }
            (Type::FixedArray(_, count), Value::Blob(bytes)) => {
                let count = usize::try_from(*count)
                    .map_err(|_| self.invalid(None, "fixed-array count does not fit usize"))?;
                if index >= count {
                    return Ok(None);
                }
                let start = index
                    .checked_mul(layout.size)
                    .ok_or_else(|| self.invalid(None, "fixed-array callback offset overflows"))?;
                let end = start
                    .checked_add(layout.size)
                    .ok_or_else(|| self.invalid(None, "fixed-array callback range overflows"))?;
                let element = bytes.get(start..end).ok_or_else(|| {
                    self.invalid(None, "fixed-array callback reads outside its blob")
                })?;
                self.unpack(element_ty, element).map(Some)
            }
            (Type::Array(_), other) => Err(type_error("runtime array handle", other)),
            (Type::FixedArray(_, _), other) => Err(type_error("fixed array", other)),
            _ => Err(self.invalid(None, "array callback receiver has a non-array type")),
        }
    }

    fn new_array(&mut self, element_ty: &Type) -> Result<*mut u8, InterpretError> {
        let layout = self
            .layout_cached(element_ty)
            .ok_or_else(|| self.invalid(None, "array result element has no layout"))?;
        let handle = self.context.array_new(layout.size, 0);
        self.check_runtime(&Pos::new("<array-callback>", 1, 1))?;
        self.root_handle(handle);
        Ok(handle)
    }

    fn array_push_value(
        &mut self,
        array: *mut u8,
        element_ty: &Type,
        value: &Value,
    ) -> Result<(), InterpretError> {
        let packed = self.pack(element_ty, value)?;
        // SAFETY: live array and one exactly packed element.
        unsafe { ffi::subscript_rt_array_push(&mut *self.context, array, packed.as_ptr(), 0) };
        self.check_runtime(&Pos::new("<array-callback>", 1, 1))
    }

    fn array_store_value(
        &mut self,
        array: *mut u8,
        element_ty: &Type,
        index: usize,
        value: &Value,
    ) -> Result<(), InterpretError> {
        let packed = self.pack(element_ty, value)?;
        let index = i32::try_from(index)
            .map_err(|_| self.invalid(None, "array callback index overflows i32"))?;
        // SAFETY: index came from the live receiver's captured length.
        let pointer = unsafe { self.context.array_elem_ptr(array, index, 0) };
        self.check_runtime(&Pos::new("<array-callback>", 1, 1))?;
        unsafe { std::ptr::copy_nonoverlapping(packed.as_ptr(), pointer, packed.len()) };
        Ok(())
    }

    fn intrinsic_map(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        _type_argument: Option<&Type>,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        if operation == "GroupBy" {
            let element_ty = match parameter_types.first() {
                Some(l::ValueType::Data(Type::Array(element))) => element.as_ref(),
                _ => return Err(self.invalid(None, "Map.groupBy items type is not an array")),
            };
            let key_ty = match result_ty {
                Some(l::ValueType::Data(Type::Map(key, value))) if matches!(value.as_ref(), Type::Array(element) if element.as_ref() == element_ty) => {
                    key.as_ref()
                }
                _ => return Err(self.invalid(None, "Map.groupBy result type is not Map<K,T[]>")),
            };
            let items = operands
                .first()
                .ok_or_else(|| self.invalid(None, "Map.groupBy has no items"))?
                .as_handle()?;
            let callable = self.callable_operand(operands.get(1), "Map.groupBy")?;
            let key_layout = self
                .layout_cached(key_ty)
                .ok_or_else(|| self.invalid(None, "Map.groupBy key has no layout"))?;
            let mut state = CallbackState {
                interpreter: (self as *mut Self).cast(),
                callable,
                first_ty: element_ty.clone(),
                second_ty: Some(key_ty.clone()),
                error: None,
            };
            // SAFETY: the runtime call is synchronous; the bridge copies the
            // element, invokes the interpreter callback, and writes one key.
            let result = unsafe {
                ffi::subscript_rt_map_group_by(
                    &mut *self.context,
                    items,
                    group_by_callback_bridge as *const u8,
                    (&mut state as *mut CallbackState).cast(),
                    group_by_callback_bridge as *const u8,
                    key_layout.size as u64,
                    assoc_key_kind(key_ty, self.module),
                    0,
                )
            };
            if let Some(error) = state.error {
                return Err(error);
            }
            self.check_runtime(&Pos::new("<map>", 1, 1))?;
            self.root_handle(result);
            return Ok(Value::Handle(result));
        }
        let shape = parameter_types
            .first()
            .or(result_ty)
            .and_then(|ty| match ty {
                l::ValueType::Data(Type::Map(key, value)) => Some((key.as_ref(), value.as_ref())),
                _ => None,
            })
            .ok_or_else(|| self.invalid(None, format!("Map.{operation} has no Map<K,V> type")))?;
        let key_layout = self
            .layout_cached(shape.0)
            .ok_or_else(|| self.invalid(None, "Map key has no layout"))?;
        let value_layout = self
            .layout_cached(shape.1)
            .ok_or_else(|| self.invalid(None, "Map value has no layout"))?;
        let context = &mut *self.context as *mut Context;
        let interpreter = (self as *mut Self).cast();
        let receiver = || -> Result<*mut u8, InterpretError> {
            operands
                .first()
                .ok_or_else(|| self.invalid(None, format!("Map.{operation} has no receiver")))?
                .as_handle()
        };
        let packed_key = || -> Result<Vec<u8>, InterpretError> {
            self.pack(
                shape.0,
                operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, format!("Map.{operation} has no key")))?,
            )
        };
        let value = match operation {
            "New" => {
                // SAFETY: concrete widths and runtime key-kind table.
                Value::Handle(unsafe {
                    ffi::subscript_rt_map_new(
                        context,
                        key_layout.size as u64,
                        value_layout.size as u64,
                        assoc_key_kind(shape.0, self.module),
                        0,
                    )
                })
            }
            // SAFETY: live Map receiver.
            "Size" => Value::I(unsafe { ffi::subscript_rt_map_size(context, receiver()?) } as i64),
            "Get" => {
                let key = packed_key()?;
                let mut out = vec![0; value_layout.size];
                // SAFETY: live receiver and exact key/value storage.
                if unsafe {
                    ffi::subscript_rt_map_get(context, receiver()?, key.as_ptr(), out.as_mut_ptr())
                } == 0
                {
                    Value::Null
                } else {
                    self.unpack(shape.1, &out)?
                }
            }
            "GetOr" => {
                let key = packed_key()?;
                let fallback = self.pack(
                    shape.1,
                    operands
                        .get(2)
                        .ok_or_else(|| self.invalid(None, "Map.getOr has no fallback"))?,
                )?;
                let mut out = vec![0; value_layout.size];
                // SAFETY: live receiver and exact key/value storage.
                unsafe {
                    ffi::subscript_rt_map_get_or(
                        context,
                        receiver()?,
                        key.as_ptr(),
                        fallback.as_ptr(),
                        out.as_mut_ptr(),
                    )
                };
                self.unpack(shape.1, &out)?
            }
            "Set" => {
                let key = packed_key()?;
                let stored = self.pack(
                    shape.1,
                    operands
                        .get(2)
                        .ok_or_else(|| self.invalid(None, "Map.set has no value"))?,
                )?;
                // SAFETY: live receiver and exact key/value storage.
                Value::Handle(unsafe {
                    ffi::subscript_rt_map_set(
                        context,
                        receiver()?,
                        key.as_ptr(),
                        stored.as_ptr(),
                        0,
                    )
                })
            }
            "Has" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_map_has(context, receiver()?, key.as_ptr()) } != 0,
                )
            }
            "Delete" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_map_delete(context, receiver()?, key.as_ptr()) }
                        != 0,
                )
            }
            "Clear" => {
                // SAFETY: live receiver.
                unsafe { ffi::subscript_rt_map_clear(context, receiver()?) };
                Value::Void
            }
            "ForEach" => {
                let callable = self.callable_operand(operands.get(1), "Map.forEach")?;
                let mut state = CallbackState {
                    interpreter,
                    callable,
                    first_ty: shape.1.clone(),
                    second_ty: Some(shape.0.clone()),
                    error: None,
                };
                // SAFETY: the runtime owns insertion-order traversal; the
                // bridge copies the entry before invoking script code.
                unsafe {
                    ffi::subscript_rt_map_for_each(
                        context,
                        receiver()?,
                        map_callback_bridge as *const u8,
                        (&mut state as *mut CallbackState).cast(),
                        map_callback_bridge as *const u8,
                    )
                };
                if let Some(error) = state.error {
                    return Err(error);
                }
                Value::Void
            }
            _ => return Err(self.invalid(None, format!("unknown Map intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<map>", 1, 1))?;
        if let Value::Handle(handle) = value {
            if operation == "New" {
                self.root_handle(handle);
            }
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    fn intrinsic_set(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        _type_argument: Option<&Type>,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let key_ty = parameter_types
            .first()
            .and_then(|ty| match ty {
                l::ValueType::Data(Type::Set(key)) => Some(key.as_ref()),
                _ => None,
            })
            .or({
                // `new Set<K>()` has no receiver; the explicit monomorphized
                // type argument is carried by the intrinsic.
                _type_argument
            })
            .or_else(|| match result_ty {
                Some(l::ValueType::Data(Type::Set(key))) => Some(key.as_ref()),
                _ => None,
            })
            .ok_or_else(|| self.invalid(None, format!("Set.{operation} has no key type")))?;
        let layout = self
            .layout_cached(key_ty)
            .ok_or_else(|| self.invalid(None, "Set key has no layout"))?;
        let context = &mut *self.context as *mut Context;
        let interpreter = (self as *mut Self).cast();
        let receiver = || -> Result<*mut u8, InterpretError> {
            operands
                .first()
                .ok_or_else(|| self.invalid(None, format!("Set.{operation} has no receiver")))?
                .as_handle()
        };
        let packed_key = || -> Result<Vec<u8>, InterpretError> {
            self.pack(
                key_ty,
                operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, format!("Set.{operation} has no key")))?,
            )
        };
        let value = match operation {
            "New" => {
                // SAFETY: concrete width and runtime key-kind table.
                Value::Handle(unsafe {
                    ffi::subscript_rt_set_new(
                        context,
                        layout.size as u64,
                        assoc_key_kind(key_ty, self.module),
                        0,
                    )
                })
            }
            // SAFETY: live Set receiver.
            "Size" => Value::I(unsafe { ffi::subscript_rt_set_size(context, receiver()?) } as i64),
            "Add" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Handle(unsafe {
                    ffi::subscript_rt_set_add(context, receiver()?, key.as_ptr(), 0)
                })
            }
            "Has" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_set_has(context, receiver()?, key.as_ptr()) } != 0,
                )
            }
            "Delete" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_set_delete(context, receiver()?, key.as_ptr()) }
                        != 0,
                )
            }
            "Clear" => {
                // SAFETY: live receiver.
                unsafe { ffi::subscript_rt_set_clear(context, receiver()?) };
                Value::Void
            }
            "ForEach" => {
                let callable = self.callable_operand(operands.get(1), "Set.forEach")?;
                let mut state = CallbackState {
                    interpreter,
                    callable,
                    first_ty: key_ty.clone(),
                    second_ty: None,
                    error: None,
                };
                // SAFETY: runtime owns fixed-bound insertion-order traversal;
                // the bridge copies each key before calling script code.
                unsafe {
                    ffi::subscript_rt_set_for_each(
                        context,
                        receiver()?,
                        set_callback_bridge as *const u8,
                        (&mut state as *mut CallbackState).cast(),
                        set_callback_bridge as *const u8,
                    )
                };
                if let Some(error) = state.error {
                    return Err(error);
                }
                Value::Void
            }
            "Union" => Value::Handle(unsafe {
                ffi::subscript_rt_set_union(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.invalid(None, "Set.union has no argument"))?
                        .as_handle()?,
                    0,
                )
            }),
            "Intersection" => Value::Handle(unsafe {
                ffi::subscript_rt_set_intersection(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.invalid(None, "Set.intersection has no argument"))?
                        .as_handle()?,
                    0,
                )
            }),
            "Difference" => Value::Handle(unsafe {
                ffi::subscript_rt_set_difference(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.invalid(None, "Set.difference has no argument"))?
                        .as_handle()?,
                    0,
                )
            }),
            "SymmetricDifference" => Value::Handle(unsafe {
                ffi::subscript_rt_set_symmetric_difference(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| {
                            self.invalid(None, "Set.symmetricDifference has no argument")
                        })?
                        .as_handle()?,
                    0,
                )
            }),
            "IsSubsetOf" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_set_is_subset_of(
                        context,
                        receiver()?,
                        operands
                            .get(1)
                            .ok_or_else(|| self.invalid(None, "Set.isSubsetOf has no argument"))?
                            .as_handle()?,
                    )
                } != 0,
            ),
            "IsSupersetOf" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_set_is_superset_of(
                        context,
                        receiver()?,
                        operands
                            .get(1)
                            .ok_or_else(|| self.invalid(None, "Set.isSupersetOf has no argument"))?
                            .as_handle()?,
                    )
                } != 0,
            ),
            "IsDisjointFrom" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_set_is_disjoint_from(
                        context,
                        receiver()?,
                        operands
                            .get(1)
                            .ok_or_else(|| {
                                self.invalid(None, "Set.isDisjointFrom has no argument")
                            })?
                            .as_handle()?,
                    )
                } != 0,
            ),
            _ => return Err(self.invalid(None, format!("unknown Set intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<set>", 1, 1))?;
        if let Value::Handle(handle) = value {
            if matches!(
                operation,
                "New" | "Union" | "Intersection" | "Difference" | "SymmetricDifference"
            ) {
                self.root_handle(handle);
            }
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    fn compute_class_layouts(&mut self) -> Result<(), InterpretError> {
        for class in &self.module.classes {
            let _ = self.class_layout(class.id)?;
        }
        Ok(())
    }

    fn class_layout(&mut self, id: ClassId) -> Result<Layout, InterpretError> {
        if let Some(layout) = self.class_layouts.get(&id) {
            return Ok(Layout {
                size: layout.size,
                align: layout.align,
            });
        }
        let class = self
            .module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .cloned()
            .ok_or_else(|| self.invalid(None, format!("class {:?} is missing", id)))?;
        // Insert a sentinel before recursion so an invalid by-value cycle is
        // diagnosed rather than recursing indefinitely.
        self.class_layouts.insert(id, Layout { size: 0, align: 1 });
        let mut offset = 0usize;
        let mut aggregate_align = 1usize;
        for field in &class.fields {
            let layout = self.type_layout(&field.ty)?;
            offset = align_up(offset, layout.align);
            self.field_layouts
                .insert(field.id, (offset, field.ty.clone()));
            offset = offset.checked_add(layout.size).ok_or_else(|| {
                self.invalid(Some(field.pos.clone()), "class layout overflows host usize")
            })?;
            aggregate_align = aggregate_align.max(layout.align);
        }
        if let Some(override_align) = class.alignment {
            aggregate_align = aggregate_align.max(override_align as usize);
        }
        let layout = Layout {
            size: align_up(offset, aggregate_align),
            align: aggregate_align,
        };
        self.class_layouts.insert(
            id,
            Layout {
                size: layout.size,
                align: layout.align,
            },
        );
        Ok(layout)
    }

    fn type_layout(&mut self, ty: &Type) -> Result<Layout, InterpretError> {
        if let Some((size, align)) = scalar_size_align(ty) {
            return Ok(Layout {
                size: size as usize,
                align: align as usize,
            });
        }
        match ty {
            Type::Class(id) => {
                let class = self
                    .module
                    .classes
                    .get(id.0)
                    .ok_or_else(|| self.invalid(None, format!("class {:?} is missing", id)))?;
                if class.is_value {
                    self.class_layout(*id)
                } else {
                    Ok(Layout { size: 8, align: 8 })
                }
            }
            Type::FixedArray(element, count) => {
                let element = self.type_layout(element)?;
                let stride = align_up(element.size, element.align);
                Ok(Layout {
                    size: stride.checked_mul(*count as usize).ok_or_else(|| {
                        self.invalid(None, "fixed-array layout overflows host usize")
                    })?,
                    align: element.align,
                })
            }
            Type::IterResult(value) => {
                let value = self.type_layout(value)?;
                let value_offset = align_up(1, value.align);
                Ok(Layout {
                    size: align_up(value_offset + value.size, value.align),
                    align: value.align,
                })
            }
            other => Err(self.invalid(None, format!("no storage layout for {other:?}"))),
        }
    }

    fn zero(&self, ty: &Type) -> Value {
        match ty {
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Enum(_)
            | Type::StringAlias(_)
            | Type::Date => Value::I(0),
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::F16 => Value::U(0),
            Type::F32 => Value::F32(0.0),
            Type::F64 => Value::F64(0.0),
            Type::Bool => Value::Bool(false),
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                let size = self.class_layouts.get(id).map_or(0, |layout| layout.size);
                Value::Blob(vec![0; size])
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => {
                // Layouts are already computed by `new`; zero-sized fallback is
                // rejected when the value is used if the module is inconsistent.
                let size = self.layout_cached(ty).map_or(0, |layout| layout.size);
                Value::Blob(vec![0; size])
            }
            Type::Void => Value::Void,
            Type::Null
            | Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Func(_)
            | Type::Nullable(_)
            | Type::Generator(_) => Value::Null,
            Type::Error => Value::Void,
            _ => Value::Void,
        }
    }

    fn layout_cached(&self, ty: &Type) -> Option<Layout> {
        if let Some((size, align)) = scalar_size_align(ty) {
            return Some(Layout {
                size: size as usize,
                align: align as usize,
            });
        }
        match ty {
            Type::Class(id) => {
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value)
                {
                    self.class_layouts.get(id).map(|layout| Layout {
                        size: layout.size,
                        align: layout.align,
                    })
                } else {
                    Some(Layout { size: 8, align: 8 })
                }
            }
            Type::FixedArray(element, count) => {
                let element = self.layout_cached(element)?;
                Some(Layout {
                    size: align_up(element.size, element.align) * *count as usize,
                    align: element.align,
                })
            }
            Type::IterResult(value) => {
                let value = self.layout_cached(value)?;
                let offset = align_up(1, value.align);
                Some(Layout {
                    size: align_up(offset + value.size, value.align),
                    align: value.align,
                })
            }
            _ => None,
        }
    }

    fn address_pointee(
        &self,
        result_ty: Option<&l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<Type, InterpretError> {
        match result_ty {
            Some(l::ValueType::Address(address)) => Ok(address.pointee.clone()),
            _ => Err(self.invalid(
                Some(instruction.pos.clone()),
                format!("{:?} has no address result type", instruction.kind),
            )),
        }
    }

    fn data_result_type<'a>(
        &self,
        result_ty: Option<&'a l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<&'a Type, InterpretError> {
        match result_ty {
            Some(l::ValueType::Data(ty)) => Ok(ty),
            _ => Err(self.invalid(
                Some(instruction.pos.clone()),
                format!("{:?} has no data result type", instruction.kind),
            )),
        }
    }

    fn allocate_class(
        &mut self,
        id: ClassId,
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let class = self
            .module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .ok_or_else(|| self.invalid(Some(pos.clone()), format!("class {:?} is missing", id)))?;
        let layout = self.class_layouts.get(&id).ok_or_else(|| {
            self.invalid(Some(pos.clone()), format!("class {:?} has no layout", id))
        })?;
        if class.is_value {
            if !matches!(
                result_ty,
                Some(l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(result),
                    array_base: None,
                })) if *result == id
            ) {
                return Err(self.invalid(
                    Some(pos.clone()),
                    "value-class allocation result type is inconsistent",
                ));
            }
            return Ok(Value::Address(Address {
                target: AddressTarget::Slot(Rc::new(RefCell::new(Value::Blob(vec![
                    0;
                    layout.size
                ])))),
                pointee: Type::Class(id),
                poison: Rc::new(RefCell::new(None)),
            }));
        }
        if !matches!(result_ty, Some(l::ValueType::Data(Type::Class(result))) if *result == id) {
            return Err(self.invalid(
                Some(pos.clone()),
                "AllocateClass result type is inconsistent",
            ));
        }
        let handle = self.context.alloc(layout.size, id.0 as u32, 0);
        self.check_runtime(pos)?;
        self.root_handle(handle);
        Ok(Value::Handle(handle))
    }

    fn root_handle(&mut self, handle: *mut u8) {
        if handle.is_null() || handle.addr() & 1 != 0 {
            return;
        }
        let slot = Rc::new(Cell::new(handle as usize));
        self.context.root_add(slot.as_ptr() as usize, 1);
        self.roots.push(slot);
    }

    fn alloc_string(&mut self, bytes: &[u8], pos: &Pos) -> Result<*mut u8, InterpretError> {
        let handle = self.context.alloc_str(bytes, 0);
        self.check_runtime(pos)?;
        self.root_handle(handle);
        Ok(handle)
    }

    fn string_bytes(&self, handle: *mut u8) -> Result<Vec<u8>, InterpretError> {
        if handle.is_null() {
            return Err(self.invalid(None, "null used as a string"));
        }
        // SAFETY: handles in interpreter values are produced by this Context.
        Ok(unsafe { self.context.str_bytes(handle) }.to_vec())
    }

    fn field_info(&self, field: l::FieldRef) -> Result<(usize, Type), InterpretError> {
        match field {
            l::FieldRef::Class(id) => self
                .field_layouts
                .get(&id)
                .cloned()
                .ok_or_else(|| self.invalid(None, format!("field {} has no layout", id.0))),
            l::FieldRef::IterDone => Ok((0, Type::Bool)),
            l::FieldRef::IterValue => Err(self.invalid(
                None,
                "IterResult.value requires its result address type to determine the offset",
            )),
        }
    }

    fn address_field(
        &self,
        base: &Value,
        field: l::FieldRef,
        result_ty: Option<&l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<Address, InterpretError> {
        let pointee = match result_ty {
            Some(l::ValueType::Address(address)) => address.pointee.clone(),
            Some(l::ValueType::Data(ty)) => ty.clone(),
            _ => match field {
                l::FieldRef::Class(id) => self.field_info(l::FieldRef::Class(id))?.1,
                l::FieldRef::IterDone => Type::Bool,
                l::FieldRef::IterValue => {
                    return Err(self.invalid(
                        Some(instruction.pos.clone()),
                        "LoadField IterValue has no result type",
                    ));
                }
            },
        };
        let offset = self.field_offset(field, &pointee, instruction)?;
        match base {
            Value::Handle(handle) if !handle.is_null() => Ok(Address {
                // SAFETY: the verified field layout is within this class payload.
                target: AddressTarget::Pointer(unsafe { handle.add(offset) }),
                pointee,
                poison: Rc::new(RefCell::new(None)),
            }),
            Value::Address(address) => {
                address.check(&instruction.kind)?;
                Ok(Address {
                    target: match &address.target {
                        AddressTarget::Slot(slot) => AddressTarget::SlotBytes {
                            slot: slot.clone(),
                            offset,
                        },
                        AddressTarget::SlotBytes { slot, offset: base } => {
                            AddressTarget::SlotBytes {
                                slot: slot.clone(),
                                offset: base + offset,
                            }
                        }
                        AddressTarget::Pointer(pointer) => {
                            // SAFETY: the verified nested field layout is in bounds.
                            AddressTarget::Pointer(unsafe { pointer.add(offset) })
                        }
                    },
                    pointee,
                    poison: address.poison.clone(),
                })
            }
            other => Err(type_error("class handle or aggregate address", other)),
        }
    }

    fn field_offset(
        &self,
        field: l::FieldRef,
        pointee: &Type,
        instruction: &l::Instruction,
    ) -> Result<usize, InterpretError> {
        Ok(match field {
            l::FieldRef::Class(id) => self.field_info(l::FieldRef::Class(id))?.0,
            l::FieldRef::IterDone => 0,
            l::FieldRef::IterValue => {
                let layout = self.layout_cached(pointee).ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        "IterResult.value type has no layout",
                    )
                })?;
                align_up(1, layout.align)
            }
        })
    }

    fn address_index(
        &mut self,
        base: &Value,
        index: i64,
        result_ty: Option<&l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<Address, InterpretError> {
        let pointee = self.address_pointee(result_ty, instruction)?;
        let poison = Rc::new(RefCell::new(None));
        match base {
            Value::Handle(array) => {
                // The runtime owns bounds checks and the dynamic storage.
                // SAFETY: the handle is a live runtime array and `array_elem_ptr`
                // validates the index before returning an element pointer.
                let pointer = unsafe {
                    self.context
                        .array_elem_ptr(*array, i32::try_from(index).unwrap_or(i32::MIN), 0)
                };
                self.check_runtime(&instruction.pos)?;
                Ok(Address {
                    target: AddressTarget::Pointer(pointer),
                    pointee,
                    poison,
                })
            }
            Value::Address(address) => {
                address.check(&instruction.kind)?;
                let layout = self.layout_cached(&pointee).ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        "indexed element has no layout",
                    )
                })?;
                let offset = usize::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_mul(align_up(layout.size, layout.align)))
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            "fixed-array index is negative or overflows",
                        )
                    })?;
                Ok(Address {
                    target: match &address.target {
                        AddressTarget::Slot(slot) => AddressTarget::SlotBytes {
                            slot: slot.clone(),
                            offset,
                        },
                        AddressTarget::SlotBytes { slot, offset: base } => {
                            AddressTarget::SlotBytes {
                                slot: slot.clone(),
                                offset: base + offset,
                            }
                        }
                        AddressTarget::Pointer(pointer) => {
                            // SAFETY: fixed-array bounds are guarded by the LIR trap site.
                            AddressTarget::Pointer(unsafe { pointer.add(offset) })
                        }
                    },
                    pointee,
                    poison: address.poison.clone(),
                })
            }
            other => Err(type_error("array handle or fixed-array address", other)),
        }
    }

    fn load_address(&self, address: &Address) -> Result<Value, InterpretError> {
        match &address.target {
            AddressTarget::Slot(slot) => Ok(slot.borrow().clone()),
            AddressTarget::SlotBytes { slot, offset } => {
                let borrowed = slot.borrow();
                let Value::Blob(bytes) = &*borrowed else {
                    return Err(type_error("aggregate storage", &borrowed));
                };
                self.unpack(&address.pointee, bytes.get(*offset..).unwrap_or_default())
            }
            AddressTarget::Pointer(pointer) => {
                let layout = self.layout_cached(&address.pointee).ok_or_else(|| {
                    self.invalid(
                        None,
                        format!("no layout for address pointee {:?}", address.pointee),
                    )
                })?;
                if pointer.is_null() {
                    return Err(self.invalid(None, "null address"));
                }
                // SAFETY: the address was derived from verified storage and covers
                // the pointee layout.
                let bytes = unsafe { std::slice::from_raw_parts(*pointer, layout.size) };
                self.unpack(&address.pointee, bytes)
            }
        }
    }

    fn store_address(&self, address: &Address, value: &Value) -> Result<(), InterpretError> {
        match &address.target {
            AddressTarget::Slot(slot) => {
                *slot.borrow_mut() = value.clone();
                Ok(())
            }
            AddressTarget::SlotBytes { slot, offset } => {
                let mut borrowed = slot.borrow_mut();
                let Value::Blob(bytes) = &mut *borrowed else {
                    return Err(type_error("aggregate storage", &borrowed));
                };
                self.pack_into(
                    &address.pointee,
                    value,
                    bytes.get_mut(*offset..).unwrap_or_default(),
                )
            }
            AddressTarget::Pointer(pointer) => {
                let layout = self.layout_cached(&address.pointee).ok_or_else(|| {
                    self.invalid(
                        None,
                        format!("no layout for address pointee {:?}", address.pointee),
                    )
                })?;
                if pointer.is_null() {
                    return Err(self.invalid(None, "null address"));
                }
                // SAFETY: the address was derived from writable verified storage.
                let bytes = unsafe { std::slice::from_raw_parts_mut(*pointer, layout.size) };
                self.pack_into(&address.pointee, value, bytes)
            }
        }
    }

    fn zero_padding(&self, ty: &Type, bytes: &mut [u8], base: usize) -> Result<(), InterpretError> {
        match ty {
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                let class = self.module.classes[id.0].clone();
                let class_layout = self.class_layouts.get(id).ok_or_else(|| {
                    self.invalid(None, format!("class {:?} has no cached layout", id))
                })?;
                let mut cursor = 0usize;
                for field in class.fields {
                    let (offset, field_ty) = self.field_info(l::FieldRef::Class(field.id))?;
                    self.zero_byte_range(bytes, base + cursor, base + offset)?;
                    self.zero_padding(&field_ty, bytes, base + offset)?;
                    let field_layout = self.layout_cached(&field_ty).ok_or_else(|| {
                        self.invalid(None, format!("field {:?} has no layout", field.id))
                    })?;
                    cursor = offset + field_layout.size;
                }
                self.zero_byte_range(bytes, base + cursor, base + class_layout.size)?;
            }
            Type::FixedArray(element, count) => {
                let element_layout = self.layout_cached(element).ok_or_else(|| {
                    self.invalid(None, "fixed-array element has no padding layout")
                })?;
                let stride = align_up(element_layout.size, element_layout.align);
                for index in 0..*count as usize {
                    let element_base = base + index * stride;
                    self.zero_padding(element, bytes, element_base)?;
                    self.zero_byte_range(
                        bytes,
                        element_base + element_layout.size,
                        element_base + stride,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn zero_byte_range(
        &self,
        bytes: &mut [u8],
        start: usize,
        end: usize,
    ) -> Result<(), InterpretError> {
        let range = bytes
            .get_mut(start..end)
            .ok_or_else(|| self.invalid(None, "padding range exceeds aggregate storage"))?;
        range.fill(0);
        Ok(())
    }

    fn pack(&self, ty: &Type, value: &Value) -> Result<Vec<u8>, InterpretError> {
        let layout = self
            .layout_cached(ty)
            .ok_or_else(|| self.invalid(None, format!("no layout for {ty:?}")))?;
        let mut bytes = vec![0; layout.size];
        self.pack_into(ty, value, &mut bytes)?;
        Ok(bytes)
    }

    fn pack_into(&self, ty: &Type, value: &Value, out: &mut [u8]) -> Result<(), InterpretError> {
        let layout = self
            .layout_cached(ty)
            .ok_or_else(|| self.invalid(None, format!("no layout for {ty:?}")))?;
        if out.len() < layout.size {
            return Err(self.invalid(None, format!("storage for {ty:?} is too short")));
        }
        match ty {
            Type::I8 => out[0] = value.as_i64()? as i8 as u8,
            Type::U8 | Type::Bool => {
                out[0] = if matches!(ty, Type::Bool) {
                    u8::from(value.as_bool()?)
                } else {
                    value.as_u64()? as u8
                }
            }
            Type::I16 => out[..2].copy_from_slice(&(value.as_i64()? as i16).to_ne_bytes()),
            Type::U16 | Type::F16 => {
                out[..2].copy_from_slice(&(value.as_u64()? as u16).to_ne_bytes())
            }
            Type::I32 | Type::Enum(_) | Type::StringAlias(_) => {
                out[..4].copy_from_slice(&(value.as_i64()? as i32).to_ne_bytes())
            }
            Type::U32 => out[..4].copy_from_slice(&(value.as_u64()? as u32).to_ne_bytes()),
            Type::I64 | Type::Date => out[..8].copy_from_slice(&value.as_i64()?.to_ne_bytes()),
            Type::U64 => out[..8].copy_from_slice(&value.as_u64()?.to_ne_bytes()),
            Type::F32 => {
                let Value::F32(value) = value else {
                    return Err(type_error("f32", value));
                };
                out[..4].copy_from_slice(&value.to_bits().to_ne_bytes());
            }
            Type::F64 => out[..8].copy_from_slice(&value.as_f64()?.to_bits().to_ne_bytes()),
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                let Value::Blob(value) = value else {
                    return Err(type_error("aggregate", value));
                };
                if value.len() != layout.size {
                    return Err(self.invalid(None, "aggregate size disagrees with its type"));
                }
                out[..layout.size].copy_from_slice(value);
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => {
                let Value::Blob(value) = value else {
                    return Err(type_error("aggregate", value));
                };
                if value.len() != layout.size {
                    return Err(self.invalid(None, "aggregate size disagrees with its type"));
                }
                out[..layout.size].copy_from_slice(value);
            }
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Nullable(_)
            | Type::Generator(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Null => {
                let handle = value.as_handle()? as usize as u64;
                out[..8].copy_from_slice(&handle.to_ne_bytes());
            }
            Type::Func(_) => {
                return Err(self.invalid(
                    None,
                    "function values cannot be packed by the reference interpreter",
                ));
            }
            Type::Void | Type::Error => {}
            _ => return Err(self.invalid(None, format!("packing {ty:?} is not defined"))),
        }
        Ok(())
    }

    fn unpack(&self, ty: &Type, bytes: &[u8]) -> Result<Value, InterpretError> {
        let need = self
            .layout_cached(ty)
            .ok_or_else(|| self.invalid(None, format!("no layout for {ty:?}")))?
            .size;
        if bytes.len() < need {
            return Err(self.invalid(None, format!("storage for {ty:?} is too short")));
        }
        Ok(match ty {
            Type::I8 => Value::I(i8::from_ne_bytes([bytes[0]]) as i64),
            Type::U8 => Value::U(bytes[0] as u64),
            Type::Bool => Value::Bool(bytes[0] != 0),
            Type::I16 => {
                Value::I(i16::from_ne_bytes(bytes[..2].try_into().unwrap_or([0; 2])) as i64)
            }
            Type::U16 | Type::F16 => {
                Value::U(u16::from_ne_bytes(bytes[..2].try_into().unwrap_or([0; 2])) as u64)
            }
            Type::I32 | Type::Enum(_) | Type::StringAlias(_) => {
                Value::I(i32::from_ne_bytes(bytes[..4].try_into().unwrap_or([0; 4])) as i64)
            }
            Type::U32 => {
                Value::U(u32::from_ne_bytes(bytes[..4].try_into().unwrap_or([0; 4])) as u64)
            }
            Type::I64 | Type::Date => {
                Value::I(i64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8])))
            }
            Type::U64 => Value::U(u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8]))),
            Type::F32 => Value::F32(f32::from_bits(u32::from_ne_bytes(
                bytes[..4].try_into().unwrap_or([0; 4]),
            ))),
            Type::F64 => Value::F64(f64::from_bits(u64::from_ne_bytes(
                bytes[..8].try_into().unwrap_or([0; 8]),
            ))),
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                Value::Blob(bytes[..need].to_vec())
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => Value::Blob(bytes[..need].to_vec()),
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Nullable(_)
            | Type::Generator(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Null => {
                let handle =
                    u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8])) as usize as *mut u8;
                if handle.is_null() {
                    Value::Null
                } else {
                    Value::Handle(handle)
                }
            }
            Type::Void | Type::Error => Value::Void,
            _ => return Err(self.invalid(None, format!("unpacking {ty:?} is not defined"))),
        })
    }

    fn copy_value(
        &self,
        value: &Value,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        if let Some(l::ValueType::Data(Type::Class(id))) = result_ty {
            if self
                .module
                .classes
                .get(id.0)
                .is_some_and(|class| class.is_value)
            {
                let Value::Blob(bytes) = value else {
                    return Err(type_error("value class", value));
                };
                return Ok(Value::Blob(bytes.clone()));
            }
        }
        Ok(value.clone())
    }

    fn unary(
        &self,
        operator: l::UnaryOp,
        value: &Value,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let ty = match result_ty {
            Some(l::ValueType::Data(ty)) => ty,
            _ => return Err(self.invalid(None, "unary instruction has no data result")),
        };
        match operator {
            l::UnaryOp::Not => Ok(Value::Bool(!value.as_bool()?)),
            l::UnaryOp::Neg => match ty {
                Type::F32 => Ok(Value::F32(-(value.as_f64()? as f32))),
                Type::F64 => Ok(Value::F64(-value.as_f64()?)),
                _ => self.integer_result(ty, value.as_i64()?.wrapping_neg() as u64),
            },
            l::UnaryOp::BitNot => self.integer_result(ty, !value.as_u64()?),
        }
    }

    fn binary(
        &mut self,
        operator: l::BinaryOp,
        left: &Value,
        right: &Value,
        result_ty: Option<&l::ValueType>,
        function: &l::Function,
        instruction: &l::Instruction,
    ) -> Result<Value, InterpretError> {
        let ty = match result_ty {
            Some(l::ValueType::Data(ty)) => ty,
            _ => {
                return Err(self.invalid(
                    Some(instruction.pos.clone()),
                    "binary instruction has no data result",
                ));
            }
        };
        if matches!(operator, l::BinaryOp::Eq | l::BinaryOp::Ne) {
            let operand_ty = self
                .instruction_operand_type(function, instruction, 0)
                .unwrap_or(ty);
            let equal = self.equal(left, right, operand_ty)?;
            return Ok(Value::Bool(if operator == l::BinaryOp::Eq {
                equal
            } else {
                !equal
            }));
        }
        if matches!(
            operator,
            l::BinaryOp::Lt | l::BinaryOp::Le | l::BinaryOp::Gt | l::BinaryOp::Ge
        ) {
            let order = match (left, right) {
                (Value::F32(a), Value::F32(b)) => {
                    compare_f64(f64::from(*a), f64::from(*b), operator)
                }
                (Value::F64(a), Value::F64(b)) => compare_f64(*a, *b, operator),
                (Value::I(a), Value::I(b)) => compare_i64(*a, *b, operator),
                _ => compare_u64(left.as_u64()?, right.as_u64()?, operator),
            };
            return Ok(Value::Bool(order));
        }
        if matches!(ty, Type::Str) && operator == l::BinaryOp::Add {
            let mut bytes = self.string_bytes(left.as_handle()?)?;
            bytes.extend_from_slice(&self.string_bytes(right.as_handle()?)?);
            return Ok(Value::Handle(self.alloc_string(&bytes, &instruction.pos)?));
        }
        match ty {
            Type::F32 => {
                let a = left.as_f64()? as f32;
                let b = right.as_f64()? as f32;
                Ok(Value::F32(match operator {
                    l::BinaryOp::Add => a + b,
                    l::BinaryOp::Sub => a - b,
                    l::BinaryOp::Mul => a * b,
                    l::BinaryOp::Div => a / b,
                    l::BinaryOp::Rem => a % b,
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            format!("{operator:?} is not f32 arithmetic"),
                        ));
                    }
                }))
            }
            Type::F64 => {
                let a = left.as_f64()?;
                let b = right.as_f64()?;
                Ok(Value::F64(match operator {
                    l::BinaryOp::Add => a + b,
                    l::BinaryOp::Sub => a - b,
                    l::BinaryOp::Mul => a * b,
                    l::BinaryOp::Div => a / b,
                    l::BinaryOp::Rem => a % b,
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            format!("{operator:?} is not f64 arithmetic"),
                        ));
                    }
                }))
            }
            _ => {
                let a = left.as_u64()?;
                let b = right.as_u64()?;
                if b == 0 && matches!(operator, l::BinaryOp::Div | l::BinaryOp::Rem) {
                    let trap = instruction
                        .traps
                        .iter()
                        .find(|trap| trap.kind == l::TrapKind::DivisionByZero)
                        .ok_or_else(|| {
                            self.invalid(
                                Some(instruction.pos.clone()),
                                "integer division has no DivisionByZero site",
                            )
                        })?;
                    return Err(self.trap_error(trap));
                }
                let bits = integer_bits(ty).ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        format!("{ty:?} is not integer"),
                    )
                })?;
                let shift = (b & u64::from(bits - 1)) as u32;
                let value = match operator {
                    l::BinaryOp::Add => a.wrapping_add(b),
                    l::BinaryOp::Sub => a.wrapping_sub(b),
                    l::BinaryOp::Mul => a.wrapping_mul(b),
                    l::BinaryOp::Div if is_signed(ty) => {
                        sign_extend(a, bits).wrapping_div(sign_extend(b, bits)) as u64
                    }
                    l::BinaryOp::Div => a / b,
                    l::BinaryOp::Rem if is_signed(ty) => {
                        sign_extend(a, bits).wrapping_rem(sign_extend(b, bits)) as u64
                    }
                    l::BinaryOp::Rem => a % b,
                    l::BinaryOp::BitAnd => a & b,
                    l::BinaryOp::BitOr => a | b,
                    l::BinaryOp::BitXor => a ^ b,
                    l::BinaryOp::Shl => a.wrapping_shl(shift),
                    l::BinaryOp::Shr if is_signed(ty) => (sign_extend(a, bits) >> shift) as u64,
                    l::BinaryOp::Shr => mask_bits(a, bits) >> shift,
                    l::BinaryOp::UShr => mask_bits(a, bits) >> shift,
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            format!("{operator:?} is not integer arithmetic"),
                        ));
                    }
                };
                self.integer_result(ty, value)
            }
        }
    }

    fn equal(&self, left: &Value, right: &Value, ty: &Type) -> Result<bool, InterpretError> {
        Ok(match ty {
            Type::F32 => left.as_f64()? as f32 == right.as_f64()? as f32,
            Type::F64 => left.as_f64()? == right.as_f64()?,
            Type::Str => {
                self.string_bytes(left.as_handle()?)? == self.string_bytes(right.as_handle()?)?
            }
            Type::Bool => left.as_bool()? == right.as_bool()?,
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                match (left, right) {
                    (Value::Blob(a), Value::Blob(b)) => a == b,
                    _ => false,
                }
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => match (left, right) {
                (Value::Blob(a), Value::Blob(b)) => a == b,
                _ => false,
            },
            Type::Null
            | Type::Nullable(_)
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::RegExp
            | Type::Generator(_) => left.as_handle()? == right.as_handle()?,
            _ if is_signed(ty) => left.as_i64()? == right.as_i64()?,
            _ => left.as_u64()? == right.as_u64()?,
        })
    }

    fn integer_result(&self, ty: &Type, value: u64) -> Result<Value, InterpretError> {
        let bits =
            integer_bits(ty).ok_or_else(|| self.invalid(None, format!("{ty:?} is not integer")))?;
        let value = mask_bits(value, bits);
        if is_signed(ty) {
            Ok(Value::I(sign_extend(value, bits)))
        } else {
            Ok(Value::U(value))
        }
    }

    fn convert(
        &self,
        value: &Value,
        result_ty: Option<&l::ValueType>,
        source_ty: Option<&Type>,
        instruction: &l::Instruction,
    ) -> Result<Value, InterpretError> {
        let ty = self.data_result_type(result_ty, instruction)?;
        Ok(match ty {
            Type::F16 => Value::U(ffi::subscript_rt_f16_from_f64(value.as_f64()?) as u64),
            Type::F32 => Value::F32(if matches!(source_ty, Some(Type::F16)) {
                ffi::subscript_rt_f16_to_f64(value.as_u64()? as u16) as f32
            } else {
                value.as_f64()? as f32
            }),
            Type::F64 => Value::F64(if matches!(source_ty, Some(Type::F16)) {
                ffi::subscript_rt_f16_to_f64(value.as_u64()? as u16)
            } else {
                value.as_f64()?
            }),
            Type::Bool => Value::Bool(value.as_bool()?),
            ty if integer_bits(ty).is_some() => {
                let raw = match value {
                    Value::F32(v) => *v as i128 as u64,
                    Value::F64(v) => *v as i128 as u64,
                    _ => value.as_u64()?,
                };
                self.integer_result(ty, raw)?
            }
            Type::Nullable(_) | Type::Object | Type::Class(_) => value.clone(),
            _ => value.clone(),
        })
    }

    fn length(&self, value: &Value, ty: Option<&Type>) -> Result<i32, InterpretError> {
        match (value, ty) {
            (Value::Handle(handle), Some(Type::Str)) => Ok(ffi_len_string(&self.context, *handle)),
            (Value::Handle(handle), Some(Type::Array(_))) => {
                // SAFETY: verified dynamic-array operand.
                Ok(unsafe { self.context.array_len(*handle) })
            }
            (Value::Blob(_), Some(Type::FixedArray(_, count))) => Ok(*count as i32),
            (Value::Blob(bytes), _) => Ok(bytes.len() as i32),
            (other, _) => Err(type_error("container", other)),
        }
    }

    fn array_literal(
        &mut self,
        ty: &Type,
        elements: &[Value],
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        if let Type::FixedArray(element_ty, count) = ty {
            if elements.len() != *count as usize {
                return Err(self.invalid(
                    Some(pos.clone()),
                    format!(
                        "fixed array has {} elements for declared count {count}",
                        elements.len()
                    ),
                ));
            }
            let element_layout = self.type_layout(element_ty)?;
            let stride = align_up(element_layout.size, element_layout.align);
            let mut bytes = vec![0; stride * *count as usize];
            for (index, element) in elements.iter().enumerate() {
                self.pack_into(element_ty, element, &mut bytes[index * stride..])?;
            }
            return Ok(Value::Blob(bytes));
        }
        let Type::Array(element_ty) = ty else {
            return Err(self.invalid(
                Some(pos.clone()),
                "ArrayLiteral result is neither dynamic nor fixed array",
            ));
        };
        let element_layout = self.type_layout(element_ty)?;
        // SAFETY: Context and element size meet the runtime array contract.
        let array = unsafe {
            ffi::subscript_rt_array_new(&mut *self.context, element_layout.size as u64, 0)
        };
        self.check_runtime(pos)?;
        self.root_handle(array);
        for element in elements {
            let bytes = self.pack(element_ty, element)?;
            // SAFETY: `bytes` is exactly one element and the array is live.
            unsafe { ffi::subscript_rt_array_push(&mut *self.context, array, bytes.as_ptr(), 0) };
            self.check_runtime(pos)?;
        }
        Ok(Value::Handle(array))
    }

    fn array_spread_literal(
        &mut self,
        ty: &Type,
        parts: &[Option<l::SpreadKind>],
        operands: &[Value],
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let Type::Array(element_ty) = ty else {
            return Err(self.invalid(
                Some(pos.clone()),
                "spread literal result is not a dynamic array",
            ));
        };
        let created = self.array_literal(ty, &[], pos)?;
        let Value::Handle(out) = created else {
            return Err(self.invalid(
                Some(pos.clone()),
                "spread literal allocation did not return an array handle",
            ));
        };
        for (part, operand) in parts.iter().zip(operands) {
            match part {
                None => {
                    let bytes = self.pack(element_ty, operand)?;
                    // SAFETY: one correctly packed element.
                    unsafe {
                        ffi::subscript_rt_array_push(&mut *self.context, out, bytes.as_ptr(), 0)
                    };
                }
                Some(l::SpreadKind::Array) => {
                    // SAFETY: verified identical-element array handles.
                    unsafe {
                        ffi::subscript_rt_array_spread_array(
                            &mut *self.context,
                            out,
                            operand.as_handle()?,
                            0,
                        )
                    };
                }
                Some(l::SpreadKind::FixedArray) => {
                    let Value::Blob(bytes) = operand else {
                        return Err(type_error("fixed array", operand));
                    };
                    let width = self
                        .layout_cached(element_ty)
                        .ok_or_else(|| {
                            self.invalid(Some(pos.clone()), "spread element has no layout")
                        })?
                        .size;
                    // SAFETY: fixed blob consists of whole elements.
                    unsafe {
                        ffi::subscript_rt_array_spread_fixed(
                            &mut *self.context,
                            out,
                            bytes.as_ptr(),
                            (bytes.len() / width) as u64,
                            0,
                        )
                    };
                }
                Some(l::SpreadKind::MapKeys | l::SpreadKind::SetValues) => {
                    // SAFETY: runtime association traversal owns order/bound.
                    unsafe {
                        ffi::subscript_rt_array_spread_assoc(
                            &mut *self.context,
                            out,
                            operand.as_handle()?,
                            0,
                        )
                    };
                }
                Some(l::SpreadKind::StringCodePoints) => {
                    // SAFETY: runtime string code-point traversal.
                    unsafe {
                        ffi::subscript_rt_array_spread_string(
                            &mut *self.context,
                            out,
                            operand.as_handle()?,
                            0,
                        )
                    };
                }
            }
            self.check_runtime(pos)?;
        }
        Ok(Value::Handle(out))
    }

    fn template(
        &mut self,
        parts: &[l::TemplatePart],
        operands: &[Value],
        function: &l::Function,
        instruction: &l::Instruction,
    ) -> Result<*mut u8, InterpretError> {
        let mut bytes = Vec::new();
        for part in parts {
            match part {
                l::TemplatePart::Text(text) => bytes.extend_from_slice(text.as_bytes()),
                l::TemplatePart::Operand(index) => {
                    let value = operands.get(*index as usize).ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("template operand {index} is missing"),
                        )
                    })?;
                    let ty = self.instruction_operand_type(function, instruction, *index as usize);
                    bytes.extend_from_slice(&self.format_value(value, ty)?);
                }
            }
        }
        self.alloc_string(&bytes, &instruction.pos)
    }

    fn format_value(&self, value: &Value, ty: Option<&Type>) -> Result<Vec<u8>, InterpretError> {
        Ok(match value {
            Value::I(value)
                if matches!(ty, Some(Type::I8 | Type::I16 | Type::I32 | Type::Enum(_))) =>
            {
                subscript_runtime::fmt::fmt_i32(*value as i32).into_bytes()
            }
            Value::I(value) if matches!(ty, Some(Type::StringAlias(_))) => {
                let alias = match ty {
                    Some(Type::StringAlias(alias)) => alias,
                    _ => return Err(self.invalid(None, "string-alias formatter has no alias type")),
                };
                let alias = self
                    .module
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| self.invalid(None, "string alias is missing"))?;
                let index = if let Some(wire) = &alias.wire_values {
                    wire.iter()
                        .position(|candidate| i64::from(*candidate) == *value)
                } else {
                    usize::try_from(*value).ok()
                };
                alias
                    .members
                    .get(index.unwrap_or(usize::MAX))
                    .ok_or_else(|| self.invalid(None, "string alias value has no member"))?
                    .as_bytes()
                    .to_vec()
            }
            Value::I(value) => subscript_runtime::fmt::fmt_i64(*value).into_bytes(),
            Value::U(value) if matches!(ty, Some(Type::U8 | Type::U16 | Type::U32)) => {
                subscript_runtime::fmt::fmt_u32(*value as u32).into_bytes()
            }
            Value::U(value) if matches!(ty, Some(Type::F16)) => {
                subscript_runtime::fmt::fmt_f64(ffi::subscript_rt_f16_to_f64(*value as u16))
                    .into_bytes()
            }
            Value::U(value) => subscript_runtime::fmt::fmt_u64(*value).into_bytes(),
            Value::F32(value) => subscript_runtime::fmt::fmt_f32(*value).into_bytes(),
            Value::F64(value) => subscript_runtime::fmt::fmt_f64(*value).into_bytes(),
            Value::Bool(value) => subscript_runtime::fmt::fmt_bool(*value).into_bytes(),
            Value::Handle(handle) => self.string_bytes(*handle)?,
            Value::Null => b"null".to_vec(),
            other => return Err(type_error("interpolatable scalar", other)),
        })
    }

    fn iterator_create(
        &mut self,
        kind: l::ForOfKind,
        subject: Value,
        subject_ty: Option<&Type>,
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let assoc_bound = if matches!(
            kind,
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
        ) {
            // SAFETY: verified Map/Set subject.
            Some(unsafe {
                ffi::subscript_rt_assoc_iter_begin(&mut *self.context, subject.as_handle()?, 0)
            })
        } else {
            None
        };
        self.check_runtime(pos)?;
        let fixed_bound = match subject_ty {
            Some(Type::FixedArray(_, count)) => Some(*count as i32),
            _ => None,
        };
        let assoc_probe_size = match result_ty {
            Some(l::ValueType::Iterator(iterator))
                if matches!(
                    kind,
                    l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
                ) =>
            {
                Some(
                    self.layout_cached(&iterator.element)
                        .ok_or_else(|| {
                            self.invalid(Some(pos.clone()), "association element has no layout")
                        })?
                        .size,
                )
            }
            _ => None,
        };
        Ok(Value::Iterator(Rc::new(IteratorCursor {
            kind,
            subject,
            assoc_bound,
            position: 0,
            next_position: Cell::new(0),
            fixed_bound,
            assoc_probe_size,
        })))
    }

    fn iterator_has_next(
        &mut self,
        cursor: &Rc<IteratorCursor>,
        index: i64,
        bound: i64,
        pos: &Pos,
    ) -> Result<bool, InterpretError> {
        match cursor.kind {
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys => {
                let current = unsafe { self.context.array_len(cursor.subject.as_handle()?) };
                Ok(index < bound && index < i64::from(current))
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let width = cursor.assoc_probe_size.ok_or_else(|| {
                    self.invalid(Some(pos.clone()), "association cursor has no probe layout")
                })?;
                let mut probe = vec![0_u8; width.max(1)];
                let mut position = index.max(cursor.position).max(0);
                while position < bound {
                    let active = unsafe {
                        ffi::subscript_rt_assoc_iter_copy(
                            &mut *self.context,
                            cursor.subject.as_handle()?,
                            position as u64,
                            u32::from(cursor.kind == l::ForOfKind::MapValues),
                            probe.as_mut_ptr(),
                            0,
                        )
                    };
                    self.check_runtime(pos)?;
                    if active != 0 {
                        cursor.next_position.set(position);
                        return Ok(true);
                    }
                    position += 1;
                }
                Ok(false)
            }
            l::ForOfKind::StringCodePoints => Ok(cursor.position < bound),
            l::ForOfKind::FixedArrayValues => Ok(index < bound),
        }
    }

    fn iterator_bound(&self, cursor: &Value) -> Result<i32, InterpretError> {
        let cursor = cursor.as_iterator()?;
        if let Some(bound) = cursor.assoc_bound {
            return i32::try_from(bound)
                .map_err(|_| self.invalid(None, "iterator bound exceeds i32"));
        }
        match (&cursor.kind, &cursor.subject) {
            (l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys, Value::Handle(array)) => {
                // SAFETY: live array cursor subject.
                Ok(unsafe { self.context.array_len(*array) })
            }
            (l::ForOfKind::StringCodePoints, Value::Handle(string)) => {
                Ok(ffi_len_string(&self.context, *string))
            }
            (l::ForOfKind::FixedArrayValues, Value::Blob(_)) => cursor
                .fixed_bound
                .ok_or_else(|| self.invalid(None, "fixed-array cursor has no declared count")),
            _ => Err(self.invalid(None, "iterator cursor kind and subject disagree")),
        }
    }

    fn iterator_value(
        &mut self,
        cursor: &Value,
        index: i64,
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let cursor = cursor.as_iterator()?;
        let ty = match result_ty {
            Some(l::ValueType::Data(ty)) => ty,
            _ => return Err(self.invalid(Some(pos.clone()), "IteratorValue has no data result")),
        };
        match cursor.kind {
            l::ForOfKind::ArrayKeys => Ok(Value::I(index)),
            l::ForOfKind::ArrayValues => {
                let array = cursor.subject.as_handle()?;
                // SAFETY: HasNext established index < captured bound; runtime also
                // checks the current live array length after removals.
                let pointer = unsafe { self.context.array_elem_ptr(array, index as i32, 0) };
                self.check_runtime(pos)?;
                let layout = self.layout_cached(ty).ok_or_else(|| {
                    self.invalid(Some(pos.clone()), "iterator element has no layout")
                })?;
                // SAFETY: runtime returned one element pointer.
                self.unpack(ty, unsafe {
                    std::slice::from_raw_parts(pointer, layout.size)
                })
            }
            l::ForOfKind::FixedArrayValues => {
                let Value::Blob(bytes) = &cursor.subject else {
                    return Err(type_error("fixed array", &cursor.subject));
                };
                let layout = self.layout_cached(ty).ok_or_else(|| {
                    self.invalid(Some(pos.clone()), "iterator element has no layout")
                })?;
                let offset = index as usize * align_up(layout.size, layout.align);
                self.unpack(ty, bytes.get(offset..).unwrap_or_default())
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let layout = self.layout_cached(ty).ok_or_else(|| {
                    self.invalid(Some(pos.clone()), "association element has no layout")
                })?;
                let mut bytes = vec![0; layout.size];
                // SAFETY: traversal token, index, selection, and output follow the
                // runtime's fixed-bound association protocol.
                let active = unsafe {
                    ffi::subscript_rt_assoc_iter_copy(
                        &mut *self.context,
                        cursor.subject.as_handle()?,
                        cursor.next_position.get() as u64,
                        u32::from(cursor.kind == l::ForOfKind::MapValues),
                        bytes.as_mut_ptr(),
                        0,
                    )
                };
                self.check_runtime(pos)?;
                if active == 0 {
                    return Err(self.invalid(
                        Some(pos.clone()),
                        "iteration selected an entry removed before its visit",
                    ));
                }
                self.unpack(ty, &bytes)
            }
            l::ForOfKind::StringCodePoints => {
                let mut next = cursor.position as i32;
                // SAFETY: runtime performs UTF-8 scalar stepping and returns its
                // interned one-code-point string.
                let value = unsafe {
                    ffi::subscript_rt_str_iter_code_point(
                        &mut *self.context,
                        cursor.subject.as_handle()?,
                        cursor.position as i32,
                        &mut next,
                        0,
                    )
                };
                self.check_runtime(pos)?;
                cursor.next_position.set(i64::from(next));
                self.root_handle(value);
                Ok(Value::Handle(value))
            }
        }
    }

    fn iterator_advance(&self, cursor: &Value) -> Result<Value, InterpretError> {
        let cursor = cursor.as_iterator()?;
        let position = match cursor.kind {
            l::ForOfKind::StringCodePoints => cursor.next_position.get(),
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                cursor.next_position.get().saturating_add(1)
            }
            _ => cursor.position.saturating_add(1),
        };
        Ok(Value::Iterator(Rc::new(IteratorCursor {
            kind: cursor.kind,
            subject: cursor.subject.clone(),
            assoc_bound: cursor.assoc_bound,
            position,
            next_position: Cell::new(position),
            fixed_bound: cursor.fixed_bound,
            assoc_probe_size: cursor.assoc_probe_size,
        })))
    }

    fn iter_result(
        &self,
        done: bool,
        value: Value,
        value_ty: &Type,
    ) -> Result<Value, InterpretError> {
        let layout = self
            .layout_cached(value_ty)
            .ok_or_else(|| self.invalid(None, "generator value has no layout"))?;
        let offset = align_up(1, layout.align);
        let total = align_up(offset + layout.size, layout.align);
        let mut bytes = vec![0; total];
        bytes[0] = u8::from(done);
        self.pack_into(value_ty, &value, &mut bytes[offset..])?;
        Ok(Value::Blob(bytes))
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
    state.error = Some(error);
    // The runtime uses its trap flag to stop the traversal immediately. The
    // interpreter returns the more precise saved error after the FFI call.
    unsafe { (*ctx).trap(RuntimeTrapKind::Internal, message, 0) };
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
        let _ = interpreter.invoke_callable(&callable, vec![value, key])?;
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
        let _ = interpreter.invoke_callable(&callable, vec![key])?;
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
        let key = interpreter.invoke_callable(&callable, vec![element])?;
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
mod tests {
    use super::*;

    fn empty_module(functions: Vec<l::Function>) -> l::Module {
        l::Module {
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
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: Vec::new(),
            return_type: Type::I32,
            locals: Vec::new(),
            values: Vec::new(),
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
            parameters: Vec::new(),
            return_type: Type::I32,
            locals: Vec::new(),
            values: vec![
                l::Value {
                    id: l::ValueId(0),
                    ty: l::ValueType::Data(Type::I32),
                    source_name: Some("resume".to_string()),
                },
                l::Value {
                    id: l::ValueId(1),
                    ty: l::ValueType::Data(Type::I32),
                    source_name: Some("live".to_string()),
                },
                l::Value {
                    id: l::ValueId(2),
                    ty: l::ValueType::Data(Type::I32),
                    source_name: Some("live.resume".to_string()),
                },
                l::Value {
                    id: l::ValueId(3),
                    ty: l::ValueType::Data(Type::I32),
                    source_name: None,
                },
            ],
            blocks: vec![
                l::BasicBlock {
                    id: l::BlockId(0),
                    source_name: Some("entry".to_string()),
                    parameters: Vec::new(),
                    instructions: vec![l::Instruction {
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
                    instructions: vec![l::Instruction {
                        result: Some(l::ValueId(3)),
                        kind: l::InstructionKind::Binary(l::BinaryOp::Add),
                        operands: vec![
                            l::Operand::Value(l::ValueId(0)),
                            l::Operand::Value(l::ValueId(2)),
                        ],
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                        pos: pos.clone(),
                    }],
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
}

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
