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
        "SubLogCallback",
        "subAccessMatches",
        "subBulk",
        "subBoundaryString",
        "subProbeTexture",
        "subProbeComputePipeline",
        "subProbeRenderPipeline",
        "subProbeProgrammableStage",
        "subProbeFullRenderPipeline",
        "SGPUProbeColorTargetState",
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

#[path = "../../../codegen/tests/corpus/mod.rs"]
#[allow(dead_code)]
mod directory_corpus;

/// All directory programs, in stable order.
#[allow(dead_code)]
pub fn directories(accept: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = std::fs::read_dir(accept)
        .expect("accept directory")
        .map(|entry| entry.expect("corpus entry").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Loads a directory program and the ambient mirrors that it uses.
#[allow(dead_code)]
pub fn directory_sources(directory: &std::path::Path) -> Vec<subscript_compiler::SourceFile> {
    directory_corpus::entry_sources(
        directory.parent().expect("corpus root"),
        directory
            .file_name()
            .expect("entry name")
            .to_str()
            .expect("entry UTF-8"),
    )
}
