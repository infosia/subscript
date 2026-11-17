//! Static snapshot records for finite watch plans (compiler.md §325 rules 5 and 8).
use super::internal;
use crate::layout::Layouts;
use subscript_compiler::{
    crossing::{WatchMember, WatchPlan},
    lir, Type,
};

#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub stride: u32,
    pub words: u32,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub offset: u32,
    pub word: u32,
    pub name: String,
    pub kind: Kind,
}
#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Slot {
        nullable: bool,
    },
    Pointer {
        child: Record,
        child_word: u32,
        capture: bool,
    },
    Embedded(Record),
    Array {
        element: Record,
    },
}

pub(crate) fn record(
    module: &lir::Module,
    layouts: &Layouts,
    plan: &WatchPlan,
) -> Result<Record, String> {
    if plan.recursive {
        return Err(internal("recursive watch needs the runtime walker"));
    }
    node(module, layouts, plan, plan.root)
}
fn node(
    module: &lir::Module,
    layouts: &Layouts,
    plan: &WatchPlan,
    index: usize,
) -> Result<Record, String> {
    let definition = &plan.nodes[index];
    let class = &module.classes[definition.class.0];
    let layout = layouts.class(definition.class.0)?;
    let mut result = Record {
        stride: layout.size,
        words: 0,
        entries: Vec::new(),
    };
    let mut slots = std::collections::HashMap::new();
    for member in &definition.members {
        let word = result.words;
        let (field, kind, word) = match member {
            WatchMember::Compare { field, nullable } => {
                slots.insert(*field, word);
                result.words = add_words(result.words, 1)?;
                (
                    *field,
                    Kind::Slot {
                        nullable: *nullable,
                    },
                    word,
                )
            }
            WatchMember::Pointer { field, target } => {
                let previous = slots.get(field).copied();
                if previous.is_none() {
                    result.words = add_words(result.words, 1)?;
                }
                let child_word = result.words;
                let child = node(module, layouts, plan, *target)?;
                result.words = add_words(result.words, child.words)?;
                (
                    *field,
                    Kind::Pointer {
                        child,
                        child_word,
                        capture: previous.is_none(),
                    },
                    previous.unwrap_or(word),
                )
            }
            WatchMember::Embedded { field, target }
            | WatchMember::Element { field, target, .. } => {
                let child = node(module, layouts, plan, *target)?;
                result.words = add_words(result.words, child.words)?;
                (*field, Kind::Embedded(child), word)
            }
            WatchMember::Array { field, target } => {
                result.words = add_words(result.words, 3)?;
                (
                    *field,
                    Kind::Array {
                        element: node(module, layouts, plan, *target)?,
                    },
                    word,
                )
            }
            other => return Err(internal(format!("unsupported watch member {other:?}"))),
        };
        let mut offset = layout.field_offsets[field];
        let mut name = class.fields[field].source_name.clone();
        if let WatchMember::Element { index, .. } = member {
            let Type::FixedArray(element, _) = &class.fields[field].ty else {
                return Err(internal("fixed watch has no element"));
            };
            offset += index * layouts.stride(element)?;
            name.push_str(&format!("[{index}]"));
        }
        result.entries.push(Entry {
            offset,
            word,
            name,
            kind,
        });
    }
    Ok(result)
}

fn add_words(words: u32, extra: u32) -> Result<u32, String> {
    words
        .checked_add(extra)
        .ok_or_else(|| internal("watch snapshot size overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{
        check_program,
        crossing::{call_plan, ParameterPlan},
        SourceFile,
    };

    #[test]
    fn a_pointer_route_reuses_its_compared_before_value() {
        let hir = check_program(&[
            SourceFile::ambient("record.d.ts", "// @subscript-c-header include=\"record.h\"\ndeclare class Leaf { userdata: object | null; constructor(userdata: object | null); }\ndeclare class Extension { base: Leaf; value: i32; constructor(base: Leaf, value: i32); }\ndeclare class Root { leaf: Leaf | null; constructor(leaf: Leaf | null); }\ndeclare function fill(out: Root | null): void;"),
            SourceFile::new("main.ts", "export function main(): void {}"),
        ]).expect("header family checks");
        let module = crate::lir::lower_module(&hir).expect("lowers");
        let declaration = module
            .foreign_functions
            .iter()
            .find(|f| f.source_name == "fill")
            .expect("fill");
        let plan = call_plan(&module, declaration.id).expect("plan");
        let ParameterPlan::Pointer(pointer) = &plan.parameters[0] else {
            panic!("pointer plan")
        };
        let layouts = Layouts::build_lir(&module).expect("layouts");
        let record = record(
            &module,
            &layouts,
            pointer.watch.as_ref().expect("pointer watch"),
        )
        .expect("finite record");
        assert_eq!(
            record.words, 2,
            "one before word for each of the two watched slots"
        );
        assert_eq!(record.entries[0].word, 0);
        let Kind::Pointer {
            child_word,
            capture,
            child,
        } = &record.entries[1].kind
        else {
            panic!("pointer route")
        };
        assert_eq!(
            record.entries[1].word, 0,
            "the route uses the compared pointer value"
        );
        assert_eq!(*child_word, 1);
        assert!(!capture);
        assert_eq!(child.words, 1);
    }
}
