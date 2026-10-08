//! The verifier compares class release records with independently computed layout offsets and field types.

use super::*;
use subscript_compiler::{check_program, SourceFile};

#[test]
fn wrong_offset_or_omitted_counted_field_fails_and_a_complete_description_passes() {
    let checked = check_program(&[SourceFile::new(
        "description.ts",
        "export function main():void{}",
    )])
    .expect("checked source");
    for variant in [0, 1, 2] {
        let complete = variant != 0;
        let mut module = lower_module(&checked).expect("valid base module");
        let ty = Type::Array(Box::new(Type::FixedArray(
            Box::new(Type::AsyncHandle(Box::new(Type::Void))),
            2,
        )));
        let id = ClassId(module.classes.len());
        module.classes.push(l::Class {
            id,
            source_name: "Holder".into(),
            is_value: false,
            is_descriptor: false,
            is_boundary: false,
            copies_boundary_bytes: false,
            boundary_header: None,
            is_embedded_header: false,
            callback_lifetime: subscript_compiler::types::CallbackLifetime::Context,
            alignment: None,
            fields: vec![l::Field {
                id: l::FieldId(0),
                source_name: "jobs".into(),
                ty: ty.clone(),
                is_defaulted: false,
                is_absence_capable: false,
                foreign_provenance: None,
                pos: Pos::new("description.ts", 1, 1),
            }],
            field_releases: if complete {
                vec![(
                    if variant == 2 { 8 } else { 0 },
                    l::CountAction::for_type(&ty),
                )]
            } else {
                Vec::new()
            },
            constructor: None,
            methods: Vec::new(),
            index_signature: None,
            pos: Pos::new("description.ts", 1, 1),
        });
        if variant == 1 {
            verify_module(&module).expect("complete field release description");
        } else {
            let errors = verify_module(&module).expect_err("invalid counted field description");
            assert!(errors.iter().any(|error| error.message
                == format!(
                    "class {} has an inconsistent counted field release description",
                    id.0
                )));
        }
    }
}

#[test]
fn generic_field_description_uses_the_substituted_type_and_c_layout() {
    let source = "class Box<T>{n:i32=7;v:T;constructor(v:T){this.v=v;}}async function work():Promise<void>{return;}export async function main():Promise<void>{const h=work();await h;const o=new Box<Promise<void>[]>([h]);Context.free(o);}";
    let checked = check_program(&[SourceFile::new("generic-description.ts", source)])
        .expect("checked source");
    let module = lower_module(&checked).expect("verified LIR");
    let class = module
        .classes
        .iter()
        .find(|class| !class.field_releases.is_empty())
        .expect("counted generic instance");
    assert_eq!(
        class.field_releases,
        vec![(
            8,
            l::CountAction::Counted(subscript_compiler::types::CountedType::Array(Box::new(
                subscript_compiler::types::CountedType::Handle
            )))
        )]
    );
    let layouts = crate::layout::Layouts::build_lir(&module).expect("layouts");
    let bytes = crate::counted::class_description(&layouts, class).expect("class description");
    let words = bytes
        .chunks_exact(8)
        .map(|word| u64::from_ne_bytes(word.try_into().expect("word")))
        .collect::<Vec<_>>();
    assert_eq!(words, [1, 8, 48, 2, 0, 0, 1, 0, 0]);
}

#[test]
fn a_counted_class_free_requires_a_call_trap() {
    let checked = check_program(&[SourceFile::new(
        "class-free.ts",
        "class Holder{v:Promise<void>[]=[];}export function main():void{}",
    )])
    .expect("checked source");
    let classes = lower_module(&checked).expect("verified class").classes;
    let holder = classes
        .iter()
        .find(|class| class.source_name == "Holder")
        .expect("holder")
        .id;
    let operation = hir::AmbientFn::ALL
        .iter()
        .position(|operation| *operation == hir::AmbientFn::UnsafeDelete)
        .expect("free operation");
    for guarded in [false, true] {
        let mut module = super::verifier_tests::hand_built_call_module_with_count(
            l::CallTargetKind::Intrinsic(l::Intrinsic {
                family: l::IntrinsicFamily::Ambient,
                operation: operation as u16,
                type_argument: None,
                worker_entry: None,
            }),
            Vec::new(),
            None,
            vec![l::ValueType::Data(Type::Class(holder))],
            None,
            None,
            if guarded {
                vec![l::Trap {
                    kind: l::TrapKind::Call,
                    pos: Pos::new("class-free.ts", 1, 1),
                }]
            } else {
                Vec::new()
            },
        );
        module.classes = classes.clone();
        let mut errors = Vec::new();
        super::verify_counted_operations::verify(&module, &module.functions[0], &mut errors);
        if guarded {
            assert!(errors.is_empty(), "{errors:?}");
        } else {
            assert!(errors
                .iter()
                .any(|error| error.message.contains("without a Call trap")));
        }
    }
}

#[test]
fn collect_requires_a_call_trap() {
    let operation = hir::AmbientFn::ALL
        .iter()
        .position(|op| *op == hir::AmbientFn::Collect)
        .expect("Collect");
    for guarded in [false, true] {
        let module = super::verifier_tests::hand_built_call_module_with_count(
            l::CallTargetKind::Intrinsic(l::Intrinsic {
                family: l::IntrinsicFamily::Ambient,
                operation: operation as u16,
                type_argument: None,
                worker_entry: None,
            }),
            Vec::new(),
            None,
            Vec::new(),
            None,
            None,
            if guarded {
                vec![l::Trap {
                    kind: l::TrapKind::Call,
                    pos: Pos::new("collect.ts", 1, 1),
                }]
            } else {
                Vec::new()
            },
        );
        let mut errors = Vec::new();
        super::verify_counted_operations::verify(&module, &module.functions[0], &mut errors);
        if guarded {
            assert!(errors.is_empty(), "{errors:?}");
        } else {
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains("without a Call trap")),
                "{errors:?}"
            );
        }
    }
}
