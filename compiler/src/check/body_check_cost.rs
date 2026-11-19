//! Counts body checks for compiler.md §140 acceptance 4.

use oxc_span::Span;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    static CHECKS: Cell<usize> = const { Cell::new(0) };
    static LOOP_CHECKS: RefCell<BTreeMap<u32, usize>> = const { RefCell::new(BTreeMap::new()) };
}

pub(super) fn record_loop(span: Span) {
    LOOP_CHECKS.with(|checks| *checks.borrow_mut().entry(span.start).or_default() += 1);
}

/// §315 acceptance 7: count checks, independently of machine speed.
/// Cost: 0.04 seconds for three programs; no engine subprocess.
#[test]
fn failed_nested_loop_assumptions_have_linear_body_check_cost() {
    for depth in [1, 5, 10] {
        let mut source = String::from("class A{x:i32=1;} function mk():A|null{return null;} function read(e:A|null):void{if(e===null){return;}");
        for level in 0..depth {
            source.push_str(&format!(
                "for(let i{level}=0;i{level}<3;i{level}++){{e.x; e=new A();"
            ));
        }
        for _ in 0..depth {
            source.push_str("e=mk();}");
        }
        source.push('}');
        let files = [crate::SourceFile::new("nested.ts", source)];
        LOOP_CHECKS.with(|checks| checks.borrow_mut().clear());
        let started = std::time::Instant::now();
        let errors = crate::check_program(&files).expect_err("nullable back edges");
        let elapsed = started.elapsed();
        assert!(
            errors.iter().all(|error| error.code.as_str() == "S011"),
            "{errors:?}"
        );
        let checks =
            LOOP_CHECKS.with(|checks| checks.borrow().values().copied().collect::<Vec<_>>());
        assert_eq!(checks.len(), depth);
        let innermost = checks[depth - 1];
        println!(
            "s315 depth {depth}: {innermost} innermost body checks, {elapsed:?}; all={checks:?}"
        );
        // One preliminary pass, one assumed pass, and at most one retry per level.
        assert_eq!(innermost, depth + 2);
    }
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
