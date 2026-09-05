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

/// Whether the leaves form a Homogeneous Floating-point Aggregate (AAPCS
/// 6.4.2 / Win64): 1 to 4 leaves, all of one fundamental float type. Such
/// an aggregate travels in SIMD registers, so the integer-image path must
/// not marshal it.
fn is_pure_hfa_leaves(leaves: &[(u32, types::Type)]) -> bool {
    if !matches!(leaves.len(), 1..=4) {
        return false;
    }
    leaves.iter().all(|(_, ty)| *ty == types::F32) || leaves.iter().all(|(_, ty)| *ty == types::F64)
}

/// Whether a leaf crosses an eightbyte boundary or sits off its natural
/// alignment. Such an aggregate is SysV class MEMORY.
fn sysv_leaf_is_unaligned(offset: u32, ty: types::Type) -> bool {
    let width = ty.bytes();
    let align = width.clamp(1, 8);
    !offset.is_multiple_of(align) || offset % 8 + width > 8
}

/// The CLIF type of one SysV SSE-class image. A lone trailing `f32` uses an
/// `F32` register image; every other all-float eightbyte uses `F64`.
fn sysv_image_type(offset: u32, leaves: &[(u32, types::Type)], total: u32) -> types::Type {
    if leaves == [(offset, types::F32)] && total <= offset + 4 {
        types::F32
    } else {
        types::F64
    }
}

/// The register-image plan for one by-value aggregate of `total` bytes.
/// It is separate from SSA materialization, so a unit test pins the ABI
/// class — the packing, the HFA rule, and the indirect threshold — and not
/// only the corpus shapes that cross a foreign boundary today.
fn plan_aggregate_arg(
    abi: AggregateAbi,
    leaves: &[(u32, types::Type)],
    total: u32,
) -> Result<AggregateArgPlan, String> {
    let integer_images = |total: u32| {
        (0..total.div_ceil(8))
            .map(|index| EightbyteImage {
                offset: index * 8,
                class: RegisterClass::Integer,
                ty: types::I64,
            })
            .collect::<Vec<_>>()
    };
    Ok(match abi {
        AggregateAbi::Aapcs64 => {
            if is_pure_hfa_leaves(leaves) {
                AggregateArgPlan::Hfa(leaves.to_vec())
            } else if total <= 16 {
                AggregateArgPlan::Images(integer_images(total))
            } else {
                AggregateArgPlan::Indirect
            }
        }
        AggregateAbi::Win64 => {
            let packed = |ty: types::Type| {
                AggregateArgPlan::Images(vec![EightbyteImage {
                    offset: 0,
                    class: RegisterClass::Integer,
                    ty,
                }])
            };
            match total {
                1 => packed(types::I8),
                2 => packed(types::I16),
                4 => packed(types::I32),
                8 => packed(types::I64),
                _ => AggregateArgPlan::Indirect,
            }
        }
        AggregateAbi::SysV => {
            if total > 16
                || leaves
                    .iter()
                    .any(|(offset, ty)| sysv_leaf_is_unaligned(*offset, *ty))
            {
                return Ok(AggregateArgPlan::Memory {
                    stack_size: round_up_layout(total.max(1), 8, "boundary aggregate stack copy")?,
                });
            }
            let images = (0..total.div_ceil(8))
                .map(|index| {
                    let offset = index * 8;
                    let inside = leaves
                        .iter()
                        .copied()
                        .filter(|(leaf, _)| *leaf >= offset && *leaf < offset + 8)
                        .collect::<Vec<_>>();
                    let all_float = !inside.is_empty()
                        && inside
                            .iter()
                            .all(|(_, ty)| matches!(*ty, types::F32 | types::F64));
                    if all_float {
                        EightbyteImage {
                            offset,
                            class: RegisterClass::Sse,
                            ty: sysv_image_type(offset, &inside, total),
                        }
                    } else {
                        EightbyteImage {
                            offset,
                            class: RegisterClass::Integer,
                            ty: types::I64,
                        }
                    }
                })
                .collect();
            AggregateArgPlan::Images(images)
        }
    })
}

/// Whether any `f16` leaf falls inside a register-class image. `f16` is
/// storage-only here (`specs/blocks/compiler.md` §16.2), so its register
/// image has no verified rule.
fn sysv_images_contain_f16(images: &[EightbyteImage], f16_offsets: &[u32]) -> bool {
    f16_offsets.iter().any(|offset| {
        images
            .iter()
            .any(|image| *offset >= image.offset && *offset < image.offset + 8)
    })
}

/// Confirms the SysV argument registers this aggregate needs are free. If
/// they are not, the C ABI reverts the aggregate to MEMORY, which this
/// marshaler does not build; the call fails loud instead.
fn ensure_sysv_argument_register_capacity(
    signature: &Signature,
    images: &[EightbyteImage],
    f16_offsets: &[u32],
) -> Result<(), String> {
    if sysv_images_contain_f16(images, f16_offsets) {
        return Err(internal(
            "SysV by-value struct with an f16 field in a register-class eightbyte is not \
             supported; f16 is storage-only (compiler.md §16.2)",
        ));
    }
    let mut used_integer = 0usize;
    let mut used_sse = 0usize;
    for parameter in &signature.params {
        if matches!(parameter.purpose, ArgumentPurpose::StructArgument(_)) {
            continue;
        }
        if parameter.value_type.is_float() {
            used_sse += 1;
        } else {
            used_integer += 1;
        }
    }
    let required_integer = images
        .iter()
        .filter(|image| image.class == RegisterClass::Integer)
        .count();
    let required_sse = images
        .iter()
        .filter(|image| image.class == RegisterClass::Sse)
        .count();
    if required_integer > 6usize.saturating_sub(used_integer)
        || required_sse > 8usize.saturating_sub(used_sse)
    {
        return Err(internal(
            "foreign call passing a SysV boundary struct by value under argument register \
             pressure requires the SysV MEMORY-on-stack revert path, not yet implemented \
             (compiler.md §12.3a — fail loud, never a silent mis-marshal)",
        ));
    }
    Ok(())
}

