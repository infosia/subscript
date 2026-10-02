//! Namespace cycle input-order parity (§148): 26 ms in debug.
//! The interpreter checks six orders and one control without native compilation.
#[allow(dead_code)]
#[path = "corpus/mod.rs"]
mod corpus;

use subscript_codegen::{interpreter::interpret, lir::lower_module};
use subscript_compiler::check_program;

#[test]
fn namespace_cycle_input_orders_keep_initialization_with_a_firing_control() {
    let started = std::time::Instant::now();
    let accept = corpus::corpus_accept();
    let files = corpus::entry_sources(&accept, "a321-namespace-cycle");
    let expected = corpus::golden_bytes(&accept, "a321-namespace-cycle");
    for a in 0..3 {
        for b in 0..3 {
            for c in 0..3 {
                let order = [a, b, c];
                if (0..3).any(|i| order[..i].contains(&order[i])) {
                    continue;
                }
                let ordered: Vec<_> = order.iter().map(|i| files[*i].clone()).collect();
                let module = lower_module(&check_program(&ordered).unwrap()).unwrap();
                assert_eq!(interpret(&module).unwrap(), expected, "{order:?}");
            }
        }
    }
    let mut changed = files;
    let y = changed.iter_mut().find(|file| file.name == "y.ts").unwrap();
    y.source = y.source.replace("= 2;", "= 8;");
    let module = lower_module(&check_program(&changed).unwrap()).unwrap();
    let control = interpret(&module).unwrap();
    assert_ne!(control, expected);
    assert_eq!(control, b"y\nx\nmain\n9\n");
    eprintln!("namespace cycle input orders: {:?}", started.elapsed());
}
