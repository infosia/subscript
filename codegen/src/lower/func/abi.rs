//! Aggregate argument and struct-return planning for the target ABI.

use super::*;

/// Whether the leaves form a Homogeneous Floating-point Aggregate (AAPCS
/// 6.4.2 / Win64): 1 to 4 leaves, all of one fundamental float type. Such
/// an aggregate travels in SIMD registers, so the integer-image path must
/// not marshal it.
pub(super) fn is_pure_hfa_leaves(leaves: &[(u32, types::Type)]) -> bool {
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
pub(super) fn plan_aggregate_arg(
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
pub(super) fn ensure_sysv_argument_register_capacity(
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
pub(super) fn plan_sysv_struct_return(
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
