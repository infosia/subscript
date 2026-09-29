//! Declaration symbols and generic-instance identity (compiler.md §125).

use super::*;

use crate::hir::declaration_label as label;

fn encoded_identity(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    value
        .as_bytes()
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

pub(super) fn instance_symbol(key: &str, name: &str) -> String {
    format!("[[identity:instance:{}]]{name}", encoded_identity(key))
}

impl Checker<'_> {
    pub(super) fn declaration_symbol(&self, file: usize, name: &str) -> String {
        let module = encoded_identity(&self.prog.files[file].name);
        format!("[[identity:module:{module}]]{name}")
    }
}

/// Module order determines which host entry owns a name (compiler.md §125 rule 4).
pub(super) fn host_entry_diagnostics(module: &hir::Module) -> Vec<Diagnostic> {
    let mut entries = HashMap::<&str, &Pos>::new();
    let mut diagnostics = Vec::new();
    for function in &module.functions {
        if function.host_entry_trap_sites(module).is_none() {
            continue;
        }
        if let Some(first) = entries.get(function.name.as_str()) {
            let mut diagnostic = Diagnostic::new(
                RuleCode::S017,
                format!(
                    "duplicate host entry `{}` in modules `{}` and `{}`",
                    function.name, first.file, function.pos.file
                ),
                function.pos.clone(),
            );
            diagnostic.divergence = Some(Divergence::HostEntryCollision);
            diagnostics.push(diagnostic);
        } else {
            entries.insert(&function.name, &function.pos);
        }
    }
    diagnostics
}

pub(super) fn declaration_label<'a>(
    symbol: &str,
    declarations: impl Iterator<Item = (&'a str, &'a str, &'a Pos)> + Clone,
) -> String {
    let Some((_, name, pos)) = declarations.clone().find(|(key, _, _)| *key == symbol) else {
        return source_name(symbol);
    };
    label(
        name,
        pos,
        declarations.filter(|(_, other, _)| *other == name).count() > 1,
    )
}

pub(super) fn module_declaration_label(module: &hir::Module, symbol: &str) -> String {
    declaration_label(
        symbol,
        module
            .globals
            .iter()
            .map(|g| (g.symbol.as_str(), g.name.as_str(), &g.pos))
            .chain(
                module
                    .functions
                    .iter()
                    .map(|f| (f.symbol.as_str(), f.name.as_str(), &f.pos)),
            ),
    )
}

pub(super) fn class_member_label(
    classes: &[hir::ClassDef],
    class: &hir::ClassDef,
    member: &str,
) -> String {
    let name = format!("{}.{}", class.name, source_name(member));
    label(
        &name,
        &class.pos,
        classes
            .iter()
            .filter(|other| other.name == class.name)
            .count()
            > 1,
    )
}
