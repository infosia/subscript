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
