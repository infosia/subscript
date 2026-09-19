//! Native-library description for the synthetic interop test fixture.

// MSVC cannot compile the fixture's `_Float16` boundary (compiler.md §11c).
// Callers must obtain a fixture before they can use its native symbols.

/// A native fixture that this configuration can run.
pub struct Fixture {
    #[cfg(all(windows, target_env = "msvc"))]
    unavailable: Unavailable,
}

#[cfg(all(windows, target_env = "msvc"))]
enum Unavailable {}

/// Returns no fixture when the host C compiler cannot compile it.
pub fn fixture() -> Option<Fixture> {
    #[cfg(all(windows, target_env = "msvc"))]
    {
        None
    }
    #[cfg(not(all(windows, target_env = "msvc")))]
    {
        Some(Fixture {})
    }
}

// Naming the dev-dependency propagates its test-only native archive into
// this integration-test link.
#[cfg(not(all(windows, target_env = "msvc")))]
extern crate subscript_interop_fixture;

use subscript_codegen::NativeLibrary;

// The fixture crate compiles the implementation into this test process.
// Only addresses are taken here; generated code calls them through the C
// signatures declared by the committed mirror.
#[cfg(not(all(windows, target_env = "msvc")))]
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
    fn subProbeFullRenderPipelineWithHandleCheck();
    fn subProbeFullRenderPipelineWithNestedBlendCheck();
    fn subProbeFullRenderPipelineWithUnmarkedBlendCheck();
    fn subProbeBreadthRenderPipelineCheck();
    fn subProbeWideRenderPipelineCheck();
    fn subProbeQueueSubmitCheck();
    fn subProbeSetBindGroupCheck();
    fn subByValueI32OneReport();
    fn subByValueI32PairReport();
    fn subByValueI32TripleReport();
    fn subByValueI16I16I32Report();
    fn subByValueU8FourReport();
    fn subByValueI64PairReport();
    fn subByValueF32Hfa2Report();
    fn subByValueF32Hfa4Report();
    fn subByValueI32F32Report();
    fn subByValueI32I64Report();
    fn subByValueI64TripleReport();
    fn subHostOwnedStateCreate();
    fn subHostOwnedStateDestroy();
    fn subHostOwnedStateBorrow() -> *mut std::ffi::c_void;
    fn subHostOwnedStateAdvance(state: *mut std::ffi::c_void) -> i32;
    fn subHostOwnedStatePreEntry(ctx: *mut std::ffi::c_void);
    fn subHostOwnedStatePostRun(ctx: *mut std::ffi::c_void);
    fn subExternalDeviceIdentity();
    fn subExternalDeviceTag();
    fn subWireModeNext();
    fn subWireModeEcho();
    fn subWireModeUnknown();
    fn subBindToneNext();
    fn subBindToneEcho();
    fn subWireModeRecordEchoMode();
    fn subWireModeRecordEchoTone();
    fn subWireModeRecordEchoElement();
    fn subWireModeRecordFill();
    fn subWireModeRecordFillUnknown();
    fn subRequestStart();
    fn subRequestSubscribe();
    fn subRequestNotify();
    fn subRequestUnsubscribe();
    fn subRequestPump();
    fn subRequestReleaseActive();
    fn subRequestReleaseCount();
    fn subRequestMarkCharge();
    fn subRequestChargeFellBy();
    fn subRequestReleaseAndRefire();
}

impl Fixture {
    /// Runs the fixture's ship-tier pre-entry hook for a dev-tier session.
    // This shared support module is also compiled into test targets that use
    // only `library`, so the lifecycle helpers are intentionally unused there.
    #[allow(dead_code)]
    pub fn host_owned_state_pre_entry(&self) {
        #[cfg(all(windows, target_env = "msvc"))]
        {
            match self.unavailable {}
        }
        #[cfg(not(all(windows, target_env = "msvc")))]
        {
            // SAFETY: the fixture hook ignores the Context argument and creates its
            // own state. The linked function has the declared C signature.
            unsafe { subHostOwnedStatePreEntry(std::ptr::null_mut()) };
        }
    }

    /// Borrows the fixture state, advances it once, and returns its handle.
    // Some test targets use only the native library.
    #[allow(dead_code)]
    pub fn host_owned_state_borrow_and_advance(&self) -> *mut std::ffi::c_void {
        #[cfg(all(windows, target_env = "msvc"))]
        {
            match self.unavailable {}
        }
        #[cfg(not(all(windows, target_env = "msvc")))]
        {
            // SAFETY: the caller starts the fixture lifecycle first. The fixture owns
            // the returned state until the paired post-run hook destroys it.
            let state = unsafe { subHostOwnedStateBorrow() };
            // SAFETY: `state` is the live handle returned by the fixture.
            let _ = unsafe { subHostOwnedStateAdvance(state) };
            state
        }
    }

