//! Shared corpus-test helpers.

/// Returns true when source text names the generated interop mirror.
pub fn references_interop(source: &str) -> bool {
    const TOKENS: &[&str] = &[
        "subDevice",
        "subChainPayloadValue",
        "subSlice",
        "SubDrawList",
        "subDrawListTotal",
        "SUB_ACCESS",
        "subAccessMatches",
        "subBulk",
        "subBoundaryString",
        "subProbeTexture",
        "subProbeComputePipeline",
        "subProbeRenderPipeline",
        "subProbeProgrammableStage",
        "subProbeFullRenderPipeline",
        "subProbeBreadthRenderPipeline",
        "subProbeWideRenderPipeline",
        "subProbeQueueSubmit",
        "subProbeSetBindGroup",
        "SUB_STAGE",
        "subStageMatches",
        "subFutureMake",
        "subStatsMake",
        "SubQueryStatus",
        "SubWaitEntry",
        "subByValue",
        "subHostOwnedState",
        "subWireMode",
        "subBindTone",
        "subProbePipelineLayout",
        "subProbeBindGroupEntry",
    ];
    TOKENS.iter().any(|token| source.contains(token))
}
