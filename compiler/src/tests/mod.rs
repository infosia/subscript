use super::*;

fn check_one(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::new("test.ts", src)])
}

mod collections;
mod language;
mod poisoned_containers;
