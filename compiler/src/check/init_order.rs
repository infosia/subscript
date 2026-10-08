//! Derives the module initializer order from all import dependencies.

use std::collections::HashMap;

use swc_ecma_ast as ast;

use crate::parse::ParsedProgram;

use super::normalize_module_specifier;

pub(super) fn files(prog: &ParsedProgram, entry: Option<usize>) -> Vec<usize> {
    let stems: HashMap<_, _> = prog
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.dts)
        .map(|(index, file)| (file.stem.as_str(), index))
        .collect();
    let mut visited = vec![false; prog.files.len()];
    let mut order = Vec::new();

    fn visit(
        index: usize,
        prog: &ParsedProgram,
        stems: &HashMap<&str, usize>,
        visited: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        if visited[index] {
            return;
        }
        visited[index] = true;
        for item in &prog.files[index].module.body {
            let source = match item {
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) => {
                    Some(import.src.value.as_str())
                }
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportNamed(export)) => {
                    export.src.as_ref().map(|source| source.value.as_str())
                }
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportAll(export)) => {
                    Some(export.src.value.as_str())
                }
                _ => None,
            };
            if let Some(next) = source.and_then(|source| {
                stems
                    .get(normalize_module_specifier(source).as_str())
                    .copied()
            }) {
                visit(next, prog, stems, visited, order);
            }
        }
        order.push(index);
    }

    if let Some(entry) = entry {
        visit(entry, prog, &stems, &mut visited, &mut order);
    }
    for (index, file) in prog.files.iter().enumerate() {
        if !file.dts {
            visit(index, prog, &stems, &mut visited, &mut order);
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
