//! Finds an internal C symbol by its source function name.

pub fn symbol(module: &subscript_compiler::hir::Module, name: &str) -> String {
    // Internal C declarations carry LIR ids, without source names.
    let lir = subscript_codegen::lir::lower_module(module).expect("lower C function names");
    let function = lir
        .functions
        .iter()
        .find(|function| {
            function.kind == subscript_compiler::lir::FunctionKind::Free
                && function.source_name == name
        })
        .expect("named free function");
    format!("sub_f{}", function.id.0)
}
