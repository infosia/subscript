use subscript_codegen::ReloadSession;

#[path = "support/operand_raise_program.rs"]
mod fixture;
use fixture::{new_program, program};

fn live_after_main(iterations: usize, immutable: bool) -> usize {
    let mut session = ReloadSession::new(&program(iterations, immutable)).expect("session");
    session.run_main().expect("main catches each raise");
    assert_eq!(
        session.take_output(),
        format!("1\ncaught {}\n", iterations * 2).as_bytes()
    );
    session.live_allocations()
}

// Cost: four development JIT sessions; 0.17 s on the development host.
#[test]
fn a_raising_operand_releases_earlier_generators() {
    let counts = [10, 50].map(|n| live_after_main(n, false));
    let control = [10, 50].map(|n| live_after_main(n, true));
    eprintln!("fresh generators: 10={}, 50={}", counts[0], counts[1]);
    eprintln!("immutable generators: 10={}, 50={}", control[0], control[1]);
    assert_eq!(counts, control, "no allocations beyond the control");
    assert_eq!(counts[1] - counts[0], control[1] - control[0]);
}

// Cost: four development JIT sessions; 0.10 s on the development host.
#[test]
fn a_raising_new_initializer_releases_earlier_generators() {
    let live = |n, immutable| {
        let mut session = ReloadSession::new(&new_program(n, immutable)).expect("session");
        session.run_main().expect("main catches each raise");
        assert_eq!(session.take_output(), format!("caught {n}\n").as_bytes());
        session.live_allocations()
    };
    let counts = [10, 50].map(|n| live(n, false));
    let control = [10, 50].map(|n| live(n, true));
    eprintln!("new fresh generators: 10={}, 50={}", counts[0], counts[1]);
    eprintln!(
        "new immutable generators: 10={}, 50={}",
        control[0], control[1]
    );
    assert_eq!(counts, control, "no allocations beyond the control");
    assert_eq!(counts[1] - counts[0], control[1] - control[0]);
}

// Cost: one check, one lowering, one interpreter run, and one native build; 0.23 s on the development host.
#[test]
fn raising_operands_keep_the_problem_program_output_in_the_ship_tier_and_interpreter() {
    let files = program(10, false);
    let hir = subscript_compiler::check_program(&files).expect("problem program checks");
    let lir = subscript_codegen::lir::lower_module(&hir).expect("problem program lowers");
    let expected = b"1\ncaught 20\n";
    assert_eq!(
        subscript_codegen::interpreter::interpret(&lir).expect("interpreter"),
        expected
    );
    assert_eq!(
        subscript_codegen::run_c_aot(&files).expect("ship tier"),
        expected
    );
}
