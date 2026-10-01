//! Counts body checks for compiler.md §140 acceptance 4.

use std::cell::Cell;

thread_local! {
    static CHECKS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn record() {
    CHECKS.with(|checks| checks.set(checks.get() + 1));
}

#[test]
fn opaque_body_checks_are_linear_in_the_number_of_templates() {
    for templates in [30, 60, 120] {
        let mut source = String::new();
        for index in 0..templates {
            source.push_str(&format!("function f{index}<T>(x: T): void {{"));
            for next in index + 1..=(index + 3).min(templates - 1) {
                source.push_str(&format!("f{next}<T>(x);"));
            }
            source.push_str("}\n");
        }
        source.push_str("export function main(): void {}\n");
        CHECKS.with(|checks| checks.set(0));
        crate::check_program(&[crate::SourceFile::new("main.ts", source)])
            .expect("generic helpers with no program instance");
        let checks = CHECKS.with(Cell::get);
        println!("{templates} templates: {checks} body checks");

        // Both checker runs check each opaque root and main once (§140 rule 4a).
        assert_eq!(checks, 2 * (templates + 1), "{templates} templates");
    }
}
