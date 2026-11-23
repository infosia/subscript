//! Derives the module initializer order from all import dependencies.

use oxc_ast::ast;

use crate::parse::ParsedProgram;

pub(super) fn files(prog: &ParsedProgram, entry: Option<usize>) -> Vec<usize> {
    let mut visited = vec![false; prog.files.len()];
    let mut order = Vec::new();

    fn visit(index: usize, prog: &ParsedProgram, visited: &mut [bool], order: &mut Vec<usize>) {
        if visited[index] {
            return;
        }
        visited[index] = true;
        for item in &prog.files[index].module.body {
            let source = match item {
                ast::Statement::ImportDeclaration(import) => Some(import.source.value.as_str()),
                ast::Statement::ExportFromDeclaration(export) => Some(export.source.value.as_str()),
                ast::Statement::ExportAllDeclaration(export) => Some(export.source.value.as_str()),
                _ => None,
            };
            // The edge names the module that import resolution names (compiler.md §354 rule 6).
            if let Some(next) = source.and_then(|source| prog.import_target(index, source)) {
                visit(next, prog, visited, order);
            }
        }
        order.push(index);
    }

    if let Some(entry) = entry {
        visit(entry, prog, &mut visited, &mut order);
    }
    for (index, file) in prog.files.iter().enumerate() {
        if !file.dts {
            visit(index, prog, &mut visited, &mut order);
        }
    }
    order
}

pub(super) fn statement_pos(statement: &crate::hir::Stmt) -> Option<&crate::Pos> {
    use crate::hir::Stmt;
    match statement {
        Stmt::Expr(expression) => Some(&expression.pos),
        Stmt::Let { pos, .. }
        | Stmt::Return { pos, .. }
        | Stmt::If {
            pos,
            cond: _,
            then: _,
            els: _,
        }
        | Stmt::While {
            pos,
            cond: _,
            body: _,
        }
        | Stmt::DoWhile {
            pos,
            body: _,
            cond: _,
        }
        | Stmt::For {
            pos,
            init: _,
            cond: _,
            step: _,
            body: _,
        }
        | Stmt::ForOf {
            pos,
            name: _,
            ty: _,
            subject: _,
            kind: _,
            body: _,
        }
        | Stmt::GeneratorForOf {
            pos,
            name: _,
            ty: _,
            mutable: _,
            subject: _,
            body: _,
        }
        | Stmt::Switch {
            pos,
            disc: _,
            cases: _,
        }
        | Stmt::Throw { pos, .. }
        | Stmt::Try {
            pos,
            body: _,
            binding: _,
            handler: _,
        }
        | Stmt::Using {
            pos,
            bindings: _,
            body: _,
            finalizer: _,
        }
        | Stmt::Break(pos)
        | Stmt::Continue(pos) => Some(pos),
        Stmt::Block(body) => body.iter().find_map(statement_pos),
    }
}
