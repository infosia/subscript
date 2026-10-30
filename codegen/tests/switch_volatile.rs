//! MSVC switch scrutinees preserve shadow-root stores (§210).

use subscript_codegen::emit_c;
use subscript_compiler::{check_program, SourceFile};

#[test]
fn every_emitted_switch_has_the_msvc_only_volatile_form() {
    // Cost: Check and emit one small program; no native compiler or corpus walk.
    let started = std::time::Instant::now();
    let source = include_str!("../../corpus/accept/a383-exhaustive-enum-switch.ts");
    let module =
        check_program(&[SourceFile::new("switch.ts", source)]).expect("switch program checks");
    let c = emit_c(&module).expect("switch program emits C").source;
    let condition = "#if defined(_MSC_VER) && !defined(__clang__)\n\
                     #define SUBSCRIPT_SWITCH_VOLATILE volatile\n\
                     #else\n\
                     #define SUBSCRIPT_SWITCH_VOLATILE\n\
                     #endif\n";
    assert!(c.contains(condition), "volatile applies only to MSVC");
    assert_eq!(c.matches("#define SUBSCRIPT_SWITCH_VOLATILE").count(), 2);

    let lines: Vec<_> = c.lines().map(str::trim).collect();
    let mut switches = 0;
    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("switch (") {
            continue;
        }
        switches += 1;
        assert_eq!(*line, "switch (_disc) {");
        let declaration = lines[index - 1];
        assert!(
            declaration.starts_with("SUBSCRIPT_SWITCH_VOLATILE ")
                && declaration.contains(" _disc = ")
                && declaration.ends_with(';'),
            "switch lacks its qualified local: {declaration}"
        );
    }
    assert!(switches > 0, "the program must exercise the switch emitter");
    eprintln!(
        "switch form: {switches} switches; elapsed {:?}",
        started.elapsed()
    );
}
