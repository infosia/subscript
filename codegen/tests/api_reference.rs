//! Executable divergence witnesses for the generated API reference.

use std::process::Command;

use subscript_codegen::{run_jit, RunError};
use subscript_compiler::api_reference::{DivergenceWitness, WitnessOutcome, DIVERGENCE_WITNESSES};
use subscript_compiler::SourceFile;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Observed {
    Value(Vec<u8>),
    Trap,
}

fn expected(outcome: WitnessOutcome) -> Observed {
    match outcome {
        WitnessOutcome::Value(value) => Observed::Value(value.as_bytes().to_vec()),
        WitnessOutcome::Trap => Observed::Trap,
        _ => panic!("unknown witness outcome"),
    }
}

fn run_subscript(witness: &DivergenceWitness) -> Observed {
    match run_jit(&[SourceFile::new(
        format!("{}.ts", witness.id),
        witness.subscript,
    )]) {
        Ok(stdout) => Observed::Value(stdout),
        Err(RunError::Trap(_)) => Observed::Trap,
        Err(other) => panic!("{}: subscript witness did not execute: {other}", witness.id),
    }
}

fn run_node(witness: &DivergenceWitness) -> Observed {
    let output = Command::new("node")
        .args(["--input-type=commonjs", "--eval", witness.javascript])
        .output()
        .unwrap_or_else(|error| panic!("{}: run node: {error}", witness.id));
    if output.status.success() {
        Observed::Value(output.stdout)
    } else {
        Observed::Trap
    }
}

#[test]
fn every_generated_reference_witness_diverges_from_node() {
    assert!(
        !DIVERGENCE_WITNESSES.is_empty(),
        "the generated divergence reference must not be empty"
    );
    for witness in DIVERGENCE_WITNESSES {
        let subscript = run_subscript(witness);
        let node = run_node(witness);
        assert_eq!(
            subscript,
            expected(witness.subscript_outcome),
            "{}: subscript result drifted",
            witness.id
        );
        assert_eq!(
            node,
            expected(witness.javascript_outcome),
            "{}: Node result drifted",
            witness.id
        );
        assert_ne!(
            subscript, node,
            "{}: recorded divergence now agrees with Node",
            witness.id
        );
    }
}
