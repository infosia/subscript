//! Capture helper (compiler.md §2): runs one accept-corpus entry
//! under the dev JIT and writes the raw stdout bytes of the run to
//! this process's stdout. Exit status: 0 on normal completion, 1 on
//! trap or rejection, 2 on usage/IO errors.
//!
//! Usage: `cargo run --offline -p subscript-codegen --bin capture -- <entry-id>`
//! e.g. `capture a22-matrix-propagation`. The orchestrator redirects
//! stdout into the golden file after review; this tool never writes
//! `.expected` files itself.
//!
//! Interop entries additionally use `--features capture-interop`, which
//! links the synthetic native fixture into this capture process only.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use subscript_codegen::run_jit;
#[cfg(all(feature = "capture-interop", not(all(windows, target_env = "msvc"))))]
use subscript_codegen::{run_jit_with_native_libraries, NativeLibrary};
use subscript_compiler::SourceFile;

#[cfg(all(feature = "capture-interop", not(all(windows, target_env = "msvc"))))]
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
    fn subProbeSetBindGroupCheck();
}

fn references_interop(source: &str) -> bool {
    const TOKENS: &[&str] = &[
        "subDevice",
        "subChainPayloadValue",
        "subSlice",
        "SubDrawList",
        "subDrawListTotal",
        "SUB_ACCESS",
        "subAccessMatches",
        "subBulk",
        "SUB_STAGE",
        "subStageMatches",
        "subFutureMake",
        "subStatsMake",
        "SubQueryStatus",
        "SubWaitEntry",
        "subBoundaryString",
        "subProbeTexture",
        "subProbePipelineLayout",
        "subProbeBindGroupEntry",
        "subProbeComputePipeline",
        "subProbeRenderPipeline",
        "subProbeProgrammableStage",
        "subProbeFullRenderPipeline",
        "subProbeQueueSubmit",
        "subProbeSetBindGroup",
    ];
    TOKENS.iter().any(|token| source.contains(token))
}

#[cfg(all(feature = "capture-interop", not(all(windows, target_env = "msvc"))))]
fn interop_library() -> NativeLibrary {
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
        (
            "subProbeSetBindGroupCheck".to_string(),
            subProbeSetBindGroupCheck as *const u8,
        ),
    ];
    // SAFETY: the opt-in fixture dependency links each static-lifetime
    // function above into this capture process, with signatures matching the
    // committed mirror and header.
    unsafe {
        NativeLibrary::new(
            vec![directory.clone()],
            vec![directory.join("interop.c")],
            symbols,
        )
    }
}

fn main() -> ExitCode {
    let Some(id) = std::env::args().nth(1) else {
        eprintln!("usage: capture <entry-id>   (e.g. capture a22-matrix-propagation)");
        return ExitCode::from(2);
    };
    let accept = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/accept");

    let dir = accept.join(&id);
    let mut sources: Vec<SourceFile> = if dir.is_dir() {
        let mut names: Vec<String> = match fs::read_dir(&dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".ts"))
                .collect(),
            Err(e) => {
                eprintln!("capture: read {}: {e}", dir.display());
                return ExitCode::from(2);
            }
        };
        names.sort();
        names.sort_by_key(|n| !n.contains("main"));
        let mut out = Vec::new();
        for n in names {
            match fs::read_to_string(dir.join(&n)) {
                Ok(text) => out.push(SourceFile::new(n, text)),
                Err(e) => {
                    eprintln!("capture: read {n}: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        out
    } else {
        let path = accept.join(format!("{id}.ts"));
        match fs::read_to_string(&path) {
            Ok(text) => vec![SourceFile::new(format!("{id}.ts"), text)],
            Err(e) => {
                eprintln!("capture: read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
    };

    let interop = sources
        .iter()
        .any(|source| references_interop(&source.source));
    if interop {
        let mirror = accept.join("../interop/interop.generated.d.ts");
        let text = match fs::read_to_string(&mirror) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("capture: read {}: {e}", mirror.display());
                return ExitCode::from(2);
            }
        };
        sources.insert(0, SourceFile::ambient("interop.generated.d.ts", text));
    }

    let result = if interop {
        #[cfg(all(feature = "capture-interop", not(all(windows, target_env = "msvc"))))]
        {
            run_jit_with_native_libraries(&sources, &[interop_library()])
        }
        #[cfg(not(all(feature = "capture-interop", not(all(windows, target_env = "msvc")))))]
        {
            eprintln!(
                "capture: {id}: interop capture requires `--features capture-interop` \
                 and is unavailable on windows-msvc"
            );
            return ExitCode::from(2);
        }
    } else {
        run_jit(&sources)
    };

    match result {
        Ok(bytes) => {
            if std::io::stdout().write_all(&bytes).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("capture: {id}: {e}");
            ExitCode::FAILURE
        }
    }
}
