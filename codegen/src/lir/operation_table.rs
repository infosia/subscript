//! Immutable operation rows and indexed builder lookups.
use super::*;
use std::sync::OnceLock;

struct Table {
    rows: Vec<l::IntrinsicOperation>,
    index: HashMap<(usize, u16), usize>,
}
static TABLE: OnceLock<Table> = OnceLock::new();
#[cfg(test)]
thread_local! {
    static ROW_COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn table() -> &'static Table {
    TABLE.get_or_init(|| {
        let rows = build_rows();
        let index = rows
            .iter()
            .enumerate()
            .map(|(i, row)| ((row.family as usize, row.operation), i))
            .collect();
        Table { rows, index }
    })
}

pub(super) fn intrinsic_operations() -> Vec<l::IntrinsicOperation> {
    #[cfg(test)]
    ROW_COPIES.with(|copies| copies.set(copies.get() + 1));
    table().rows.clone()
}

pub(super) fn lookup(kind: &l::CallTargetKind) -> &'static [l::IntrinsicOperation] {
    let l::CallTargetKind::Intrinsic(intrinsic) = kind else {
        return &[];
    };
    let table = table();
    table
        .index
        .get(&(intrinsic.family as usize, intrinsic.operation))
        .map_or(&[], |&i| &table.rows[i..i + 1])
}

fn build_rows() -> Vec<l::IntrinsicOperation> {
    fn append<T: fmt::Debug>(
        table: &mut Vec<l::IntrinsicOperation>,
        family: l::IntrinsicFamily,
        values: &[T],
    ) {
        table.extend(
            values
                .iter()
                .enumerate()
                .map(|(operation, value)| l::IntrinsicOperation {
                    family,
                    operation: operation as u16,
                    semantic_name: format!("{value:?}"),
                    runtime_symbol: intrinsic_runtime_symbol(family, &format!("{value:?}"))
                        .map(str::to_string),
                    signatures: Vec::new(),
                }),
        );
    }

    let mut table = Vec::new();
    append(
        &mut table,
        l::IntrinsicFamily::Ambient,
        &hir::AmbientFn::ALL,
    );
    append(
        &mut table,
        l::IntrinsicFamily::ContextBytes,
        &hir::ContextBytesFn::ALL,
    );
    append(&mut table, l::IntrinsicFamily::Math, &hir::MathFn::ALL);
    append(&mut table, l::IntrinsicFamily::Number, &hir::NumFn::ALL);
    append(&mut table, l::IntrinsicFamily::Date, &hir::DateFn::ALL);
    append(&mut table, l::IntrinsicFamily::Json, &hir::JsonFn::ALL);
    append(&mut table, l::IntrinsicFamily::String, &hir::StrFn::ALL);
    append(&mut table, l::IntrinsicFamily::Regex, &hir::RegexFn::ALL);
    append(&mut table, l::IntrinsicFamily::Array, &hir::ArrFn::ALL);
    append(&mut table, l::IntrinsicFamily::Map, &hir::MapFn::ALL);
    append(&mut table, l::IntrinsicFamily::Set, &hir::SetFn::ALL);
    table.extend(
        hir::WorkerFn::ALL
            .iter()
            .enumerate()
            .map(|(operation, value)| {
                let semantic_name = format!("{value:?}");
                l::IntrinsicOperation {
                    family: l::IntrinsicFamily::Worker,
                    operation: operation as u16,
                    semantic_name: semantic_name
                        .split_once('(')
                        .map_or(semantic_name.as_str(), |(name, _)| name)
                        .to_string(),
                    runtime_symbol: intrinsic_runtime_symbol(
                        l::IntrinsicFamily::Worker,
                        semantic_name
                            .split_once('(')
                            .map_or(semantic_name.as_str(), |(name, _)| name),
                    )
                    .map(str::to_string),
                    signatures: Vec::new(),
                }
            }),
    );
    append(&mut table, l::IntrinsicFamily::Text, &hir::TextFn::ALL);
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    #[test]
    fn many_calls_share_one_table_across_modules() {
        let source = "export function main(): void { const xs: i32[] = [1,2]; }";
        let calls = "xs.slice();".repeat(256);
        let source = source.replace("[1,2];", &format!("[1,2]; {calls}"));
        let hir = check_program(&[SourceFile::new("calls.ts", source)]).unwrap();
        ROW_COPIES.with(|copies| copies.set(0));
        let first = super::super::lower_module(&hir).unwrap();
        let calls = first
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind, l::InstructionKind::Call(_)))
            .count();
        assert!(calls >= 256);
        assert_eq!(ROW_COPIES.with(std::cell::Cell::get), 1);
        let second = super::super::lower_module(&hir).unwrap();
        assert_eq!(ROW_COPIES.with(std::cell::Cell::get), 2);
        assert_eq!(
            first.intrinsic_operations.len(),
            second.intrinsic_operations.len()
        );
        assert_ne!(
            first.intrinsic_operations.as_ptr(),
            second.intrinsic_operations.as_ptr()
        );
        for row in &first.intrinsic_operations {
            let target = l::CallTargetKind::Intrinsic(l::Intrinsic {
                family: row.family,
                operation: row.operation,
                type_argument: None,
                worker_entry: None,
            });
            assert_eq!(lookup(&target)[0].semantic_name, row.semantic_name);
        }
    }
}
