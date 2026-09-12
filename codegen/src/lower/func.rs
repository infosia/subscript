//! LIR-to-Cranelift function transcriber for the development tier.
//!
//! LIR fixes evaluation order, control flow, entity identity, traps, and
//! suspension live-ins. This module assigns target storage and emits CLIF.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    types, AbiParam, ArgumentPurpose, Block, BlockArg, InstBuilder, MemFlags, Signature,
    StackSlotData, StackSlotKind, Value,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use subscript_compiler::lir as l;
use subscript_compiler::types::{CRANELIFT_FRAME_ALIGNMENT, MAX_FRAME_BYTES};
use subscript_compiler::{ClassId, Pos, Type};
use subscript_runtime::context as rtc;
use subscript_runtime::TrapKind;

use crate::layout::{closure_environment_layout, is_unsigned, managed_words, Layouts, Repr};
use crate::lir_types::{
    array_element_kind, array_format_kind, association_key_kind, boundary_box_class,
    boundary_class_contains_pointer, boundary_class_needs_scratch, boundary_class_requires_build,
    capture_parameters, data_type, explicit_parameters, foreign_parameter_type_matches,
    is_userdata_slot, operand_type, runtime_trap_kind, value_type,
};
use crate::lower::{
    checked_layout_add, checked_layout_mul, internal, round_up_layout, FnKey, GlobalSlot, ModLower,
};
use crate::root_storage::{self, RootStoragePlan};

mod abi;
mod aggregate;
mod boundary;
mod builtin;
mod call;
mod coroutine;
mod expr;
mod instruction;
mod intrinsic;
mod iterator;
mod place;
mod terminator;
mod value;

use self::coroutine::{ensure_explicit_frame_supported, plan_coroutine};

#[derive(Debug, Clone, Copy)]
enum RV {
    None,
    Scalar(Value),
    Pair(Value, Value),
    Aggregate(Value),
}

#[derive(Debug, Clone, Copy)]
enum StructRet {
    Sret(Value),
    Registers {
        slot: Value,
        count: u32,
        ty: types::Type,
    },
}

/// The target ABI whose by-value aggregate rule the dev JIT must build.
/// The ship tier hands every aggregate to the platform C compiler, so this
/// exists for the dev JIT alone (`specs/blocks/compiler.md` §12.3a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AggregateAbi {
    Aapcs64,
    Win64,
    SysV,
}

impl AggregateAbi {
    /// The by-value aggregate ABI of `triple`, or `None` when this dev host
    /// has no implemented and verified rule. Lowering reads this function,
    /// so a host it does not name fails loud instead of a silent
    /// mis-marshal (dev-JIT ≠ ship-C).
    fn of(triple: &target_lexicon::Triple) -> Option<Self> {
        use target_lexicon::{Architecture, OperatingSystem};
        match triple.architecture {
            Architecture::Aarch64(_) => Some(Self::Aapcs64),
            Architecture::X86_64 => Some(match triple.operating_system {
                OperatingSystem::Windows => Self::Win64,
                _ => Self::SysV,
            }),
            _ => None,
        }
    }
}

/// The register class of one aggregate image. AAPCS64 and Win64 images are
/// always `Integer`; SysV classifies each eightbyte separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterClass {
    Integer,
    Sse,
}

/// One aggregate register image: its byte offset in the C struct, its
/// register class, and the CLIF type that carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EightbyteImage {
    offset: u32,
    class: RegisterClass,
    ty: types::Type,
}

/// How the target ABI passes one by-value boundary aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AggregateArgPlan {
    /// AAPCS64 HFA: the leaves stay component-wise float-register arguments.
    Hfa(Vec<(u32, types::Type)>),
    /// One register argument per image, read from the aggregate's bytes.
    Images(Vec<EightbyteImage>),
    /// The address of a caller copy is the argument.
    Indirect,
    /// SysV MEMORY class: the caller copy occupies `stack_size` bytes, rounded to whole eightbytes.
    Memory { stack_size: u32 },
}

/// The C-layout leaves of one by-value boundary aggregate, with the byte
/// offsets of its `f16` fields listed apart: `f16` is storage-only
/// (`specs/blocks/compiler.md` §16.2) and has no verified register image.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BoundaryLeaves {
    leaves: Vec<(u32, types::Type)>,
    f16_offsets: Vec<u32>,
}

