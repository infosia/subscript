//! Aggregate argument and struct-return planning for the target ABI.

use super::*;

/// Whether AAPCS64 classifies one to four identical floating-point leaves as an HFA.
pub(super) fn is_pure_hfa_leaves(leaves: &[(u32, types::Type)]) -> bool {
    if !matches!(leaves.len(), 1..=4) {
        return false;
    }
    let class = leaves
        .first()
        .and_then(|(_, ty)| crate::layout::boundary_hfa_class(*ty));
    class.is_some()
        && leaves
            .iter()
            .all(|(_, ty)| crate::layout::boundary_hfa_class(*ty) == class)
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
                            .all(|(_, ty)| crate::layout::boundary_hfa_class(*ty).is_some());
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

/// Plans the endpoint with the platform composite register-pressure fallback.
#[cfg(test)]
pub(super) fn plan_completion_endpoint_arg(
    abi: AggregateAbi,
    signature: &Signature,
) -> Result<AggregateArgPlan, String> {
    plan_aggregate_arg_for_signature(abi, &[(0, types::I64), (8, types::I64)], 16, signature)
}

/// Applies register pressure to the platform composite plan.
pub(super) fn plan_aggregate_arg_for_signature(
    abi: AggregateAbi,
    leaves: &[(u32, types::Type)],
    total: u32,
    signature: &Signature,
) -> Result<AggregateArgPlan, String> {
    let plan = plan_aggregate_arg(abi, leaves, total)?;
    if abi == AggregateAbi::Aapcs64 {
        return Aapcs64Arguments::from_signature(signature).allocate(plan, total, signature);
    }
    if let AggregateArgPlan::Images(images) = &plan {
        if abi == AggregateAbi::SysV
            && ensure_sysv_argument_register_capacity(signature, images).is_err()
        {
            return Ok(AggregateArgPlan::Memory {
                stack_size: round_up_layout(total.max(1), 8, "boundary aggregate stack copy")?,
            });
        }
    }
    Ok(plan)
}

/// AAPCS64 Stage C has independent general and SIMD argument cursors.
/// The signature includes scalars, indirect pointers, images, and skipped registers.
struct Aapcs64Arguments {
    ngrn: usize,
    nsrn: usize,
}

impl Aapcs64Arguments {
    fn from_signature(signature: &Signature) -> Self {
        let mut state = Self { ngrn: 0, nsrn: 0 };
        for parameter in &signature.params {
            if parameter.purpose == ArgumentPurpose::StructReturn {
                continue;
            }
            if parameter.value_type.is_float() {
                state.nsrn = (state.nsrn + 1).min(8);
            } else {
                state.ngrn = (state.ngrn + 1).min(8);
            }
        }
        state
    }

    fn allocate(
        self,
        plan: AggregateArgPlan,
        total: u32,
        signature: &Signature,
    ) -> Result<AggregateArgPlan, String> {
        let (padding, padding_type, images) = match &plan {
            AggregateArgPlan::Hfa(leaves) if leaves.len() > 8 - self.nsrn => {
                // C.12 exhausts SIMD registers without consuming general registers.
                // Apple packs stack leaves naturally; base AAPCS64 uses eightbyte slots.
                let images =
                    if signature.call_conv == cranelift_codegen::isa::CallConv::AppleAarch64 {
                        leaves
                            .iter()
                            .map(|(offset, ty)| EightbyteImage {
                                offset: *offset,
                                class: RegisterClass::Sse,
                                ty: *ty,
                            })
                            .collect()
                    } else {
                        (0..total.div_ceil(8))
                            .map(|i| EightbyteImage {
                                offset: i * 8,
                                class: RegisterClass::Sse,
                                ty: types::F64,
                            })
                            .collect()
                    };
                (8 - self.nsrn, types::F64, images)
            }
            AggregateArgPlan::Images(images) if images.len() > 8 - self.ngrn => {
                // C.13 exhausts general registers and passes the entire composite on stack.
                (8 - self.ngrn, types::I64, images.clone())
            }
            _ => return Ok(plan),
        };
        Ok(AggregateArgPlan::StackImages {
            padding,
            padding_type,
            images,
        })
    }
}

/// Checks whether the SysV aggregate fits the remaining argument registers.
pub(super) fn ensure_sysv_argument_register_capacity(
    signature: &Signature,
    images: &[EightbyteImage],
) -> Result<(), String> {
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
) -> Result<Option<Vec<EightbyteImage>>, String> {
    match plan_aggregate_arg(AggregateAbi::SysV, leaves, size)? {
        AggregateArgPlan::Images(images) => Ok(Some(images)),
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
    fn sysv_float_returns_use_typed_eightbyte_images() {
        let sse = [(0, types::F64), (8, types::F64)];
        assert_eq!(
            plan_sysv_struct_return(&sse, 16).expect("SSE return"),
            Some(vec![
                image(0, RegisterClass::Sse, types::F64),
                image(8, RegisterClass::Sse, types::F64)
            ])
        );
        let half = [(0, types::F16), (2, types::F16)];
        assert_eq!(
            plan_sysv_struct_return(&half, 4).expect("half return"),
            Some(vec![image(0, RegisterClass::Sse, types::F64)])
        );
        assert_eq!(
            plan_sysv_struct_return(&[(0, types::F64), (8, types::F64), (16, types::F64)], 24,)
                .expect("MEMORY return"),
            None
        );
    }

    #[test]
    fn completion_endpoint_uses_target_abi_and_sysv_stack_fallback() {
        let mut signature = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        for _ in 0..4 {
            signature.params.push(AbiParam::new(types::I64));
        }
        for abi in [AggregateAbi::SysV, AggregateAbi::Aapcs64] {
            assert!(
                matches!(plan_completion_endpoint_arg(abi, &signature).expect("endpoint plan"), AggregateArgPlan::Images(images) if images.len() == 2)
            );
        }
        assert!(matches!(
            plan_completion_endpoint_arg(AggregateAbi::Win64, &signature).expect("endpoint plan"),
            AggregateArgPlan::Indirect
        ));
        signature.params.push(AbiParam::new(types::I64));
        assert!(matches!(
            plan_completion_endpoint_arg(AggregateAbi::SysV, &signature).expect("endpoint plan"),
            AggregateArgPlan::Memory { stack_size: 16 }
        ));
    }

    #[test]
    fn aapcs64_c13_places_the_whole_composite_on_the_stack() {
        let mut signature = Signature::new(cranelift_codegen::isa::CallConv::AppleAarch64);
        for count in 0..=8 {
            let plan =
                plan_completion_endpoint_arg(AggregateAbi::Aapcs64, &signature).expect("plan");
            if count <= 6 {
                assert!(matches!(plan, AggregateArgPlan::Images(_)));
            } else {
                assert!(
                    matches!(plan, AggregateArgPlan::StackImages { padding, images, .. } if padding == 8 - count && images.len() == 2)
                );
            }
            assert!(matches!(
                plan_completion_endpoint_arg(AggregateAbi::Win64, &signature).expect("Win64"),
                AggregateArgPlan::Indirect
            ));
            let sysv = plan_completion_endpoint_arg(AggregateAbi::SysV, &signature).expect("SysV");
            if count <= 4 {
                assert!(matches!(sysv, AggregateArgPlan::Images(_)));
            } else {
                assert_eq!(sysv, AggregateArgPlan::Memory { stack_size: 16 });
            }
            signature.params.push(AbiParam::new(types::I32));
        }
        let mut signature = Signature::new(cranelift_codegen::isa::CallConv::AppleAarch64);
        signature
            .params
            .extend((0..8).map(|_| AbiParam::new(types::I32)));
        assert!(matches!(
            plan_aggregate_arg_for_signature(AggregateAbi::Aapcs64,
                &[(0, types::I32), (4, types::I32), (8, types::I32)], 12, &signature).expect("stack composite rounding"),
            AggregateArgPlan::StackImages { padding: 0, images, .. } if images.len() == 2 && images.iter().all(|i| i.ty == types::I64)
        ));
    }

    #[test]
    fn float_pressure_shapes_follow_sysv_and_win64_rules() {
        let f32x4 = [
            (0, types::F32),
            (4, types::F32),
            (8, types::F32),
            (12, types::F32),
        ];
        let f64x2 = [(0, types::F64), (8, types::F64)];
        let mut signature = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        signature
            .params
            .extend((0..5).map(|_| AbiParam::new(types::F32)));
        let plan = plan_aggregate_arg_for_signature(AggregateAbi::SysV, &f32x4, 16, &signature)
            .expect("SysV");
        assert!(matches!(plan, AggregateArgPlan::Images(images) if images.len() == 2));
        signature.params.clear();
        signature
            .params
            .extend((0..7).map(|_| AbiParam::new(types::F64)));
        assert_eq!(
            plan_aggregate_arg_for_signature(AggregateAbi::SysV, &f64x2, 16, &signature)
                .expect("SysV"),
            AggregateArgPlan::Memory { stack_size: 16 }
        );
        // SysV reverts the aggregate allocation. The following double retains xmm7.
        signature.params.push(AbiParam::special(
            types::I64,
            ArgumentPurpose::StructArgument(16),
        ));
        ensure_sysv_argument_register_capacity(
            &signature,
            &[image(0, RegisterClass::Sse, types::F64)],
        )
        .expect("following double");
        for leaves in [&f32x4[..], &f64x2[..]] {
            assert_eq!(
                plan_aggregate_arg_for_signature(AggregateAbi::Win64, leaves, 16, &signature)
                    .expect("Win64"),
                AggregateArgPlan::Indirect
            );
        }
    }

    #[test]
    fn sysv_argument_register_pressure_fails_loud() {
        let mut signature = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        for _ in 0..6 {
            signature.params.push(AbiParam::new(types::I64));
        }
        let images = [image(0, RegisterClass::Integer, types::I64)];
        let error = ensure_sysv_argument_register_capacity(&signature, &images)
            .expect_err("no integer argument register is free");
        assert!(error.contains("register pressure"), "{error}");
    }
}
