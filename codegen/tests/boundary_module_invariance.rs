//! OBS-3 §44.9: unrelated module declarations cannot change recursive
//! boundary lowering. The two pointer targets deliberately come from helper
//! returns, so their storage must remain valid after those helpers return.

#![cfg(not(all(windows, target_env = "msvc")))]

use std::fmt::Write as _;

#[path = "support/native_fixture.rs"]
mod native_fixture;

use subscript_codegen::{
    run_c_aot_with_native_libraries, run_jit_with_native_libraries,
};
use subscript_compiler::SourceFile;

const MIRROR: &str = include_str!("../../corpus/interop/interop.generated.d.ts");
const EXPECTED: &[u8] = b"16\n1\n301\n302\n1\n101\n102\n2\n103\n104\n201\n202\n2\n203\n204\n";
const PADDING_COUNTS: [usize; 6] = [20, 40, 60, 80, 100, 120];

fn program(padding: usize) -> String {
    let mut source = String::from(
        "function makeDepth(): SGPUProbeBreadthDepthStencilState {\n\
           return new SGPUProbeBreadthDepthStencilState(\n\
             new SGPUProbeBreadthNestedState(101, 102),\n\
             [103, 104],\n\
           );\n\
         }\n\
         function makeFragment(): SGPUProbeBreadthFragmentState {\n\
           return new SGPUProbeBreadthFragmentState(\n\
             new SGPUProbeBreadthNestedState(201, 202),\n\
             [203, 204],\n\
           );\n\
         }\n\
         function makeDescriptor(): SGPUProbeBreadthRenderPipelineDescriptor {\n\
           return new SGPUProbeBreadthRenderPipelineDescriptor(\n\
             \"module-invariant\",\n\
             makeDepth(),\n\
             new SGPUProbeBreadthPrimitiveState(301, 302),\n\
             makeFragment(),\n\
           );\n\
         }\n\
         export function main(): void {\n\
           let selector: u32 = 1;\n\
           while (selector <= 15) {\n\
             print(`${subProbeBreadthRenderPipelineCheck(makeDescriptor(), selector)}`);\n\
             selector = selector + 1;\n\
           }\n\
         }\n",
    );
    for index in 0..padding {
        writeln!(
            source,
            "function pad{index}(v: u32): u32 {{ return v + {index}; }}"
        )
        .unwrap();
    }
    source
}

fn files(padding: usize) -> [SourceFile; 2] {
    [
        SourceFile::ambient("interop.generated.d.ts", MIRROR),
        SourceFile::new("boundary-module-invariance.ts", program(padding)),
    ]
}

#[test]
fn two_pointer_descriptor_output_is_invariant_under_uncalled_function_padding() {
    let mut reference: Option<Vec<u8>> = None;
    for padding in PADDING_COUNTS {
        let jit = run_jit_with_native_libraries(&files(padding), &[native_fixture::library()])
            .unwrap_or_else(|error| panic!("N={padding} dev-JIT run failed: {error}"));
        let ship =
            run_c_aot_with_native_libraries(&files(padding), &[native_fixture::library()])
                .unwrap_or_else(|error| panic!("N={padding} ship-C-AOT run failed: {error}"));
        assert_eq!(jit, EXPECTED, "N={padding} boundary observations are wrong");
        assert_eq!(ship, jit, "N={padding} tier outputs differ");
        if let Some(expected) = &reference {
            assert_eq!(&jit, expected, "N={padding} changed the program output");
        } else {
            reference = Some(jit);
        }
    }
}

