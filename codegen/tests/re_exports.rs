//! Input file order cannot change export identities (compiler.md §128 rule 4).

#[allow(dead_code)]
mod corpus;

use subscript_codegen::{interpreter::interpret, lir::lower_module};
use subscript_compiler::check_program;

#[test]
fn every_input_order_resolves_the_same_declarations_with_a_firing_control() {
    let start = std::time::Instant::now();
    let root = corpus::corpus_accept();
    let files = corpus::entry_sources(&root, "a288-re-export-kinds");
    let expected = std::fs::read(root.join("a288-re-export-kinds.expected")).unwrap();
    // The interpreter checks 24 orders without repeating native compilation.
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let order = [a, b, c, d];
                    if (0..4).any(|i| order[..i].contains(&order[i])) {
                        continue;
                    }
                    let ordered: Vec<_> = order.iter().map(|i| files[*i].clone()).collect();
                    let module = check_program(&ordered).expect("every file order resolves");
                    let lir = lower_module(&module).expect("LIR");
                    assert_eq!(interpret(&lir).expect("interpreter"), expected, "{order:?}");
                }
            }
        }
    }
    let mut changed = files;
    let lib = changed.iter_mut().find(|f| f.name == "lib.ts").unwrap();
    lib.source = lib.source.replace("return 1;", "return 9;");
    let module = check_program(&changed).expect("changed source declaration");
    let lir = lower_module(&module).expect("control LIR");
    let control = interpret(&lir).expect("control interpreter");
    assert_ne!(control, expected);
    assert_eq!(
        control,
        String::from_utf8(expected)
            .unwrap()
            .replace("function=1", "function=9")
            .as_bytes()
    );
    eprintln!("export order test: {:?}", start.elapsed());
}

#[test]
fn directory_reader_rejects_unreachable_sources() {
    let started = std::time::Instant::now();
    let root = std::env::temp_dir().join(format!("subscript-unreachable-{}", std::process::id()));
    let entry = root.join("entry");
    std::fs::create_dir_all(&entry).unwrap();
    std::fs::write(entry.join("main.ts"), "export {};").unwrap();
    std::fs::write(entry.join("lost.ts"), "export const x: i32 = 1;").unwrap();
    let failure = std::panic::catch_unwind(|| corpus::entry_sources(&root, "entry"))
        .expect_err("an unreachable module must fail");
    let message = failure.downcast_ref::<String>().unwrap();
    assert_eq!(message, "entry: unreachable corpus source lost.ts");
    std::fs::write(entry.join("main.ts"), "export { x } from './lost';").unwrap();
    assert_eq!(corpus::entry_sources(&root, "entry").len(), 2);
    std::fs::remove_dir_all(root).unwrap();
    eprintln!("unreachable source test: {:?}", started.elapsed());
}