impl BoundaryLeaves {
    /// A `(pointer, length)` descriptor. Both halves are integer-class on
    /// every supported ABI.
    fn descriptor() -> Self {
        Self {
            leaves: vec![(0, types::I64), (8, types::I64)],
            f16_offsets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BoundaryPtrWriteback {
    class: usize,
    source: Value,
    scratch: Value,
}

#[derive(Debug, Clone, Copy)]
enum TrapOperand {
    Pending,
    Value(Value),
    Condition(Value),
    Index {
        condition: Value,
        index: Value,
        length: Value,
    },
    WireValue {
        wire: Value,
        valid: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoroutineKind {
    Generator,
    Async,
}

#[derive(Debug, Clone)]
struct FrameSlot {
    offset: u32,
    ty: l::ValueType,
}

#[derive(Debug, Clone)]
struct SuspendPlan {
    state: i64,
    arguments: Vec<FrameSlot>,
    child: Option<u32>,
}

#[derive(Debug, Clone)]
struct CoroutinePlan {
    parameter_slots: Vec<FrameSlot>,
    local_slots: Vec<Option<FrameSlot>>,
    suspends: HashMap<l::BlockId, SuspendPlan>,
    stable_addresses: HashMap<l::ValueId, u32>,
    closure_environments: HashMap<l::ValueId, u32>,
    size: u32,
}

fn runtime_traps(function: &l::Function) -> Vec<l::Trap> {
    let mut traps = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            traps.extend(instruction.traps.iter().cloned());
        }
        match &block.terminator {
            l::Terminator::Trap(trap) => traps.push(trap.clone()),
            l::Terminator::Unreachable { .. } => {}
            l::Terminator::Suspend { traps: sites, .. } => {
                traps.extend(sites.iter().cloned());
            }
            l::Terminator::Branch(_)
            | l::Terminator::ConditionalBranch { .. }
            | l::Terminator::Switch { .. }
            | l::Terminator::Return { .. } => {}
        }
    }
    traps
}

pub(super) fn verify_trap_consumption(
    function: &l::Function,
    expected: &[l::Trap],
    consumed: &[l::Trap],
) -> Result<(), String> {
    verify_trap_consumption_for(
        function.id.0,
        &function.source_name,
        &function.pos,
        expected,
        consumed,
    )
}

fn verify_trap_consumption_for(
    function_id: u32,
    function_name: &str,
    function_pos: &Pos,
    expected: &[l::Trap],
    consumed: &[l::Trap],
) -> Result<(), String> {
    let mut matched = vec![false; consumed.len()];
    let mut missing = Vec::new();
    for trap in expected {
        if let Some(index) = consumed
            .iter()
            .zip(&matched)
            .position(|(candidate, matched)| !matched && candidate == trap)
        {
            matched[index] = true;
        } else {
            missing.push(trap);
        }
    }
    let extra = consumed
        .iter()
        .zip(matched)
        .filter_map(|(trap, matched)| (!matched).then_some(trap))
        .collect::<Vec<_>>();
    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }
    let site = missing
        .first()
        .copied()
        .or_else(|| extra.first().copied())
        .map_or(function_pos, |trap| &trap.pos);
    Err(internal(format!(
        "function {} `{}` trap-consumption mismatch at {site}: LIR carries {} site(s), transcriber consumed {}; missing {missing:?}; extra {extra:?}",
        function_id,
        function_name,
        expected.len(),
        consumed.len()
    )))
}

#[cfg(test)]
mod trap_consumption_tests {
    use super::*;

    #[test]
    fn duplicate_lir_site_fails_with_function_and_site() {
        let pos = Pos::new("trap-probe.ts", 4, 9);
        let trap = l::Trap {
            kind: l::TrapKind::Call,
            pos: pos.clone(),
        };
        let error =
            verify_trap_consumption_for(7, "probe", &pos, &[trap.clone(), trap.clone()], &[trap])
                .expect_err("one consumed site cannot satisfy two LIR sites");
        assert!(error.contains("function 7 `probe`"), "{error}");
        assert!(error.contains("trap-probe.ts:4:9"), "{error}");
        assert!(
            error.contains("LIR carries 2 site(s), transcriber consumed 1"),
            "{error}"
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct LocalSlot {
    address: Value,
}

const COROUTINE_DONE: i64 = 0x7fff_ffff;
const COROUTINE_RESUME_OFFSET: i32 = 8;
const GENERATOR_EPOCH_OFFSET: i32 = 4;
const COROUTINE_PAYLOAD_OFFSET: u32 = 16;
const ARRAY_LEN_OFFSET: i32 = 0;
const ARRAY_CAP_OFFSET: i32 = 8;
const ARRAY_ELEM_SIZE_OFFSET: i32 = 16;
const ARRAY_DATA_OFFSET: i32 = 24;

fn flags() -> MemFlags {
    MemFlags::trusted()
}

fn align_shift(align: u32) -> u8 {
    align.max(1).trailing_zeros() as u8
}

fn ctx_off(offset: usize) -> Result<i32, String> {
    i32::try_from(offset).map_err(|_| internal("context offset does not fit in i32"))
}

fn shift_mask(ty: &Type) -> Result<i64, String> {
    Ok(match ty {
        Type::I8 | Type::U8 => 7,
        Type::I16 | Type::U16 => 15,
        Type::I32 | Type::U32 => 31,
        Type::I64 | Type::U64 => 63,
        other => return Err(internal(format!("shift width for {other:?}"))),
    })
}

fn value_repr(layouts: &Layouts, value: &l::ValueType) -> Result<Repr, String> {
    match value {
        l::ValueType::Data(ty) => layouts.repr(ty),
        l::ValueType::Address(_) => Ok(Repr::Scalar(types::I64)),
        l::ValueType::Iterator(_) => Ok(Repr::Agg { size: 32, align: 8 }),
    }
}

fn value_size_align(layouts: &Layouts, value: &l::ValueType) -> Result<(u32, u32), String> {
    match value {
        l::ValueType::Data(ty) => layouts.size_align(ty),
        l::ValueType::Address(_) => Ok((8, 8)),
        l::ValueType::Iterator(_) => Ok((32, 8)),
    }
}

fn append_value_params(
    layouts: &Layouts,
    builder: &mut FunctionBuilder<'_>,
    block: Block,
    ty: &l::ValueType,
) -> Result<(), String> {
    match value_repr(layouts, ty)? {
        Repr::None => {}
        Repr::Scalar(value) => {
            builder.append_block_param(block, value);
        }
        Repr::Pair => {
            builder.append_block_param(block, types::I64);
            builder.append_block_param(block, types::I64);
        }
        Repr::Agg { .. } => {
            builder.append_block_param(block, types::I64);
        }
    }
    Ok(())
}

fn rv_args(value: RV) -> Vec<BlockArg> {
    match value {
        RV::None => Vec::new(),
        RV::Scalar(value) | RV::Aggregate(value) => vec![BlockArg::Value(value)],
        RV::Pair(code, env) => vec![BlockArg::Value(code), BlockArg::Value(env)],
    }
}

fn rv_from_params(
    layouts: &Layouts,
    ty: &l::ValueType,
    values: &[Value],
    cursor: &mut usize,
) -> Result<RV, String> {
    let take = |cursor: &mut usize| -> Result<Value, String> {
        let value = values
            .get(*cursor)
            .copied()
            .ok_or_else(|| internal("missing Cranelift block parameter"))?;
        *cursor += 1;
        Ok(value)
    };
    Ok(match value_repr(layouts, ty)? {
        Repr::None => RV::None,
        Repr::Scalar(_) => RV::Scalar(take(cursor)?),
        Repr::Pair => RV::Pair(take(cursor)?, take(cursor)?),
        Repr::Agg { .. } => RV::Aggregate(take(cursor)?),
    })
}

fn receiver_parameter(function: &l::Function) -> Option<&l::Parameter> {
    function
        .parameters
        .iter()
        .find(|parameter| parameter.kind == l::ParameterKind::Receiver)
}

fn function_has_receiver(function: &l::Function) -> bool {
    receiver_parameter(function).is_some()
}

fn function_has_environment(function: &l::Function) -> bool {
    matches!(function.kind, l::FunctionKind::Lambda)
}

fn coroutine_kind(function: &l::Function) -> Option<CoroutineKind> {
    if function.is_generator {
        Some(CoroutineKind::Generator)
    } else if function.is_async {
        Some(CoroutineKind::Async)
    } else {
        None
    }
}

fn function_key(function: &l::Function) -> FnKey {
    FnKey::LirFunction(function.id)
}

fn resume_key(function: &l::Function) -> FnKey {
    FnKey::LirResume(function.id)
}

struct Body<'f, 'm, 'a, 'l, M: Module> {
    ml: &'m mut ModLower<'a, M>,
    builder: FunctionBuilder<'f>,
    function: &'l l::Function,
    ctx: Value,
    sret: Option<Value>,
    frame: Option<Value>,
    out: Option<Value>,
    coroutine: Option<CoroutineKind>,
    values: Vec<Option<RV>>,
    locals: Vec<LocalSlot>,
    frame_local_slots: Vec<Option<FrameSlot>>,
    blocks: Vec<Block>,
    unwind: Option<Block>,
    shadow: Option<Value>,
    value_roots: HashMap<l::ValueId, u32>,
    root_storage: RootStoragePlan,
    resume_adapters: HashMap<l::BlockId, Block>,
    suspend_plans: HashMap<l::BlockId, SuspendPlan>,
    stable_addresses: HashMap<l::ValueId, u32>,
    closure_environments: HashMap<l::ValueId, u32>,
    closure_environment_layout: Option<(u32, u32)>,
    consumed_traps: Vec<l::Trap>,
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn emit_unwind(&mut self) -> Result<(), String> {
        let Some(block) = self.unwind else {
            return Ok(());
        };
        self.builder.switch_to_block(block);
        self.pop_shadow()?;
        if self.coroutine.is_some() {
            let one = self.iconst(types::I8, 1);
            self.builder.ins().return_(&[one]);
            return Ok(());
        }
        let mut returns = Vec::new();
        match self.ml.layouts.repr(&self.function.return_type)? {
            Repr::None | Repr::Agg { .. } => {}
            Repr::Scalar(ty) => returns.push(self.zero_scalar(ty)),
            Repr::Pair => {
                let zero = self.iconst(types::I64, 0);
                returns.extend([zero, zero]);
            }
        }
        self.builder.ins().return_(&returns);
        Ok(())
    }

    fn pop_shadow(&mut self) -> Result<(), String> {
        if self.shadow.is_some() {
            self.call_runtime(self.ml.rt.shadow_pop, &[self.ctx], false)?;
        }
        Ok(())
    }

    fn emit_graph(&mut self) -> Result<(), String> {
        for source in &self.function.blocks {
            let block = self.blocks[source.id.0 as usize];
            let parameters = self.builder.block_params(block).to_vec();
            let mut cursor = 0usize;
            for parameter in &source.parameters {
                let ty = self.value_type(*parameter)?.clone();
                let value = rv_from_params(&self.ml.layouts, &ty, &parameters, &mut cursor)?;
                let slot = self
                    .values
                    .get_mut(parameter.0 as usize)
                    .ok_or_else(|| internal(format!("value {} slot is missing", parameter.0)))?;
                *slot = Some(value);
            }
        }
        for source in &self.function.blocks {
            self.builder
                .switch_to_block(self.blocks[source.id.0 as usize]);
            let parameters = self
                .builder
                .block_params(self.blocks[source.id.0 as usize])
                .to_vec();
            let mut cursor = 0usize;
            let mut incoming = Vec::with_capacity(source.parameters.len());
            for parameter in &source.parameters {
                let ty = self.value_type(*parameter)?.clone();
                let mut value = rv_from_params(&self.ml.layouts, &ty, &parameters, &mut cursor)?;
                if self.closure_environment_layout.is_some()
                    && matches!(ty, l::ValueType::Data(Type::Func(_)))
                {
                    value = self.snapshot_closure_environment(value)?;
                }
                incoming.push((*parameter, value));
            }
            for (parameter, value) in incoming {
                self.set_value(parameter, value)?;
            }
            let entry_clears = self.root_storage.clear_at_block_entry[source.id.0 as usize].clone();
            self.clear_root_slots(&entry_clears)?;
            for (instruction_index, instruction) in source.instructions.iter().enumerate() {
                self.emit_instruction(instruction).map_err(|error| {
                    internal(format!(
                        "function {} block {} instruction {:?}: {error}",
                        self.function.id.0, source.id.0, instruction.kind
                    ))
                })?;
                let clears = self.root_storage.clear_after_instruction[source.id.0 as usize]
                    [instruction_index]
                    .clone();
                self.clear_root_slots(&clears)?;
            }
            self.emit_terminator(source.id, &source.terminator)?;
        }
        self.emit_unwind()?;
        Ok(())
    }
}

fn initialize_storage<M: Module>(body: &mut Body<'_, '_, '_, '_, M>) -> Result<(), String> {
    let mut words = body.root_storage.words;
    for (index, slot) in body.root_storage.value_slots.iter().copied().enumerate() {
        if let Some(slot) = slot {
            body.value_roots.insert(
                l::ValueId(index as u32),
                body.root_storage.slots[slot].offset,
            );
        }
    }
    let locals = body.function.locals.clone();
    let mut local_offsets = Vec::with_capacity(locals.len());
    for local in &locals {
        if local.storage == l::LocalStorageClass::Frame {
            local_offsets.push(None);
            continue;
        }
        let managed = match &local.ty {
            l::ValueType::Data(ty) => managed_words(&body.ml.layouts, ty)?,
            l::ValueType::Iterator(_) => 4,
            l::ValueType::Address(_) => 0,
        };
        if managed == 0 {
            local_offsets.push(None);
        } else {
            local_offsets.push(Some(words));
            words = checked_layout_add(words, managed, "LIR shadow local layout")?;
        }
    }
    let mut bytes = checked_layout_mul(words, 8, "LIR shadow frame")?;
    let mut shadow_align = 8u32;
    if body.coroutine.is_none() {
        if let Some((environment_size, environment_align)) = body.closure_environment_layout {
            for value in &body.function.values {
                if !matches!(value.ty, l::ValueType::Data(Type::Func(_))) {
                    continue;
                }
                bytes = round_up_layout(
                    bytes,
                    environment_align,
                    "closure shadow environment layout",
                )?;
                body.closure_environments.insert(value.id, bytes);
                bytes = checked_layout_add(
                    bytes,
                    environment_size,
                    "closure shadow environment layout",
                )?;
            }
            shadow_align = shadow_align.max(environment_align);
        }
    }
    if bytes != 0 {
        bytes = round_up_layout(bytes, 8, "final LIR shadow frame")?;
        let shadow = body.stack_slot(bytes, shadow_align);
        body.zero_bytes(shadow, bytes, 8);
        let count = body.iconst(types::I64, i64::from(bytes / 8));
        body.call_runtime(body.ml.rt.shadow_push, &[body.ctx, shadow, count], false)?;
        body.shadow = Some(shadow);
    }
    for (index, (local, root)) in locals.iter().zip(local_offsets).enumerate() {
        let address = if local.storage == l::LocalStorageClass::Frame {
            let frame = body
                .frame
                .ok_or_else(|| internal("frame-class local has no coroutine frame"))?;
            let slot = body
                .frame_local_slots
                .get(index)
                .and_then(Option::as_ref)
                .ok_or_else(|| internal("frame-class local has no frame layout slot"))?;
            body.address_offset(frame, i64::from(slot.offset))
        } else if let Some(root) = root {
            let shadow = body
                .shadow
                .ok_or_else(|| internal("rooted local has no shadow"))?;
            body.address_offset(shadow, i64::from(root) * 8)
        } else {
            let (size, align) = value_size_align(&body.ml.layouts, &local.ty)?;
            let address = body.stack_slot(size.max(1), align.max(1));
            body.zero_bytes(address, size.max(1), align.max(1));
            address
        };
        body.locals.push(LocalSlot { address });
    }
    Ok(())
}

fn define_assoc_bridge<M: Module>(
    ml: &mut ModLower<'_, M>,
    key: &Type,
    value: Option<&Type>,
) -> Result<cranelift_module::FuncId, String> {
    let mut signature = Signature::new(ml.call_conv);
    let fixed_parameters = if value.is_some() { 5 } else { 4 };
    for _ in 0..fixed_parameters {
        signature.params.push(AbiParam::new(types::I64));
    }
    let name = format!("subscript_assoc_bridge{}", ml.lambda_count);
    ml.lambda_count += 1;
    let id = ml
        .module
        .declare_function(&name, Linkage::Local, &signature)
        .map_err(|error| internal(format!("declare {name}: {error}")))?;
    let script_parameters = match value {
        Some(value) => vec![value.clone(), key.clone()],
        None => vec![key.clone()],
    };
    let script_signature = ml.make_sig(&script_parameters, &Type::Void, true, false)?;
    let mut context = ml.module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let fixed = builder.block_params(entry).to_vec();
        let pointers = if value.is_some() {
            vec![fixed[3], fixed[4]]
        } else {
            vec![fixed[3]]
        };
        let mut arguments = vec![fixed[0], fixed[2]];
        for (ty, pointer) in script_parameters.iter().zip(pointers) {
            match ml.layouts.repr(ty)? {
                Repr::None => {}
                Repr::Scalar(repr) => {
                    arguments.push(builder.ins().load(repr, flags(), pointer, 0));
                }
                Repr::Pair => {
                    arguments.push(builder.ins().load(types::I64, flags(), pointer, 0));
                    arguments.push(builder.ins().load(types::I64, flags(), pointer, 8));
                }
                Repr::Agg { size, align } => {
                    let slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        size.max(1),
                        align_shift(align.max(1)),
                    ));
                    let copy = builder.ins().stack_addr(types::I64, slot, 0);
                    let config = ml.module.isa().frontend_config();
                    let access_align = 1u32 << size.max(1).trailing_zeros();
                    let copy_align = align.max(1).min(access_align);
                    builder.emit_small_memory_copy(
                        config,
                        copy,
                        pointer,
                        u64::from(size),
                        copy_align as u8,
                        copy_align as u8,
                        true,
                        MemFlags::new(),
                    );
                    arguments.push(copy);
                }
            }
        }
        let signature = builder.import_signature(script_signature);
        builder.ins().call_indirect(signature, fixed[1], &arguments);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(ml, id, &mut context, &name)?;
    Ok(id)
}

fn define_group_bridge<M: Module>(
    ml: &mut ModLower<'_, M>,
    element: &Type,
    key: &Type,
) -> Result<cranelift_module::FuncId, String> {
    let Repr::Scalar(key_repr) = ml.layouts.repr(key)? else {
        return Err(internal(format!(
            "Map.GroupBy key representation is {key:?}"
        )));
    };
    let mut signature = Signature::new(ml.call_conv);
    for _ in 0..5 {
        signature.params.push(AbiParam::new(types::I64));
    }
    let name = format!("subscript_group_bridge{}", ml.lambda_count);
    ml.lambda_count += 1;
    let id = ml
        .module
        .declare_function(&name, Linkage::Local, &signature)
        .map_err(|error| internal(format!("declare {name}: {error}")))?;
    let script_signature = ml.make_sig(std::slice::from_ref(element), key, true, false)?;
    let mut context = ml.module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let fixed = builder.block_params(entry).to_vec();
        let mut arguments = vec![fixed[0], fixed[2]];
        match ml.layouts.repr(element)? {
            Repr::None => {}
            Repr::Scalar(repr) => {
                arguments.push(builder.ins().load(repr, flags(), fixed[3], 0));
            }
            Repr::Pair => {
                arguments.push(builder.ins().load(types::I64, flags(), fixed[3], 0));
                arguments.push(builder.ins().load(types::I64, flags(), fixed[3], 8));
            }
            Repr::Agg { size, align } => {
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    size.max(1),
                    align_shift(align.max(1)),
                ));
                let copy = builder.ins().stack_addr(types::I64, slot, 0);
                let config = ml.module.isa().frontend_config();
                let access_align = 1u32 << size.max(1).trailing_zeros();
                let copy_align = align.max(1).min(access_align);
                builder.emit_small_memory_copy(
                    config,
                    copy,
                    fixed[3],
                    u64::from(size),
                    copy_align as u8,
                    copy_align as u8,
                    true,
                    MemFlags::new(),
                );
                arguments.push(copy);
            }
        }
        let signature = builder.import_signature(script_signature);
        let call = builder.ins().call_indirect(signature, fixed[1], &arguments);
        let result = builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or_else(|| internal("Map.GroupBy callback has no result"))?;
        debug_assert_eq!(builder.func.dfg.value_type(result), key_repr);
        builder.ins().store(flags(), result, fixed[4], 0);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(ml, id, &mut context, &name)?;
    Ok(id)
}

fn define_context<M: Module>(
    ml: &mut ModLower<'_, M>,
    id: cranelift_module::FuncId,
    context: &mut cranelift_codegen::Context,
    label: &str,
) -> Result<(), String> {
    ensure_explicit_frame_supported(&context.func, label)?;
    #[cfg(test)]
    DEFINED_FUNCTION_TEXTS.with(|texts| {
        texts
            .borrow_mut()
            .push((label.to_string(), context.func.to_string()));
    });
    ml.module
        .define_function(id, context)
        .map_err(|error| internal(format!("define {label}: {error:?}")))?;
    ml.module.clear_context(context);
    Ok(())
}

#[cfg(test)]
thread_local! {
    static DEFINED_FUNCTION_TEXTS: std::cell::RefCell<Vec<(String, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(super) fn take_defined_function_texts() -> Vec<(String, String)> {
    DEFINED_FUNCTION_TEXTS.with(|texts| std::mem::take(&mut *texts.borrow_mut()))
}

#[cfg(test)]
mod completed_child_tests {
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::default_libcall_names;
    use subscript_compiler::{check_program, SourceFile};

    use super::take_defined_function_texts;
    use crate::lower::{dev_flags, lower_lir_module_with, LowerOptions};

    #[test]
    fn cranelift_clears_completed_async_child_slots() {
        let hir = check_program(&[SourceFile::new(
            "clear-async-child.ts",
            r#"
async function child(): Promise<i32> {
  await Context.suspend();
  return 1;
}

export async function main(): Promise<void> {
  await child();
  const held: Promise<i32> = child();
  await held;
}
"#,
        )])
        .expect("hand-built async module");
        let lir = crate::lir::lower_module(&hir).expect("hand-built async LIR");
        let main = lir
            .functions
            .iter()
            .find(|function| function.source_name == "main")
            .expect("main LIR function");
        let label = format!("LIR coroutine resume {}", main.id.0);

        let isa = cranelift_native::builder()
            .expect("host ISA")
            .finish(dev_flags().expect("dev flags"))
            .expect("ISA flags");
        let builder = JITBuilder::with_isa(isa, default_libcall_names());
        let mut module = JITModule::new(builder);
        let _ = take_defined_function_texts();
        lower_lir_module_with(&mut module, &lir, LowerOptions::default())
            .expect("hand-built async Cranelift lowering");
        let functions = take_defined_function_texts();
        // SAFETY: no finalized function address escapes this test.
        unsafe { module.free_memory() };
        let resume = functions
            .iter()
            .find(|(candidate, _)| candidate == &label)
            .map(|(_, text)| text)
            .expect("main coroutine resume CLIF");

        let zero_stores_at = |offset: u32| {
            let address = format!(", v1+{offset}");
            resume
                .lines()
                .filter(|line| {
                    line.contains("store notrap aligned")
                        && line.contains(&address)
                        && line.ends_with("= 0")
                })
                .count()
        };
        // §94.1 rules 2 and 3: an await registers and returns, so each child
        // slot is cleared on exactly one path, the scheduled resume.
        assert_eq!(zero_stores_at(16), 1, "direct child clear on its resume");
        assert_eq!(zero_stores_at(32), 1, "held child clear on its resume");
    }
}

/// Defines one ordinary LIR graph.
pub(crate) fn define_function<M: Module>(
    ml: &mut ModLower<'_, M>,
    function: &l::Function,
) -> Result<(), String> {
    let id = ml.func_id(&function_key(function))?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let abi = builder.block_params(entry).to_vec();
        let ctx = abi[0];
        let mut abi_cursor = 1usize;
        let environment = function_has_environment(function).then(|| {
            let value = abi[abi_cursor];
            abi_cursor += 1;
            value
        });
        let sret = matches!(ml.layouts.repr(&function.return_type)?, Repr::Agg { .. }).then(|| {
            let value = abi[abi_cursor];
            abi_cursor += 1;
            value
        });
        let receiver = function_has_receiver(function).then(|| {
            let value = abi[abi_cursor];
            abi_cursor += 1;
            value
        });
        let mut blocks = Vec::with_capacity(function.blocks.len());
        for source in &function.blocks {
            let block = builder.create_block();
            for parameter in &source.parameters {
                append_value_params(
                    &ml.layouts,
                    &mut builder,
                    block,
                    &function.values[parameter.0 as usize].ty,
                )?;
            }
            blocks.push(block);
        }
        let closure_environment_layout = closure_environment_layout(ml.lir, &ml.layouts)?;
        let root_storage = root_storage::plan(function, &ml.layouts)?;
        let mut body = Body {
            ml,
            builder,
            function,
            ctx,
            sret,
            frame: None,
            out: None,
            coroutine: None,
            values: vec![None; function.values.len()],
            locals: Vec::with_capacity(function.locals.len()),
            frame_local_slots: vec![None; function.locals.len()],
            blocks,
            unwind: None,
            shadow: None,
            value_roots: HashMap::new(),
            root_storage,
            resume_adapters: HashMap::new(),
            suspend_plans: HashMap::new(),
            stable_addresses: HashMap::new(),
            closure_environments: HashMap::new(),
            closure_environment_layout,
            consumed_traps: Vec::new(),
        };
        initialize_storage(&mut body)?;
        if matches!(function.kind, l::FunctionKind::ModuleInitializer) {
            initialize_module_globals(body.ml, &mut body.builder, body.ctx)?;
        }
        if let Some(environment) = environment {
            let mut offset = 0u32;
            for parameter in capture_parameters(function) {
                let ty = body.value_type(parameter.value)?.clone();
                let (size, align) = value_size_align(&body.ml.layouts, &ty)?;
                offset = round_up_layout(offset, align.max(1), "closure capture load")?;
                let value = body.load_value_type(&ty, environment, offset as i32)?;
                body.set_value(parameter.value, value)?;
                offset = checked_layout_add(offset, size.max(1), "closure capture load")?;
            }
        }
        if let Some(receiver) = receiver {
            let parameter = receiver_parameter(function)
                .ok_or_else(|| internal("receiver ABI value has no LIR parameter"))?;
            body.set_value(parameter.value, RV::Scalar(receiver))?;
        }
        let explicit = explicit_parameters(function).cloned().collect::<Vec<_>>();
        for parameter in &explicit {
            let ty = body.value_type(parameter.value)?.clone();
            let value = match value_repr(&body.ml.layouts, &ty)? {
                Repr::None => RV::None,
                Repr::Scalar(_) => {
                    let value = abi[abi_cursor];
                    abi_cursor += 1;
                    RV::Scalar(value)
                }
                Repr::Pair => {
                    let value = RV::Pair(abi[abi_cursor], abi[abi_cursor + 1]);
                    abi_cursor += 2;
                    value
                }
                Repr::Agg { .. } => {
                    let value = abi[abi_cursor];
                    abi_cursor += 1;
                    RV::Aggregate(value)
                }
            };
            body.set_value(parameter.value, value)?;
        }
        for parameter in &function.parameters {
            if let Some(storage) = parameter.storage {
                let value = body.value(parameter.value)?;
                let ty = body.value_type(parameter.value)?.clone();
                let address = body.locals[storage.0 as usize].address;
                body.store_value_type(&ty, address, 0, value)?;
            }
        }
        let destination = body.blocks[function.entry.0 as usize];
        body.builder.ins().jump(destination, &[]);
        body.emit_graph()?;
        verify_trap_consumption(function, &runtime_traps(function), &body.consumed_traps)?;
        body.builder.seal_all_blocks();
        body.builder.finalize();
    }
    define_context(
        ml,
        id,
        &mut context,
        &format!("LIR function {}", function.id.0),
    )
}

/// Defines the env-taking forwarding target used by `FunctionRef`.
pub(crate) fn define_wrapper<M: Module>(
    ml: &mut ModLower<'_, M>,
    function: &l::Function,
) -> Result<(), String> {
    if function.is_generator || function.is_async || !matches!(function.kind, l::FunctionKind::Free)
    {
        return Ok(());
    }
    let id = ml.func_id(&FnKey::LirWrapper(function.id))?;
    let target = ml.func_id(&FnKey::LirFunction(function.id))?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let incoming = builder.block_params(block).to_vec();
        let mut arguments = vec![incoming[0]];
        arguments.extend_from_slice(&incoming[2..]);
        let call = if ml.opts.reload {
            let slot = ml.slot_of(&FnKey::LirFunction(function.id))?;
            let displacement = i32::try_from(u64::from(slot) * 8)
                .map_err(|_| internal("wrapper function slot offset does not fit i32"))?;
            let table_offset = ctx_off(rtc::Context::fn_table_offset())?;
            let table = builder
                .ins()
                .load(types::I64, flags(), incoming[0], table_offset);
            let code = builder.ins().load(types::I64, flags(), table, displacement);
            let signature = builder.import_signature(ml.signature_of(target));
            builder.ins().call_indirect(signature, code, &arguments)
        } else {
            let target = ml.module.declare_func_in_func(target, builder.func);
            builder.ins().call(target, &arguments)
        };
        let results = builder.inst_results(call).to_vec();
        builder.ins().return_(&results);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(
        ml,
        id,
        &mut context,
        &format!("LIR wrapper {}", function.id.0),
    )
}

/// Defines the creator and resume halves of one LIR coroutine.
pub(crate) fn define_coroutine<M: Module>(
    ml: &mut ModLower<'_, M>,
    function: &l::Function,
) -> Result<(), String> {
    let plan = plan_coroutine(&ml.layouts, ml.lir, function)?;
    let closure_environment_layout = closure_environment_layout(ml.lir, &ml.layouts)?;
    let creator_id = ml.func_id(&function_key(function))?;
    let resume_id = ml.func_id(&resume_key(function))?;

    // Creator: allocate the exact LIR frame, stamp its resume identity, and
    // copy entry parameters. No source body executes until the first resume.
    {
        let mut context = ml.module.make_context();
        context.func.signature = ml.signature_of(creator_id);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            let abi = builder.block_params(entry).to_vec();
            let ctx = abi[0];
            let root_storage = root_storage::plan(function, &ml.layouts)?;
            let mut body = Body {
                ml,
                builder,
                function,
                ctx,
                sret: None,
                frame: None,
                out: None,
                coroutine: None,
                values: vec![None; function.values.len()],
                locals: Vec::new(),
                frame_local_slots: vec![None; function.locals.len()],
                blocks: Vec::new(),
                unwind: None,
                shadow: None,
                value_roots: HashMap::new(),
                root_storage,
                resume_adapters: HashMap::new(),
                suspend_plans: HashMap::new(),
                stable_addresses: HashMap::new(),
                closure_environments: HashMap::new(),
                closure_environment_layout,
                consumed_traps: Vec::new(),
            };
            let size = body.iconst(types::I64, i64::from(plan.size));
            let class = body.iconst(types::I32, i64::from(rtc::CLASS_GENERATOR));
            let allocation = function
                .creation_traps
                .iter()
                .find(|trap| trap.kind == l::TrapKind::Allocation)
                .ok_or_else(|| internal("coroutine creation has no allocation trap"))?;
            let position = body.position_id(&allocation.pos);
            let position = body.iconst(types::I32, position);
            let frame = body
                .call_runtime(body.ml.rt.alloc, &[body.ctx, size, class, position], false)?
                .ok_or_else(|| internal("coroutine allocation has no result"))?;
            body.emit_trap(allocation, TrapOperand::Pending)?;
            verify_trap_consumption(function, &function.creation_traps, &body.consumed_traps)?;
            let resume = body
                .ml
                .module
                .declare_func_in_func(resume_id, body.builder.func);
            let resume = body.builder.ins().func_addr(types::I64, resume);
            body.builder
                .ins()
                .store(flags(), resume, frame, COROUTINE_RESUME_OFFSET);
            if function.is_async {
                let result_size = if function.return_type == Type::Void {
                    0
                } else {
                    body.ml.layouts.size_align(&function.return_type)?.0
                };
                let result_size = body.iconst(types::I64, i64::from(result_size));
                body.call_runtime(
                    body.ml.rt.async_register,
                    &[body.ctx, frame, result_size],
                    false,
                )?;
            } else if body.ml.opts.reload {
                let offset = ctx_off(rtc::Context::reload_epoch_offset())?;
                let epoch = body
                    .builder
                    .ins()
                    .load(types::I32, flags(), body.ctx, offset);
                body.builder
                    .ins()
                    .store(flags(), epoch, frame, GENERATOR_EPOCH_OFFSET);
            }
            let mut cursor = 1usize;
            for (parameter, slot) in function.parameters.iter().zip(&plan.parameter_slots) {
                let ty = body.value_type(parameter.value)?.clone();
                let value = match value_repr(&body.ml.layouts, &ty)? {
                    Repr::None => RV::None,
                    Repr::Scalar(_) => {
                        let value = abi[cursor];
                        cursor += 1;
                        RV::Scalar(value)
                    }
                    Repr::Pair => {
                        let value = RV::Pair(abi[cursor], abi[cursor + 1]);
                        cursor += 2;
                        value
                    }
                    Repr::Agg { .. } => {
                        let value = abi[cursor];
                        cursor += 1;
                        RV::Aggregate(value)
                    }
                };
                body.store_value_type(&slot.ty, frame, slot.offset as i32, value)?;
            }
            body.builder.ins().return_(&[frame]);
            if let Some(unwind) = body.unwind {
                body.builder.switch_to_block(unwind);
                let zero = body.iconst(types::I64, 0);
                body.builder.ins().return_(&[zero]);
            }
            body.builder.seal_all_blocks();
            body.builder.finalize();
        }
        define_context(
            ml,
            creator_id,
            &mut context,
            &format!("LIR coroutine creator {}", function.id.0),
        )?;
    }

    // Resume: dispatch from the frame state to the LIR entry or an exact
    // suspend successor adapter, then transcribe the graph normally.
    {
        let mut context = ml.module.make_context();
        context.func.signature = ml.signature_of(resume_id);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            let abi = builder.block_params(entry).to_vec();
            let (ctx, frame, out) = (abi[0], abi[1], abi[2]);
            let mut blocks = Vec::with_capacity(function.blocks.len());
            for source in &function.blocks {
                let block = builder.create_block();
                for parameter in &source.parameters {
                    append_value_params(
                        &ml.layouts,
                        &mut builder,
                        block,
                        &function.values[parameter.0 as usize].ty,
                    )?;
                }
                blocks.push(block);
            }
            let mut resume_adapters = HashMap::new();
            for source in &function.blocks {
                if matches!(source.terminator, l::Terminator::Suspend { .. }) {
                    resume_adapters.insert(source.id, builder.create_block());
                }
            }
            let root_storage = root_storage::plan(function, &ml.layouts)?;
            let mut body = Body {
                ml,
                builder,
                function,
                ctx,
                sret: None,
                frame: Some(frame),
                out: Some(out),
                coroutine: coroutine_kind(function),
                values: vec![None; function.values.len()],
                locals: Vec::with_capacity(function.locals.len()),
                frame_local_slots: plan.local_slots.clone(),
                blocks,
                unwind: None,
                shadow: None,
                value_roots: HashMap::new(),
                root_storage,
                resume_adapters,
                suspend_plans: plan.suspends.clone(),
                stable_addresses: plan.stable_addresses.clone(),
                closure_environments: plan.closure_environments.clone(),
                closure_environment_layout,
                consumed_traps: Vec::new(),
            };
            initialize_storage(&mut body)?;
            for (parameter, slot) in function.parameters.iter().zip(&plan.parameter_slots) {
                let value = body.load_value_type(&slot.ty, frame, slot.offset as i32)?;
                body.set_value(parameter.value, value)?;
                if let Some(storage) = parameter.storage {
                    let address = body.locals[storage.0 as usize].address;
                    body.store_value_type(&slot.ty, address, 0, value)?;
                }
            }
            let state = body.builder.ins().load(types::I32, flags(), frame, 0);
            let start = body.blocks[function.entry.0 as usize];
            let fresh = body.builder.ins().icmp_imm(IntCC::Equal, state, 0);
            let mut next = body.builder.create_block();
            body.builder.ins().brif(fresh, start, &[], next, &[]);
            for source in &function.blocks {
                let Some(suspend) = plan.suspends.get(&source.id) else {
                    continue;
                };
                body.builder.switch_to_block(next);
                let matches = body
                    .builder
                    .ins()
                    .icmp_imm(IntCC::Equal, state, suspend.state);
                let following = body.builder.create_block();
                let adapter = body.resume_adapters[&source.id];
                body.builder
                    .ins()
                    .brif(matches, adapter, &[], following, &[]);
                next = following;
            }
            body.builder.switch_to_block(next);
            let one = body.builder.ins().iconst(types::I8, 1);
            body.builder.ins().return_(&[one]);
            body.emit_resume_adapters(&plan)?;
            body.emit_graph()?;
            verify_trap_consumption(function, &runtime_traps(function), &body.consumed_traps)?;
            body.builder.seal_all_blocks();
            body.builder.finalize();
        }
        define_context(
            ml,
            resume_id,
            &mut context,
            &format!("LIR coroutine resume {}", function.id.0),
        )?;
    }
    Ok(())
}

