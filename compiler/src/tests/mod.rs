use super::*;

fn check_one(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::new("test.ts", src)])
}

mod collections;
mod equality;
mod implements_clause;
mod interface_constraint;
mod language;
mod local_functions;
mod method_assignment;
mod nullable_array;
mod nullable_string;
mod poisoned_containers;
mod private_names;
mod removal;
mod type_alias;

#[test]
fn check_options_reject_an_unknown_enabled_module() {
    let files = [SourceFile::entry(
        "main.ts",
        "export function main(): void {}",
    )];
    let mut options = CheckOptions::default();
    options.enabled_modules.push("node:fs".to_owned());
    let diagnostics = check_program_with(&files, &options).unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0]
        .message
        .contains("unknown standard module `node:fs`"));
    // Control: the one standard module name is accepted.
    options.enabled_modules = STANDARD_MODULES.iter().map(|m| (*m).to_owned()).collect();
    assert!(check_program_with(&files, &options).is_ok());
}
