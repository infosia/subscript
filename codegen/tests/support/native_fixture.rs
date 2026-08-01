//! Native-library description for the synthetic interop test fixture.

// The fixture compiles `corpus/interop/interop.c` (which uses `_Float16`),
// unbuildable by MSVC `cl`, so the fixture crate is excluded on windows-msvc
// (compiler.md §11c). This module is included via `#[path]` into four test
// targets; gating its whole body makes it expose nothing and reference no
// fixture symbol there. Every call site is excluded under the same predicate.
#![cfg(not(all(windows, target_env = "msvc")))]

// Naming the dev-dependency propagates its test-only native archive into
// this integration-test link.
extern crate subscript_interop_fixture;

use std::path::PathBuf;

use subscript_codegen::NativeLibrary;

// The fixture crate compiles the implementation into this test process.
// Only addresses are taken here; generated code calls them through the C
// signatures declared by the committed mirror.
extern "C" {
    fn subChainPayloadValue();
    fn subDeviceCreate();
    fn subDeviceRetain();
    fn subDeviceRelease();
    fn subDeviceSubmit();
    fn subDeviceSetLogger();
    fn subDeviceSetLabel();
    fn subDevicePoll();
    fn subSliceChecksumF32();
    fn subSliceChecksumI32();
    fn subSliceChecksumF64();
    fn subSliceChecksumI64();
    fn subSliceChecksumU8();
    fn subSliceChecksumI8();
    fn subSliceChecksumU16();
    fn subSliceChecksumI16();
    fn subSliceChecksumF16();
    fn subDrawListTotal();
    fn subAccessMatches();
    fn subBulkConsume();
    fn subBulkConsumeF32();
    fn subDeviceOnComplete();
    fn subDevicePump();
    fn subCommandBufferTotal();
    fn subStageMatches();
    fn subFutureMake();
    fn subStatsMake();
    fn subDeviceQuery();
    fn subDeviceKickAsync();
    fn subDeviceWait();
    fn subDeviceSumBytes();
    fn subDeviceFillBytes();
    fn subDeviceFillShorts();
    fn subBoundaryStringCheck();
    fn subBoundaryStringFill();
    fn subProbeTextureDescriptorCheck();
    fn subProbeTextureDescriptorFill();
    fn subProbePipelineLayoutCheck();
    fn subProbeBindGroupEntryCheck();
    fn subProbeBindGroupEntryFill();
    fn subProbeComputePipelineCheck();
    fn subProbeRenderPipelineCheck();
    fn subProbeProgrammableStageCheck();
    fn subProbeFullRenderPipelineCheck();
    fn subProbeQueueSubmitCheck();
}

/// Returns the native-library inputs for the committed interop fixture.
pub fn library() -> NativeLibrary {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop");
    let symbols = vec![
        ("subChainPayloadValue".to_string(), subChainPayloadValue as *const u8),
        ("subDeviceCreate".to_string(), subDeviceCreate as *const u8),
        ("subDeviceRetain".to_string(), subDeviceRetain as *const u8),
        ("subDeviceRelease".to_string(), subDeviceRelease as *const u8),
        ("subDeviceSubmit".to_string(), subDeviceSubmit as *const u8),
        ("subDeviceSetLogger".to_string(), subDeviceSetLogger as *const u8),
        ("subDeviceSetLabel".to_string(), subDeviceSetLabel as *const u8),
        ("subDevicePoll".to_string(), subDevicePoll as *const u8),
        ("subSliceChecksumF32".to_string(), subSliceChecksumF32 as *const u8),
        ("subSliceChecksumI32".to_string(), subSliceChecksumI32 as *const u8),
        ("subSliceChecksumF64".to_string(), subSliceChecksumF64 as *const u8),
        ("subSliceChecksumI64".to_string(), subSliceChecksumI64 as *const u8),
        ("subSliceChecksumU8".to_string(), subSliceChecksumU8 as *const u8),
        ("subSliceChecksumI8".to_string(), subSliceChecksumI8 as *const u8),
        ("subSliceChecksumU16".to_string(), subSliceChecksumU16 as *const u8),
        ("subSliceChecksumI16".to_string(), subSliceChecksumI16 as *const u8),
        ("subSliceChecksumF16".to_string(), subSliceChecksumF16 as *const u8),
        ("subDrawListTotal".to_string(), subDrawListTotal as *const u8),
        ("subAccessMatches".to_string(), subAccessMatches as *const u8),
        ("subBulkConsume".to_string(), subBulkConsume as *const u8),
        ("subBulkConsumeF32".to_string(), subBulkConsumeF32 as *const u8),
        ("subDeviceOnComplete".to_string(), subDeviceOnComplete as *const u8),
        ("subDevicePump".to_string(), subDevicePump as *const u8),
        ("subCommandBufferTotal".to_string(), subCommandBufferTotal as *const u8),
        ("subStageMatches".to_string(), subStageMatches as *const u8),
        ("subFutureMake".to_string(), subFutureMake as *const u8),
        ("subStatsMake".to_string(), subStatsMake as *const u8),
        ("subDeviceQuery".to_string(), subDeviceQuery as *const u8),
        ("subDeviceKickAsync".to_string(), subDeviceKickAsync as *const u8),
        ("subDeviceWait".to_string(), subDeviceWait as *const u8),
        ("subDeviceSumBytes".to_string(), subDeviceSumBytes as *const u8),
        ("subDeviceFillBytes".to_string(), subDeviceFillBytes as *const u8),
        (
            "subDeviceFillShorts".to_string(),
            subDeviceFillShorts as *const u8,
        ),
        (
            "subBoundaryStringCheck".to_string(),
            subBoundaryStringCheck as *const u8,
        ),
        (
            "subBoundaryStringFill".to_string(),
            subBoundaryStringFill as *const u8,
        ),
        (
            "subProbeTextureDescriptorCheck".to_string(),
            subProbeTextureDescriptorCheck as *const u8,
        ),
        (
            "subProbeTextureDescriptorFill".to_string(),
            subProbeTextureDescriptorFill as *const u8,
        ),
        (
            "subProbePipelineLayoutCheck".to_string(),
            subProbePipelineLayoutCheck as *const u8,
        ),
        (
            "subProbeBindGroupEntryCheck".to_string(),
            subProbeBindGroupEntryCheck as *const u8,
        ),
        (
            "subProbeBindGroupEntryFill".to_string(),
            subProbeBindGroupEntryFill as *const u8,
        ),
        (
            "subProbeComputePipelineCheck".to_string(),
            subProbeComputePipelineCheck as *const u8,
        ),
        (
            "subProbeRenderPipelineCheck".to_string(),
            subProbeRenderPipelineCheck as *const u8,
        ),
        (
            "subProbeProgrammableStageCheck".to_string(),
            subProbeProgrammableStageCheck as *const u8,
        ),
        (
            "subProbeFullRenderPipelineCheck".to_string(),
            subProbeFullRenderPipelineCheck as *const u8,
        ),
        (
            "subProbeQueueSubmitCheck".to_string(),
            subProbeQueueSubmitCheck as *const u8,
        ),
    ];
    // SAFETY: the test-only fixture crate links these static-lifetime
    // functions into the test process, and every address corresponds to the
    // same-name signature in the committed mirror and header.
    unsafe {
        NativeLibrary::new(
            vec![directory.clone()],
            vec![directory.join("interop.c")],
            symbols,
        )
    }
}