/// Defines the zero-argument host wrapper for an exported async root.
pub(crate) fn define_async_export<M: Module>(
    ml: &mut ModLower<'_, M>,
    function: &l::Function,
) -> Result<(), String> {
    if explicit_parameters(function).next().is_some() || function.return_type != Type::Void {
        return Err(internal(format!(
            "exported async function {} is not zero-argument Promise<void>",
            function.id.0
        )));
    }
    let id = ml.func_id(&FnKey::LirAsyncExport(function.id))?;
    let creator = ml.func_id(&FnKey::LirFunction(function.id))?;
    let resume = ml.func_id(&FnKey::LirResume(function.id))?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let ctx = builder.block_params(block)[0];
        let creator = ml.module.declare_func_in_func(creator, builder.func);
        let call = builder.ins().call(creator, &[ctx]);
        let frame = builder.inst_results(call)[0];
        let resume = ml.module.declare_func_in_func(resume, builder.func);
        let resume = builder.ins().func_addr(types::I64, resume);
        let kick = ml
            .module
            .declare_func_in_func(ml.rt.async_kick, builder.func);
        builder.ins().call(kick, &[ctx, frame, resume]);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(
        ml,
        id,
        &mut context,
        &format!("LIR async export {}", function.id.0),
    )
}

/// Defines the LIR module initializer (or an empty initializer).
pub(crate) fn define_init<M: Module>(ml: &mut ModLower<'_, M>) -> Result<(), String> {
    if let Some(function) = ml
        .lir
        .initializer
        .and_then(|id| ml.lir.functions.get(id.0 as usize))
        .cloned()
    {
        return define_function(ml, &function);
    }
    let id = ml.func_id(&FnKey::Init)?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let ctx = builder.block_params(entry)[0];
        initialize_module_globals(ml, &mut builder, ctx)?;
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(ml, id, &mut context, "empty LIR initializer")
}