    /// Runs the fixture's ship-tier post-run hook for a dev-tier session.
    // This shared support module is also compiled into test targets that use
    // only `library`, so the lifecycle helpers are intentionally unused there.
    #[allow(dead_code)]
    pub fn host_owned_state_post_run(&self) {
        #[cfg(all(windows, target_env = "msvc"))]
        {
            match self.unavailable {}
        }
        #[cfg(not(all(windows, target_env = "msvc")))]
        {
            // SAFETY: paired with `host_owned_state_pre_entry`; the fixture hook
            // ignores the Context argument, destroys its state, and clears its slot.
            unsafe { subHostOwnedStatePostRun(std::ptr::null_mut()) };
        }
    }

    /// Returns the native-library inputs for the committed interop fixture.
    pub fn library(&self) -> NativeLibrary {
        #[cfg(all(windows, target_env = "msvc"))]
        {
            match self.unavailable {}
        }
        #[cfg(not(all(windows, target_env = "msvc")))]
        {
            use std::path::PathBuf;

            let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/interop");
            let symbols = vec![
                (
                    "subChainPayloadValue".to_string(),
                    subChainPayloadValue as *const u8,
                ),
                ("subDeviceCreate".to_string(), subDeviceCreate as *const u8),
                ("subDeviceRetain".to_string(), subDeviceRetain as *const u8),
                (
                    "subDeviceRelease".to_string(),
                    subDeviceRelease as *const u8,
                ),
                ("subDeviceSubmit".to_string(), subDeviceSubmit as *const u8),
                (
                    "subDeviceSetLogger".to_string(),
                    subDeviceSetLogger as *const u8,
                ),
                (
                    "subDeviceSetLabel".to_string(),
                    subDeviceSetLabel as *const u8,
                ),
                ("subDevicePoll".to_string(), subDevicePoll as *const u8),
                (
                    "subSliceChecksumF32".to_string(),
                    subSliceChecksumF32 as *const u8,
                ),
                (
                    "subSliceChecksumI32".to_string(),
                    subSliceChecksumI32 as *const u8,
                ),
                (
                    "subSliceChecksumF64".to_string(),
                    subSliceChecksumF64 as *const u8,
                ),
                (
                    "subSliceChecksumI64".to_string(),
                    subSliceChecksumI64 as *const u8,
                ),
                (
                    "subSliceChecksumU8".to_string(),
                    subSliceChecksumU8 as *const u8,
                ),
                (
                    "subSliceChecksumI8".to_string(),
                    subSliceChecksumI8 as *const u8,
                ),
                (
                    "subSliceChecksumU16".to_string(),
                    subSliceChecksumU16 as *const u8,
                ),
                (
                    "subSliceChecksumI16".to_string(),
                    subSliceChecksumI16 as *const u8,
                ),
                (
                    "subSliceChecksumF16".to_string(),
                    subSliceChecksumF16 as *const u8,
                ),
                (
                    "subDrawListTotal".to_string(),
                    subDrawListTotal as *const u8,
                ),
                (
                    "subAccessMatches".to_string(),
                    subAccessMatches as *const u8,
                ),
                ("subBulkConsume".to_string(), subBulkConsume as *const u8),
                (
                    "subBulkConsumeF32".to_string(),
                    subBulkConsumeF32 as *const u8,
                ),
                (
                    "subDeviceOnComplete".to_string(),
                    subDeviceOnComplete as *const u8,
                ),
                ("subDevicePump".to_string(), subDevicePump as *const u8),
                (
                    "subCommandBufferTotal".to_string(),
                    subCommandBufferTotal as *const u8,
                ),
                ("subStageMatches".to_string(), subStageMatches as *const u8),
                ("subFutureMake".to_string(), subFutureMake as *const u8),
                ("subStatsMake".to_string(), subStatsMake as *const u8),
                ("subDeviceQuery".to_string(), subDeviceQuery as *const u8),
                (
                    "subDeviceKickAsync".to_string(),
                    subDeviceKickAsync as *const u8,
                ),
                ("subDeviceWait".to_string(), subDeviceWait as *const u8),
                (
                    "subDeviceSumBytes".to_string(),
                    subDeviceSumBytes as *const u8,
                ),
                (
                    "subDeviceFillBytes".to_string(),
                    subDeviceFillBytes as *const u8,
                ),
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
                    "subProbeFullRenderPipelineWithHandleCheck".to_string(),
                    subProbeFullRenderPipelineWithHandleCheck as *const u8,
                ),
                (
                    "subProbeFullRenderPipelineWithNestedBlendCheck".to_string(),
                    subProbeFullRenderPipelineWithNestedBlendCheck as *const u8,
                ),
                (
                    "subProbeFullRenderPipelineWithUnmarkedBlendCheck".to_string(),
                    subProbeFullRenderPipelineWithUnmarkedBlendCheck as *const u8,
                ),
                (
                    "subProbeBreadthRenderPipelineCheck".to_string(),
                    subProbeBreadthRenderPipelineCheck as *const u8,
                ),
                (
                    "subProbeWideRenderPipelineCheck".to_string(),
                    subProbeWideRenderPipelineCheck as *const u8,
                ),
                (
                    "subProbeQueueSubmitCheck".to_string(),
                    subProbeQueueSubmitCheck as *const u8,
                ),
                (
                    "subProbeSetBindGroupCheck".to_string(),
                    subProbeSetBindGroupCheck as *const u8,
                ),
                (
                    "subByValueI32OneReport".to_string(),
                    subByValueI32OneReport as *const u8,
                ),
                (
                    "subByValueI32PairReport".to_string(),
                    subByValueI32PairReport as *const u8,
                ),
                (
                    "subByValueI32TripleReport".to_string(),
                    subByValueI32TripleReport as *const u8,
                ),
                (
                    "subByValueI16I16I32Report".to_string(),
                    subByValueI16I16I32Report as *const u8,
                ),
                (
                    "subByValueU8FourReport".to_string(),
                    subByValueU8FourReport as *const u8,
                ),
                (
                    "subByValueI64PairReport".to_string(),
                    subByValueI64PairReport as *const u8,
                ),
                (
                    "subByValueF32Hfa2Report".to_string(),
                    subByValueF32Hfa2Report as *const u8,
                ),
                (
                    "subByValueF32Hfa4Report".to_string(),
                    subByValueF32Hfa4Report as *const u8,
                ),
                (
                    "subByValueI32F32Report".to_string(),
                    subByValueI32F32Report as *const u8,
                ),
                (
                    "subByValueI32I64Report".to_string(),
                    subByValueI32I64Report as *const u8,
                ),
                (
                    "subByValueI64TripleReport".to_string(),
                    subByValueI64TripleReport as *const u8,
                ),
                (
                    "subHostOwnedStateCreate".to_string(),
                    subHostOwnedStateCreate as *const u8,
                ),
                (
                    "subHostOwnedStateDestroy".to_string(),
                    subHostOwnedStateDestroy as *const u8,
                ),
                (
                    "subHostOwnedStateBorrow".to_string(),
                    subHostOwnedStateBorrow as *const u8,
                ),
                (
                    "subHostOwnedStateAdvance".to_string(),
                    subHostOwnedStateAdvance as *const u8,
                ),
                (
                    "subHostOwnedStatePreEntry".to_string(),
                    subHostOwnedStatePreEntry as *const u8,
                ),
                (
                    "subHostOwnedStatePostRun".to_string(),
                    subHostOwnedStatePostRun as *const u8,
                ),
                (
                    "subExternalDeviceIdentity".to_string(),
                    subExternalDeviceIdentity as *const u8,
                ),
                (
                    "subExternalDeviceTag".to_string(),
                    subExternalDeviceTag as *const u8,
                ),
                ("subWireModeNext".to_string(), subWireModeNext as *const u8),
                ("subWireModeEcho".to_string(), subWireModeEcho as *const u8),
                (
                    "subWireModeUnknown".to_string(),
                    subWireModeUnknown as *const u8,
                ),
                ("subBindToneNext".to_string(), subBindToneNext as *const u8),
                ("subBindToneEcho".to_string(), subBindToneEcho as *const u8),
                (
                    "subWireModeRecordEchoMode".to_string(),
                    subWireModeRecordEchoMode as *const u8,
                ),
                (
                    "subWireModeRecordEchoTone".to_string(),
                    subWireModeRecordEchoTone as *const u8,
                ),
                (
                    "subWireModeRecordEchoElement".to_string(),
                    subWireModeRecordEchoElement as *const u8,
                ),
                (
                    "subWireModeRecordFill".to_string(),
                    subWireModeRecordFill as *const u8,
                ),
                (
                    "subWireModeRecordFillUnknown".to_string(),
                    subWireModeRecordFillUnknown as *const u8,
                ),
                // Callback registrations with an explicit end (compiler.md §111).
                ("subRequestStart".to_string(), subRequestStart as *const u8),
                (
                    "subRequestSubscribe".to_string(),
                    subRequestSubscribe as *const u8,
                ),
                (
                    "subRequestNotify".to_string(),
                    subRequestNotify as *const u8,
                ),
                (
                    "subRequestUnsubscribe".to_string(),
                    subRequestUnsubscribe as *const u8,
                ),
                ("subRequestPump".to_string(), subRequestPump as *const u8),
                (
                    "subRequestReleaseActive".to_string(),
                    subRequestReleaseActive as *const u8,
                ),
                (
                    "subRequestReleaseCount".to_string(),
                    subRequestReleaseCount as *const u8,
                ),
                (
                    "subRequestMarkCharge".to_string(),
                    subRequestMarkCharge as *const u8,
                ),
                (
                    "subRequestChargeFellBy".to_string(),
                    subRequestChargeFellBy as *const u8,
                ),
                (
                    "subRequestReleaseAndRefire".to_string(),
                    subRequestReleaseAndRefire as *const u8,
                ),
            ];
            // SAFETY: the test-only fixture crate links these static-lifetime
            // functions into the test process, and every address corresponds to the
            // same-name signature in the committed mirror and header.
            unsafe {
                NativeLibrary::new(
                    vec![directory.clone()],
                    vec![
                        directory.join("interop.c"),
                        directory.join("external-device.c"),
                        directory.join("wire-enum.c"),
                    ],
                    symbols,
                )
            }
        }
    }
}