/// The SysV register images for one by-value struct return, or `None` for
/// the MEMORY class, which returns through a hidden pointer.
fn plan_sysv_struct_return(
    leaves: &[(u32, types::Type)],
    size: u32,
    f16_offsets: &[u32],
) -> Result<Option<Vec<EightbyteImage>>, String> {
    match plan_aggregate_arg(AggregateAbi::SysV, leaves, size)? {
        AggregateArgPlan::Images(images) => {
            if sysv_images_contain_f16(&images, f16_offsets) {
                return Err(internal(
                    "foreign call returning a SysV by-value struct with an f16 field in a \
                     register-class eightbyte is not supported; f16 is storage-only \
                     (compiler.md §16.2)",
                ));
            }
            if images.iter().any(|image| image.class == RegisterClass::Sse) {
                return Err(internal(
                    "foreign call returning a SysV SSE-class boundary struct by value is not \
                     supported in the dev JIT: the float return register path is not modeled \
                     (compiler.md §12.3a — fail loud, never a silent mis-marshal)",
                ));
            }
            Ok(Some(images))
        }
        AggregateArgPlan::Memory { .. } => Ok(None),
        other => Err(internal(format!(
            "SysV struct-return planner produced {other:?}"
        ))),
    }
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

#[cfg(test)]
mod aggregate_abi_tests {
    use super::*;
    use std::str::FromStr;
    use target_lexicon::Triple;

    fn abi(triple: &str) -> Option<AggregateAbi> {
        AggregateAbi::of(&Triple::from_str(triple).expect("triple"))
    }

    fn image(offset: u32, class: RegisterClass, ty: types::Type) -> EightbyteImage {
        EightbyteImage { offset, class, ty }
    }

    /// The dev-JIT by-value aggregate marshaler implements AAPCS64, Win64,
    /// and x86-64 SysV (compiler.md §12.3a). Lowering reads this same
    /// function, so a host named here cannot fail in the marshaler for want
    /// of an ABI rule. An unnamed host fails loud.
    #[test]
    fn every_supported_dev_host_names_its_aggregate_abi() {
        assert_eq!(abi("aarch64-apple-darwin"), Some(AggregateAbi::Aapcs64));
        assert_eq!(abi("aarch64-linux-android"), Some(AggregateAbi::Aapcs64));
        assert_eq!(abi("x86_64-pc-windows-msvc"), Some(AggregateAbi::Win64));
        assert_eq!(abi("x86_64-unknown-linux-gnu"), Some(AggregateAbi::SysV));
        assert_eq!(abi("x86_64-apple-darwin"), Some(AggregateAbi::SysV));
        assert_eq!(abi("i686-unknown-linux-gnu"), None);
    }

    #[test]
    fn aapcs64_passes_a_small_composite_as_eightbyte_images() {
        let leaves = [(0, types::I32), (4, types::I32), (8, types::I64)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::Aapcs64, &leaves, 16)
                .expect("aggregate argument plan"),
            AggregateArgPlan::Images(vec![
                image(0, RegisterClass::Integer, types::I64),
                image(8, RegisterClass::Integer, types::I64),
            ])
        );
    }

    #[test]
    fn aapcs64_passes_an_hfa_component_wise_and_a_large_struct_by_reference() {
        let hfa = [(0, types::F32), (4, types::F32)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::Aapcs64, &hfa, 8).expect("aggregate argument plan"),
            AggregateArgPlan::Hfa(hfa.to_vec())
        );
        let wide = [(0, types::I64), (8, types::I64), (16, types::I64)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::Aapcs64, &wide, 24).expect("aggregate argument plan"),
            AggregateArgPlan::Indirect
        );
    }

    /// Win64 passes a 1/2/4/8-byte aggregate as one packed integer register
    /// and every other size by reference. It has no HFA case, so a pair of
    /// floats that AAPCS64 splits into two SIMD registers is one packed
    /// eightbyte here.
    #[test]
    fn win64_packs_one_two_four_and_eight_byte_aggregates_only() {
        let byte = [(0, types::I8)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::Win64, &byte, 1).expect("aggregate argument plan"),
            AggregateArgPlan::Images(vec![image(0, RegisterClass::Integer, types::I8)])
        );
        let pair = [(0, types::F32), (4, types::F32)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::Win64, &pair, 8).expect("aggregate argument plan"),
            AggregateArgPlan::Images(vec![image(0, RegisterClass::Integer, types::I64)])
        );
        for size in [3u32, 5, 6, 7, 12, 16, 24] {
            assert_eq!(
                plan_aggregate_arg(AggregateAbi::Win64, &pair, size)
                    .expect("aggregate argument plan"),
                AggregateArgPlan::Indirect,
                "size {size} must pass by reference on Win64"
            );
        }
    }

    #[test]
    fn sysv_classifies_each_eightbyte_and_reverts_a_wide_one_to_memory() {
        let mixed = [(0, types::I32), (4, types::I32), (8, types::F64)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::SysV, &mixed, 16).expect("aggregate argument plan"),
            AggregateArgPlan::Images(vec![
                image(0, RegisterClass::Integer, types::I64),
                image(8, RegisterClass::Sse, types::F64),
            ])
        );
        let wide = [(0, types::I64), (8, types::I64), (16, types::I64)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::SysV, &wide, 24).expect("aggregate argument plan"),
            AggregateArgPlan::Memory { stack_size: 24 }
        );
    }

    #[test]
    fn sysv_memory_arguments_occupy_whole_eightbytes() {
        let twenty = [
            (0, types::I8),
            (4, types::F32),
            (8, types::F32),
            (12, types::F32),
            (16, types::I16),
        ];
        let twenty_four = [(0, types::I64), (8, types::I64), (16, types::I64)];
        // The psABI assigns three whole eightbytes to each caller copy.
        for (leaves, size) in [(&twenty[..], 20), (&twenty_four[..], 24)] {
            assert_eq!(
                plan_aggregate_arg(AggregateAbi::SysV, leaves, size).expect("MEMORY argument plan"),
                AggregateArgPlan::Memory { stack_size: 24 },
                "aggregate size {size}"
            );
        }
    }

    #[test]
    fn sysv_gives_a_lone_trailing_f32_an_f32_image() {
        let leaves = [(0, types::I64), (8, types::F32)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::SysV, &leaves, 12).expect("aggregate argument plan"),
            AggregateArgPlan::Images(vec![
                image(0, RegisterClass::Integer, types::I64),
                image(8, RegisterClass::Sse, types::F32),
            ])
        );
    }

    #[test]
    fn sysv_reverts_an_unaligned_leaf_to_memory() {
        let straddling = [(0, types::I32), (5, types::I64)];
        assert_eq!(
            plan_aggregate_arg(AggregateAbi::SysV, &straddling, 16)
                .expect("aggregate argument plan"),
            AggregateArgPlan::Memory { stack_size: 16 }
        );
    }

    #[test]
    fn a_sysv_sse_class_return_and_an_f16_image_both_fail_loud() {
        let sse = [(0, types::F64), (8, types::F64)];
        let error = plan_sysv_struct_return(&sse, 16, &[])
            .expect_err("an SSE-class return has no modeled float return register");
        assert!(error.contains("SSE-class"), "{error}");

        let f16 = [(0, types::I16), (8, types::I64)];
        let error = plan_sysv_struct_return(&f16, 16, &[0])
            .expect_err("f16 is storage-only, so it has no register image");
        assert!(error.contains("f16"), "{error}");

        assert_eq!(
            plan_sysv_struct_return(
                &[(0, types::I64), (8, types::I64), (16, types::I64)],
                24,
                &[]
            )
            .expect("a wide return is MEMORY class"),
            None
        );
    }

    #[test]
    fn sysv_argument_register_pressure_fails_loud() {
        let mut signature = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        for _ in 0..6 {
            signature.params.push(AbiParam::new(types::I64));
        }
        let images = [image(0, RegisterClass::Integer, types::I64)];
        let error = ensure_sysv_argument_register_capacity(&signature, &images, &[])
            .expect_err("no integer argument register is free");
        assert!(error.contains("register pressure"), "{error}");
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

fn ensure_explicit_frame_supported(
    function: &cranelift_codegen::ir::Function,
    label: &str,
) -> Result<(), String> {
    let mut bytes = 0u32;
    for slot in function.sized_stack_slots.values() {
        bytes = checked_layout_add(bytes, slot.size, "Cranelift explicit stack frame")?;
        bytes = round_up_layout(
            bytes,
            1u32 << slot.align_shift,
            "Cranelift explicit stack frame",
        )?;
    }
    if bytes > MAX_FRAME_BYTES {
        return Err(internal(format!(
            "{label} needs {bytes} bytes of explicit stack storage; maximum is {MAX_FRAME_BYTES}"
        )));
    }
    let _ = CRANELIFT_FRAME_ALIGNMENT;
    Ok(())
}

fn plan_coroutine(
    layouts: &Layouts,
    module: &l::Module,
    function: &l::Function,
) -> Result<CoroutinePlan, String> {
    let mut offset = COROUTINE_PAYLOAD_OFFSET;
    let mut parameter_slots = Vec::with_capacity(function.parameters.len());
    for parameter in &function.parameters {
        let ty = function
            .values
            .get(parameter.value.0 as usize)
            .ok_or_else(|| internal(format!("parameter value {} is missing", parameter.value.0)))?
            .ty
            .clone();
        let (size, align) = value_size_align(layouts, &ty)?;
        offset = round_up_layout(offset, align.max(1), "coroutine parameter layout")?;
        parameter_slots.push(FrameSlot { offset, ty });
        offset = checked_layout_add(offset, size.max(1), "coroutine parameter layout")?;
    }
    let mut local_slots = Vec::with_capacity(function.locals.len());
    for local in &function.locals {
        if local.storage == l::LocalStorageClass::Frame {
            let (size, align) = value_size_align(layouts, &local.ty)?;
            offset = round_up_layout(offset, align.max(1), "coroutine local layout")?;
            local_slots.push(Some(FrameSlot {
                offset,
                ty: local.ty.clone(),
            }));
            offset = checked_layout_add(offset, size.max(1), "coroutine local layout")?;
        } else {
            local_slots.push(None);
        }
    }
    let mut suspends = HashMap::new();
    let mut state = 1i64;
    for block in &function.blocks {
        let l::Terminator::Suspend {
            kind,
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            continue;
        };
        let destination = function
            .blocks
            .get(successor.0 as usize)
            .ok_or_else(|| internal(format!("suspend successor {} is missing", successor.0)))?;
        let start = usize::from(resume_value.is_some());
        let mut arguments = Vec::new();
        for value in destination.parameters.iter().skip(start) {
            let ty = function
                .values
                .get(value.0 as usize)
                .ok_or_else(|| internal(format!("resume value {} is missing", value.0)))?
                .ty
                .clone();
            let (size, align) = value_size_align(layouts, &ty)?;
            offset = round_up_layout(offset, align.max(1), "suspend live-in layout")?;
            arguments.push(FrameSlot { offset, ty });
            offset = checked_layout_add(offset, size.max(1), "suspend live-in layout")?;
        }
        let child = if matches!(
            kind,
            l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. }
        ) {
            offset = round_up_layout(offset, 8, "async child layout")?;
            let child = offset;
            offset = checked_layout_add(offset, 8, "async child layout")?;
            Some(child)
        } else {
            None
        };
        suspends.insert(
            block.id,
            SuspendPlan {
                state,
                arguments,
                child,
            },
        );
        state += 1;
    }
    let live_across_suspend = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            l::Terminator::Suspend { arguments, .. } => Some(arguments),
            _ => None,
        })
        .flatten()
        .filter_map(|operand| match operand {
            l::Operand::Value(value) => Some(*value),
            l::Operand::Constant(_) => None,
        })
        .collect::<HashSet<_>>();
    let mut stable_addresses = HashMap::new();
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        if !matches!(
            instruction.kind,
            l::InstructionKind::AllocateClass(_) | l::InstructionKind::AddressOfValue
        ) {
            continue;
        }
        let Some(result) = instruction
            .result
            .filter(|result| live_across_suspend.contains(result))
        else {
            continue;
        };
        let Some(l::ValueType::Address(address)) = function
            .values
            .get(result.0 as usize)
            .map(|value| &value.ty)
        else {
            // Reference-class allocations are handles and remain valid across
            // suspension without pinning target storage in the frame.
            continue;
        };
        let (size, align) = layouts.size_align(&address.pointee)?;
        offset = round_up_layout(offset, align.max(1), "stable coroutine address layout")?;
        stable_addresses.insert(result, offset);
        offset = checked_layout_add(offset, size.max(1), "stable coroutine address layout")?;
    }
    let mut closure_environments = HashMap::new();
    if let Some((size, align)) = closure_environment_layout(module, layouts)? {
        for value in &function.values {
            if !matches!(value.ty, l::ValueType::Data(Type::Func(_))) {
                continue;
            }
            offset = round_up_layout(offset, align, "coroutine closure environment layout")?;
            closure_environments.insert(value.id, offset);
            offset = checked_layout_add(offset, size, "coroutine closure environment layout")?;
        }
    }
    let size = round_up_layout(offset, 8, "final coroutine layout")?;
    Ok(CoroutinePlan {
        parameter_slots,
        local_slots,
        suspends,
        stable_addresses,
        closure_environments,
        size,
    })
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
    fn iconst(&mut self, ty: types::Type, value: i64) -> Value {
        let value = if ty == types::I8 {
            i64::from(value as i8)
        } else if ty == types::I16 {
            i64::from(value as i16)
        } else if ty == types::I32 {
            i64::from(value as i32)
        } else {
            value
        };
        self.builder.ins().iconst(ty, value)
    }

    fn zero_scalar(&mut self, ty: types::Type) -> Value {
        if ty == types::F32 {
            self.builder.ins().f32const(0.0)
        } else if ty == types::F64 {
            self.builder.ins().f64const(0.0)
        } else {
            self.iconst(ty, 0)
        }
    }

    fn address_offset(&mut self, address: Value, offset: i64) -> Value {
        if offset == 0 {
            address
        } else {
            self.builder.ins().iadd_imm(address, offset)
        }
    }

    fn stack_slot(&mut self, size: u32, align: u32) -> Value {
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size.max(1),
            align_shift(align.max(1)),
        ));
        self.builder.ins().stack_addr(types::I64, slot, 0)
    }

    fn copy_bytes(&mut self, destination: Value, source: Value, size: u32, align: u32) {
        let config = self.ml.module.isa().frontend_config();
        self.builder.emit_small_memory_copy(
            config,
            destination,
            source,
            u64::from(size),
            align.max(1) as u8,
            align.max(1) as u8,
            true,
            MemFlags::new(),
        );
    }

    fn closure_environment_address(&mut self, id: l::ValueId) -> Result<Value, String> {
        let offset = self.closure_environments.get(&id).copied().ok_or_else(|| {
            internal(format!(
                "function value {} has no environment storage",
                id.0
            ))
        })?;
        let base = if self.coroutine.is_some() {
            self.frame
                .ok_or_else(|| internal("coroutine environment storage has no frame"))?
        } else {
            self.shadow
                .ok_or_else(|| internal("closure environment storage has no shadow frame"))?
        };
        Ok(self.address_offset(base, i64::from(offset)))
    }

    fn relocate_closure_environment(
        &mut self,
        code: Value,
        environment: Value,
        destination: Value,
    ) -> Result<RV, String> {
        let Some((size, align)) = self.closure_environment_layout else {
            return Ok(RV::Pair(code, environment));
        };
        let copy = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let present = self.builder.ins().icmp_imm(IntCC::NotEqual, environment, 0);
        self.builder
            .ins()
            .brif(present, copy, &[], done, &[BlockArg::Value(environment)]);
        self.builder.switch_to_block(copy);
        self.copy_bytes(destination, environment, size, align);
        self.builder
            .ins()
            .jump(done, &[BlockArg::Value(destination)]);
        self.builder.switch_to_block(done);
        Ok(RV::Pair(code, self.builder.block_params(done)[0]))
    }

    fn own_closure_environment(&mut self, id: l::ValueId, value: RV) -> Result<RV, String> {
        let (code, environment) = self.expect_pair(value)?;
        let destination = self.closure_environment_address(id)?;
        self.relocate_closure_environment(code, environment, destination)
    }

    fn snapshot_closure_environment(&mut self, value: RV) -> Result<RV, String> {
        let Some((size, align)) = self.closure_environment_layout else {
            return Ok(value);
        };
        let (code, environment) = self.expect_pair(value)?;
        let destination = self.stack_slot(size, align);
        self.zero_bytes(destination, size, align);
        self.relocate_closure_environment(code, environment, destination)
    }

    fn zero_bytes(&mut self, destination: Value, size: u32, align: u32) {
        let config = self.ml.module.isa().frontend_config();
        self.builder.emit_small_memset(
            config,
            destination,
            0,
            u64::from(size),
            align.max(1) as u8,
            MemFlags::new(),
        );
    }

    fn load_data(&mut self, ty: &Type, address: Value, offset: i32) -> Result<RV, String> {
        Ok(match self.ml.layouts.repr(ty)? {
            Repr::None => RV::None,
            Repr::Scalar(value) => {
                RV::Scalar(self.builder.ins().load(value, flags(), address, offset))
            }
            Repr::Pair => {
                let code = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), address, offset);
                let env = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), address, offset + 8);
                RV::Pair(code, env)
            }
            Repr::Agg { .. } => RV::Aggregate(self.address_offset(address, i64::from(offset))),
        })
    }

    fn store_data(
        &mut self,
        ty: &Type,
        address: Value,
        offset: i32,
        value: RV,
    ) -> Result<(), String> {
        match (self.ml.layouts.repr(ty)?, value) {
            (Repr::None, _) => Ok(()),
            (Repr::Scalar(_), RV::Scalar(value)) => {
                self.builder.ins().store(flags(), value, address, offset);
                Ok(())
            }
            (Repr::Pair, RV::Pair(code, env)) => {
                self.builder.ins().store(flags(), code, address, offset);
                self.builder.ins().store(flags(), env, address, offset + 8);
                Ok(())
            }
            (Repr::Agg { size, align }, RV::Aggregate(source)) => {
                let destination = self.address_offset(address, i64::from(offset));
                self.copy_bytes(destination, source, size, align);
                Ok(())
            }
            (Repr::Scalar(_), RV::Aggregate(source)) if self.is_boundary_struct_pointer(ty) => {
                self.builder.ins().store(flags(), source, address, offset);
                Ok(())
            }
            (repr, value) => Err(internal(format!("store mismatch {repr:?} and {value:?}"))),
        }
    }

    fn load_value_type(
        &mut self,
        ty: &l::ValueType,
        address: Value,
        offset: i32,
    ) -> Result<RV, String> {
        match ty {
            l::ValueType::Data(ty) => self.load_data(ty, address, offset),
            l::ValueType::Address(_) => Ok(RV::Scalar(self.builder.ins().load(
                types::I64,
                flags(),
                address,
                offset,
            ))),
            l::ValueType::Iterator(_) => Ok(RV::Aggregate(
                self.address_offset(address, i64::from(offset)),
            )),
        }
    }

    fn store_value_type(
        &mut self,
        ty: &l::ValueType,
        address: Value,
        offset: i32,
        value: RV,
    ) -> Result<(), String> {
        match ty {
            l::ValueType::Data(ty) => self.store_data(ty, address, offset, value),
            l::ValueType::Address(_) => {
                let value = self.expect_scalar(value)?;
                self.builder.ins().store(flags(), value, address, offset);
                Ok(())
            }
            l::ValueType::Iterator(_) => {
                let source = self.expect_aggregate(value)?;
                let destination = self.address_offset(address, i64::from(offset));
                self.copy_bytes(destination, source, 32, 8);
                Ok(())
            }
        }
    }

    fn expect_scalar(&self, value: RV) -> Result<Value, String> {
        match value {
            RV::Scalar(value) => Ok(value),
            other => Err(internal(format!("expected scalar, got {other:?}"))),
        }
    }

    fn expect_pair(&self, value: RV) -> Result<(Value, Value), String> {
        match value {
            RV::Pair(code, env) => Ok((code, env)),
            other => Err(internal(format!("expected pair, got {other:?}"))),
        }
    }

    fn expect_aggregate(&self, value: RV) -> Result<Value, String> {
        match value {
            RV::Aggregate(value) => Ok(value),
            other => Err(internal(format!("expected aggregate, got {other:?}"))),
        }
    }

    fn value_type(&self, id: l::ValueId) -> Result<&l::ValueType, String> {
        value_type(self.function, id)
    }

    fn value(&mut self, id: l::ValueId) -> Result<RV, String> {
        if let Some(root) = self.value_roots.get(&id).copied() {
            let base = self
                .shadow
                .ok_or_else(|| internal("managed value has no shadow frame"))?;
            let address = self.address_offset(base, i64::from(root) * 8);
            let ty = self.value_type(id)?.clone();
            return self.load_value_type(&ty, address, 0);
        }
        self.values
            .get(id.0 as usize)
            .and_then(|value| *value)
            .ok_or_else(|| internal(format!("value {} is not available", id.0)))
    }

    fn set_value(&mut self, id: l::ValueId, mut value: RV) -> Result<(), String> {
        let ty = self.value_type(id)?.clone();
        if self.closure_environment_layout.is_some()
            && matches!(ty, l::ValueType::Data(Type::Func(_)))
        {
            value = self.own_closure_environment(id, value)?;
        }
        if let Some(root) = self.value_roots.get(&id).copied() {
            let base = self
                .shadow
                .ok_or_else(|| internal("managed value has no shadow frame"))?;
            let address = self.address_offset(base, i64::from(root) * 8);
            self.store_value_type(&ty, address, 0, value)?;
            if matches!(value_repr(&self.ml.layouts, &ty)?, Repr::Agg { .. }) {
                value = RV::Aggregate(address);
            }
        }
        let slot = self
            .values
            .get_mut(id.0 as usize)
            .ok_or_else(|| internal(format!("value {} slot is missing", id.0)))?;
        *slot = Some(value);
        Ok(())
    }

    fn clear_root_slots(&mut self, slots: &[usize]) -> Result<(), String> {
        if slots.is_empty() {
            return Ok(());
        }
        let shadow = self
            .shadow
            .ok_or_else(|| internal("root clear has no shadow frame"))?;
        let clears = slots
            .iter()
            .map(|slot| {
                self.root_storage
                    .slots
                    .get(*slot)
                    .map(|slot| (slot.offset, slot.words))
                    .ok_or_else(|| internal(format!("root slot {slot} is missing")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (offset, words) in clears {
            let address = self.address_offset(shadow, i64::from(offset) * 8);
            self.zero_bytes(address, words * 8, 8);
        }
        Ok(())
    }

    fn constant(&mut self, constant: &l::Constant) -> Result<RV, String> {
        Ok(match (&constant.ty, &constant.kind) {
            (Type::F32, l::ConstantKind::FloatBits(bits)) => {
                RV::Scalar(self.builder.ins().f32const(f32::from_bits(*bits as u32)))
            }
            (Type::F64, l::ConstantKind::FloatBits(bits)) => {
                RV::Scalar(self.builder.ins().f64const(f64::from_bits(*bits)))
            }
            (Type::F16, l::ConstantKind::FloatBits(bits)) => {
                let wide = self.builder.ins().f64const(f64::from_bits(*bits));
                let result = self
                    .call_runtime(self.ml.rt.f16_from_f64, &[wide], false)?
                    .ok_or_else(|| internal("f16 constant conversion has no result"))?;
                RV::Scalar(result)
            }
            (Type::Bool, l::ConstantKind::Boolean(value)) => {
                RV::Scalar(self.iconst(types::I8, i64::from(*value)))
            }
            (_, l::ConstantKind::Null) => RV::Scalar(self.iconst(types::I64, 0)),
            (ty, l::ConstantKind::Integer(value)) => {
                let Repr::Scalar(repr) = self.ml.layouts.repr(ty)? else {
                    return Err(internal(format!(
                        "integer constant has aggregate type {ty:?}"
                    )));
                };
                RV::Scalar(self.iconst(repr, *value))
            }
            (ty, kind) => {
                return Err(internal(format!(
                    "constant payload {kind:?} disagrees with {ty:?}"
                )))
            }
        })
    }

    fn operand(&mut self, operand: &l::Operand) -> Result<RV, String> {
        match operand {
            l::Operand::Value(value) => self.value(*value),
            l::Operand::Constant(constant) => self.constant(constant),
        }
    }

    fn operand_type(&self, operand: &l::Operand) -> Result<l::ValueType, String> {
        operand_type(self.function, operand)
    }

    fn position_id(&mut self, position: &Pos) -> i64 {
        i64::from(self.ml.pos_id(position))
    }

    fn call_runtime(
        &mut self,
        function: cranelift_module::FuncId,
        arguments: &[Value],
        checked: bool,
    ) -> Result<Option<Value>, String> {
        let reference = self
            .ml
            .module
            .declare_func_in_func(function, self.builder.func);
        let call = self.builder.ins().call(reference, arguments);
        if checked {
            self.trap_check();
        }
        Ok(self.builder.inst_results(call).first().copied())
    }

    fn call_script(
        &mut self,
        key: &FnKey,
        arguments: &[Value],
        checked: bool,
    ) -> Result<Vec<Value>, String> {
        if !self.ml.opts.reload {
            return self.call_script_direct(key, arguments, checked);
        }
        let id = self.ml.func_id(key)?;
        let slot = self.ml.slot_of(key)?;
        let displacement = i32::try_from(u64::from(slot) * 8)
            .map_err(|_| internal("function slot offset does not fit in i32"))?;
        let signature = self.builder.import_signature(self.ml.signature_of(id));
        let table_offset = ctx_off(rtc::Context::fn_table_offset())?;
        let table = self
            .builder
            .ins()
            .load(types::I64, flags(), self.ctx, table_offset);
        let code = self
            .builder
            .ins()
            .load(types::I64, flags(), table, displacement);
        let call = self.builder.ins().call_indirect(signature, code, arguments);
        if checked {
            self.trap_check();
        }
        Ok(self.builder.inst_results(call).to_vec())
    }

    fn call_script_direct(
        &mut self,
        key: &FnKey,
        arguments: &[Value],
        checked: bool,
    ) -> Result<Vec<Value>, String> {
        let id = self.ml.func_id(key)?;
        let reference = self.ml.module.declare_func_in_func(id, self.builder.func);
        let call = self.builder.ins().call(reference, arguments);
        if checked {
            self.trap_check();
        }
        Ok(self.builder.inst_results(call).to_vec())
    }

    fn unwind_block(&mut self) -> Block {
        if let Some(block) = self.unwind {
            block
        } else {
            let block = self.builder.create_block();
            self.unwind = Some(block);
            block
        }
    }

    fn trap_check(&mut self) {
        let trap = self.builder.ins().load(types::I32, flags(), self.ctx, 0);
        let clear = self.builder.ins().icmp_imm(IntCC::Equal, trap, 0);
        let next = self.builder.create_block();
        let unwind = self.unwind_block();
        self.builder.ins().brif(clear, next, &[], unwind, &[]);
        self.builder.switch_to_block(next);
    }

    fn guard(&mut self, condition: Value, kind: TrapKind, position: &Pos) -> Result<(), String> {
        let ok = self.builder.create_block();
        let bad = self.builder.create_block();
        self.builder.ins().brif(condition, ok, &[], bad, &[]);
        self.builder.switch_to_block(bad);
        let kind = self.iconst(types::I32, i64::from(kind as u32));
        let position_id = self.position_id(position);
        let position_id = self.iconst(types::I32, position_id);
        self.call_runtime(self.ml.rt.trap, &[self.ctx, kind, position_id], false)?;
        let unwind = self.unwind_block();
        self.builder.ins().jump(unwind, &[]);
        self.builder.switch_to_block(ok);
        Ok(())
    }

    fn index_guard(
        &mut self,
        condition: Value,
        index: Value,
        length: Value,
        position: &Pos,
    ) -> Result<(), String> {
        let ok = self.builder.create_block();
        let bad = self.builder.create_block();
        self.builder.ins().brif(condition, ok, &[], bad, &[]);
        self.builder.switch_to_block(bad);
        let position_id = self.position_id(position);
        let position_id = self.iconst(types::I32, position_id);
        self.call_runtime(
            self.ml.rt.trap_index_out_of_bounds,
            &[self.ctx, index, length, position_id],
            false,
        )?;
        let unwind = self.unwind_block();
        self.builder.ins().jump(unwind, &[]);
        self.builder.switch_to_block(ok);
        Ok(())
    }

    fn live_check(&mut self, pointer: Value, position: &Pos) -> Result<(), String> {
        let state = self
            .builder
            .ins()
            .load(types::I64, flags(), pointer, rtc::STATE_OFFSET);
        let live = self
            .builder
            .ins()
            .icmp_imm(IntCC::Equal, state, rtc::LIVE_STATE as i64);
        let kind = runtime_trap_kind(&l::TrapKind::DevOnlyLifetime)
            .ok_or_else(|| internal("lifetime trap has no direct runtime kind"))?;
        self.guard(live, kind, position)
    }

    fn reload_epoch_check(&mut self, frame: Value, position: &Pos) -> Result<(), String> {
        if !self.ml.opts.reload {
            return Ok(());
        }
        let valid = if self.function.is_async {
            let stale = self
                .call_runtime(self.ml.rt.async_is_stale, &[self.ctx, frame], false)?
                .ok_or_else(|| internal("async stale check has no result"))?;
            self.builder.ins().icmp_imm(IntCC::Equal, stale, 0)
        } else {
            let offset = ctx_off(rtc::Context::reload_epoch_offset())?;
            let current = self
                .builder
                .ins()
                .load(types::I32, flags(), self.ctx, offset);
            let created =
                self.builder
                    .ins()
                    .load(types::I32, flags(), frame, GENERATOR_EPOCH_OFFSET);
            self.builder.ins().icmp(IntCC::Equal, current, created)
        };
        let kind = runtime_trap_kind(&l::TrapKind::DevReloadOnlyStaleCoroutine)
            .ok_or_else(|| internal("stale-coroutine trap has no direct runtime kind"))?;
        self.guard(valid, kind, position)
    }

    fn emit_trap(&mut self, trap: &l::Trap, operand: TrapOperand) -> Result<(), String> {
        let runtime_kind = runtime_trap_kind(&trap.kind);
        let direct_kind = || {
            runtime_kind
                .ok_or_else(|| internal(format!("trap {:?} has no direct runtime kind", trap.kind)))
        };
        let value = match operand {
            TrapOperand::Value(value) | TrapOperand::Condition(value) => Some(value),
            _ => None,
        };
        match &trap.kind {
            l::TrapKind::Allocation | l::TrapKind::Call => {
                if !matches!(operand, TrapOperand::Pending) {
                    return Err(internal("pending trap received an explicit operand"));
                }
                self.trap_check();
            }
            l::TrapKind::Unreachable => {
                let false_value = self.iconst(types::I8, 0);
                self.guard(false_value, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::DivisionByZero => {
                let divisor = value.ok_or_else(|| internal("division trap has no divisor"))?;
                let nonzero = self.builder.ins().icmp_imm(IntCC::NotEqual, divisor, 0);
                self.guard(nonzero, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::IndexRead | l::TrapKind::IndexWrite => match operand {
                TrapOperand::Pending => self.trap_check(),
                TrapOperand::Index {
                    condition,
                    index,
                    length,
                } => {
                    if direct_kind()? != TrapKind::IndexOutOfBounds {
                        return Err(internal("index trap has a non-index runtime kind"));
                    }
                    self.index_guard(condition, index, length, &trap.pos)?;
                }
                TrapOperand::Value(_)
                | TrapOperand::Condition(_)
                | TrapOperand::WireValue { .. } => {
                    return Err(internal("index trap received no index/length payload"))
                }
            },
            l::TrapKind::JsonResultValue(_) => {
                let condition =
                    value.ok_or_else(|| internal("JSON result trap has no condition"))?;
                self.guard(condition, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::NullNarrowing => {
                let pointer = value.ok_or_else(|| internal("null trap has no pointer"))?;
                let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, pointer, 0);
                self.guard(nonnull, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::ClassMismatch(class) => {
                let pointer = value.ok_or_else(|| internal("class trap has no pointer"))?;
                let class_id =
                    self.builder
                        .ins()
                        .load(types::I32, flags(), pointer, rtc::CLASS_ID_OFFSET);
                let matches = self
                    .builder
                    .ins()
                    .icmp_imm(IntCC::Equal, class_id, class.0 as i64);
                self.guard(matches, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::DevOnlyLifetime => match operand {
                TrapOperand::Pending => self.trap_check(),
                TrapOperand::Value(pointer) | TrapOperand::Condition(pointer) => {
                    self.live_check(pointer, &trap.pos)?;
                }
                TrapOperand::Index { .. } | TrapOperand::WireValue { .. } => {
                    return Err(internal("lifetime trap received a wire operand"))
                }
            },
            l::TrapKind::DevReloadOnlyStaleCoroutine => {
                let frame = value.ok_or_else(|| internal("stale trap has no frame"))?;
                self.reload_epoch_check(frame, &trap.pos)?;
            }
            l::TrapKind::WireEnumValue(alias) => {
                if direct_kind()? != TrapKind::WireEnumUnknownValue {
                    return Err(internal("wire-enum trap has a non-wire runtime kind"));
                }
                let TrapOperand::WireValue { wire, valid } = operand else {
                    return Err(internal("wire enum trap has no wire operand"));
                };
                let definition = self
                    .ml
                    .lir
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
                let data = self.ml.literal_data(definition.source_name.as_bytes())?;
                let global = self.ml.module.declare_data_in_func(data, self.builder.func);
                let name = self.builder.ins().symbol_value(types::I64, global);
                let length = self.iconst(types::I64, definition.source_name.len() as i64);
                let position = self.position_id(&trap.pos);
                let position = self.iconst(types::I32, position);
                let ok = self.builder.create_block();
                let bad = self.builder.create_block();
                self.builder.ins().brif(valid, ok, &[], bad, &[]);
                self.builder.switch_to_block(bad);
                self.call_runtime(
                    self.ml.rt.trap_wire_enum,
                    &[self.ctx, name, length, wire, position],
                    false,
                )?;
                let unwind = self.unwind_block();
                self.builder.ins().jump(unwind, &[]);
                self.builder.switch_to_block(ok);
            }
        }
        self.consumed_traps.push(trap.clone());
        Ok(())
    }

    fn is_boundary_struct_pointer(&self, ty: &Type) -> bool {
        boundary_box_class(self.ml.lir, ty).is_some()
    }
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn global_address(&mut self, id: l::GlobalId) -> Result<(Value, Type), String> {
        let definition = self
            .ml
            .lir
            .globals
            .get(id.0 as usize)
            .filter(|global| global.id == id)
            .ok_or_else(|| internal(format!("global {} is missing", id.0)))?;
        let (slot, ty) = self
            .ml
            .globals
            .get(&definition.source_name)
            .cloned()
            .ok_or_else(|| internal(format!("global {} has no target slot", id.0)))?;
        let address = match slot {
            GlobalSlot::Data(data) => {
                let global = self.ml.module.declare_data_in_func(data, self.builder.func);
                self.builder.ins().symbol_value(types::I64, global)
            }
            GlobalSlot::Offset(offset) => {
                let base_offset = ctx_off(rtc::Context::globals_offset())?;
                let base = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), self.ctx, base_offset);
                self.address_offset(base, i64::from(offset))
            }
        };
        Ok((address, ty))
    }

    fn field_definition(&self, id: l::FieldId) -> Result<(ClassId, usize, &l::Field), String> {
        self.ml
            .lir
            .classes
            .iter()
            .find_map(|class| {
                class
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.id == id)
                    .map(|(index, field)| (class.id, index, field))
            })
            .ok_or_else(|| internal(format!("field {} is missing", id.0)))
    }

    fn field_address(
        &mut self,
        field: l::FieldRef,
        base: RV,
        base_type: &l::ValueType,
        traps: &[l::Trap],
    ) -> Result<(Value, Type), String> {
        match field {
            l::FieldRef::Class(field) => {
                let (class, index, definition) = self.field_definition(field)?;
                let ty = definition.ty.clone();
                let offset = *self
                    .ml
                    .layouts
                    .class(class.0)?
                    .field_offsets
                    .get(index)
                    .ok_or_else(|| internal(format!("field {} has no layout offset", field.0)))?;
                let address = match base_type {
                    l::ValueType::Data(Type::Class(class)) => {
                        if self.ml.layouts.class(class.0)?.is_value {
                            let pointer = self.expect_aggregate(base)?;
                            self.address_offset(pointer, i64::from(offset))
                        } else {
                            let pointer = self.expect_scalar(base)?;
                            for trap in traps {
                                if trap.kind == l::TrapKind::DevOnlyLifetime {
                                    self.emit_trap(trap, TrapOperand::Value(pointer))?;
                                }
                            }
                            self.address_offset(pointer, i64::from(offset))
                        }
                    }
                    l::ValueType::Data(Type::Nullable(inner)) if matches!(inner.as_ref(), Type::Class(id) if *id == class) =>
                    {
                        let pointer = self.expect_scalar(base)?;
                        self.address_offset(pointer, i64::from(offset))
                    }
                    l::ValueType::Address(_) => {
                        let pointer = self.expect_scalar(base)?;
                        self.address_offset(pointer, i64::from(offset))
                    }
                    other => {
                        return Err(internal(format!(
                            "field {} has invalid base {other:?}",
                            field.0
                        )))
                    }
                };
                Ok((address, ty))
            }
            l::FieldRef::IterDone => {
                let address = self.expect_aggregate(base)?;
                Ok((address, Type::Bool))
            }
            l::FieldRef::IterValue => {
                let l::ValueType::Data(Type::IterResult(value)) = base_type else {
                    return Err(internal("IterResult.value has invalid base type"));
                };
                let address = self.expect_aggregate(base)?;
                let offset = self.ml.layouts.iter_result_value_offset(value)?;
                Ok((
                    self.address_offset(address, i64::from(offset)),
                    (**value).clone(),
                ))
            }
        }
    }

    /// Guards the semantic `JsonResult<T>.value` field load with its
    /// materialized sibling `ok` field. Ordinary field loads can carry other
    /// trap kinds, so the LIR trap distinguishes this checked access after
    /// the HIR expression kind has been transcribed away.
    fn guard_json_result_value(
        &mut self,
        field: l::FieldRef,
        base: RV,
        traps: &[l::Trap],
    ) -> Result<(), String> {
        let mut json_traps = traps.iter().filter_map(|trap| match trap.kind {
            l::TrapKind::JsonResultValue(ok_field) => Some((trap, ok_field)),
            _ => None,
        });
        let Some((first_trap, ok_field)) = json_traps.next() else {
            return Ok(());
        };
        let l::FieldRef::Class(field) = field else {
            return Err(internal(
                "JSON result trap is attached to a synthetic field",
            ));
        };
        let (class, _, _) = self.field_definition(field)?;
        let definition = self
            .ml
            .lir
            .classes
            .get(class.0)
            .filter(|definition| definition.id == class)
            .ok_or_else(|| internal("JSON result class is missing"))?;
        let ok_index = definition
            .fields
            .iter()
            .position(|field| field.id == ok_field && field.ty == Type::Bool)
            .ok_or_else(|| internal("JSON result guard field id is invalid"))?;
        if json_traps.any(|(_, candidate)| candidate != ok_field) {
            return Err(internal("JSON result traps disagree on the guard field id"));
        }
        if definition.is_value {
            return Err(internal("JSON result unexpectedly has value-class layout"));
        }
        let ok_offset = *self
            .ml
            .layouts
            .class(class.0)?
            .field_offsets
            .get(ok_index)
            .ok_or_else(|| internal("JSON result ok field offset is missing"))?;
        let pointer = self.expect_scalar(base)?;
        let ok = self.load_data(&Type::Bool, pointer, ok_offset as i32)?;
        let ok = self.expect_scalar(ok)?;
        self.emit_trap(first_trap, TrapOperand::Condition(ok))?;
        Ok(())
    }

    fn index_address(
        &mut self,
        base: RV,
        base_type: &l::ValueType,
        index: Value,
        index_type: &Type,
        checked: bool,
        traps: &[l::Trap],
    ) -> Result<(Value, Type), String> {
        let (base_address, length, runtime_stride, element) = match base_type {
            l::ValueType::Data(Type::Array(element)) => {
                let handle = self.expect_scalar(base)?;
                for trap in traps {
                    if trap.kind == l::TrapKind::DevOnlyLifetime {
                        self.emit_trap(trap, TrapOperand::Value(handle))?;
                    }
                }
                let length = checked.then(|| {
                    self.builder
                        .ins()
                        .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET)
                });
                let stride =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), handle, ARRAY_ELEM_SIZE_OFFSET);
                let data = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_DATA_OFFSET);
                (data, length, Some(stride), (**element).clone())
            }
            l::ValueType::Data(Type::FixedArray(element, count)) => {
                let address = self.expect_aggregate(base)?;
                let length = checked.then(|| self.iconst(types::I64, i64::from(*count)));
                (address, length, None, (**element).clone())
            }
            l::ValueType::Address(address) => match &address.pointee {
                Type::FixedArray(element, count) => {
                    let base = self.expect_scalar(base)?;
                    let length = checked.then(|| self.iconst(types::I64, i64::from(*count)));
                    (base, length, None, (**element).clone())
                }
                other => {
                    return Err(internal(format!(
                        "indexed address points to invalid type {other:?}"
                    )))
                }
            },
            other => return Err(internal(format!("invalid indexed base {other:?}"))),
        };
        let index64 = if self.builder.func.dfg.value_type(index) == types::I64 {
            index
        } else if is_unsigned(index_type) {
            self.builder.ins().uextend(types::I64, index)
        } else {
            self.builder.ins().sextend(types::I64, index)
        };
        if checked {
            let length = length.ok_or_else(|| internal("checked index has no captured length"))?;
            let index32 = if self.builder.func.dfg.value_type(index) == types::I32 {
                index
            } else {
                self.builder.ins().ireduce(types::I32, index)
            };
            let below = self
                .builder
                .ins()
                .icmp(IntCC::UnsignedLessThan, index64, length);
            let valid = if is_unsigned(index_type) {
                below
            } else {
                let nonnegative =
                    self.builder
                        .ins()
                        .icmp_imm(IntCC::SignedGreaterThanOrEqual, index32, 0);
                self.builder.ins().band(nonnegative, below)
            };
            let trap_length = self.builder.ins().ireduce(types::I32, length);
            for trap in traps {
                if matches!(trap.kind, l::TrapKind::IndexRead | l::TrapKind::IndexWrite) {
                    self.emit_trap(
                        trap,
                        TrapOperand::Index {
                            condition: valid,
                            index: index32,
                            length: trap_length,
                        },
                    )?;
                }
            }
        }
        let offset = if let Some(stride) = runtime_stride {
            self.builder.ins().imul(index64, stride)
        } else {
            let stride = self.ml.layouts.stride(&element)?;
            self.builder.ins().imul_imm(index64, i64::from(stride))
        };
        Ok((self.builder.ins().iadd(base_address, offset), element))
    }

    fn string_literal(&mut self, text: &str, traps: &[l::Trap], pos: &Pos) -> Result<RV, String> {
        let data = self.ml.literal_data(text.as_bytes())?;
        let global = self.ml.module.declare_data_in_func(data, self.builder.func);
        let pointer = self.builder.ins().symbol_value(types::I64, global);
        let length = self.iconst(types::I64, text.len() as i64);
        let position = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::Allocation)
            .map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        let value = self
            .call_runtime(
                self.ml.rt.str_lit,
                &[self.ctx, pointer, length, position],
                false,
            )?
            .ok_or_else(|| internal("string literal has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Allocation {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(value))
    }

    fn zero(&mut self, ty: &Type) -> Result<RV, String> {
        Ok(match self.ml.layouts.repr(ty)? {
            Repr::None => RV::None,
            Repr::Scalar(ty) => RV::Scalar(self.zero_scalar(ty)),
            Repr::Pair => {
                let zero = self.iconst(types::I64, 0);
                RV::Pair(zero, zero)
            }
            Repr::Agg { size, align } => {
                let address = self.stack_slot(size, align);
                self.zero_bytes(address, size, align);
                RV::Aggregate(address)
            }
        })
    }

    fn unary(&mut self, op: l::UnaryOp, operand: RV, ty: &Type) -> Result<RV, String> {
        let value = self.expect_scalar(operand)?;
        Ok(RV::Scalar(match op {
            l::UnaryOp::Not => self.builder.ins().bxor_imm(value, 1),
            l::UnaryOp::BitNot => self.builder.ins().bnot(value),
            l::UnaryOp::Neg if ty == &Type::F16 => {
                self.builder.ins().bxor_imm(value, i64::from(i16::MIN))
            }
            l::UnaryOp::Neg if ty.is_float() => self.builder.ins().fneg(value),
            l::UnaryOp::Neg => self.builder.ins().ineg(value),
        }))
    }

    fn binary(
        &mut self,
        op: l::BinaryOp,
        left: RV,
        right: RV,
        operand_ty: &Type,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let left = self.expect_scalar(left)?;
        let right = self.expect_scalar(right)?;
        if operand_ty == &Type::Str {
            return match op {
                l::BinaryOp::Add => {
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    let result = self
                        .call_runtime(
                            self.ml.rt.str_concat,
                            &[self.ctx, left, right, position],
                            false,
                        )?
                        .ok_or_else(|| internal("string concatenation has no result"))?;
                    for trap in traps {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                    Ok(RV::Scalar(result))
                }
                l::BinaryOp::Eq | l::BinaryOp::Ne => {
                    let result = self
                        .call_runtime(self.ml.rt.str_eq, &[self.ctx, left, right], false)?
                        .ok_or_else(|| internal("string equality has no result"))?;
                    let condition = self.builder.ins().icmp_imm(
                        if op == l::BinaryOp::Eq {
                            IntCC::NotEqual
                        } else {
                            IntCC::Equal
                        },
                        result,
                        0,
                    );
                    Ok(RV::Scalar(condition))
                }
                other => Err(internal(format!(
                    "invalid string binary operator {other:?}"
                ))),
            };
        }
        if operand_ty == &Type::F16 {
            let left = self
                .call_runtime(self.ml.rt.f16_to_f64, &[left], false)?
                .ok_or_else(|| internal("f16 left conversion has no result"))?;
            let right = self
                .call_runtime(self.ml.rt.f16_to_f64, &[right], false)?
                .ok_or_else(|| internal("f16 right conversion has no result"))?;
            let cc = match op {
                l::BinaryOp::Eq => FloatCC::Equal,
                l::BinaryOp::Ne => FloatCC::NotEqual,
                l::BinaryOp::Lt => FloatCC::LessThan,
                l::BinaryOp::Le => FloatCC::LessThanOrEqual,
                l::BinaryOp::Gt => FloatCC::GreaterThan,
                l::BinaryOp::Ge => FloatCC::GreaterThanOrEqual,
                other => return Err(internal(format!("invalid f16 operator {other:?}"))),
            };
            return Ok(RV::Scalar(self.builder.ins().fcmp(cc, left, right)));
        }
        let float = operand_ty.is_float();
        let unsigned = is_unsigned(operand_ty);
        let right = if matches!(op, l::BinaryOp::Shl | l::BinaryOp::Shr | l::BinaryOp::UShr) {
            self.builder.ins().band_imm(right, shift_mask(operand_ty)?)
        } else {
            right
        };
        let result = match op {
            l::BinaryOp::Add => {
                if float {
                    self.builder.ins().fadd(left, right)
                } else {
                    self.builder.ins().iadd(left, right)
                }
            }
            l::BinaryOp::Sub => {
                if float {
                    self.builder.ins().fsub(left, right)
                } else {
                    self.builder.ins().isub(left, right)
                }
            }
            l::BinaryOp::Mul => {
                if float {
                    self.builder.ins().fmul(left, right)
                } else {
                    self.builder.ins().imul(left, right)
                }
            }
            l::BinaryOp::Div | l::BinaryOp::Rem if float => {
                if op == l::BinaryOp::Div {
                    self.builder.ins().fdiv(left, right)
                } else {
                    let (left, right) = if operand_ty == &Type::F32 {
                        (
                            self.builder.ins().fpromote(types::F64, left),
                            self.builder.ins().fpromote(types::F64, right),
                        )
                    } else {
                        (left, right)
                    };
                    let remainder = self
                        .call_runtime(self.ml.rt.fmod, &[self.ctx, left, right], false)?
                        .ok_or_else(|| internal("floating remainder has no runtime result"))?;
                    if operand_ty == &Type::F32 {
                        self.builder.ins().fdemote(types::F32, remainder)
                    } else {
                        remainder
                    }
                }
            }
            l::BinaryOp::Div | l::BinaryOp::Rem => {
                for trap in traps {
                    if trap.kind == l::TrapKind::DivisionByZero {
                        self.emit_trap(trap, TrapOperand::Value(right))?;
                    }
                }
                if unsigned {
                    if op == l::BinaryOp::Div {
                        self.builder.ins().udiv(left, right)
                    } else {
                        self.builder.ins().urem(left, right)
                    }
                } else {
                    return self
                        .signed_division(op == l::BinaryOp::Div, left, right)
                        .map(RV::Scalar);
                }
            }
            l::BinaryOp::Eq
            | l::BinaryOp::Ne
            | l::BinaryOp::Lt
            | l::BinaryOp::Le
            | l::BinaryOp::Gt
            | l::BinaryOp::Ge => {
                if float {
                    let cc = match op {
                        l::BinaryOp::Eq => FloatCC::Equal,
                        l::BinaryOp::Ne => FloatCC::NotEqual,
                        l::BinaryOp::Lt => FloatCC::LessThan,
                        l::BinaryOp::Le => FloatCC::LessThanOrEqual,
                        l::BinaryOp::Gt => FloatCC::GreaterThan,
                        _ => FloatCC::GreaterThanOrEqual,
                    };
                    self.builder.ins().fcmp(cc, left, right)
                } else {
                    let cc = match (op, unsigned) {
                        (l::BinaryOp::Eq, _) => IntCC::Equal,
                        (l::BinaryOp::Ne, _) => IntCC::NotEqual,
                        (l::BinaryOp::Lt, false) => IntCC::SignedLessThan,
                        (l::BinaryOp::Le, false) => IntCC::SignedLessThanOrEqual,
                        (l::BinaryOp::Gt, false) => IntCC::SignedGreaterThan,
                        (l::BinaryOp::Ge, false) => IntCC::SignedGreaterThanOrEqual,
                        (l::BinaryOp::Lt, true) => IntCC::UnsignedLessThan,
                        (l::BinaryOp::Le, true) => IntCC::UnsignedLessThanOrEqual,
                        (l::BinaryOp::Gt, true) => IntCC::UnsignedGreaterThan,
                        _ => IntCC::UnsignedGreaterThanOrEqual,
                    };
                    self.builder.ins().icmp(cc, left, right)
                }
            }
            l::BinaryOp::BitAnd => self.builder.ins().band(left, right),
            l::BinaryOp::BitOr => self.builder.ins().bor(left, right),
            l::BinaryOp::BitXor => self.builder.ins().bxor(left, right),
            l::BinaryOp::Shl => self.builder.ins().ishl(left, right),
            l::BinaryOp::Shr if unsigned => self.builder.ins().ushr(left, right),
            l::BinaryOp::Shr => self.builder.ins().sshr(left, right),
            l::BinaryOp::UShr => self.builder.ins().ushr(left, right),
        };
        Ok(RV::Scalar(result))
    }

    fn signed_division(
        &mut self,
        division: bool,
        left: Value,
        right: Value,
    ) -> Result<Value, String> {
        let ty = self.builder.func.dfg.value_type(left);
        let minus_one = self.builder.ins().icmp_imm(IntCC::Equal, right, -1);
        let exceptional = self.builder.create_block();
        let ordinary = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, ty);
        self.builder
            .ins()
            .brif(minus_one, exceptional, &[], ordinary, &[]);
        self.builder.switch_to_block(exceptional);
        let value = if division {
            self.builder.ins().ineg(left)
        } else {
            self.zero_scalar(ty)
        };
        self.builder.ins().jump(merge, &[BlockArg::Value(value)]);
        self.builder.switch_to_block(ordinary);
        let value = if division {
            self.builder.ins().sdiv(left, right)
        } else {
            self.builder.ins().srem(left, right)
        };
        self.builder.ins().jump(merge, &[BlockArg::Value(value)]);
        self.builder.switch_to_block(merge);
        Ok(self.builder.block_params(merge)[0])
    }

    fn convert(
        &mut self,
        value: RV,
        source: &Type,
        target: &Type,
        traps: &[l::Trap],
    ) -> Result<RV, String> {
        if source == target {
            return Ok(value);
        }
        if let RV::Aggregate(address) = value {
            if matches!(target, Type::Nullable(_)) {
                return Ok(RV::Scalar(address));
            }
            return Err(internal(format!(
                "cannot convert aggregate {source:?} to {target:?}"
            )));
        }
        let scalar = self.expect_scalar(value)?;
        for trap in traps {
            if matches!(
                trap.kind,
                l::TrapKind::NullNarrowing
                    | l::TrapKind::DevOnlyLifetime
                    | l::TrapKind::ClassMismatch(_)
            ) {
                self.emit_trap(trap, TrapOperand::Value(scalar))?;
            }
        }
        if matches!(self.ml.layouts.repr(target)?, Repr::Agg { .. }) {
            return Ok(RV::Aggregate(scalar));
        }
        if target == &Type::F16 {
            let wide = match source {
                Type::F32 => self.builder.ins().fpromote(types::F64, scalar),
                Type::F64 => scalar,
                other => return Err(internal(format!("cannot convert {other:?} to f16"))),
            };
            let result = self
                .call_runtime(self.ml.rt.f16_from_f64, &[wide], false)?
                .ok_or_else(|| internal("f16 conversion has no result"))?;
            return Ok(RV::Scalar(result));
        }
        if source == &Type::F16 {
            let wide = self
                .call_runtime(self.ml.rt.f16_to_f64, &[scalar], false)?
                .ok_or_else(|| internal("f16 widening has no result"))?;
            return Ok(RV::Scalar(match target {
                Type::F32 => self.builder.ins().fdemote(types::F32, wide),
                Type::F64 => wide,
                other => return Err(internal(format!("cannot convert f16 to {other:?}"))),
            }));
        }
        if (source.is_integer() || matches!(source, Type::Enum(_))) && target.is_integer() {
            let Repr::Scalar(source_repr) = self.ml.layouts.repr(source)? else {
                return Err(internal("integer source has a non-scalar representation"));
            };
            let Repr::Scalar(target_repr) = self.ml.layouts.repr(target)? else {
                return Err(internal("integer target has a non-scalar representation"));
            };
            let result = if source_repr == target_repr {
                scalar
            } else if source_repr.bits() < target_repr.bits() {
                if is_unsigned(source) {
                    self.builder.ins().uextend(target_repr, scalar)
                } else {
                    self.builder.ins().sextend(target_repr, scalar)
                }
            } else {
                self.builder.ins().ireduce(target_repr, scalar)
            };
            return Ok(RV::Scalar(result));
        }
        if source.is_integer() && matches!(target, Type::F32 | Type::F64) {
            let target_repr = if target == &Type::F32 {
                types::F32
            } else {
                types::F64
            };
            let Repr::Scalar(source_repr) = self.ml.layouts.repr(source)? else {
                return Err(internal("integer source has a non-scalar representation"));
            };
            let scalar = if source_repr.bits() < 32 {
                if is_unsigned(source) {
                    self.builder.ins().uextend(types::I32, scalar)
                } else {
                    self.builder.ins().sextend(types::I32, scalar)
                }
            } else {
                scalar
            };
            return Ok(RV::Scalar(if is_unsigned(source) {
                self.builder.ins().fcvt_from_uint(target_repr, scalar)
            } else {
                self.builder.ins().fcvt_from_sint(target_repr, scalar)
            }));
        }
        if matches!(source, Type::F32 | Type::F64) && target.is_integer() {
            let Repr::Scalar(target_repr) = self.ml.layouts.repr(target)? else {
                return Err(internal("integer target has a non-scalar representation"));
            };
            let result = if target_repr.bits() >= 32 {
                if is_unsigned(target) {
                    self.builder.ins().fcvt_to_uint_sat(target_repr, scalar)
                } else {
                    self.builder.ins().fcvt_to_sint_sat(target_repr, scalar)
                }
            } else if is_unsigned(target) {
                let wide = self.builder.ins().fcvt_to_uint_sat(types::I32, scalar);
                let maximum = self.iconst(types::I32, (1i64 << target_repr.bits()) - 1);
                let clamped = self.builder.ins().umin(wide, maximum);
                self.builder.ins().ireduce(target_repr, clamped)
            } else {
                let wide = self.builder.ins().fcvt_to_sint_sat(types::I32, scalar);
                let minimum = self.iconst(types::I32, -(1i64 << (target_repr.bits() - 1)));
                let maximum = self.iconst(types::I32, (1i64 << (target_repr.bits() - 1)) - 1);
                let low = self.builder.ins().smax(wide, minimum);
                let clamped = self.builder.ins().smin(low, maximum);
                self.builder.ins().ireduce(target_repr, clamped)
            };
            return Ok(RV::Scalar(result));
        }
        Ok(RV::Scalar(match (source, target) {
            (Type::F32, Type::F64) => self.builder.ins().fpromote(types::F64, scalar),
            (Type::F64, Type::F32) => self.builder.ins().fdemote(types::F32, scalar),
            _ => scalar,
        }))
    }

    fn allocate_class(
        &mut self,
        class: ClassId,
        stable_address: Option<Value>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(class.0)
            .ok_or_else(|| internal(format!("class {} is missing", class.0)))?;
        let (size, align) = {
            let layout = self.ml.layouts.class(class.0)?;
            (layout.size, layout.align)
        };
        if definition.is_value {
            let address = stable_address.unwrap_or_else(|| self.stack_slot(size, align));
            self.zero_bytes(address, size, align);
            return Ok(RV::Aggregate(address));
        }
        let size = self.iconst(types::I64, i64::from(size));
        let class_id = self.iconst(types::I32, class.0 as i64);
        let position = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::Allocation)
            .map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        let pointer = self
            .call_runtime(
                self.ml.rt.alloc,
                &[self.ctx, size, class_id, position],
                false,
            )?
            .ok_or_else(|| internal("class allocation has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Allocation {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(pointer))
    }

    fn box_boundary_value(
        &mut self,
        value: RV,
        ty: &l::ValueType,
        payload: ClassId,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let l::ValueType::Data(Type::Class(class)) = ty else {
            return Err(internal("BoxBoundaryValue operand is not a class value"));
        };
        if *class != payload {
            return Err(internal(
                "BoxBoundaryValue payload disagrees with its operand",
            ));
        }
        let layout = self.ml.layouts.class(payload.0)?;
        if !layout.is_value {
            return Err(internal("BoxBoundaryValue operand is not a value class"));
        }
        let size = self.iconst(types::I64, i64::from(layout.size));
        let class_id = self.iconst(types::I32, payload.0 as i64);
        let position = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::Allocation)
            .map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        let pointer = self
            .call_runtime(
                self.ml.rt.alloc,
                &[self.ctx, size, class_id, position],
                false,
            )?
            .ok_or_else(|| internal("boundary value box allocation has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Allocation {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        self.store_data(&Type::Class(payload), pointer, 0, value)?;
        Ok(RV::Scalar(pointer))
    }
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn materialize(&mut self, value: RV, ty: &Type) -> Result<Value, String> {
        match self.ml.layouts.repr(ty)? {
            Repr::None => Ok(self.iconst(types::I64, 0)),
            Repr::Agg { .. } => self.expect_aggregate(value),
            Repr::Scalar(repr) => {
                let slot = self.stack_slot(repr.bytes(), repr.bytes());
                let value = self.expect_scalar(value)?;
                self.builder.ins().store(flags(), value, slot, 0);
                Ok(slot)
            }
            Repr::Pair => {
                let slot = self.stack_slot(16, 8);
                let (code, env) = self.expect_pair(value)?;
                self.builder.ins().store(flags(), code, slot, 0);
                self.builder.ins().store(flags(), env, slot, 8);
                Ok(slot)
            }
        }
    }

    fn array_literal(
        &mut self,
        result_ty: &Type,
        operands: &[RV],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        match result_ty {
            Type::FixedArray(element, count) => {
                if operands.len() != *count as usize {
                    return Err(internal("fixed array literal arity mismatch"));
                }
                let (size, align) = self.ml.layouts.size_align(result_ty)?;
                let destination = self.stack_slot(size, align);
                let stride = self.ml.layouts.stride(element)?;
                for (index, value) in operands.iter().copied().enumerate() {
                    let offset = u32::try_from(index)
                        .ok()
                        .and_then(|index| index.checked_mul(stride))
                        .and_then(|offset| i32::try_from(offset).ok())
                        .ok_or_else(|| internal("fixed array literal offset overflows"))?;
                    self.store_data(element, destination, offset, value)?;
                }
                Ok(RV::Aggregate(destination))
            }
            Type::Array(element) => {
                let stride = self.ml.layouts.stride(element)?;
                let stride = self.iconst(types::I64, i64::from(stride));
                let first_position = traps.first().map_or(pos, |trap| &trap.pos);
                let position = self.position_id(first_position);
                let position = self.iconst(types::I32, position);
                let handle = self
                    .call_runtime(self.ml.rt.array_new, &[self.ctx, stride, position], false)?
                    .ok_or_else(|| internal("array literal allocation has no result"))?;
                if let Some(trap) = traps.first() {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
                for (index, value) in operands.iter().copied().enumerate() {
                    let pointer = self.materialize(value, element)?;
                    let trap = traps.get(index + 1);
                    let position = trap.map_or(pos, |trap| &trap.pos);
                    let position = self.position_id(position);
                    let position = self.iconst(types::I32, position);
                    self.call_runtime(
                        self.ml.rt.array_push,
                        &[self.ctx, handle, pointer, position],
                        false,
                    )?;
                    if let Some(trap) = trap {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                Ok(RV::Scalar(handle))
            }
            other => Err(internal(format!(
                "array literal has invalid type {other:?}"
            ))),
        }
    }

    fn array_with_capacity(
        &mut self,
        result_ty: &Type,
        capacity: RV,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let Type::Array(element) = result_ty else {
            return Err(internal("capacity array result is not an array"));
        };
        let capacity = self.expect_scalar(capacity)?;
        let capacity = if self.builder.func.dfg.value_type(capacity) == types::I64 {
            capacity
        } else {
            self.builder.ins().uextend(types::I64, capacity)
        };
        let stride = self.ml.layouts.stride(element)?;
        let stride = self.iconst(types::I64, i64::from(stride));
        let diagnostic = traps.first().map_or(pos, |trap| &trap.pos);
        let diagnostic = self.position_id(diagnostic);
        let diagnostic = self.iconst(types::I32, diagnostic);
        let mut signature = Signature::new(self.ml.call_conv);
        for ty in [types::I64, types::I64, types::I64, types::I32] {
            signature.params.push(AbiParam::new(ty));
        }
        signature.returns.push(AbiParam::new(types::I64));
        let function = self
            .ml
            .module
            .declare_function(
                "subscript_rt_array_with_capacity",
                Linkage::Import,
                &signature,
            )
            .map_err(|error| internal(format!("declare array capacity allocator: {error}")))?;
        let handle = self
            .call_runtime(function, &[self.ctx, capacity, stride, diagnostic], false)?
            .ok_or_else(|| internal("capacity array allocation has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Call {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(handle))
    }

    fn spread_array_literal(
        &mut self,
        result_ty: &Type,
        spreads: &[Option<l::SpreadKind>],
        operands: &[RV],
        operand_types: &[l::ValueType],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let Type::Array(element) = result_ty else {
            return Err(internal("spread literal result is not an array"));
        };
        let stride = self.ml.layouts.stride(element)?;
        let stride = self.iconst(types::I64, i64::from(stride));
        let first_position = traps.first().map_or(pos, |trap| &trap.pos);
        let position = self.position_id(first_position);
        let position = self.iconst(types::I32, position);
        let handle = self
            .call_runtime(self.ml.rt.array_new, &[self.ctx, stride, position], false)?
            .ok_or_else(|| internal("spread literal allocation has no result"))?;
        if let Some(trap) = traps.first() {
            self.emit_trap(trap, TrapOperand::Pending)?;
        }
        for (index, ((spread, value), value_ty)) in
            spreads.iter().zip(operands).zip(operand_types).enumerate()
        {
            let trap = traps.get(index + 1);
            let position = trap.map_or(pos, |trap| &trap.pos);
            let position = self.position_id(position);
            let position = self.iconst(types::I32, position);
            match spread {
                None => {
                    let pointer = self.materialize(*value, element)?;
                    self.call_runtime(
                        self.ml.rt.array_push,
                        &[self.ctx, handle, pointer, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::Array) => {
                    let source = self.expect_scalar(*value)?;
                    self.call_runtime(
                        self.ml.rt.array_spread_array,
                        &[self.ctx, handle, source, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::FixedArray) => {
                    let source = self.expect_aggregate(*value)?;
                    let l::ValueType::Data(Type::FixedArray(_, count)) = value_ty else {
                        return Err(internal("fixed spread has invalid source type"));
                    };
                    let count = self.iconst(types::I64, i64::from(*count));
                    self.call_runtime(
                        self.ml.rt.array_spread_fixed,
                        &[self.ctx, handle, source, count, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::MapKeys | l::SpreadKind::SetValues) => {
                    let source = self.expect_scalar(*value)?;
                    self.call_runtime(
                        self.ml.rt.array_spread_assoc,
                        &[self.ctx, handle, source, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::StringCodePoints) => {
                    let source = self.expect_scalar(*value)?;
                    self.call_runtime(
                        self.ml.rt.array_spread_string,
                        &[self.ctx, handle, source, position],
                        false,
                    )?;
                }
            }
            if let Some(trap) = trap {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(handle))
    }

    fn format_value(
        &mut self,
        value: RV,
        format: l::FormatKind,
        trap: Option<&l::Trap>,
        pos: &Pos,
    ) -> Result<Value, String> {
        let value = self.expect_scalar(value)?;
        if format == l::FormatKind::Str {
            return Ok(value);
        }
        let position = trap.map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        if let l::FormatKind::StringAlias(alias) = format {
            let table = self.ml.string_alias_table_data(alias)?;
            let global = self
                .ml
                .module
                .declare_data_in_func(table, self.builder.func);
            let base = self.builder.ins().symbol_value(types::I64, global);
            let definition = self
                .ml
                .lir
                .string_aliases
                .get(alias.0)
                .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
            let index = if let Some(wire_values) = &definition.wire_values {
                let mut selected = self.iconst(types::I64, 0);
                for (index, wire) in wire_values.iter().enumerate() {
                    let equal = self
                        .builder
                        .ins()
                        .icmp_imm(IntCC::Equal, value, i64::from(*wire));
                    let index = self.iconst(types::I64, index as i64);
                    selected = self.builder.ins().select(equal, index, selected);
                }
                selected
            } else {
                self.builder.ins().uextend(types::I64, value)
            };
            let offset = self.builder.ins().ishl_imm(index, 4);
            let entry = self.builder.ins().iadd(base, offset);
            let pointer = self.builder.ins().load(types::I64, flags(), entry, 0);
            let length = self.builder.ins().load(types::I64, flags(), entry, 8);
            let result = self
                .call_runtime(
                    self.ml.rt.str_lit,
                    &[self.ctx, pointer, length, position],
                    false,
                )?
                .ok_or_else(|| internal("string alias formatting has no result"))?;
            if let Some(trap) = trap {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
            return Ok(result);
        }
        let (function, argument) = match format {
            l::FormatKind::I32 => {
                let argument = if self.builder.func.dfg.value_type(value) == types::I32 {
                    value
                } else {
                    self.builder.ins().sextend(types::I32, value)
                };
                (self.ml.rt.fmt_i32, argument)
            }
            l::FormatKind::U32 => {
                let argument = if self.builder.func.dfg.value_type(value) == types::I32 {
                    value
                } else {
                    self.builder.ins().uextend(types::I32, value)
                };
                (self.ml.rt.fmt_u32, argument)
            }
            l::FormatKind::I64 => (self.ml.rt.fmt_i64, value),
            l::FormatKind::U64 => (self.ml.rt.fmt_u64, value),
            l::FormatKind::F32 => (self.ml.rt.fmt_f32, value),
            l::FormatKind::F64 => (self.ml.rt.fmt_f64, value),
            l::FormatKind::F16 => {
                let wide = self
                    .call_runtime(self.ml.rt.f16_to_f64, &[value], false)?
                    .ok_or_else(|| internal("f16 formatting conversion has no result"))?;
                (self.ml.rt.fmt_f64, wide)
            }
            l::FormatKind::Bool => (
                self.ml.rt.fmt_bool,
                self.builder.ins().uextend(types::I32, value),
            ),
            l::FormatKind::Str | l::FormatKind::StringAlias(_) => {
                return Err(internal("direct string format reached numeric dispatch"))
            }
        };
        let result = self
            .call_runtime(function, &[self.ctx, argument, position], false)?
            .ok_or_else(|| internal("formatting runtime call has no result"))?;
        if let Some(trap) = trap {
            self.emit_trap(trap, TrapOperand::Pending)?;
        }
        Ok(result)
    }

    fn template(
        &mut self,
        parts: &[l::TemplatePart],
        operands: &[RV],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let mut trap_cursor = 0usize;
        let mut accumulated = None;
        for part in parts {
            let piece = match part {
                l::TemplatePart::Text(text) => {
                    let trap = traps.get(trap_cursor);
                    trap_cursor += usize::from(trap.is_some());
                    let piece =
                        self.string_literal(text, trap.map_or(&[], std::slice::from_ref), pos)?;
                    self.expect_scalar(piece)?
                }
                l::TemplatePart::Operand { index, format } => {
                    let index = *index as usize;
                    let value = *operands
                        .get(index)
                        .ok_or_else(|| internal("template operand index is out of range"))?;
                    let trap = (*format != l::FormatKind::Str)
                        .then(|| traps.get(trap_cursor))
                        .flatten();
                    trap_cursor += usize::from(trap.is_some());
                    self.format_value(value, *format, trap, pos)?
                }
            };
            accumulated = Some(match accumulated {
                None => piece,
                Some(previous) => {
                    let trap = traps.get(trap_cursor);
                    trap_cursor += usize::from(trap.is_some());
                    let position = trap.map_or(pos, |trap| &trap.pos);
                    let position = self.position_id(position);
                    let position = self.iconst(types::I32, position);
                    let result = self
                        .call_runtime(
                            self.ml.rt.str_concat,
                            &[self.ctx, previous, piece, position],
                            false,
                        )?
                        .ok_or_else(|| internal("template concatenation has no result"))?;
                    if let Some(trap) = trap {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                    result
                }
            });
        }
        if let Some(value) = accumulated {
            Ok(RV::Scalar(value))
        } else {
            self.string_literal("", traps, pos)
        }
    }
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn push_argument(
        &mut self,
        output: &mut Vec<Value>,
        value: RV,
        ty: &l::ValueType,
    ) -> Result<(), String> {
        match value_repr(&self.ml.layouts, ty)? {
            Repr::None => {}
            Repr::Scalar(_) => output.push(self.expect_scalar(value)?),
            Repr::Pair => {
                let (code, env) = self.expect_pair(value)?;
                output.extend([code, env]);
            }
            Repr::Agg { .. } => output.push(self.expect_aggregate(value)?),
        }
        Ok(())
    }

    fn call_result(
        &mut self,
        ty: Option<&l::ValueType>,
        results: &[Value],
        sret: Option<Value>,
    ) -> Result<RV, String> {
        let Some(ty) = ty else {
            return Ok(RV::None);
        };
        Ok(match value_repr(&self.ml.layouts, ty)? {
            Repr::None => RV::None,
            Repr::Scalar(_) => RV::Scalar(
                *results
                    .first()
                    .ok_or_else(|| internal("call has no scalar result"))?,
            ),
            Repr::Pair => RV::Pair(
                *results
                    .first()
                    .ok_or_else(|| internal("call has no code result"))?,
                *results
                    .get(1)
                    .ok_or_else(|| internal("call has no environment result"))?,
            ),
            Repr::Agg { .. } => {
                RV::Aggregate(sret.ok_or_else(|| internal("aggregate call has no result slot"))?)
            }
        })
    }

    fn method_function(&self, method: l::MethodId) -> Result<l::FunctionId, String> {
        self.ml
            .lir
            .classes
            .iter()
            .flat_map(|class| class.constructor.iter().chain(class.methods.iter()))
            .find(|candidate| candidate.id == method)
            .map(|candidate| candidate.function)
            .ok_or_else(|| internal(format!("method {} is missing", method.0)))
    }

    fn script_call(
        &mut self,
        function: l::FunctionId,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        receiver: bool,
    ) -> Result<RV, String> {
        let target = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|target| target.id == function)
            .ok_or_else(|| internal(format!("function {} is missing", function.0)))?;
        let mut arguments = vec![self.ctx];
        let sret = if let Some(l::ValueType::Data(ty)) = return_type {
            match self.ml.layouts.repr(ty)? {
                Repr::Agg { size, align } => {
                    let slot = self.stack_slot(size, align);
                    arguments.push(slot);
                    Some(slot)
                }
                _ => None,
            }
        } else {
            None
        };
        let mut operand_index = 0usize;
        if receiver {
            let value = *operands
                .first()
                .ok_or_else(|| internal("method call has no receiver"))?;
            let ty = parameter_types
                .first()
                .ok_or_else(|| internal("method call has no receiver type"))?;
            self.push_argument(&mut arguments, value, ty)?;
            operand_index = 1;
        }
        for (value, ty) in operands
            .iter()
            .copied()
            .skip(operand_index)
            .zip(parameter_types.iter().skip(operand_index))
        {
            self.push_argument(&mut arguments, value, ty)?;
        }
        let results = self.call_script(&function_key(target), &arguments, false)?;
        self.call_result(return_type, &results, sret)
    }

    fn static_closure_call(
        &mut self,
        function: l::FunctionId,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
    ) -> Result<RV, String> {
        let target = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|target| target.id == function)
            .ok_or_else(|| internal(format!("function {} is missing", function.0)))?;
        let callable = *operands
            .first()
            .ok_or_else(|| internal("static closure call has no callable"))?;
        let (_, environment) = self.expect_pair(callable)?;
        let mut arguments = vec![self.ctx, environment];
        let sret = if let Some(l::ValueType::Data(ty)) = return_type {
            match self.ml.layouts.repr(ty)? {
                Repr::Agg { size, align } => {
                    let slot = self.stack_slot(size, align);
                    arguments.push(slot);
                    Some(slot)
                }
                _ => None,
            }
        } else {
            None
        };
        for (value, ty) in operands
            .iter()
            .copied()
            .skip(1)
            .zip(parameter_types.iter().skip(1))
        {
            self.push_argument(&mut arguments, value, ty)?;
        }
        // Lambda bodies belong to the current reload generation and have no
        // stable cross-generation slot. The callable operand supplies that
        // generation's environment, so call its declared body directly.
        let results = self.call_script_direct(&function_key(target), &arguments, false)?;
        self.call_result(return_type, &results, sret)
    }

    fn indirect_call(
        &mut self,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
    ) -> Result<RV, String> {
        let callable = *operands
            .first()
            .ok_or_else(|| internal("indirect call has no callable"))?;
        let (code, env) = self.expect_pair(callable)?;
        let Type::Func(signature) = data_type(
            parameter_types
                .first()
                .ok_or_else(|| internal("indirect call has no callable type"))?,
        )?
        else {
            return Err(internal("indirect call operand is not a function"));
        };
        let mut arguments = vec![self.ctx, env];
        let sret = match self.ml.layouts.repr(&signature.ret)? {
            Repr::Agg { size, align } => {
                let slot = self.stack_slot(size, align);
                arguments.push(slot);
                Some(slot)
            }
            _ => None,
        };
        for (value, ty) in operands
            .iter()
            .copied()
            .skip(1)
            .zip(parameter_types.iter().skip(1))
        {
            self.push_argument(&mut arguments, value, ty)?;
        }
        let signature = self
            .ml
            .make_sig(&signature.params, &signature.ret, true, false)?;
        let signature = self.builder.import_signature(signature);
        let call = self
            .builder
            .ins()
            .call_indirect(signature, code, &arguments);
        let results = self.builder.inst_results(call).to_vec();
        self.call_result(return_type, &results, sret)
    }

    fn intrinsic_name(&self, intrinsic: &l::Intrinsic) -> Result<&str, String> {
        self.ml
            .lir
            .intrinsic_operations
            .iter()
            .find(|operation| {
                operation.family == intrinsic.family && operation.operation == intrinsic.operation
            })
            .map(|operation| operation.semantic_name.as_str())
            .ok_or_else(|| {
                internal(format!(
                    "intrinsic {:?}.{} is missing",
                    intrinsic.family, intrinsic.operation
                ))
            })
    }

    fn simple_runtime_intrinsic(
        &mut self,
        function: cranelift_module::FuncId,
        operands: &[RV],
        position: Option<&Pos>,
        checked: bool,
        bool_result: bool,
    ) -> Result<RV, String> {
        let mut arguments = vec![self.ctx];
        for value in operands {
            arguments.push(self.expect_scalar(*value)?);
        }
        if let Some(position) = position {
            let position = self.position_id(position);
            arguments.push(self.iconst(types::I32, position));
        }
        let result = self.call_runtime(function, &arguments, checked)?;
        Ok(match result {
            Some(value) if bool_result => {
                RV::Scalar(self.builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
            }
            Some(value) => RV::Scalar(value),
            None => RV::None,
        })
    }

    fn intrinsic_call(
        &mut self,
        intrinsic: &l::Intrinsic,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let name = self.intrinsic_name(intrinsic)?.to_string();
        if name != "UnsafeDelete"
            && traps
                .iter()
                .any(|trap| trap.kind == l::TrapKind::DevOnlyLifetime)
        {
            let receiver = self.expect_scalar(
                *operands
                    .first()
                    .ok_or_else(|| internal(format!("{name} has no lifetime operand")))?,
            )?;
            for trap in traps {
                if trap.kind == l::TrapKind::DevOnlyLifetime {
                    self.emit_trap(trap, TrapOperand::Value(receiver))?;
                }
            }
        }
        let checked = traps
            .iter()
            .any(|trap| matches!(trap.kind, l::TrapKind::Allocation | l::TrapKind::Call));
        let result = match intrinsic.family {
            l::IntrinsicFamily::Ambient => match name.as_str() {
                "Print" => {
                    let value = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("Print has no operand"))?,
                    )?;
                    self.call_runtime(self.ml.rt.print, &[self.ctx, value], false)?;
                    RV::None
                }
                "Collect" => {
                    self.call_runtime(self.ml.rt.collect, &[self.ctx], false)?;
                    RV::None
                }
                "UnsafeDelete" => {
                    let value = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("UnsafeDelete has no operand"))?,
                    )?;
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    self.call_runtime(self.ml.rt.delete, &[self.ctx, value, position], false)?;
                    for trap in traps {
                        if trap.kind == l::TrapKind::DevOnlyLifetime {
                            self.emit_trap(trap, TrapOperand::Pending)?;
                        }
                    }
                    RV::None
                }
                "Unreachable" => {
                    let trap = traps
                        .iter()
                        .find(|trap| trap.kind == l::TrapKind::Unreachable)
                        .ok_or_else(|| internal("Unreachable has no trap"))?;
                    self.emit_trap(trap, TrapOperand::Pending)?;
                    RV::None
                }
                other => return Err(internal(format!("unknown Ambient intrinsic {other}"))),
            },
            l::IntrinsicFamily::Math => {
                let function = *self
                    .ml
                    .rt
                    .math
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Math.{name} operation is out of range")))?;
                self.simple_runtime_intrinsic(function, operands, None, checked, false)?
            }
            l::IntrinsicFamily::Number => {
                let function = *self
                    .ml
                    .rt
                    .num
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Number.{name} operation is out of range")))?;
                let bool_result = matches!(
                    name.as_str(),
                    "IsNaN" | "IsFinite" | "IsInteger" | "IsSafeInteger"
                );
                let takes_position = !bool_result;
                self.simple_runtime_intrinsic(
                    function,
                    operands,
                    takes_position.then_some(pos),
                    checked,
                    bool_result,
                )?
            }
            l::IntrinsicFamily::Json => {
                let function = *self
                    .ml
                    .rt
                    .json
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Json.{name} operation is out of range")))?;
                let bool_result = matches!(
                    name.as_str(),
                    "Visit" | "ParseIsKind" | "ParseNumberFits" | "ParseBool"
                );
                self.simple_runtime_intrinsic(function, operands, Some(pos), checked, bool_result)?
            }
            l::IntrinsicFamily::String => {
                let function = *self
                    .ml
                    .rt
                    .str_ops
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("String.{name} operation is out of range")))?;
                let bool_result = matches!(name.as_str(), "Includes" | "StartsWith" | "EndsWith");
                let takes_position = !matches!(
                    name.as_str(),
                    "IndexOf" | "LastIndexOf" | "Includes" | "StartsWith" | "EndsWith"
                );
                self.simple_runtime_intrinsic(
                    function,
                    operands,
                    takes_position.then_some(pos),
                    checked,
                    bool_result,
                )?
            }
            l::IntrinsicFamily::Regex => {
                let function = *self
                    .ml
                    .rt
                    .regex_ops
                    .get(intrinsic.operation as usize)
                    .ok_or_else(|| internal(format!("Regex.{name} operation is out of range")))?;
                self.simple_runtime_intrinsic(
                    function,
                    operands,
                    Some(pos),
                    checked,
                    name == "Test",
                )?
            }
            l::IntrinsicFamily::Date => self.date_intrinsic(&name, operands, checked, pos)?,
            l::IntrinsicFamily::Array => self.array_intrinsic(
                intrinsic,
                &name,
                operands,
                parameter_types,
                return_type,
                checked,
                pos,
            )?,
            l::IntrinsicFamily::Map => self.map_intrinsic(
                intrinsic,
                &name,
                operands,
                parameter_types,
                return_type,
                checked,
                pos,
            )?,
            l::IntrinsicFamily::Set => self.set_intrinsic(
                intrinsic,
                &name,
                operands,
                parameter_types,
                return_type,
                checked,
                pos,
            )?,
            l::IntrinsicFamily::ContextBytes => {
                self.context_bytes_intrinsic(intrinsic, &name, operands, checked, pos)?
            }
            l::IntrinsicFamily::Worker => {
                self.worker_intrinsic(intrinsic, &name, operands, checked)?
            }
        };
        if checked {
            for trap in traps {
                if matches!(trap.kind, l::TrapKind::Allocation | l::TrapKind::Call) {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
            }
        }
        Ok(result)
    }

    fn date_intrinsic(
        &mut self,
        name: &str,
        operands: &[RV],
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let scalar = |this: &Self, index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Date.{name} operand {index} is missing")))
                .and_then(|value| this.expect_scalar(value))
        };
        match name {
            "New" => {
                let value = scalar(self, 0)?;
                self.simple_runtime_intrinsic(
                    self.ml.rt.date_new,
                    &[RV::Scalar(value)],
                    Some(pos),
                    checked,
                    false,
                )
            }
            "Utc" => self.simple_runtime_intrinsic(
                self.ml.rt.date_utc,
                operands,
                Some(pos),
                checked,
                false,
            ),
            "Now" => {
                self.simple_runtime_intrinsic(self.ml.rt.date_now, operands, None, checked, false)
            }
            "ToIso" => self.simple_runtime_intrinsic(
                self.ml.rt.date_to_iso,
                operands,
                Some(pos),
                checked,
                false,
            ),
            accessor => {
                let code = match accessor {
                    "GetUtcFullYear" => 0,
                    "GetUtcMonth" => 1,
                    "GetUtcDate" => 2,
                    "GetUtcDay" => 3,
                    "GetUtcHours" => 4,
                    "GetUtcMinutes" => 5,
                    "GetUtcSeconds" => 6,
                    "GetUtcMilliseconds" => 7,
                    other => return Err(internal(format!("unknown Date intrinsic {other}"))),
                };
                let value = scalar(self, 0)?;
                let code = self.iconst(types::I32, code);
                let result = self
                    .call_runtime(self.ml.rt.date_get, &[self.ctx, value, code], checked)?
                    .ok_or_else(|| internal("Date accessor has no result"))?;
                Ok(RV::Scalar(result))
            }
        }
    }

    fn array_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let function = *self
            .ml
            .rt
            .arr_ops
            .get(intrinsic.operation as usize)
            .ok_or_else(|| internal(format!("Array.{name} operation is out of range")))?;
        let receiver_ty = parameter_types
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver type")))?;
        let (element, fixed_count) = match receiver_ty {
            l::ValueType::Data(Type::Array(element)) => ((**element).clone(), None),
            l::ValueType::Data(Type::FixedArray(element, count)) => {
                ((**element).clone(), Some(*count))
            }
            other => return Err(internal(format!("Array.{name} receiver is {other:?}"))),
        };
        let receiver = *operands
            .first()
            .ok_or_else(|| internal(format!("Array.{name} has no receiver")))?;
        let receiver = if fixed_count.is_some() {
            self.expect_aggregate(receiver)?
        } else {
            self.expect_scalar(receiver)?
        };
        let function = if fixed_count.is_some() {
            self.ml
                .rt
                .fixed_arr_ops
                .get(intrinsic.operation as usize)
                .copied()
                .flatten()
                .ok_or_else(|| internal(format!("Array.{name} is not a FixedArray method")))?
        } else {
            function
        };
        let scalar = |this: &Self, index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Array.{name} operand {index} is missing")))
                .and_then(|value| this.expect_scalar(value))
        };
        let callback = |this: &Self| {
            operands
                .get(1)
                .copied()
                .ok_or_else(|| internal(format!("Array.{name} callback is missing")))
                .and_then(|value| this.expect_pair(value))
        };
        let callback_indexed = || -> Result<bool, String> {
            let expected = match name {
                "ForEach" | "Map" | "Filter" | "Some" | "Every" | "FindIndex" => 2,
                "Reduce" | "ReduceRight" => 3,
                other => return Err(internal(format!("Array.{other} has no indexed callback"))),
            };
            let l::ValueType::Data(Type::Func(function)) = parameter_types
                .get(1)
                .ok_or_else(|| internal(format!("Array.{name} callback type is missing")))?
            else {
                return Err(internal(format!("Array.{name} callback is not a function")));
            };
            match function.params.len() {
                arity if arity + 1 == expected => Ok(false),
                arity if arity == expected => Ok(true),
                arity => Err(internal(format!(
                    "Array.{name} callback arity {arity} escaped the checker"
                ))),
            }
        };
        match name {
            "IndexOf" | "LastIndexOf" | "Includes" => {
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal(format!("Array.{name} search value is missing")))?;
                let value = self.materialize(value, &element)?;
                let kind = array_element_kind(self.ml.lir, &element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                let result = self
                    .call_runtime(function, &[self.ctx, receiver, value, kind], checked)?
                    .ok_or_else(|| internal(format!("Array.{name} has no result")))?;
                Ok(RV::Scalar(if name == "Includes" {
                    self.builder.ins().icmp_imm(IntCC::NotEqual, result, 0)
                } else {
                    result
                }))
            }
            "Join" => {
                let separator = scalar(self, 1)?;
                let kind = array_format_kind(&element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(
                        function,
                        &[self.ctx, receiver, separator, kind, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Array.Join has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Slice" => {
                let start = scalar(self, 1)?;
                let end = scalar(self, 2)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(
                        function,
                        &[self.ctx, receiver, start, end, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Array.Slice has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Fill" => {
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal("Array.Fill value is missing"))?;
                let value = self.materialize(value, &element)?;
                let start = scalar(self, 2)?;
                let end = scalar(self, 3)?;
                self.call_runtime(function, &[self.ctx, receiver, value, start, end], checked)?;
                Ok(RV::Scalar(receiver))
            }
            "Reverse" => {
                self.call_runtime(function, &[self.ctx, receiver], checked)?;
                Ok(RV::Scalar(receiver))
            }
            "Concat" => {
                let other = scalar(self, 1)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(function, &[self.ctx, receiver, other, position], checked)?
                    .ok_or_else(|| internal("Array.Concat has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Splice" => {
                let start = scalar(self, 1)?;
                let delete_count = scalar(self, 2)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(
                        function,
                        &[self.ctx, receiver, start, delete_count, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Array.Splice has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Shift" => {
                let (size, align) = self.ml.layouts.size_align(&element)?;
                let output = self.stack_slot(size.max(8), align.max(8));
                self.zero_bytes(output, size.max(8), align.max(8));
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(function, &[self.ctx, receiver, output, position], checked)?;
                self.load_data(&element, output, 0)
            }
            "Unshift" => {
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal("Array.Unshift value is missing"))?;
                let value = self.materialize(value, &element)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let result = self
                    .call_runtime(function, &[self.ctx, receiver, value, position], checked)?
                    .ok_or_else(|| internal("Array.Unshift has no result"))?;
                Ok(RV::Scalar(result))
            }
            "CopyWithin" => {
                let target = scalar(self, 1)?;
                let start = scalar(self, 2)?;
                let end = scalar(self, 3)?;
                self.call_runtime(function, &[self.ctx, receiver, target, start, end], checked)?;
                Ok(RV::Scalar(receiver))
            }
            "ForEach" | "Filter" | "Some" | "Every" | "FindIndex" => {
                let (code, environment) = callback(self)?;
                let indexed = callback_indexed()?;
                let kind = array_element_kind(self.ml.lir, &element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                let indexed = self.iconst(types::I32, i64::from(indexed));
                let mut arguments = vec![self.ctx, receiver];
                if let Some(count) = fixed_count {
                    let stride = self.ml.layouts.stride(&element)?;
                    arguments.push(self.iconst(types::I64, i64::from(count)));
                    arguments.push(self.iconst(types::I64, i64::from(stride)));
                }
                arguments.extend([code, environment, kind]);
                if name == "Filter" {
                    let position = self.position_id(pos);
                    arguments.push(self.iconst(types::I32, position));
                }
                arguments.push(indexed);
                let result = self.call_runtime(function, &arguments, checked)?;
                Ok(match name {
                    "ForEach" => RV::None,
                    "Some" | "Every" => {
                        let result = result
                            .ok_or_else(|| internal(format!("Array.{name} has no result")))?;
                        RV::Scalar(self.builder.ins().icmp_imm(IntCC::NotEqual, result, 0))
                    }
                    _ => RV::Scalar(
                        result.ok_or_else(|| internal(format!("Array.{name} has no result")))?,
                    ),
                })
            }
            "Sort" => {
                let (code, environment) = callback(self)?;
                let kind = array_element_kind(self.ml.lir, &element)?;
                let kind = self.iconst(types::I32, i64::from(kind));
                self.call_runtime(
                    function,
                    &[self.ctx, receiver, code, environment, kind],
                    checked,
                )?;
                Ok(RV::Scalar(receiver))
            }
            "Map" => {
                let (code, environment) = callback(self)?;
                let indexed = callback_indexed()?;
                let l::ValueType::Data(Type::Array(result_element)) =
                    return_type.ok_or_else(|| internal("Array.Map result type is missing"))?
                else {
                    return Err(internal("Array.Map result is not an array"));
                };
                let source_kind = array_element_kind(self.ml.lir, &element)?;
                let result_kind = array_element_kind(self.ml.lir, result_element)?;
                let result_stride = self.ml.layouts.stride(result_element)?;
                let mut arguments = vec![self.ctx, receiver];
                if let Some(count) = fixed_count {
                    let stride = self.ml.layouts.stride(&element)?;
                    arguments.push(self.iconst(types::I64, i64::from(count)));
                    arguments.push(self.iconst(types::I64, i64::from(stride)));
                }
                arguments.extend([
                    code,
                    environment,
                    self.iconst(types::I32, i64::from(source_kind)),
                    self.iconst(types::I32, i64::from(result_kind)),
                    self.iconst(types::I64, i64::from(result_stride)),
                ]);
                let position = self.position_id(pos);
                arguments.push(self.iconst(types::I32, position));
                arguments.push(self.iconst(types::I32, i64::from(indexed)));
                let result = self
                    .call_runtime(function, &arguments, checked)?
                    .ok_or_else(|| internal("Array.Map has no result"))?;
                Ok(RV::Scalar(result))
            }
            "Reduce" | "ReduceRight" => {
                let (code, environment) = callback(self)?;
                let indexed = callback_indexed()?;
                let accumulator_ty = data_type(
                    return_type.ok_or_else(|| internal("Array.Reduce result type is missing"))?,
                )?;
                let initial = *operands
                    .get(2)
                    .ok_or_else(|| internal("Array.Reduce initial value is missing"))?;
                let accumulator = self.materialize(initial, accumulator_ty)?;
                let element_kind = array_element_kind(self.ml.lir, &element)?;
                let accumulator_kind = array_element_kind(self.ml.lir, accumulator_ty)?;
                let accumulator_stride = self.ml.layouts.stride(accumulator_ty)?;
                let mut arguments = vec![self.ctx, receiver];
                if let Some(count) = fixed_count {
                    let stride = self.ml.layouts.stride(&element)?;
                    arguments.push(self.iconst(types::I64, i64::from(count)));
                    arguments.push(self.iconst(types::I64, i64::from(stride)));
                }
                arguments.extend([
                    code,
                    environment,
                    self.iconst(types::I32, i64::from(element_kind)),
                    self.iconst(types::I32, i64::from(accumulator_kind)),
                    self.iconst(types::I64, i64::from(accumulator_stride)),
                    accumulator,
                    self.iconst(types::I32, i64::from(indexed)),
                ]);
                self.call_runtime(function, &arguments, checked)?;
                self.load_data(accumulator_ty, accumulator, 0)
            }
            other => Err(internal(format!(
                "Array.{other} needs its typed runtime adapter"
            ))),
        }
    }

    fn map_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let function = *self
            .ml
            .rt
            .map_ops
            .get(intrinsic.operation as usize)
            .ok_or_else(|| internal(format!("Map.{name} operation is out of range")))?;
        if name == "GroupBy" {
            let (key, element) = match (return_type, parameter_types.first()) {
                (
                    Some(l::ValueType::Data(Type::Map(key, value))),
                    Some(l::ValueType::Data(Type::Array(element))),
                ) => match &**value {
                    Type::Array(group_element) if **group_element == **element => {
                        ((**key).clone(), (**element).clone())
                    }
                    other => {
                        return Err(internal(format!("Map.GroupBy result value is {other:?}")))
                    }
                },
                other => return Err(internal(format!("Map.GroupBy shape is {other:?}"))),
            };
            let items = self.expect_scalar(
                *operands
                    .first()
                    .ok_or_else(|| internal("Map.GroupBy items are missing"))?,
            )?;
            self.live_check(items, pos)?;
            let (code, environment) = self.expect_pair(
                *operands
                    .get(1)
                    .ok_or_else(|| internal("Map.GroupBy callback is missing"))?,
            )?;
            let bridge = define_group_bridge(self.ml, &element, &key)?;
            let bridge = self
                .ml
                .module
                .declare_func_in_func(bridge, self.builder.func);
            let bridge = self.builder.ins().func_addr(types::I64, bridge);
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let kind = association_key_kind(self.ml.lir, &key)?;
            let position = self.position_id(pos);
            let arguments = [
                self.ctx,
                items,
                code,
                environment,
                bridge,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I32, i64::from(kind)),
                self.iconst(types::I32, position),
            ];
            return self
                .call_runtime(function, &arguments, checked)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Map.GroupBy has no result"));
        }

        let (key, value) = if name == "New" {
            match return_type {
                Some(l::ValueType::Data(Type::Map(key, value))) => {
                    ((**key).clone(), (**value).clone())
                }
                other => return Err(internal(format!("Map.New result is {other:?}"))),
            }
        } else {
            match parameter_types.first() {
                Some(l::ValueType::Data(Type::Map(key, value))) => {
                    ((**key).clone(), (**value).clone())
                }
                other => return Err(internal(format!("Map.{name} receiver is {other:?}"))),
            }
        };
        if name == "New" {
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let (value_size, _) = self.ml.layouts.size_align(&value)?;
            let kind = association_key_kind(self.ml.lir, &key)?;
            let position = self.position_id(pos);
            let arguments = [
                self.ctx,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I64, i64::from(value_size)),
                self.iconst(types::I32, i64::from(kind)),
                self.iconst(types::I32, position),
            ];
            return self
                .call_runtime(function, &arguments, checked)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Map.New has no result"));
        }

        let handle = self.expect_scalar(
            *operands
                .first()
                .ok_or_else(|| internal(format!("Map.{name} receiver is missing")))?,
        )?;
        self.live_check(handle, pos)?;
        let operand = |index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Map.{name} operand {index} is missing")))
        };
        match name {
            "Size" => self
                .call_runtime(function, &[self.ctx, handle], false)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Map.Size has no result")),
            "Get" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let (size, align) = self.ml.layouts.size_align(&value)?;
                let output = self.stack_slot(size.max(8), align.max(8));
                self.zero_bytes(output, size.max(8), align.max(8));
                self.call_runtime(function, &[self.ctx, handle, key_address, output], false)?;
                self.load_data(&value, output, 0)
            }
            "GetOr" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let fallback = self.materialize(operand(2)?, &value)?;
                let (size, align) = self.ml.layouts.size_align(&value)?;
                let slot_size = size.max(8);
                let access_align = 1u32 << slot_size.trailing_zeros();
                let output = self.stack_slot(slot_size, align.max(8));
                self.zero_bytes(output, slot_size, align.max(8).min(access_align));
                self.call_runtime(
                    function,
                    &[self.ctx, handle, key_address, fallback, output],
                    false,
                )?;
                self.load_data(&value, output, 0)
            }
            "Set" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let value_address = self.materialize(operand(2)?, &value)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, key_address, value_address, position],
                    checked,
                )?;
                Ok(RV::Scalar(handle))
            }
            "Has" | "Delete" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let result = self
                    .call_runtime(function, &[self.ctx, handle, key_address], checked)?
                    .ok_or_else(|| internal(format!("Map.{name} has no result")))?;
                Ok(RV::Scalar(self.builder.ins().icmp_imm(
                    IntCC::NotEqual,
                    result,
                    0,
                )))
            }
            "Clear" => {
                self.call_runtime(function, &[self.ctx, handle], false)?;
                Ok(RV::None)
            }
            "ForEach" => {
                let (code, environment) = self.expect_pair(operand(1)?)?;
                let bridge = define_assoc_bridge(self.ml, &key, Some(&value))?;
                let bridge = self
                    .ml
                    .module
                    .declare_func_in_func(bridge, self.builder.func);
                let bridge = self.builder.ins().func_addr(types::I64, bridge);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, code, environment, bridge],
                    checked,
                )?;
                Ok(RV::None)
            }
            other => Err(internal(format!("unknown Map intrinsic {other}"))),
        }
    }

    fn set_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let function = *self
            .ml
            .rt
            .set_ops
            .get(intrinsic.operation as usize)
            .ok_or_else(|| internal(format!("Set.{name} operation is out of range")))?;
        let key = if name == "New" {
            match return_type {
                Some(l::ValueType::Data(Type::Set(key))) => (**key).clone(),
                other => return Err(internal(format!("Set.New result is {other:?}"))),
            }
        } else {
            match parameter_types.first() {
                Some(l::ValueType::Data(Type::Set(key))) => (**key).clone(),
                other => return Err(internal(format!("Set.{name} receiver is {other:?}"))),
            }
        };
        if name == "New" {
            let (key_size, _) = self.ml.layouts.size_align(&key)?;
            let kind = association_key_kind(self.ml.lir, &key)?;
            let position = self.position_id(pos);
            let arguments = [
                self.ctx,
                self.iconst(types::I64, i64::from(key_size)),
                self.iconst(types::I32, i64::from(kind)),
                self.iconst(types::I32, position),
            ];
            return self
                .call_runtime(function, &arguments, checked)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Set.New has no result"));
        }

        let handle = self.expect_scalar(
            *operands
                .first()
                .ok_or_else(|| internal(format!("Set.{name} receiver is missing")))?,
        )?;
        self.live_check(handle, pos)?;
        let operand = |index: usize| {
            operands
                .get(index)
                .copied()
                .ok_or_else(|| internal(format!("Set.{name} operand {index} is missing")))
        };
        match name {
            "Size" => self
                .call_runtime(function, &[self.ctx, handle], false)?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Set.Size has no result")),
            "Add" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, key_address, position],
                    checked,
                )?;
                Ok(RV::Scalar(handle))
            }
            "Has" | "Delete" => {
                let key_address = self.materialize(operand(1)?, &key)?;
                let result = self
                    .call_runtime(function, &[self.ctx, handle, key_address], checked)?
                    .ok_or_else(|| internal(format!("Set.{name} has no result")))?;
                Ok(RV::Scalar(self.builder.ins().icmp_imm(
                    IntCC::NotEqual,
                    result,
                    0,
                )))
            }
            "Clear" => {
                self.call_runtime(function, &[self.ctx, handle], false)?;
                Ok(RV::None)
            }
            "ForEach" => {
                let (code, environment) = self.expect_pair(operand(1)?)?;
                let bridge = define_assoc_bridge(self.ml, &key, None)?;
                let bridge = self
                    .ml
                    .module
                    .declare_func_in_func(bridge, self.builder.func);
                let bridge = self.builder.ins().func_addr(types::I64, bridge);
                self.call_runtime(
                    function,
                    &[self.ctx, handle, code, environment, bridge],
                    checked,
                )?;
                Ok(RV::None)
            }
            "Union" | "Intersection" | "Difference" | "SymmetricDifference" => {
                let other = self.expect_scalar(operand(1)?)?;
                self.live_check(other, pos)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(function, &[self.ctx, handle, other, position], checked)?
                    .map(RV::Scalar)
                    .ok_or_else(|| internal(format!("Set.{name} has no result")))
            }
            "IsSubsetOf" | "IsSupersetOf" | "IsDisjointFrom" => {
                let other = self.expect_scalar(operand(1)?)?;
                self.live_check(other, pos)?;
                let result = self
                    .call_runtime(function, &[self.ctx, handle, other], false)?
                    .ok_or_else(|| internal(format!("Set.{name} has no result")))?;
                Ok(RV::Scalar(self.builder.ins().icmp_imm(
                    IntCC::NotEqual,
                    result,
                    0,
                )))
            }
            other => Err(internal(format!("unknown Set intrinsic {other}"))),
        }
    }

    fn context_bytes_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        checked: bool,
        pos: &Pos,
    ) -> Result<RV, String> {
        let ty = intrinsic
            .type_argument
            .as_ref()
            .ok_or_else(|| internal(format!("Context.{name} has no type argument")))?;
        let (size, align) = self.ml.layouts.size_align(ty)?;
        let size_value = self.iconst(types::I32, i64::from(size));
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        match name {
            "BytesOf" => {
                let source = self.expect_aggregate(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Context.BytesOf value is missing"))?,
                )?;
                let handle = self
                    .call_runtime(
                        self.ml.rt.array_from_bytes,
                        &[self.ctx, source, size_value, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Context.BytesOf has no result"))?;
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("Context.BytesOf has no array data"))?;
                for range in self.ml.layouts.padding_ranges(ty)? {
                    let start = self.address_offset(data, i64::from(range.start));
                    self.zero_bytes(start, range.end - range.start, 1);
                }
                Ok(RV::Scalar(handle))
            }
            "BytesInto" => {
                let source = self.expect_aggregate(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Context.BytesInto value is missing"))?,
                )?;
                let target = self.expect_scalar(
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("Context.BytesInto target is missing"))?,
                )?;
                let offset = self.expect_scalar(
                    *operands
                        .get(2)
                        .ok_or_else(|| internal("Context.BytesInto offset is missing"))?,
                )?;
                let range = self
                    .call_runtime(
                        self.ml.rt.array_byte_range,
                        &[self.ctx, target, offset, size_value, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Context.BytesInto has no target range"))?;
                self.copy_bytes(range, source, size, 1);
                for padding in self.ml.layouts.padding_ranges(ty)? {
                    let start = self.address_offset(range, i64::from(padding.start));
                    self.zero_bytes(start, padding.end - padding.start, 1);
                }
                Ok(RV::None)
            }
            "FromBytes" => {
                let bytes = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Context.FromBytes source is missing"))?,
                )?;
                let offset = self.expect_scalar(
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("Context.FromBytes offset is missing"))?,
                )?;
                let range = self
                    .call_runtime(
                        self.ml.rt.array_byte_range,
                        &[self.ctx, bytes, offset, size_value, position],
                        checked,
                    )?
                    .ok_or_else(|| internal("Context.FromBytes has no source range"))?;
                let output = self.stack_slot(size, align);
                self.copy_bytes(output, range, size, 1);
                Ok(RV::Aggregate(output))
            }
            other => Err(internal(format!("unknown Context byte intrinsic {other}"))),
        }
    }

    fn worker_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        name: &str,
        operands: &[RV],
        checked: bool,
    ) -> Result<RV, String> {
        if name == "Spawn" {
            if !operands.is_empty() {
                return Err(internal("Worker.Spawn retained source operands"));
            }
            let index = intrinsic
                .worker_entry
                .ok_or_else(|| internal("Worker.Spawn has no worker entry"))?
                as usize;
            let entry = self
                .ml
                .lir
                .worker_entries
                .get(index)
                .ok_or_else(|| internal(format!("worker entry {index} is missing")))?;
            let input_class = entry.input;
            let output_class = entry.output;
            let initialize = self.ml.func_id(&FnKey::WorkerInit)?;
            let initialize = self
                .ml
                .module
                .declare_func_in_func(initialize, self.builder.func);
            let initialize = self.builder.ins().func_addr(types::I64, initialize);
            let worker = self.ml.func_id(&FnKey::WorkerEntry(index))?;
            let worker = self
                .ml
                .module
                .declare_func_in_func(worker, self.builder.func);
            let worker = self.builder.ins().func_addr(types::I64, worker);
            let input_descriptor = self.ml.worker_message_descriptor_data(input_class)?;
            let input_descriptor = self
                .ml
                .module
                .declare_data_in_func(input_descriptor, self.builder.func);
            let input_descriptor = self
                .builder
                .ins()
                .symbol_value(types::I64, input_descriptor);
            let output_descriptor = self.ml.worker_message_descriptor_data(output_class)?;
            let output_descriptor = self
                .ml
                .module
                .declare_data_in_func(output_descriptor, self.builder.func);
            let output_descriptor = self
                .builder
                .ins()
                .symbol_value(types::I64, output_descriptor);
            return self
                .call_runtime(
                    self.ml.rt.worker_spawn,
                    &[
                        self.ctx,
                        initialize,
                        worker,
                        input_descriptor,
                        output_descriptor,
                    ],
                    checked,
                )?
                .map(RV::Scalar)
                .ok_or_else(|| internal("Worker.Spawn has no result"));
        }

        let expected = match name {
            "Post" | "OutboxPost" => 2,
            "Poll" | "Close" | "Join" | "InboxWait" | "InboxPoll" => 1,
            other => return Err(internal(format!("unknown Worker intrinsic {other}"))),
        };
        if operands.len() != expected {
            return Err(internal(format!(
                "Worker.{name} has {} operand(s), expected {expected}",
                operands.len()
            )));
        }
        let mut arguments = Vec::with_capacity(expected + 1);
        arguments.push(self.ctx);
        for operand in operands {
            arguments.push(self.expect_scalar(*operand)?);
        }
        let function = match name {
            "Post" => self.ml.rt.worker_post,
            "Poll" => self.ml.rt.worker_poll,
            "Close" => self.ml.rt.worker_close,
            "Join" => self.ml.rt.worker_join,
            "InboxWait" => self.ml.rt.worker_inbox_wait,
            "InboxPoll" => self.ml.rt.worker_inbox_poll,
            "OutboxPost" => self.ml.rt.worker_outbox_post,
            _ => unreachable!("validated above"),
        };
        let result = self.call_runtime(function, &arguments, checked)?;
        Ok(match name {
            "Poll" | "InboxWait" | "InboxPoll" => {
                RV::Scalar(result.ok_or_else(|| internal(format!("Worker.{name} has no result")))?)
            }
            _ => RV::None,
        })
    }
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn function_reference(&mut self, function: l::FunctionId) -> Result<RV, String> {
        let wrapper = self.ml.func_id(&FnKey::LirWrapper(function))?;
        let reference = self
            .ml
            .module
            .declare_func_in_func(wrapper, self.builder.func);
        let code = self.builder.ins().func_addr(types::I64, reference);
        let env = self.iconst(types::I64, 0);
        Ok(RV::Pair(code, env))
    }

    fn make_closure(&mut self, function: l::FunctionId, operands: &[RV]) -> Result<RV, String> {
        let target = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|target| target.id == function)
            .ok_or_else(|| internal(format!("closure function {} is missing", function.0)))?;
        let captures = capture_parameters(target)
            .map(|parameter| {
                target
                    .values
                    .get(parameter.value.0 as usize)
                    .map(|value| value.ty.clone())
                    .ok_or_else(|| {
                        internal(format!("capture value {} is missing", parameter.value.0))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut offset = 0u32;
        let mut align = 1u32;
        let mut fields = Vec::with_capacity(captures.len());
        for ty in &captures {
            let (size, field_align) = value_size_align(&self.ml.layouts, ty)?;
            offset = round_up_layout(offset, field_align.max(1), "closure environment layout")?;
            fields.push(offset);
            offset = checked_layout_add(offset, size.max(1), "closure environment layout")?;
            align = align.max(field_align.max(1));
        }
        let _ = round_up_layout(offset.max(1), align, "final closure environment layout")?;
        let environment = if captures.is_empty() {
            self.iconst(types::I64, 0)
        } else {
            let (size, align) = self
                .closure_environment_layout
                .ok_or_else(|| internal("capturing closure has no uniform environment layout"))?;
            let environment = self.stack_slot(size, align);
            self.zero_bytes(environment, size, align);
            environment
        };
        for (((value, ty), offset), _) in
            operands.iter().copied().zip(&captures).zip(fields).zip(0..)
        {
            self.store_value_type(ty, environment, offset as i32, value)?;
        }
        let id = self.ml.func_id(&FnKey::LirFunction(function))?;
        let reference = self.ml.module.declare_func_in_func(id, self.builder.func);
        let code = self.builder.ins().func_addr(types::I64, reference);
        Ok(RV::Pair(code, environment))
    }

    fn builtin_call(
        &mut self,
        method: l::BuiltinMethod,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        match method {
            l::BuiltinMethod::ArrayPush => {
                let l::ValueType::Data(Type::Array(element)) = parameter_types
                    .first()
                    .ok_or_else(|| internal("array push has no receiver type"))?
                else {
                    return Err(internal("array push receiver is not an array"));
                };
                let handle = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("array push has no receiver"))?,
                )?;
                for trap in traps {
                    if trap.kind == l::TrapKind::DevOnlyLifetime {
                        self.emit_trap(trap, TrapOperand::Value(handle))?;
                    }
                }
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal("array push has no value"))?;
                let length = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET);
                let capacity =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), handle, ARRAY_CAP_OFFSET);
                let available = self
                    .builder
                    .ins()
                    .icmp(IntCC::UnsignedLessThan, length, capacity);
                let fast = self.builder.create_block();
                let slow = self.builder.create_block();
                let done = self.builder.create_block();
                self.builder.ins().brif(available, fast, &[], slow, &[]);

                self.builder.switch_to_block(fast);
                let data = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_DATA_OFFSET);
                let stride = self.ml.layouts.stride(element)?;
                let offset = self.builder.ins().imul_imm(length, i64::from(stride));
                let destination = self.builder.ins().iadd(data, offset);
                self.store_data(element, destination, 0, value)?;
                let next_length = self.builder.ins().iadd_imm(length, 1);
                self.builder
                    .ins()
                    .store(flags(), next_length, handle, ARRAY_LEN_OFFSET);
                self.builder.ins().jump(done, &[]);

                self.builder.switch_to_block(slow);
                let value = self.materialize(value, element)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.array_push,
                    &[self.ctx, handle, value, position],
                    false,
                )?;
                for trap in traps {
                    if matches!(trap.kind, l::TrapKind::Allocation | l::TrapKind::Call) {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                self.builder.ins().jump(done, &[]);

                self.builder.switch_to_block(done);
                let result = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET);
                let result = self.builder.ins().ireduce(types::I32, result);
                Ok(RV::Scalar(result))
            }
            l::BuiltinMethod::ArrayPop => {
                let l::ValueType::Data(Type::Array(element)) = parameter_types
                    .first()
                    .ok_or_else(|| internal("array pop has no receiver type"))?
                else {
                    return Err(internal("array pop receiver is not an array"));
                };
                let handle = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("array pop has no receiver"))?,
                )?;
                let (size, align) = self.ml.layouts.size_align(element)?;
                let output = self.stack_slot(size.max(8), align.max(8));
                self.zero_bytes(output, size.max(8), align.max(8));
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.array_pop,
                    &[self.ctx, handle, output, position],
                    false,
                )?;
                for trap in traps {
                    match trap.kind {
                        l::TrapKind::DevOnlyLifetime => {
                            self.emit_trap(trap, TrapOperand::Value(handle))?
                        }
                        l::TrapKind::Allocation | l::TrapKind::Call => {
                            self.emit_trap(trap, TrapOperand::Pending)?
                        }
                        _ => {}
                    }
                }
                self.load_data(element, output, 0)
            }
            l::BuiltinMethod::StringSlice => {
                let operation = self
                    .ml
                    .lir
                    .intrinsic_operations
                    .iter()
                    .find(|operation| {
                        operation.family == l::IntrinsicFamily::String
                            && operation.semantic_name == "Slice"
                    })
                    .ok_or_else(|| internal("String.Slice operation is missing"))?;
                let function = *self
                    .ml
                    .rt
                    .str_ops
                    .get(operation.operation as usize)
                    .ok_or_else(|| internal("String.Slice runtime function is missing"))?;
                self.simple_runtime_intrinsic(function, operands, Some(pos), true, false)
            }
            l::BuiltinMethod::GeneratorNext => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("generator next has no receiver"))?,
                )?;
                for trap in traps {
                    match trap.kind {
                        l::TrapKind::DevOnlyLifetime | l::TrapKind::DevReloadOnlyStaleCoroutine => {
                            self.emit_trap(trap, TrapOperand::Value(frame))?
                        }
                        _ => {}
                    }
                }
                let l::ValueType::Data(Type::IterResult(value)) =
                    return_type.ok_or_else(|| internal("generator next has no result type"))?
                else {
                    return Err(internal("generator next result is not IterResult"));
                };
                let result_ty = Type::IterResult(value.clone());
                let (size, align) = self.ml.layouts.size_align(&result_ty)?;
                let result = self.stack_slot(size, align);
                self.zero_bytes(result, size, align);
                let value_offset = self.ml.layouts.iter_result_value_offset(value)?;
                let output = self.address_offset(result, i64::from(value_offset));
                let resume =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), frame, COROUTINE_RESUME_OFFSET);
                let signature = self.builder.import_signature(self.ml.resume_sig());
                let call =
                    self.builder
                        .ins()
                        .call_indirect(signature, resume, &[self.ctx, frame, output]);
                let done = self.builder.inst_results(call)[0];
                for trap in traps {
                    if trap.kind == l::TrapKind::Call {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                self.builder.ins().store(flags(), done, result, 0);
                Ok(RV::Aggregate(result))
            }
        }
    }

    fn iterator_create(
        &mut self,
        kind: l::ForOfKind,
        bound_kind: l::IteratorBoundKind,
        subject: RV,
        subject_ty: &l::ValueType,
        iterator_ty: &l::IteratorType,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.stack_slot(32, 8);
        self.zero_bytes(cursor, 32, 8);
        let subject = match kind {
            l::ForOfKind::FixedArrayValues => self.expect_aggregate(subject)?,
            _ => self.expect_scalar(subject)?,
        };
        self.builder.ins().store(flags(), subject, cursor, 0);
        let fixed = self.iconst(
            types::I64,
            i64::from(bound_kind == l::IteratorBoundKind::Fixed),
        );
        self.builder.ins().store(flags(), fixed, cursor, 24);
        if kind == l::ForOfKind::FixedArrayValues
            && !matches!(subject_ty, l::ValueType::Data(Type::FixedArray(_, _)))
        {
            return Err(internal(
                "fixed-array iterator has no fixed-array subject type",
            ));
        }
        let captured = if let (
            l::ForOfKind::FixedArrayValues,
            l::ValueType::Data(Type::FixedArray(_, count)),
        ) = (kind, subject_ty)
        {
            self.iconst(types::I64, i64::from(*count))
        } else {
            self.iterator_current_bound(cursor, iterator_ty, pos)?
        };
        self.builder.ins().store(flags(), captured, cursor, 16);
        if matches!(
            kind,
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse
        ) {
            let empty = self.builder.ins().icmp_imm(IntCC::Equal, captured, 0);
            let last = self.builder.ins().iadd_imm(captured, -1);
            let exhausted = self.iconst(types::I64, -1);
            let position = self.builder.ins().select(empty, exhausted, last);
            self.builder.ins().store(flags(), position, cursor, 8);
        } else if matches!(
            kind,
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
        ) {
            let start = self.iconst(types::I64, 0);
            let position =
                self.next_live_assoc_position(subject, iterator_ty, start, captured, pos)?;
            self.builder.ins().store(flags(), position, cursor, 8);
        }
        Ok(RV::Aggregate(cursor))
    }

    fn iterator_current_bound(
        &mut self,
        cursor: Value,
        iterator_ty: &l::IteratorType,
        pos: &Pos,
    ) -> Result<Value, String> {
        let subject = self.builder.ins().load(types::I64, flags(), cursor, 0);
        let bound = match iterator_ty.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::ArrayValuesReverse
            | l::ForOfKind::ArrayKeysReverse => self
                .call_runtime(self.ml.rt.array_len, &[self.ctx, subject], false)?
                .ok_or_else(|| internal("array iterator bound has no result"))?,
            l::ForOfKind::FixedArrayValues => {
                self.builder.ins().load(types::I64, flags(), cursor, 16)
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.assoc_iter_begin,
                    &[self.ctx, subject, position],
                    true,
                )?
                .ok_or_else(|| internal("association iterator bound has no result"))?
            }
            l::ForOfKind::StringCodePoints => self
                .call_runtime(self.ml.rt.str_len, &[self.ctx, subject], false)?
                .ok_or_else(|| internal("string iterator bound has no result"))?,
        };
        Ok(if self.builder.func.dfg.value_type(bound) == types::I32 {
            self.builder.ins().uextend(types::I64, bound)
        } else {
            bound
        })
    }

    fn iterator_bound(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let current = self.iterator_current_bound(cursor, iterator_ty, pos)?;
        let captured = self.builder.ins().load(types::I64, flags(), cursor, 16);
        let fixed = self.builder.ins().load(types::I64, flags(), cursor, 24);
        let fixed = self.builder.ins().icmp_imm(IntCC::NotEqual, fixed, 0);
        let bound = self.builder.ins().select(fixed, captured, current);
        Ok(RV::Scalar(self.builder.ins().ireduce(types::I32, bound)))
    }

    fn iterator_effective_bound(
        &mut self,
        cursor: Value,
        iterator_ty: &l::IteratorType,
        captured: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        let captured = if self.builder.func.dfg.value_type(captured) == types::I64 {
            captured
        } else {
            self.builder.ins().uextend(types::I64, captured)
        };
        let current = self.iterator_current_bound(cursor, iterator_ty, pos)?;
        let fixed_limit = self.builder.ins().umin(captured, current);
        let fixed = self.builder.ins().load(types::I64, flags(), cursor, 24);
        let fixed = self.builder.ins().icmp_imm(IntCC::NotEqual, fixed, 0);
        Ok(self.builder.ins().select(fixed, fixed_limit, current))
    }

    fn next_live_assoc_position(
        &mut self,
        subject: Value,
        iterator_ty: &l::IteratorType,
        start: Value,
        bound: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        let (size, align) = self.ml.layouts.size_align(&iterator_ty.element)?;
        let output = self.stack_slot(size.max(1), align.max(1));
        let select = self.iconst(
            types::I32,
            i64::from(iterator_ty.kind == l::ForOfKind::MapValues),
        );
        let diagnostic_position = self.position_id(pos);
        let diagnostic_position = self.iconst(types::I32, diagnostic_position);
        let loop_block = self.builder.create_block();
        let found = self.builder.create_block();
        self.builder.append_block_param(loop_block, types::I64);
        self.builder.append_block_param(found, types::I64);
        self.builder
            .ins()
            .jump(loop_block, &[BlockArg::Value(start)]);
        self.builder.switch_to_block(loop_block);
        let candidate = self.builder.block_params(loop_block)[0];
        let below = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, candidate, bound);
        let inspect = self.builder.create_block();
        self.builder
            .ins()
            .brif(below, inspect, &[], found, &[BlockArg::Value(candidate)]);
        self.builder.switch_to_block(inspect);
        let active = self
            .call_runtime(
                self.ml.rt.assoc_iter_copy,
                &[
                    self.ctx,
                    subject,
                    candidate,
                    select,
                    output,
                    diagnostic_position,
                ],
                true,
            )?
            .ok_or_else(|| internal("association iterator active flag is missing"))?;
        let active = self.builder.ins().icmp_imm(IntCC::NotEqual, active, 0);
        let next = self.builder.create_block();
        self.builder
            .ins()
            .brif(active, found, &[BlockArg::Value(candidate)], next, &[]);
        self.builder.switch_to_block(next);
        let candidate = self.builder.ins().iadd_imm(candidate, 1);
        self.builder
            .ins()
            .jump(loop_block, &[BlockArg::Value(candidate)]);
        self.builder.switch_to_block(found);
        Ok(self.builder.block_params(found)[0])
    }

    fn iterator_has_next(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        index: Value,
        bound: Value,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let cursor_position = if matches!(
            iterator_ty.kind,
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys
        ) {
            self.builder.ins().uextend(types::I64, index)
        } else {
            self.builder.ins().load(types::I64, flags(), cursor, 8)
        };
        let effective = self.iterator_effective_bound(cursor, iterator_ty, bound, pos)?;
        let condition =
            self.builder
                .ins()
                .icmp(IntCC::UnsignedLessThan, cursor_position, effective);
        Ok(RV::Scalar(condition))
    }

    fn iterator_value(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        index: Value,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let subject = self.builder.ins().load(types::I64, flags(), cursor, 0);
        let cursor_position = if matches!(
            iterator_ty.kind,
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys
        ) {
            self.builder.ins().uextend(types::I64, index)
        } else {
            self.builder.ins().load(types::I64, flags(), cursor, 8)
        };
        match iterator_ty.kind {
            l::ForOfKind::ArrayKeys | l::ForOfKind::ArrayKeysReverse => Ok(RV::Scalar(
                self.builder.ins().ireduce(types::I32, cursor_position),
            )),
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayValuesReverse => {
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, subject], false)?
                    .ok_or_else(|| internal("array iterator data has no result"))?;
                let stride = self.ml.layouts.stride(&iterator_ty.element)?;
                let offset = self
                    .builder
                    .ins()
                    .imul_imm(cursor_position, i64::from(stride));
                let address = self.builder.ins().iadd(data, offset);
                self.load_data(&iterator_ty.element, address, 0)
            }
            l::ForOfKind::FixedArrayValues => {
                let stride = self.ml.layouts.stride(&iterator_ty.element)?;
                let offset = self
                    .builder
                    .ins()
                    .imul_imm(cursor_position, i64::from(stride));
                let address = self.builder.ins().iadd(subject, offset);
                self.load_data(&iterator_ty.element, address, 0)
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let (size, align) = self.ml.layouts.size_align(&iterator_ty.element)?;
                let output = self.stack_slot(size.max(1), align.max(1));
                let select = self.iconst(
                    types::I32,
                    i64::from(iterator_ty.kind == l::ForOfKind::MapValues),
                );
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.assoc_iter_copy,
                    &[self.ctx, subject, cursor_position, select, output, position],
                    true,
                )?;
                self.load_data(&iterator_ty.element, output, 0)
            }
            l::ForOfKind::StringCodePoints => {
                let current = self.builder.ins().ireduce(types::I32, cursor_position);
                let next = self.stack_slot(4, 4);
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                let value = self
                    .call_runtime(
                        self.ml.rt.str_iter_code_point,
                        &[self.ctx, subject, current, next, position],
                        true,
                    )?
                    .ok_or_else(|| internal("string iterator value is missing"))?;
                Ok(RV::Scalar(value))
            }
        }
    }

    fn iterator_advance(
        &mut self,
        iterator: RV,
        iterator_ty: &l::IteratorType,
        bound: Value,
        pos: &Pos,
    ) -> Result<RV, String> {
        let cursor = self.expect_aggregate(iterator)?;
        let next_cursor = self.stack_slot(32, 8);
        self.copy_bytes(next_cursor, cursor, 32, 8);
        let current = self.builder.ins().load(types::I64, flags(), cursor, 8);
        let effective = self.iterator_effective_bound(cursor, iterator_ty, bound, pos)?;
        let subject = self.builder.ins().load(types::I64, flags(), cursor, 0);
        let next = match iterator_ty.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::FixedArrayValues => self.builder.ins().iadd_imm(current, 1),
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse => {
                let no_prior = self.builder.ins().icmp_imm(IntCC::Equal, current, 0);
                let empty = self.builder.ins().icmp_imm(IntCC::Equal, effective, 0);
                let exhausted = self.builder.ins().bor(no_prior, empty);
                let prior = self.builder.ins().iadd_imm(current, -1);
                let last_live = self.builder.ins().iadd_imm(effective, -1);
                let clamped = self.builder.ins().umin(prior, last_live);
                let sentinel = self.iconst(types::I64, -1);
                self.builder.ins().select(exhausted, sentinel, clamped)
            }
            l::ForOfKind::StringCodePoints => {
                let current = self.builder.ins().ireduce(types::I32, current);
                let next = self.stack_slot(4, 4);
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.str_iter_code_point,
                    &[self.ctx, subject, current, next, position],
                    true,
                )?;
                let next = self.builder.ins().load(types::I32, flags(), next, 0);
                self.builder.ins().uextend(types::I64, next)
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let start = self.builder.ins().iadd_imm(current, 1);
                self.next_live_assoc_position(subject, iterator_ty, start, effective, pos)?
            }
        };
        self.builder.ins().store(flags(), next, next_cursor, 8);
        Ok(RV::Aggregate(next_cursor))
    }
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn clone_value(&mut self, value: RV, ty: &l::ValueType) -> Result<RV, String> {
        match value_repr(&self.ml.layouts, ty)? {
            Repr::Agg { size, align } => {
                let source = self.expect_aggregate(value)?;
                let destination = self.stack_slot(size, align);
                self.copy_bytes(destination, source, size, align);
                Ok(RV::Aggregate(destination))
            }
            _ => Ok(value),
        }
    }

    fn instruction_operands(&mut self, instruction: &l::Instruction) -> Result<Vec<RV>, String> {
        instruction
            .operands
            .iter()
            .map(|operand| self.operand(operand))
            .collect()
    }

    fn instruction_operand_types(
        &self,
        instruction: &l::Instruction,
    ) -> Result<Vec<l::ValueType>, String> {
        instruction
            .operands
            .iter()
            .map(|operand| self.operand_type(operand))
            .collect()
    }

    fn result_type(&self, instruction: &l::Instruction) -> Result<Option<l::ValueType>, String> {
        instruction
            .result
            .map(|result| self.value_type(result).cloned())
            .transpose()
    }

    fn length(&mut self, value: RV, ty: &l::ValueType) -> Result<RV, String> {
        let length = match ty {
            l::ValueType::Data(Type::Array(_)) => {
                let handle = self.expect_scalar(value)?;
                let length = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET);
                self.builder.ins().ireduce(types::I32, length)
            }
            l::ValueType::Data(Type::FixedArray(_, count)) => {
                self.iconst(types::I32, i64::from(*count))
            }
            l::ValueType::Data(Type::Str) => {
                let handle = self.expect_scalar(value)?;
                self.call_runtime(self.ml.rt.str_len, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("string length has no result"))?
            }
            other => return Err(internal(format!("length has invalid operand {other:?}"))),
        };
        Ok(RV::Scalar(length))
    }

    fn foreign_call(
        &mut self,
        id: l::ForeignFunctionId,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let declaration = self
            .ml
            .lir
            .foreign_functions
            .get(id.0 as usize)
            .ok_or_else(|| internal(format!("foreign function {} is missing", id.0)))?
            .clone();
        let operand_count = declaration
            .parameters
            .iter()
            .map(|parameter| usize::from(matches!(parameter.ty, Type::Array(_))) + 1)
            .sum::<usize>();
        if operands.len() != operand_count || parameter_types.len() != operand_count {
            return Err(internal(format!(
                "foreign call `{}` has inconsistent arity",
                declaration.source_name
            )));
        }
        let mut signature = Signature::new(self.ml.call_conv);
        let return_ty = return_type.map(data_type).transpose()?;
        let return_repr = return_ty
            .map(|ty| self.ml.layouts.repr(ty))
            .transpose()?
            .unwrap_or(Repr::None);
        let struct_return = match (return_ty, return_repr) {
            (Some(ty), Repr::Agg { size, align }) => {
                Some(self.plan_foreign_struct_return(ty, size, align, &mut signature, pos)?)
            }
            _ => None,
        };
        let mut arguments = Vec::new();
        if let Some(StructRet::Sret(slot)) = struct_return {
            arguments.push(slot);
        }
        let needs_scratch_scope = declaration.parameters.iter().any(|parameter| {
            matches!(
                parameter.ty,
                Type::Class(_) | Type::Array(_) | Type::Nullable(_)
            )
        });
        let scratch_mark = if needs_scratch_scope {
            Some(
                self.call_runtime(self.ml.rt.boundary_scratch_mark, &[self.ctx], false)?
                    .ok_or_else(|| internal("boundary scratch mark has no result"))?,
            )
        } else {
            None
        };
        let mut writebacks = Vec::new();
        let mut cursor = 0usize;
        for parameter in &declaration.parameters {
            let (value, array_snapshot) = if let Type::Array(element) = &parameter.ty {
                let data_ty = parameter_types
                    .get(cursor)
                    .ok_or_else(|| internal("foreign array data type is missing"))?;
                let count_ty = parameter_types
                    .get(cursor + 1)
                    .ok_or_else(|| internal("foreign array count type is missing"))?;
                let expected_data = l::ValueType::Address(l::AddressType {
                    pointee: (**element).clone(),
                    array_base: None,
                });
                if data_ty != &expected_data || count_ty != &l::ValueType::Data(Type::I32) {
                    return Err(internal(format!(
                        "foreign array parameter `{}` snapshot types disagree with the declaration",
                        parameter.source_name
                    )));
                }
                let data = self.expect_scalar(
                    *operands
                        .get(cursor)
                        .ok_or_else(|| internal("foreign array data is missing"))?,
                )?;
                let count = self.expect_scalar(
                    *operands
                        .get(cursor + 1)
                        .ok_or_else(|| internal("foreign array count is missing"))?,
                )?;
                cursor += 2;
                (RV::None, Some((data, count)))
            } else {
                let ty = parameter_types
                    .get(cursor)
                    .ok_or_else(|| internal("foreign parameter type is missing"))?;
                if !foreign_parameter_type_matches(self.ml.lir, ty, &parameter.ty) {
                    return Err(internal(format!(
                        "foreign parameter `{}` type disagrees with LIR call target",
                        parameter.source_name
                    )));
                }
                let value = *operands
                    .get(cursor)
                    .ok_or_else(|| internal("foreign parameter value is missing"))?;
                cursor += 1;
                (value, None)
            };
            self.marshal_foreign_argument(
                parameter,
                value,
                array_snapshot,
                &mut signature,
                &mut arguments,
                &mut writebacks,
                scratch_mark,
                pos,
            )?;
        }
        match return_repr {
            Repr::None | Repr::Agg { .. } => {}
            Repr::Scalar(repr) => signature.returns.push(AbiParam::new(repr)),
            Repr::Pair => return Err(internal("foreign function returns a function pair")),
        }
        let function =
            if let Some(function) = self.ml.foreign_ids.get(&declaration.source_name).copied() {
                function
            } else {
                let function = self
                    .ml
                    .module
                    .declare_function(&declaration.source_name, Linkage::Import, &signature)
                    .map_err(|error| {
                        internal(format!(
                            "declare foreign `{}`: {error}",
                            declaration.source_name
                        ))
                    })?;
                self.ml
                    .foreign_ids
                    .insert(declaration.source_name.clone(), function);
                self.ml
                    .foreign_symbols
                    .push(declaration.source_name.clone());
                function
            };
        let reference = self
            .ml
            .module
            .declare_func_in_func(function, self.builder.func);
        let call = self.builder.ins().call(reference, &arguments);
        let results = self.builder.inst_results(call).to_vec();
        for trap in traps {
            if trap.kind == l::TrapKind::Call {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        for writeback in writebacks {
            self.write_back_boundary_pointer(writeback, pos)?;
        }
        if let Some(mark) = scratch_mark {
            self.call_runtime(
                self.ml.rt.boundary_scratch_release,
                &[self.ctx, mark],
                false,
            )?;
        }
        Ok(match return_repr {
            Repr::None => RV::None,
            Repr::Scalar(_) => {
                let result = *results
                    .first()
                    .ok_or_else(|| internal("foreign scalar call has no result"))?;
                if let Some(Type::StringAlias(alias)) = return_ty {
                    let trap = traps
                        .iter()
                        .find(|trap| trap.kind == l::TrapKind::WireEnumValue(*alias))
                        .ok_or_else(|| internal("wire-enum foreign return has no trap"))?;
                    self.validate_wire_alias(*alias, result, trap)?;
                }
                RV::Scalar(result)
            }
            Repr::Agg { .. } => RV::Aggregate(self.finish_foreign_struct_return(
                struct_return.ok_or_else(|| internal("foreign struct-return plan is missing"))?,
                &results,
            )?),
            Repr::Pair => unreachable!("rejected above"),
        })
    }

    fn push_foreign_argument(
        &self,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        ty: types::Type,
        value: Value,
    ) {
        signature.params.push(AbiParam::new(ty));
        arguments.push(value);
    }

    fn validate_wire_alias(
        &mut self,
        alias: subscript_compiler::StringAliasId,
        wire: Value,
        trap: &l::Trap,
    ) -> Result<(), String> {
        let values = self
            .ml
            .lir
            .string_aliases
            .get(alias.0)
            .and_then(|definition| definition.wire_values.clone())
            .ok_or_else(|| internal("foreign string alias return has no wire mapping"))?;
        let mut valid = self.iconst(types::I8, 0);
        for value in values {
            let matches = self
                .builder
                .ins()
                .icmp_imm(IntCC::Equal, wire, i64::from(value));
            valid = self.builder.ins().bor(valid, matches);
        }
        self.emit_trap(trap, TrapOperand::WireValue { wire, valid })
    }

    fn validate_wire_alias_traps(
        &mut self,
        ty: &Type,
        value: RV,
        traps: &[l::Trap],
    ) -> Result<(), String> {
        for trap in traps {
            let l::TrapKind::WireEnumValue(alias) = trap.kind else {
                continue;
            };
            if ty != &Type::StringAlias(alias) {
                return Err(internal("wire-enum trap disagrees with its value type"));
            }
            self.validate_wire_alias(alias, self.expect_scalar(value)?, trap)?;
        }
        Ok(())
    }

    fn is_value_class(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if self.ml.layouts.class(id.0).is_ok_and(|layout| layout.is_value))
    }

    fn boundary_pointer_class(&self, ty: &Type) -> Option<usize> {
        boundary_box_class(self.ml.lir, ty).map(|class| class.0)
    }

    fn boundary_pointer_value(&self, value: RV) -> Result<Value, String> {
        match value {
            RV::Aggregate(address) | RV::Scalar(address) => Ok(address),
            other => Err(internal(format!("boundary pointer from {other:?}"))),
        }
    }

    fn boundary_c_field(&self, ty: &Type) -> Result<(u32, u32), String> {
        Ok(match ty {
            Type::Func(_) | Type::Object | Type::Nullable(_) => (8, 8),
            Type::Str | Type::Array(_) => (16, 8),
            Type::I8 | Type::U8 | Type::Bool => (1, 1),
            Type::I16 | Type::U16 | Type::F16 => (2, 2),
            Type::I32 | Type::U32 | Type::F32 | Type::Enum(_) | Type::StringAlias(_) => (4, 4),
            Type::I64 | Type::U64 | Type::F64 => (8, 8),
            Type::Class(id) if self.is_value_class(ty) => {
                let (_, size, align) = self.boundary_c_layout(id.0)?;
                (size, align)
            }
            Type::Class(_) | Type::Map(..) | Type::Set(_) => (8, 8),
            other => return Err(internal(format!("boundary C field type {other:?}"))),
        })
    }

    fn boundary_c_layout(&self, class: usize) -> Result<(Vec<u32>, u32, u32), String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(class)
            .ok_or_else(|| internal(format!("boundary class {class} is missing")))?;
        let mut offsets = Vec::with_capacity(definition.fields.len());
        let mut size = 0u32;
        let mut align = 1u32;
        for field in &definition.fields {
            let (field_size, field_align) = self.boundary_c_field(&field.ty)?;
            size = round_up_layout(size, field_align, "boundary C struct layout")?;
            offsets.push(size);
            size = checked_layout_add(size, field_size, "boundary C struct layout")?;
            align = align.max(field_align);
        }
        size = round_up_layout(size.max(1), align, "final boundary C struct layout")?;
        Ok((offsets, size, align))
    }

    /// The C-layout leaves of a boundary class, and the byte offsets of its
    /// `f16` fields. The walk is total over every field type
    /// `boundary_c_field` sizes, so a leaf list is never partial: an
    /// absorbed callback is one pointer leaf, and a string or array
    /// descriptor is two.
    fn boundary_leaf_components(&self, class: usize) -> Result<BoundaryLeaves, String> {
        fn collect<M: Module>(
            body: &Body<'_, '_, '_, '_, M>,
            class: usize,
            base: u32,
            leaves: &mut Vec<(u32, types::Type)>,
            f16_offsets: &mut Vec<u32>,
        ) -> Result<(), String> {
            let definition = body
                .ml
                .lir
                .classes
                .get(class)
                .ok_or_else(|| internal(format!("boundary class {class} is missing")))?;
            let (offsets, _, _) = body.boundary_c_layout(class)?;
            for (field, offset) in definition.fields.iter().zip(offsets) {
                let offset = checked_layout_add(base, offset, "boundary leaf offset")?;
                match &field.ty {
                    Type::Class(inner) if body.is_value_class(&field.ty) => {
                        collect(body, inner.0, offset, leaves, f16_offsets)?;
                    }
                    Type::Str | Type::Array(_) => {
                        leaves.push((offset, types::I64));
                        let second = checked_layout_add(offset, 8, "boundary leaf offset")?;
                        leaves.push((second, types::I64));
                    }
                    Type::F16 => {
                        f16_offsets.push(offset);
                        leaves.push((offset, types::I16));
                    }
                    Type::F32 => leaves.push((offset, types::F32)),
                    Type::F64 => leaves.push((offset, types::F64)),
                    Type::Bool | Type::I8 | Type::U8 => leaves.push((offset, types::I8)),
                    Type::I16 | Type::U16 => leaves.push((offset, types::I16)),
                    Type::I32 | Type::U32 | Type::Enum(_) | Type::StringAlias(_) => {
                        leaves.push((offset, types::I32))
                    }
                    Type::I64 | Type::U64 => leaves.push((offset, types::I64)),
                    Type::Func(_)
                    | Type::Object
                    | Type::Nullable(_)
                    | Type::Class(_)
                    | Type::Map(..)
                    | Type::Set(_) => leaves.push((offset, types::I64)),
                    other => {
                        return Err(internal(format!("boundary C field type {other:?}")));
                    }
                }
            }
            Ok(())
        }

        let mut leaves = Vec::new();
        let mut f16_offsets = Vec::new();
        collect(self, class, 0, &mut leaves, &mut f16_offsets)?;
        Ok(BoundaryLeaves {
            leaves,
            f16_offsets,
        })
    }

    /// Passes a by-value boundary aggregate the way the platform C ABI
    /// passes it (`specs/blocks/compiler.md` §12.3a). AAPCS64, Win64, and
    /// x86-64 SysV are implemented; any other dev host fails loud, because
    /// dev-JIT ≡ ship-C is otherwise unverifiable there.
    fn push_boundary_aggregate(
        &mut self,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        address: Value,
        size: u32,
        align: u32,
        components: &BoundaryLeaves,
    ) -> Result<(), String> {
        let triple = self.ml.module.isa().triple().clone();
        let abi = AggregateAbi::of(&triple).ok_or_else(|| {
            internal(format!(
                "foreign call passing a boundary struct by value is supported on aarch64 \
                 (AAPCS64) and on x86-64 (Win64 or SysV) in the dev JIT (compiler.md \
                 §12.3a); target {triple} is unsupported"
            ))
        })?;
        match plan_aggregate_arg(abi, &components.leaves, size)? {
            AggregateArgPlan::Hfa(hfa) => {
                for (offset, ty) in hfa {
                    let value = self.builder.ins().load(ty, flags(), address, offset as i32);
                    self.push_foreign_argument(signature, arguments, ty, value);
                }
            }
            AggregateArgPlan::Images(images) => {
                if abi == AggregateAbi::SysV {
                    ensure_sysv_argument_register_capacity(
                        signature,
                        &images,
                        &components.f16_offsets,
                    )?;
                }
                // Every image is read from a zero-filled copy, so a trailing
                // partial eightbyte carries defined bytes.
                let image_size = round_up_layout(size.max(1), 8, "boundary aggregate image")?;
                let copy = self.stack_slot(image_size, align.max(8));
                self.zero_bytes(copy, image_size, align.max(8));
                self.copy_bytes(copy, address, size, align.max(1));
                for image in images {
                    let value =
                        self.builder
                            .ins()
                            .load(image.ty, flags(), copy, image.offset as i32);
                    self.push_foreign_argument(signature, arguments, image.ty, value);
                }
            }
            AggregateArgPlan::Indirect => {
                let copy = self.stack_slot(size, align);
                self.copy_bytes(copy, address, size, align);
                self.push_foreign_argument(signature, arguments, types::I64, copy);
            }
            AggregateArgPlan::Memory { stack_size } => {
                let copy = self.stack_slot(stack_size, align.max(8));
                self.zero_bytes(copy, stack_size, align.max(8));
                self.copy_bytes(copy, address, size, align.max(1));
                signature.params.push(AbiParam::special(
                    types::I64,
                    ArgumentPurpose::StructArgument(stack_size),
                ));
                arguments.push(copy);
            }
        }
        Ok(())
    }

    fn plan_foreign_struct_return(
        &mut self,
        ty: &Type,
        size: u32,
        align: u32,
        signature: &mut Signature,
        pos: &Pos,
    ) -> Result<StructRet, String> {
        let Type::Class(class) = ty else {
            return Err(internal("foreign aggregate return is not a class"));
        };
        let triple = self.ml.module.isa().triple().clone();
        let abi = AggregateAbi::of(&triple).ok_or_else(|| {
            internal(format!(
                "foreign call returning a boundary struct by value is supported on aarch64 \
                 (AAPCS64) and on x86-64 (Win64 or SysV) in the dev JIT (compiler.md \
                 §12.3a); target {triple} is unsupported at {pos}"
            ))
        })?;
        let components = self.boundary_leaf_components(class.0)?;
        // An HFA return travels in SIMD registers on every supported ABI,
        // and the dev JIT models no float return register.
        if is_pure_hfa_leaves(&components.leaves) {
            return Err(internal(format!(
                "foreign homogeneous floating-point aggregate return is unsupported at {pos}"
            )));
        }
        let definition = self
            .ml
            .lir
            .classes
            .get(class.0)
            .ok_or_else(|| internal(format!("return class {} is missing", class.0)))?;
        if definition
            .fields
            .iter()
            .any(|field| matches!(field.ty, Type::Func(_) | Type::Array(_) | Type::Str))
        {
            return Err(internal(
                "foreign aggregate return contains an absorbed field",
            ));
        }
        let registers = match abi {
            AggregateAbi::Aapcs64 => (size <= 16).then(|| (size.div_ceil(8), types::I64)),
            AggregateAbi::Win64 => match size {
                1 => Some((1, types::I8)),
                2 => Some((1, types::I16)),
                4 => Some((1, types::I32)),
                8 => Some((1, types::I64)),
                _ => None,
            },
            AggregateAbi::SysV => {
                plan_sysv_struct_return(&components.leaves, size, &components.f16_offsets)?
                    .map(|images| (images.len() as u32, types::I64))
            }
        };
        if let Some((count, ty)) = registers {
            for _ in 0..count {
                signature.returns.push(AbiParam::new(ty));
            }
            let image_bytes = checked_layout_mul(count, ty.bytes(), "struct-return image")?;
            let slot_size = round_up_layout(size.max(image_bytes), 8, "struct-return slot")?;
            let slot = self.stack_slot(slot_size, align.max(8));
            Ok(StructRet::Registers { slot, count, ty })
        } else {
            let slot = self.stack_slot(size, align);
            signature
                .params
                .push(AbiParam::special(types::I64, ArgumentPurpose::StructReturn));
            Ok(StructRet::Sret(slot))
        }
    }

    fn finish_foreign_struct_return(
        &mut self,
        plan: StructRet,
        results: &[Value],
    ) -> Result<Value, String> {
        match plan {
            StructRet::Sret(slot) => Ok(slot),
            StructRet::Registers { slot, count, ty } => {
                if results.len() != count as usize {
                    return Err(internal("foreign struct-return register count mismatch"));
                }
                let stride = ty.bytes() as usize;
                for (index, value) in results.iter().enumerate() {
                    self.builder
                        .ins()
                        .store(flags(), *value, slot, (index * stride) as i32);
                }
                Ok(slot)
            }
        }
    }

    fn marshal_foreign_argument(
        &mut self,
        parameter: &l::ForeignParameter,
        value: RV,
        array_snapshot: Option<(Value, Value)>,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        writebacks: &mut Vec<BoundaryPtrWriteback>,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(), String> {
        match &parameter.ty {
            Type::StringAlias(alias) => {
                let definition = self
                    .ml
                    .lir
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| internal("wire alias is missing"))?;
                if definition.wire_values.is_none() {
                    return Err(internal("plain string alias reached a foreign parameter"));
                }
                let value = self.expect_scalar(value)?;
                self.push_foreign_argument(signature, arguments, types::I32, value);
                Ok(())
            }
            Type::Str => {
                let handle = self.expect_scalar(value)?;
                let data = self
                    .call_runtime(self.ml.rt.str_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("foreign string data is missing"))?;
                let length = self
                    .call_runtime(self.ml.rt.str_len, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("foreign string length is missing"))?;
                let length = self.builder.ins().uextend(types::I64, length);
                let slot = self.stack_slot(16, 8);
                self.builder.ins().store(flags(), data, slot, 0);
                self.builder.ins().store(flags(), length, slot, 8);
                self.push_boundary_aggregate(
                    signature,
                    arguments,
                    slot,
                    16,
                    8,
                    &BoundaryLeaves::descriptor(),
                )
            }
            Type::Array(element) => {
                let (data, length) =
                    array_snapshot.ok_or_else(|| internal("foreign array snapshot is missing"))?;
                let count = self.builder.ins().uextend(types::I64, length);
                let data = match &**element {
                    Type::Class(class)
                        if self.is_value_class(element)
                            && boundary_class_requires_build(self.ml.lir, *class)? =>
                    {
                        self.marshal_boundary_array(
                            class.0,
                            data,
                            length,
                            scratch_mark.ok_or_else(|| {
                                internal("recursive boundary array has no scratch scope")
                            })?,
                            pos,
                        )?
                    }
                    _ => data,
                };
                match &parameter.foreign_provenance {
                    Some(l::ForeignTypeProvenance::Descriptor { .. }) => {
                        let slot = self.stack_slot(16, 8);
                        self.builder.ins().store(flags(), data, slot, 0);
                        self.builder.ins().store(flags(), count, slot, 8);
                        self.push_boundary_aggregate(
                            signature,
                            arguments,
                            slot,
                            16,
                            8,
                            &BoundaryLeaves::descriptor(),
                        )
                    }
                    Some(l::ForeignTypeProvenance::ScalarPair { .. }) => {
                        self.push_foreign_argument(signature, arguments, types::I64, count);
                        self.push_foreign_argument(signature, arguments, types::I64, data);
                        Ok(())
                    }
                    provenance => Err(internal(format!(
                        "foreign array parameter `{}` has incompatible provenance {provenance:?}",
                        parameter.source_name
                    ))),
                }
            }
            Type::Class(class) if self.is_value_class(&parameter.ty) => {
                let address = self.expect_aggregate(value)?;
                self.marshal_boundary_struct(
                    class.0,
                    address,
                    signature,
                    arguments,
                    scratch_mark,
                    pos,
                )
            }
            ty if self.boundary_pointer_class(ty).is_some() => {
                let source = self.boundary_pointer_value(value)?;
                let class = self
                    .boundary_pointer_class(ty)
                    .ok_or_else(|| internal("boundary pointer class is missing"))?;
                if !self
                    .ml
                    .lir
                    .classes
                    .get(class)
                    .is_some_and(|class| class.is_embedded_header)
                    && boundary_class_needs_scratch(self.ml.lir, ClassId(class))?
                {
                    let (pointer, writeback) =
                        self.marshal_boundary_pointer(class, source, scratch_mark, pos)?;
                    self.push_foreign_argument(signature, arguments, types::I64, pointer);
                    writebacks.push(writeback);
                } else {
                    self.push_foreign_argument(signature, arguments, types::I64, source);
                }
                Ok(())
            }
            ty => match self.ml.layouts.repr(ty)? {
                Repr::None => Ok(()),
                Repr::Scalar(repr) => {
                    let value = self.expect_scalar(value)?;
                    self.push_foreign_argument(signature, arguments, repr, value);
                    Ok(())
                }
                other => Err(internal(format!(
                    "foreign parameter `{}` has representation {other:?}",
                    parameter.source_name
                ))),
            },
        }
    }

    fn stabilize_boundary_return_value(
        &mut self,
        class: usize,
        source: Value,
    ) -> Result<(), String> {
        self.stabilize_boundary_return_value_inner(class, source, &mut HashSet::new())
    }

    fn stabilize_boundary_return_value_inner(
        &mut self,
        class: usize,
        source: Value,
        visiting: &mut HashSet<usize>,
    ) -> Result<(), String> {
        if !visiting.insert(class) {
            return Ok(());
        }
        let definition = self
            .ml
            .lir
            .classes
            .get(class)
            .cloned()
            .ok_or_else(|| internal(format!("boundary return class {class} is missing")))?;
        let layout = self.ml.layouts.class(class)?.clone();
        for (index, field) in definition.fields.iter().enumerate() {
            let offset = *layout
                .field_offsets
                .get(index)
                .ok_or_else(|| internal("boundary return field offset is missing"))?
                as i32;
            if self.boundary_pointer_class(&field.ty).is_some() {
                // The field already owns a Context-managed box. Its payload
                // does not depend on the returning activation.
                continue;
            }
            if let Type::Class(inner) = &field.ty {
                if self.is_value_class(&field.ty) {
                    let nested = self.address_offset(source, i64::from(offset));
                    self.stabilize_boundary_return_value_inner(inner.0, nested, visiting)?;
                    continue;
                }
            }
            if let Type::Array(element) = &field.ty {
                let Type::Class(element_class) = &**element else {
                    continue;
                };
                if !self.is_value_class(element) {
                    continue;
                }
                let handle = self.builder.ins().load(types::I64, flags(), source, offset);
                let length = self
                    .call_runtime(self.ml.rt.array_len, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("boundary return array length has no result"))?;
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("boundary return array data has no result"))?;
                let stride = self.ml.layouts.stride(element)?;
                let condition = self.builder.create_block();
                let body = self.builder.create_block();
                let done = self.builder.create_block();
                self.builder.append_block_param(condition, types::I32);
                let zero = self.iconst(types::I32, 0);
                self.builder.ins().jump(condition, &[BlockArg::Value(zero)]);
                self.builder.switch_to_block(condition);
                let item = self.builder.block_params(condition)[0];
                let more = self.builder.ins().icmp(IntCC::SignedLessThan, item, length);
                self.builder.ins().brif(more, body, &[], done, &[]);
                self.builder.switch_to_block(body);
                let item64 = self.builder.ins().uextend(types::I64, item);
                let byte_offset = self.builder.ins().imul_imm(item64, i64::from(stride));
                let element_address = self.builder.ins().iadd(data, byte_offset);
                self.stabilize_boundary_return_value_inner(
                    element_class.0,
                    element_address,
                    visiting,
                )?;
                let next = self.builder.ins().iadd_imm(item, 1);
                self.builder.ins().jump(condition, &[BlockArg::Value(next)]);
                self.builder.switch_to_block(done);
            }
        }
        visiting.remove(&class);
        Ok(())
    }

    fn marshal_boundary_pointer(
        &mut self,
        class: usize,
        source: Value,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(Value, BoundaryPtrWriteback), String> {
        let (_, size, align) = self.boundary_c_layout(class)?;
        let scratch = self.stack_slot(size, align);
        self.zero_bytes(scratch, size, align);
        let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, source, 0);
        let populate = self.builder.create_block();
        let ready = self.builder.create_block();
        self.builder.ins().brif(nonnull, populate, &[], ready, &[]);
        self.builder.switch_to_block(populate);
        self.populate_boundary_value(class, source, scratch, scratch_mark, pos)?;
        self.builder.ins().jump(ready, &[]);
        self.builder.switch_to_block(ready);
        let null = self.iconst(types::I64, 0);
        let pointer = self.builder.ins().select(nonnull, scratch, null);
        Ok((
            pointer,
            BoundaryPtrWriteback {
                class,
                source,
                scratch,
            },
        ))
    }

    fn populate_boundary_value(
        &mut self,
        class: usize,
        source: Value,
        destination: Value,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(), String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(class)
            .cloned()
            .ok_or_else(|| internal(format!("boundary class {class} is missing")))?;
        let language_layout = self.ml.layouts.class(class)?.clone();
        let (c_offsets, _, _) = self.boundary_c_layout(class)?;
        let mut index = 0usize;
        while index < definition.fields.len() {
            let field = &definition.fields[index];
            let language_offset = language_layout.field_offsets[index] as i32;
            let c_offset = c_offsets[index] as i32;
            match &field.ty {
                Type::Func(_) => {
                    let code =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    let environment =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset + 8);
                    let trampoline = self
                        .ml
                        .module
                        .declare_func_in_func(self.ml.rt.cb_trampoline, self.builder.func);
                    let trampoline = self.builder.ins().func_addr(types::I64, trampoline);
                    self.builder
                        .ins()
                        .store(flags(), trampoline, destination, c_offset);
                    let first = definition
                        .fields
                        .get(index + 1)
                        .ok_or_else(|| internal("boundary callback has no userdata field"))?;
                    let first_offset = language_layout.field_offsets[index + 1] as i32;
                    let userdata =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, first_offset);
                    let has_second = definition
                        .fields
                        .get(index + 2)
                        .is_some_and(|field| is_userdata_slot(&field.ty));
                    let userdata2 = if has_second {
                        let offset = language_layout.field_offsets[index + 2] as i32;
                        self.builder.ins().load(types::I64, flags(), source, offset)
                    } else {
                        self.iconst(types::I64, 0)
                    };
                    let binding = self
                        .call_runtime(
                            self.ml.rt.cb_bind,
                            &[self.ctx, code, environment, userdata, userdata2],
                            false,
                        )?
                        .ok_or_else(|| internal("callback binding has no result"))?;
                    self.builder.ins().store(
                        flags(),
                        binding,
                        destination,
                        c_offsets[index + 1] as i32,
                    );
                    if has_second {
                        let zero = self.iconst(types::I64, 0);
                        self.builder.ins().store(
                            flags(),
                            zero,
                            destination,
                            c_offsets[index + 2] as i32,
                        );
                        index += 3;
                    } else {
                        let _ = first;
                        index += 2;
                    }
                }
                Type::Str => {
                    let handle =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    let data = self
                        .call_runtime(self.ml.rt.str_data, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary string data is missing"))?;
                    let length = self
                        .call_runtime(self.ml.rt.str_len, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary string length is missing"))?;
                    let length = self.builder.ins().uextend(types::I64, length);
                    self.builder
                        .ins()
                        .store(flags(), data, destination, c_offset);
                    self.builder
                        .ins()
                        .store(flags(), length, destination, c_offset + 8);
                    index += 1;
                }
                Type::Array(element) => {
                    let handle =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    let length = self
                        .call_runtime(self.ml.rt.array_len, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary array length is missing"))?;
                    let count = self.builder.ins().uextend(types::I64, length);
                    let source_data = self
                        .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                        .ok_or_else(|| internal("boundary array data is missing"))?;
                    let data = match &**element {
                        Type::Class(element_class)
                            if self.is_value_class(element)
                                && boundary_class_requires_build(self.ml.lir, *element_class)? =>
                        {
                            self.marshal_boundary_array(
                                element_class.0,
                                source_data,
                                length,
                                scratch_mark.ok_or_else(|| {
                                    internal("recursive boundary array has no scratch scope")
                                })?,
                                pos,
                            )?
                        }
                        _ => source_data,
                    };
                    self.builder
                        .ins()
                        .store(flags(), count, destination, c_offset);
                    self.builder
                        .ins()
                        .store(flags(), data, destination, c_offset + 8);
                    index += 1;
                }
                ty if self.boundary_pointer_class(ty).is_some() => {
                    let child_class = self
                        .boundary_pointer_class(ty)
                        .ok_or_else(|| internal("boundary child class is missing"))?;
                    let source_pointer =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), source, language_offset);
                    if self
                        .ml
                        .lir
                        .classes
                        .get(child_class)
                        .is_some_and(|class| class.is_embedded_header)
                    {
                        self.builder
                            .ins()
                            .store(flags(), source_pointer, destination, c_offset);
                        index += 1;
                        continue;
                    }
                    let zero = self.iconst(types::I64, 0);
                    self.builder
                        .ins()
                        .store(flags(), zero, destination, c_offset);
                    let nonnull = self
                        .builder
                        .ins()
                        .icmp_imm(IntCC::NotEqual, source_pointer, 0);
                    let populate = self.builder.create_block();
                    let ready = self.builder.create_block();
                    self.builder.ins().brif(nonnull, populate, &[], ready, &[]);
                    self.builder.switch_to_block(populate);
                    let (_, child_size, _) = self.boundary_c_layout(child_class)?;
                    let bytes = self.iconst(types::I64, i64::from(child_size));
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    let child = self
                        .call_runtime(
                            self.ml.rt.boundary_scratch_alloc,
                            &[self.ctx, bytes, position],
                            false,
                        )?
                        .ok_or_else(|| internal("boundary child scratch is missing"))?;
                    self.trap_check();
                    self.populate_boundary_value(
                        child_class,
                        source_pointer,
                        child,
                        scratch_mark,
                        pos,
                    )?;
                    self.builder
                        .ins()
                        .store(flags(), child, destination, c_offset);
                    self.builder.ins().jump(ready, &[]);
                    self.builder.switch_to_block(ready);
                    index += 1;
                }
                Type::Class(inner) if self.is_value_class(&field.ty) => {
                    let source = self.address_offset(source, i64::from(language_offset));
                    let destination = self.address_offset(destination, i64::from(c_offset));
                    if boundary_class_requires_build(self.ml.lir, *inner)? {
                        self.populate_boundary_value(
                            inner.0,
                            source,
                            destination,
                            scratch_mark,
                            pos,
                        )?;
                    } else {
                        let layout = self.ml.layouts.class(inner.0)?.clone();
                        self.copy_bytes(destination, source, layout.size, layout.align);
                    }
                    index += 1;
                }
                ty => {
                    let value = self.load_data(ty, source, language_offset)?;
                    let value = self.expect_scalar(value)?;
                    self.builder
                        .ins()
                        .store(flags(), value, destination, c_offset);
                    index += 1;
                }
            }
        }
        Ok(())
    }

    fn marshal_boundary_array(
        &mut self,
        element_class: usize,
        source: Value,
        length: Value,
        _scratch_mark: Value,
        pos: &Pos,
    ) -> Result<Value, String> {
        let language_layout = self.ml.layouts.class(element_class)?.clone();
        let (_, c_size, _) = self.boundary_c_layout(element_class)?;
        let length64 = self.builder.ins().uextend(types::I64, length);
        let bytes = self.builder.ins().imul_imm(length64, i64::from(c_size));
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        let scratch = self
            .call_runtime(
                self.ml.rt.boundary_scratch_alloc,
                &[self.ctx, bytes, position],
                false,
            )?
            .ok_or_else(|| internal("boundary array scratch is missing"))?;
        self.trap_check();
        let condition = self.builder.create_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(condition, types::I32);
        let zero = self.iconst(types::I32, 0);
        self.builder.ins().jump(condition, &[BlockArg::Value(zero)]);
        self.builder.switch_to_block(condition);
        let index = self.builder.block_params(condition)[0];
        let more = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThan, index, length);
        self.builder.ins().brif(more, body, &[], done, &[]);
        self.builder.switch_to_block(body);
        let index64 = self.builder.ins().uextend(types::I64, index);
        let source_offset = self
            .builder
            .ins()
            .imul_imm(index64, i64::from(language_layout.size));
        let destination_offset = self.builder.ins().imul_imm(index64, i64::from(c_size));
        let source_element = self.builder.ins().iadd(source, source_offset);
        let destination_element = self.builder.ins().iadd(scratch, destination_offset);
        self.populate_boundary_value(
            element_class,
            source_element,
            destination_element,
            Some(_scratch_mark),
            pos,
        )?;
        let next = self.builder.ins().iadd_imm(index, 1);
        self.builder.ins().jump(condition, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(done);
        Ok(scratch)
    }

    fn write_back_boundary_pointer(
        &mut self,
        writeback: BoundaryPtrWriteback,
        pos: &Pos,
    ) -> Result<(), String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(writeback.class)
            .cloned()
            .ok_or_else(|| internal(format!("boundary class {} is missing", writeback.class)))?;
        let language_layout = self.ml.layouts.class(writeback.class)?.clone();
        let (c_offsets, _, _) = self.boundary_c_layout(writeback.class)?;
        let nonnull = self
            .builder
            .ins()
            .icmp_imm(IntCC::NotEqual, writeback.source, 0);
        let copy = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.ins().brif(nonnull, copy, &[], done, &[]);
        self.builder.switch_to_block(copy);
        for (index, field) in definition.fields.iter().enumerate() {
            let language_offset = language_layout.field_offsets[index] as i32;
            let c_offset = c_offsets[index] as i32;
            match &field.ty {
                Type::Str => {
                    let data =
                        self.builder
                            .ins()
                            .load(types::I64, flags(), writeback.scratch, c_offset);
                    let length = self.builder.ins().load(
                        types::I64,
                        flags(),
                        writeback.scratch,
                        c_offset + 8,
                    );
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    let handle = self
                        .call_runtime(
                            self.ml.rt.str_from_view,
                            &[self.ctx, data, length, position],
                            false,
                        )?
                        .ok_or_else(|| internal("boundary string writeback has no result"))?;
                    self.builder
                        .ins()
                        .store(flags(), handle, writeback.source, language_offset);
                    self.trap_check();
                }
                Type::Array(_) | Type::Nullable(_) | Type::Func(_) => {}
                Type::Class(inner) if self.is_value_class(&field.ty) => {
                    if !boundary_class_requires_build(self.ml.lir, *inner)? {
                        let layout = self.ml.layouts.class(inner.0)?.clone();
                        let source = self.address_offset(writeback.scratch, i64::from(c_offset));
                        let destination =
                            self.address_offset(writeback.source, i64::from(language_offset));
                        self.copy_bytes(destination, source, layout.size, layout.align);
                    }
                }
                ty => {
                    let Repr::Scalar(repr) = self.ml.layouts.repr(ty)? else {
                        return Err(internal(format!(
                            "boundary field `{}` cannot be written back",
                            field.source_name
                        )));
                    };
                    let value = self
                        .builder
                        .ins()
                        .load(repr, flags(), writeback.scratch, c_offset);
                    self.store_data(ty, writeback.source, language_offset, RV::Scalar(value))?;
                }
            }
        }
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }

    fn marshal_boundary_struct(
        &mut self,
        class: usize,
        source: Value,
        signature: &mut Signature,
        arguments: &mut Vec<Value>,
        scratch_mark: Option<Value>,
        pos: &Pos,
    ) -> Result<(), String> {
        let (_, size, align) = self.boundary_c_layout(class)?;
        let scratch = self.stack_slot(size, align);
        self.zero_bytes(scratch, size, align);
        self.populate_boundary_value(class, source, scratch, scratch_mark, pos)?;
        let components = self.boundary_leaf_components(class)?;
        self.push_boundary_aggregate(signature, arguments, scratch, size, align, &components)
    }

    fn call(
        &mut self,
        target: &l::CallTarget,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        if matches!(target.kind, l::CallTargetKind::Method(_)) {
            let receiver = self.expect_scalar(
                *operands
                    .first()
                    .ok_or_else(|| internal("method call has no receiver"))?,
            )?;
            for trap in traps {
                if trap.kind == l::TrapKind::DevOnlyLifetime {
                    self.emit_trap(trap, TrapOperand::Value(receiver))?;
                }
            }
        }
        let result = match &target.kind {
            l::CallTargetKind::Function(function) => self.script_call(
                *function,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                false,
            )?,
            l::CallTargetKind::StaticClosure(function) => self.static_closure_call(
                *function,
                operands,
                parameter_types,
                target.return_type.as_ref(),
            )?,
            l::CallTargetKind::Method(method) => self.script_call(
                self.method_function(*method)?,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                true,
            )?,
            l::CallTargetKind::Indirect => {
                self.indirect_call(operands, parameter_types, target.return_type.as_ref())?
            }
            l::CallTargetKind::Foreign(function) => self.foreign_call(
                *function,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                traps,
                pos,
            )?,
            l::CallTargetKind::Intrinsic(intrinsic) => self.intrinsic_call(
                intrinsic,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                traps,
                pos,
            )?,
            l::CallTargetKind::BuiltinMethod(method) => self.builtin_call(
                *method,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                traps,
                pos,
            )?,
        };
        if matches!(
            target.kind,
            l::CallTargetKind::Function(_)
                | l::CallTargetKind::StaticClosure(_)
                | l::CallTargetKind::Method(_)
                | l::CallTargetKind::Indirect
        ) {
            for trap in traps {
                if trap.kind == l::TrapKind::Call {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
            }
        }
        Ok(result)
    }

    fn emit_instruction(&mut self, instruction: &l::Instruction) -> Result<(), String> {
        let operands = self.instruction_operands(instruction)?;
        let operand_types = self.instruction_operand_types(instruction)?;
        let result_ty = self.result_type(instruction)?;
        let result = match &instruction.kind {
            l::InstructionKind::Copy => Some(
                self.clone_value(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Copy has no operand"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("Copy has no operand type"))?,
                )?,
            ),
            l::InstructionKind::StringLiteral(text) => {
                Some(self.string_literal(text, &instruction.traps, &instruction.pos)?)
            }
            l::InstructionKind::LoadLocal(local) => {
                let slot = self
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} is missing", local.0)))?;
                let ty = self
                    .function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} has no type", local.0)))?
                    .ty
                    .clone();
                let value = self.load_value_type(&ty, slot.address, 0)?;
                Some(self.clone_value(value, &ty)?)
            }
            l::InstructionKind::StoreLocal(local) => {
                let address = self
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} is missing", local.0)))?
                    .address;
                let ty = self
                    .function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} has no type", local.0)))?
                    .ty
                    .clone();
                self.store_value_type(
                    &ty,
                    address,
                    0,
                    *operands
                        .first()
                        .ok_or_else(|| internal("StoreLocal has no value"))?,
                )?;
                None
            }
            l::InstructionKind::AddressOfLocal(local) => Some(RV::Scalar(
                self.locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} is missing", local.0)))?
                    .address,
            )),
            l::InstructionKind::LoadGlobal(global) => {
                let (address, ty) = self.global_address(*global)?;
                let value = self.load_data(&ty, address, 0)?;
                Some(self.clone_value(value, &l::ValueType::Data(ty))?)
            }
            l::InstructionKind::StoreGlobal(global) => {
                let (address, ty) = self.global_address(*global)?;
                self.store_data(
                    &ty,
                    address,
                    0,
                    *operands
                        .first()
                        .ok_or_else(|| internal("StoreGlobal has no value"))?,
                )?;
                None
            }
            l::InstructionKind::AddressOfGlobal(global) => {
                let (address, _) = self.global_address(*global)?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::FunctionRef(function) => Some(self.function_reference(*function)?),
            l::InstructionKind::Unary(operator) => {
                let ty = data_type(
                    operand_types
                        .first()
                        .ok_or_else(|| internal("unary operand type is missing"))?,
                )?;
                Some(
                    self.unary(
                        *operator,
                        *operands
                            .first()
                            .ok_or_else(|| internal("unary operand is missing"))?,
                        ty,
                    )?,
                )
            }
            l::InstructionKind::Binary(operator) => {
                let ty = data_type(
                    operand_types
                        .first()
                        .ok_or_else(|| internal("binary operand type is missing"))?,
                )?;
                Some(
                    self.binary(
                        *operator,
                        *operands
                            .first()
                            .ok_or_else(|| internal("binary lhs is missing"))?,
                        *operands
                            .get(1)
                            .ok_or_else(|| internal("binary rhs is missing"))?,
                        ty,
                        &instruction.traps,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::Cast | l::InstructionKind::Coerce => {
                if matches!(
                    (operand_types.first(), result_ty.as_ref()),
                    (
                        Some(l::ValueType::Data(Type::Nullable(source))),
                        Some(l::ValueType::Data(Type::Class(target)))
                    ) if matches!(source.as_ref(), Type::Class(source)
                        if source == target && self.is_value_class(&Type::Class(*target)))
                ) {
                    let pointer = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("conversion operand is missing"))?,
                    )?;
                    Some(
                        self.clone_value(
                            RV::Aggregate(pointer),
                            result_ty
                                .as_ref()
                                .ok_or_else(|| internal("conversion result type is missing"))?,
                        )?,
                    )
                } else {
                    let source = data_type(
                        operand_types
                            .first()
                            .ok_or_else(|| internal("conversion source type is missing"))?,
                    )?;
                    let target = data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("conversion result type is missing"))?,
                    )?;
                    Some(
                        self.convert(
                            *operands
                                .first()
                                .ok_or_else(|| internal("conversion operand is missing"))?,
                            source,
                            target,
                            &instruction.traps,
                        )?,
                    )
                }
            }
            l::InstructionKind::AllocateClass(class) => {
                let stable_address = instruction.result.and_then(|result| {
                    self.stable_addresses
                        .get(&result)
                        .copied()
                        .zip(self.frame)
                        .map(|(offset, frame)| self.address_offset(frame, i64::from(offset)))
                });
                let value = self.allocate_class(
                    *class,
                    stable_address,
                    &instruction.traps,
                    &instruction.pos,
                )?;
                Some(match (result_ty.as_ref(), value) {
                    (Some(l::ValueType::Address(_)), RV::Aggregate(address)) => RV::Scalar(address),
                    (_, value) => value,
                })
            }
            l::InstructionKind::BoxBoundaryValue { payload } => Some(
                self.box_boundary_value(
                    *operands
                        .first()
                        .ok_or_else(|| internal("BoxBoundaryValue operand is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("BoxBoundaryValue type is missing"))?,
                    *payload,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::AddressOfValue => {
                let ty = data_type(
                    operand_types
                        .first()
                        .ok_or_else(|| internal("AddressOfValue type is missing"))?,
                )?;
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let stable = instruction
                    .result
                    .and_then(|result| self.stable_addresses.get(&result).copied())
                    .zip(self.frame);
                let address = if let Some((offset, frame)) = stable {
                    self.address_offset(frame, i64::from(offset))
                } else {
                    self.stack_slot(size.max(1), align.max(1))
                };
                self.store_data(
                    ty,
                    address,
                    0,
                    *operands
                        .first()
                        .ok_or_else(|| internal("AddressOfValue operand is missing"))?,
                )?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::AddressOfField(field) => {
                let (address, _) = self.field_address(
                    *field,
                    *operands
                        .first()
                        .ok_or_else(|| internal("field base is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("field base type is missing"))?,
                    &instruction.traps,
                )?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::AddressOfIndex { checked } => {
                let index = self.expect_scalar(
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("index is missing"))?,
                )?;
                let index_ty = data_type(
                    operand_types
                        .get(1)
                        .ok_or_else(|| internal("index type is missing"))?,
                )?;
                let (address, _) = self.index_address(
                    *operands
                        .first()
                        .ok_or_else(|| internal("indexed base is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("indexed base type is missing"))?,
                    index,
                    index_ty,
                    *checked,
                    &instruction.traps,
                )?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::LoadAddress => {
                let address = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("load address is missing"))?,
                )?;
                let ty = data_type(
                    result_ty
                        .as_ref()
                        .ok_or_else(|| internal("load result type is missing"))?,
                )?;
                let value = self.load_data(ty, address, 0)?;
                Some(self.clone_value(value, &l::ValueType::Data(ty.clone()))?)
            }
            l::InstructionKind::StoreAddress => {
                let address = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("store address is missing"))?,
                )?;
                let l::ValueType::Address(address_ty) = operand_types
                    .first()
                    .ok_or_else(|| internal("store address type is missing"))?
                else {
                    return Err(internal("StoreAddress operand is not an address"));
                };
                self.store_data(
                    &address_ty.pointee,
                    address,
                    0,
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("stored value is missing"))?,
                )?;
                None
            }
            l::InstructionKind::LoadField(field) => {
                let (address, ty) = self.field_address(
                    *field,
                    *operands
                        .first()
                        .ok_or_else(|| internal("field base is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("field base type is missing"))?,
                    &instruction.traps,
                )?;
                self.guard_json_result_value(
                    *field,
                    *operands
                        .first()
                        .ok_or_else(|| internal("field base is missing"))?,
                    &instruction.traps,
                )?;
                let value = self.load_data(&ty, address, 0)?;
                self.validate_wire_alias_traps(&ty, value, &instruction.traps)?;
                Some(self.clone_value(value, &l::ValueType::Data(ty))?)
            }
            l::InstructionKind::Length => Some(
                self.length(
                    *operands
                        .first()
                        .ok_or_else(|| internal("length operand is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("length operand type is missing"))?,
                )?,
            ),
            l::InstructionKind::ForeignArrayData => {
                let handle = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("foreign array data has no array operand"))?,
                )?;
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("foreign array data snapshot has no result"))?;
                Some(RV::Scalar(data))
            }
            l::InstructionKind::ArrayLiteral => Some(
                self.array_literal(
                    data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("array result type is missing"))?,
                    )?,
                    &operands,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::ArrayWithCapacity => Some(
                self.array_with_capacity(
                    data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("capacity array result type is missing"))?,
                    )?,
                    *operands
                        .first()
                        .ok_or_else(|| internal("capacity array bound is missing"))?,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::ArraySpreadLiteral(spreads) => Some(
                self.spread_array_literal(
                    data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("spread result type is missing"))?,
                    )?,
                    spreads,
                    &operands,
                    &operand_types,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::Template(parts) => {
                Some(self.template(parts, &operands, &instruction.traps, &instruction.pos)?)
            }
            l::InstructionKind::MakeClosure(function) => {
                Some(self.make_closure(*function, &operands)?)
            }
            l::InstructionKind::Call(target) => Some(self.call(
                target,
                &operands,
                &operand_types,
                &instruction.traps,
                &instruction.pos,
            )?),
            l::InstructionKind::AsyncHandleCreate(target) => Some(RV::Scalar(
                self.create_async_child_from_values(target, &operands, &instruction.traps)?,
            )),
            l::InstructionKind::AsyncHandleRetain => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async retain has no handle"))?,
                )?;
                self.call_runtime(self.ml.rt.async_retain, &[self.ctx, frame], false)?;
                None
            }
            l::InstructionKind::AsyncHandleRelease => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async release has no handle"))?,
                )?;
                let pos = self.position_id(&instruction.pos);
                let pos = self.iconst(types::I32, pos);
                self.call_runtime(self.ml.rt.async_release, &[self.ctx, frame, pos], false)?;
                None
            }
            l::InstructionKind::AsyncHandleArrayRetain => {
                let array = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async array retain has no array"))?,
                )?;
                self.call_runtime(self.ml.rt.async_retain_array, &[self.ctx, array], false)?;
                None
            }
            l::InstructionKind::AsyncHandleArrayRelease => {
                let array = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async array release has no array"))?,
                )?;
                let pos = self.position_id(&instruction.pos);
                let pos = self.iconst(types::I32, pos);
                self.call_runtime(
                    self.ml.rt.async_release_array,
                    &[self.ctx, array, pos],
                    false,
                )?;
                None
            }
            l::InstructionKind::IteratorCreate { kind, bound } => {
                let iterator_ty = match result_ty.as_ref() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorCreate has no iterator result type")),
                };
                Some(
                    self.iterator_create(
                        *kind,
                        *bound,
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator subject is missing"))?,
                        operand_types
                            .first()
                            .ok_or_else(|| internal("iterator subject type is missing"))?,
                        iterator_ty,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorHasNext => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorHasNext has no iterator type")),
                };
                Some(
                    self.iterator_has_next(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        self.expect_scalar(
                            *operands
                                .get(1)
                                .ok_or_else(|| internal("iterator index is missing"))?,
                        )?,
                        self.expect_scalar(
                            *operands
                                .get(2)
                                .ok_or_else(|| internal("iterator bound is missing"))?,
                        )?,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorValue => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorValue has no iterator type")),
                };
                Some(
                    self.iterator_value(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        self.expect_scalar(
                            *operands
                                .get(1)
                                .ok_or_else(|| internal("iterator index is missing"))?,
                        )?,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorBound => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorBound has no iterator type")),
                };
                Some(
                    self.iterator_bound(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorAdvance => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorAdvance has no iterator type")),
                };
                Some(
                    self.iterator_advance(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        self.expect_scalar(
                            *operands
                                .get(2)
                                .ok_or_else(|| internal("iterator bound is missing"))?,
                        )?,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::Zero => Some(
                self.zero(data_type(
                    result_ty
                        .as_ref()
                        .ok_or_else(|| internal("Zero result type is missing"))?,
                )?)?,
            ),
        };
        match (instruction.result, result) {
            (Some(id), Some(value)) => self.set_value(id, value),
            (None, None) => Ok(()),
            (Some(_), None) => Err(internal(format!(
                "{:?} declared a result but produced none",
                instruction.kind
            ))),
            (None, Some(RV::None)) => Ok(()),
            (None, Some(value)) => Err(internal(format!(
                "{:?} produced undeclared value {value:?}",
                instruction.kind
            ))),
        }
    }

    fn branch_arguments(&mut self, arguments: &[l::Operand]) -> Result<Vec<BlockArg>, String> {
        let mut result = Vec::new();
        for argument in arguments {
            result.extend(rv_args(self.operand(argument)?));
        }
        Ok(result)
    }

    fn emit_return(&mut self, value: Option<&l::Operand>, _pos: &Pos) -> Result<(), String> {
        if let Some(kind) = self.coroutine {
            let frame = self
                .frame
                .ok_or_else(|| internal("coroutine return has no frame"))?;
            if kind == CoroutineKind::Async {
                if let Some(value) = value {
                    let value = self.operand(value)?;
                    let output = self
                        .out
                        .ok_or_else(|| internal("async return has no output"))?;
                    let return_type = self.function.return_type.clone();
                    self.store_data(&return_type, output, 0, value)?;
                }
            }
            let done = self.iconst(types::I32, COROUTINE_DONE);
            self.builder.ins().store(flags(), done, frame, 0);
            self.pop_shadow()?;
            let one = self.iconst(types::I8, 1);
            self.builder.ins().return_(&[one]);
            return Ok(());
        }
        let value = value.map(|value| self.operand(value)).transpose()?;
        let returns = match (self.ml.layouts.repr(&self.function.return_type)?, value) {
            (Repr::None, _) => Vec::new(),
            (Repr::Scalar(_), Some(RV::Scalar(value))) => vec![value],
            (Repr::Pair, Some(RV::Pair(code, env))) => vec![code, env],
            (Repr::Agg { size, align }, Some(RV::Aggregate(source))) => {
                let destination = self
                    .sret
                    .ok_or_else(|| internal("aggregate return has no sret"))?;
                self.copy_bytes(destination, source, size, align);
                if let Type::Class(class) = &self.function.return_type {
                    if self.is_value_class(&Type::Class(*class))
                        && boundary_class_contains_pointer(self.ml.lir, *class)?
                    {
                        self.stabilize_boundary_return_value(class.0, destination)?;
                    }
                }
                Vec::new()
            }
            (repr, value) => {
                return Err(internal(format!("return mismatch {repr:?} and {value:?}")))
            }
        };
        self.pop_shadow()?;
        self.builder.ins().return_(&returns);
        Ok(())
    }

    fn emit_terminator(
        &mut self,
        block: l::BlockId,
        terminator: &l::Terminator,
    ) -> Result<(), String> {
        match terminator {
            l::Terminator::Branch(target) => {
                let destination = self.blocks[target.block.0 as usize];
                let arguments = self.branch_arguments(&target.arguments)?;
                self.builder.ins().jump(destination, &arguments);
            }
            l::Terminator::ConditionalBranch {
                condition,
                then_target,
                else_target,
            } => {
                let condition_value = self.operand(condition)?;
                let condition = self.expect_scalar(condition_value)?;
                let then_block = self.blocks[then_target.block.0 as usize];
                let else_block = self.blocks[else_target.block.0 as usize];
                let then_arguments = self.branch_arguments(&then_target.arguments)?;
                let else_arguments = self.branch_arguments(&else_target.arguments)?;
                self.builder.ins().brif(
                    condition,
                    then_block,
                    &then_arguments,
                    else_block,
                    &else_arguments,
                );
            }
            l::Terminator::Switch {
                value,
                arms,
                default,
            } => {
                let switch_value = self.operand(value)?;
                let value = self.expect_scalar(switch_value)?;
                let mut next = None;
                for arm in arms {
                    if let Some(block) = next {
                        self.builder.switch_to_block(block);
                    }
                    let case_value = self.constant(&arm.value)?;
                    let case = self.expect_scalar(case_value)?;
                    let matches = self.builder.ins().icmp(IntCC::Equal, value, case);
                    let otherwise = self.builder.create_block();
                    let destination = self.blocks[arm.target.block.0 as usize];
                    let arguments = self.branch_arguments(&arm.target.arguments)?;
                    self.builder
                        .ins()
                        .brif(matches, destination, &arguments, otherwise, &[]);
                    next = Some(otherwise);
                }
                if let Some(block) = next {
                    self.builder.switch_to_block(block);
                }
                let destination = self.blocks[default.block.0 as usize];
                let arguments = self.branch_arguments(&default.arguments)?;
                self.builder.ins().jump(destination, &arguments);
            }
            l::Terminator::Return { value, pos } => self.emit_return(value.as_ref(), pos)?,
            l::Terminator::Unreachable { .. } => {
                let unwind = self.unwind_block();
                self.builder.ins().jump(unwind, &[]);
            }
            l::Terminator::Trap(trap) => {
                self.emit_trap(trap, TrapOperand::Pending)?;
                let unwind = self.unwind_block();
                self.builder.ins().jump(unwind, &[]);
            }
            l::Terminator::Suspend { .. } => self.emit_suspend(block, terminator)?,
        }
        Ok(())
    }

    fn emit_suspend(
        &mut self,
        block: l::BlockId,
        terminator: &l::Terminator,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            kind,
            arguments,
            traps,
            pos,
            ..
        } = terminator
        else {
            return Err(internal("non-suspend passed to suspend transcriber"));
        };
        let frame = self
            .frame
            .ok_or_else(|| internal("suspension has no frame"))?;
        let plan = self
            .suspend_plans
            .get(&block)
            .ok_or_else(|| internal(format!("suspend block {} has no frame plan", block.0)))?
            .clone();
        for (argument, slot) in arguments.iter().zip(&plan.arguments) {
            let value = self.operand(argument)?;
            self.store_value_type(&slot.ty, frame, slot.offset as i32, value)?;
        }
        match kind {
            l::SuspendKind::Yield(value) => {
                if let Some(value_id) = value {
                    let value = self.value(*value_id)?;
                    let ty = data_type(self.value_type(*value_id)?)?.clone();
                    let output = self.out.ok_or_else(|| internal("yield has no output"))?;
                    self.store_data(&ty, output, 0, value)?;
                }
            }
            l::SuspendKind::Async => {}
            l::SuspendKind::AsyncCall { target, operands } => {
                let child = self.create_async_child(target, operands, traps)?;
                let child_offset = plan
                    .child
                    .ok_or_else(|| internal("async call has no child-frame slot"))?;
                self.builder
                    .ins()
                    .store(flags(), child, frame, child_offset as i32);
                return self.resume_async_child(block, target, child, &plan);
            }
            l::SuspendKind::AsyncHandle { handle } => {
                let handle_value = self.value(*handle)?;
                let handle = self.expect_scalar(handle_value)?;
                let handle_offset = plan
                    .child
                    .ok_or_else(|| internal("held await has no handle-frame slot"))?;
                self.builder
                    .ins()
                    .store(flags(), handle, frame, handle_offset as i32);
                return self.resume_async_handle(block, handle, &plan, traps, true);
            }
        }
        let state = self.iconst(types::I32, plan.state);
        self.builder.ins().store(flags(), state, frame, 0);
        self.pop_shadow()?;
        let zero = self.iconst(types::I8, 0);
        self.builder.ins().return_(&[zero]);
        let _ = pos;
        Ok(())
    }

    fn create_async_child(
        &mut self,
        target: &l::CallTarget,
        operand_ids: &[l::ValueId],
        traps: &[l::Trap],
    ) -> Result<Value, String> {
        let operands = operand_ids
            .iter()
            .map(|id| self.value(*id))
            .collect::<Result<Vec<_>, _>>()?;
        self.create_async_child_from_values(target, &operands, traps)
    }

    fn create_async_child_from_values(
        &mut self,
        target: &l::CallTarget,
        operands: &[RV],
        traps: &[l::Trap],
    ) -> Result<Value, String> {
        let function = match target.kind {
            l::CallTargetKind::Function(function) => function,
            l::CallTargetKind::Method(method) => self.method_function(method)?,
            ref other => {
                return Err(internal(format!(
                    "async suspension has invalid target {other:?}"
                )))
            }
        };
        let target_function = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|candidate| candidate.id == function)
            .ok_or_else(|| internal(format!("async target {} is missing", function.0)))?;
        if !target_function.is_async {
            return Err(internal(format!(
                "async target {} is synchronous",
                function.0
            )));
        }
        let mut arguments = vec![self.ctx];
        for (value, ty) in operands.iter().zip(&target.parameter_types) {
            self.push_argument(&mut arguments, *value, ty)?;
        }
        for trap in traps {
            if trap.kind == l::TrapKind::DevOnlyLifetime {
                if let Some(first) = operands.first() {
                    let pointer = self.expect_scalar(*first)?;
                    self.emit_trap(trap, TrapOperand::Value(pointer))?;
                }
            }
        }
        let results = self.call_script(&FnKey::LirFunction(function), &arguments, false)?;
        for trap in traps {
            if trap.kind == l::TrapKind::Call {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        results
            .first()
            .copied()
            .ok_or_else(|| internal("async creator has no frame result"))
    }

    fn resume_async_child(
        &mut self,
        block: l::BlockId,
        target: &l::CallTarget,
        child: Value,
        plan: &SuspendPlan,
    ) -> Result<(), String> {
        let function = match target.kind {
            l::CallTargetKind::Function(function) => function,
            l::CallTargetKind::Method(method) => self.method_function(method)?,
            ref other => {
                return Err(internal(format!(
                    "async suspension has invalid target {other:?}"
                )))
            }
        };
        let output = match target.return_type.as_ref() {
            Some(l::ValueType::Data(ty)) => {
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let output = self.stack_slot(size.max(1), align.max(1));
                self.zero_bytes(output, size.max(1), align.max(1));
                Some((output, ty.clone()))
            }
            Some(other) => {
                return Err(internal(format!("async result has invalid type {other:?}")))
            }
            None => None,
        };
        let output_pointer = output
            .as_ref()
            .map_or_else(|| self.iconst(types::I64, 0), |(output, _)| *output);
        let results = self.call_script(
            &FnKey::LirResume(function),
            &[self.ctx, child, output_pointer],
            false,
        )?;
        self.trap_check();
        let done = *results
            .first()
            .ok_or_else(|| internal("async resume has no done result"))?;
        let completed = self.builder.create_block();
        let suspended = self.builder.create_block();
        self.builder
            .ins()
            .brif(done, completed, &[], suspended, &[]);
        self.builder.switch_to_block(suspended);
        let frame = self
            .frame
            .ok_or_else(|| internal("async parent has no frame"))?;
        let state = self.iconst(types::I32, plan.state);
        self.builder.ins().store(flags(), state, frame, 0);
        self.pop_shadow()?;
        let zero = self.iconst(types::I8, 0);
        self.builder.ins().return_(&[zero]);

        self.builder.switch_to_block(completed);
        let source = self
            .function
            .blocks
            .get(block.0 as usize)
            .ok_or_else(|| internal(format!("async suspend block {} is missing", block.0)))?;
        let l::Terminator::Suspend {
            successor,
            resume_value,
            pos,
            ..
        } = &source.terminator
        else {
            return Err(internal("async attempt source is not a suspension"));
        };
        let mut arguments = Vec::new();
        if resume_value.is_some() {
            let (output, ty) = output
                .as_ref()
                .ok_or_else(|| internal("async resume value has no output slot"))?;
            arguments.extend(rv_args(self.load_data(ty, *output, 0)?));
        }
        for slot in &plan.arguments {
            arguments.extend(rv_args(self.load_value_type(
                &slot.ty,
                frame,
                slot.offset as i32,
            )?));
        }
        let release_pos = self.position_id(pos);
        let release_pos = self.iconst(types::I32, release_pos);
        self.call_runtime(
            self.ml.rt.async_release,
            &[self.ctx, child, release_pos],
            false,
        )?;
        let child_offset = plan
            .child
            .ok_or_else(|| internal("completed async call has no child-frame slot"))?;
        let zero = self.iconst(types::I64, 0);
        self.builder
            .ins()
            .store(flags(), zero, frame, child_offset as i32);
        let successor = self.blocks[successor.0 as usize];
        self.builder.ins().jump(successor, &arguments);
        Ok(())
    }

    fn resume_async_handle(
        &mut self,
        block: l::BlockId,
        handle: Value,
        plan: &SuspendPlan,
        traps: &[l::Trap],
        consume_traps: bool,
    ) -> Result<(), String> {
        let source = self
            .function
            .blocks
            .get(block.0 as usize)
            .ok_or_else(|| internal(format!("held-await block {} is missing", block.0)))?;
        let l::Terminator::Suspend {
            successor,
            resume_value,
            pos,
            ..
        } = &source.terminator
        else {
            return Err(internal("held await source is not a suspension"));
        };
        if let Some(stale) = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::DevReloadOnlyStaleCoroutine)
        {
            if consume_traps {
                self.emit_trap(stale, TrapOperand::Value(handle))?;
            } else {
                self.reload_epoch_check(handle, &stale.pos)?;
            }
        }

        let output = if let Some(value) = resume_value {
            let ty = data_type(self.value_type(*value)?)?.clone();
            let (size, align) = self.ml.layouts.size_align(&ty)?;
            let address = self.stack_slot(size.max(1), align.max(1));
            self.zero_bytes(address, size.max(1), align.max(1));
            Some((address, ty, size))
        } else {
            None
        };
        let output_pointer = output
            .as_ref()
            .map_or_else(|| self.iconst(types::I64, 0), |(address, _, _)| *address);
        let output_size = self.iconst(
            types::I64,
            i64::from(output.as_ref().map_or(0, |(_, _, size)| *size)),
        );
        let cached = self
            .call_runtime(
                self.ml.rt.async_result,
                &[self.ctx, handle, output_pointer, output_size],
                false,
            )?
            .ok_or_else(|| internal("held async result check has no result"))?;
        let completed = self.builder.create_block();
        let poll = self.builder.create_block();
        self.builder.ins().brif(cached, completed, &[], poll, &[]);

        self.builder.switch_to_block(poll);
        let resume = self
            .builder
            .ins()
            .load(types::I64, flags(), handle, COROUTINE_RESUME_OFFSET);
        let signature = self.builder.import_signature(self.ml.resume_sig());
        let call = self.builder.ins().call_indirect(
            signature,
            resume,
            &[self.ctx, handle, output_pointer],
        );
        let done = self.builder.inst_results(call)[0];
        if let Some(call_trap) = traps.iter().find(|trap| trap.kind == l::TrapKind::Call) {
            if consume_traps {
                self.emit_trap(call_trap, TrapOperand::Pending)?;
            } else {
                self.trap_check();
            }
        }
        let newly_completed = self.builder.create_block();
        let suspended = self.builder.create_block();
        self.builder
            .ins()
            .brif(done, newly_completed, &[], suspended, &[]);

        self.builder.switch_to_block(newly_completed);
        self.call_runtime(
            self.ml.rt.async_complete,
            &[self.ctx, handle, output_pointer, output_size],
            false,
        )?;
        self.builder.ins().jump(completed, &[]);

        self.builder.switch_to_block(suspended);
        let frame = self
            .frame
            .ok_or_else(|| internal("held await parent has no frame"))?;
        let state = self.iconst(types::I32, plan.state);
        self.builder.ins().store(flags(), state, frame, 0);
        self.pop_shadow()?;
        let zero = self.iconst(types::I8, 0);
        self.builder.ins().return_(&[zero]);

        self.builder.switch_to_block(completed);
        let mut arguments = Vec::new();
        if resume_value.is_some() {
            let (address, ty, _) = output
                .as_ref()
                .ok_or_else(|| internal("held await result has no output slot"))?;
            arguments.extend(rv_args(self.load_data(ty, *address, 0)?));
        }
        for slot in &plan.arguments {
            arguments.extend(rv_args(self.load_value_type(
                &slot.ty,
                frame,
                slot.offset as i32,
            )?));
        }
        let child_offset = plan
            .child
            .ok_or_else(|| internal("completed held await has no child-frame slot"))?;
        let zero = self.iconst(types::I64, 0);
        self.builder
            .ins()
            .store(flags(), zero, frame, child_offset as i32);
        self.builder
            .ins()
            .jump(self.blocks[successor.0 as usize], &arguments);
        let _ = pos;
        Ok(())
    }

    fn emit_resume_adapters(&mut self, plan: &CoroutinePlan) -> Result<(), String> {
        for source in &self.function.blocks {
            let l::Terminator::Suspend {
                kind,
                pos,
                successor,
                resume_value,
                traps,
                ..
            } = &source.terminator
            else {
                continue;
            };
            let suspend = plan
                .suspends
                .get(&source.id)
                .ok_or_else(|| internal(format!("suspend block {} has no plan", source.id.0)))?;
            let adapter = self
                .resume_adapters
                .get(&source.id)
                .copied()
                .ok_or_else(|| internal(format!("suspend block {} has no adapter", source.id.0)))?;
            self.builder.switch_to_block(adapter);
            let frame = self
                .frame
                .ok_or_else(|| internal("resume adapter has no frame"))?;
            self.reload_epoch_check(frame, pos)?;
            if matches!(
                kind,
                l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. }
            ) {
                let child = self.builder.ins().load(
                    types::I64,
                    flags(),
                    frame,
                    suspend
                        .child
                        .ok_or_else(|| internal("async adapter has no child slot"))?
                        as i32,
                );
                match kind {
                    l::SuspendKind::AsyncCall { target, .. } => {
                        self.resume_async_child(source.id, target, child, suspend)?;
                    }
                    l::SuspendKind::AsyncHandle { .. } => {
                        self.resume_async_handle(source.id, child, suspend, traps, false)?;
                    }
                    _ => unreachable!(),
                }
                continue;
            }
            if resume_value.is_some() {
                return Err(internal("non-call suspension defines a resume value"));
            }
            let mut arguments = Vec::new();
            for slot in &suspend.arguments {
                arguments.extend(rv_args(self.load_value_type(
                    &slot.ty,
                    frame,
                    slot.offset as i32,
                )?));
            }
            let successor = self.blocks[successor.0 as usize];
            self.builder.ins().jump(successor, &arguments);
        }
        Ok(())
    }

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
        assert_eq!(zero_stores_at(16), 2, "direct child clear on both paths");
        assert_eq!(zero_stores_at(32), 2, "held child clear on both paths");
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
                body.call_runtime(body.ml.rt.async_register, &[body.ctx, frame], false)?;
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