fn initialize_module_globals<M: Module>(
    ml: &mut ModLower<'_, M>,
    builder: &mut FunctionBuilder<'_>,
    ctx: Value,
) -> Result<(), String> {
    if ml.context_globals && !ml.opts.reload {
        let size = builder.ins().iconst(types::I64, i64::from(ml.globals_size));
        let align = builder
            .ins()
            .iconst(types::I64, i64::from(ml.globals_align));
        let initialize = ml
            .module
            .declare_func_in_func(ml.rt.globals_init, builder.func);
        builder.ins().call(initialize, &[ctx, size, align]);
    }

    let roots = ml
        .lir
        .globals
        .iter()
        .map(|global| {
            managed_words(&ml.layouts, &global.ty).map(|words| (global.source_name.clone(), words))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (source_name, words) in roots {
        if words == 0 {
            continue;
        }
        let (slot, _) = ml
            .globals
            .get(&source_name)
            .cloned()
            .ok_or_else(|| internal(format!("global {source_name} has no target slot")))?;
        let address = match slot {
            GlobalSlot::Data(data) => {
                let global = ml.module.declare_data_in_func(data, builder.func);
                builder.ins().symbol_value(types::I64, global)
            }
            GlobalSlot::Offset(offset) => {
                let base_offset = ctx_off(rtc::Context::globals_offset())?;
                let base = builder.ins().load(types::I64, flags(), ctx, base_offset);
                if offset == 0 {
                    base
                } else {
                    builder.ins().iadd_imm(base, i64::from(offset))
                }
            }
        };
        let words = builder.ins().iconst(types::I64, i64::from(words));
        let root_add = ml.module.declare_func_in_func(ml.rt.root_add, builder.func);
        builder.ins().call(root_add, &[ctx, address, words]);
    }
    Ok(())
}

/// Defines the fresh-worker Context initializer adapter.
pub(crate) fn define_worker_init<M: Module>(ml: &mut ModLower<'_, M>) -> Result<(), String> {
    let id = ml.func_id(&FnKey::WorkerInit)?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let ctx = builder.block_params(block)[0];
        if ml.opts.reload {
            let size = builder.ins().iconst(types::I64, i64::from(ml.globals_size));
            let align = builder
                .ins()
                .iconst(types::I64, i64::from(ml.globals_align));
            let initialize = ml
                .module
                .declare_func_in_func(ml.rt.globals_init, builder.func);
            builder.ins().call(initialize, &[ctx, size, align]);
        }
        let initialize = ml.func_id(&FnKey::Init)?;
        let initialize = ml.module.declare_func_in_func(initialize, builder.func);
        builder.ins().call(initialize, &[ctx]);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(ml, id, &mut context, "worker initializer")
}

/// Defines one worker-entry adapter entirely from LIR ids.
pub(crate) fn define_worker_entry<M: Module>(
    ml: &mut ModLower<'_, M>,
    index: usize,
    entry: &l::WorkerEntry,
) -> Result<(), String> {
    let id = ml.func_id(&FnKey::WorkerEntry(index))?;
    let target = ml.func_id(&FnKey::LirFunction(entry.function))?;
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let arguments = builder.block_params(block).to_vec();
        let target = ml.module.declare_func_in_func(target, builder.func);
        builder.ins().call(target, &arguments);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(ml, id, &mut context, &format!("worker entry {index}"))
}

/// Defines the helper that starts non-entry async roots in LIR order.
pub(crate) fn define_async_runner<M: Module>(ml: &mut ModLower<'_, M>) -> Result<(), String> {
    let id = ml.func_id(&FnKey::AsyncRunner)?;
    let roots = ml.lir.async_roots.clone();
    let mut context = ml.module.make_context();
    context.func.signature = ml.signature_of(id);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        let ctx = builder.block_params(block)[0];
        let done = builder.create_block();
        for root in roots.into_iter().filter(|root| Some(*root) != ml.lir.entry) {
            let wrapper = ml.func_id(&FnKey::LirAsyncExport(root))?;
            let wrapper = ml.module.declare_func_in_func(wrapper, builder.func);
            builder.ins().call(wrapper, &[ctx]);
            let trap = builder.ins().load(types::I32, flags(), ctx, 0);
            let clear = builder.ins().icmp_imm(IntCC::Equal, trap, 0);
            let next = builder.create_block();
            builder.ins().brif(clear, next, &[], done, &[]);
            builder.switch_to_block(next);
        }
        builder.ins().jump(done, &[]);
        builder.switch_to_block(done);
        builder.ins().return_(&[]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    define_context(ml, id, &mut context, "async LIR runner")
}
