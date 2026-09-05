//! C-emission scaling on unused edge parameters and coroutine suspensions.

use std::hint::black_box;
use std::time::Instant;

use subscript_codegen::{lir::lower_module, CProgram};
use subscript_compiler::{check_program, lir as l, SourceFile, Type};

fn chain_module(n: u32) -> l::Module {
    let source = format!(
        "function* chain(): Generator<i32> {{ {} }} export function main(): void {{}}",
        "yield 1;".repeat(n as usize)
    );
    let hir = check_program(&[SourceFile::new("emission-chain.ts", source)]).unwrap();
    let mut module = lower_module(&hir).unwrap();
    let coroutine = module.functions.iter().find(|f| f.is_generator).unwrap();
    assert_eq!(
        coroutine
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
            .count(),
        n as usize
    );
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.source_name == "main")
        .unwrap();
    let pos = function.pos.clone();
    function.values = (0..2 * n - 1)
        .map(|id| l::Value {
            id: l::ValueId(id),
            ty: l::ValueType::Data(Type::I32),
            fresh_owner: false,
            source_name: None,
        })
        .collect();
    function.blocks = (0..n)
        .map(|id| l::BasicBlock {
            id: l::BlockId(id),
            source_name: None,
            parameters: if id == 0 {
                Vec::new()
            } else {
                vec![l::ValueId(n + id - 1)]
            },
            instructions: vec![l::Instruction {
                result: Some(l::ValueId(id)),
                kind: l::InstructionKind::Copy,
                operands: vec![l::Operand::Constant(l::Constant {
                    ty: Type::I32,
                    kind: l::ConstantKind::Integer(1),
                })],
                invalidates: Vec::new(),
                traps: Vec::new(),
                pos: pos.clone(),
            }],
            terminator: if id + 1 == n {
                l::Terminator::Return {
                    value: None,
                    pos: pos.clone(),
                }
            } else {
                l::Terminator::Branch(l::BlockTarget {
                    block: l::BlockId(id + 1),
                    arguments: vec![l::Operand::Value(l::ValueId(id))],
                })
            },
        })
        .collect();
    function.liveness = l::Liveness {
        live_ins: vec![Vec::new(); n as usize],
        value_origins: (0..2 * n - 1).map(l::ValueId).collect(),
    };
    module
}

fn median_emission_seconds(n: u32) -> f64 {
    let module = chain_module(n);
    // The public wrapper calls emit_lir_c; fixture construction is outside the timer.
    black_box(CProgram::from_lir(&module, true).unwrap());
    let mut samples = [0.0; 7];
    for sample in &mut samples {
        let start = Instant::now();
        let program = CProgram::from_lir(black_box(&module), true).unwrap();
        *sample = start.elapsed().as_secs_f64();
        black_box(program);
    }
    samples.sort_by(f64::total_cmp);
    samples[3]
}

#[test]
#[ignore = "release scaling measurement; run without concurrent builds or tests"]
fn emission_chain_scales_with_block_count() {
    let small = median_emission_seconds(64);
    let large = median_emission_seconds(256);
    let ratio = large / small;
    eprintln!("emit_lir_c: N=64 {small:.9}s; N=256 {large:.9}s; ratio={ratio:.4}; block ratio=4");
    assert!(
        ratio <= 8.0,
        "C emission ratio {ratio:.4} exceeds 8 (2 × block ratio)"
    );
}
