//! Tests for HIR structure and sites (compiler.md §20.2).

use super::*;
use crate::diag::Pos;

fn child_expr(value: i64) -> Expr {
    Expr {
        pending_work: None,
        kind: ExprKind::Int(value),
        ty: Type::I32,
        pos: Pos::new("children.ts", 1, 1),
    }
}

fn child_stmt(value: i64) -> Stmt {
    Stmt::Expr(child_expr(value))
}

fn child_values(children: Vec<HirChild<'_>>) -> Vec<i64> {
    children
        .into_iter()
        .map(|child| match child {
            HirChild::Expr(Expr {
                kind: ExprKind::Int(value),
                ..
            })
            | HirChild::Stmt(Stmt::Expr(Expr {
                kind: ExprKind::Int(value),
                ..
            })) => *value,
            other => panic!("unexpected test child {other:?}"),
        })
        .collect()
}

#[test]
fn flow_leaves_follow_only_value_positions() {
    let expression = Expr {
        pending_work: None,
        kind: ExprKind::Cond {
            cond: Box::new(child_expr(0)),
            then: Box::new(Expr {
                pending_work: None,
                kind: ExprKind::Cast(Box::new(child_expr(1))),
                ty: Type::I32,
                pos: Pos::new("flow.ts", 1, 1),
            }),
            els: Box::new(Expr {
                pending_work: None,
                kind: ExprKind::ArrayLit(vec![child_expr(2), child_expr(3)]),
                ty: Type::Array(Box::new(Type::I32)),
                pos: Pos::new("flow.ts", 1, 1),
            }),
        },
        ty: Type::I32,
        pos: Pos::new("flow.ts", 1, 1),
    };
    let values = expression
        .flow_leaves()
        .map(|leaf| match leaf.kind {
            ExprKind::Int(value) => value,
            _ => -1,
        })
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 2, 3]);
}

fn test_expr(kind: ExprKind) -> Expr {
    Expr {
        pending_work: None,
        kind,
        ty: Type::I32,
        pos: Pos::new("children.ts", 1, 1),
    }
}

#[test]
fn fresh_async_owner_expression_table_keeps_fresh_conditionals() {
    assert!(ExprKind::ArrayLit(Vec::new()).produces_fresh_async_owner());
    assert!(ExprKind::Cond {
        cond: Box::new(child_expr(0)),
        then: Box::new(test_expr(ExprKind::ArrayLit(Vec::new()))),
        els: Box::new(test_expr(ExprKind::ArraySpreadLit(Vec::new()))),
    }
    .produces_fresh_async_owner());
    assert!(!ExprKind::Cond {
        cond: Box::new(child_expr(0)),
        then: Box::new(test_expr(ExprKind::ArrayLit(Vec::new()))),
        els: Box::new(child_expr(1)),
    }
    .produces_fresh_async_owner());
}

#[test]
fn expr_carries_type_and_pos() {
    let e = Expr {
        pending_work: None,
        kind: ExprKind::Int(3),
        ty: Type::I32,
        pos: Pos::new("t.ts", 1, 1),
    };
    assert_eq!(e.ty, Type::I32);
    assert_eq!(e.pos.line, 1);
}

#[test]
fn expr_children_yield_every_child() {
    let leaf_kinds = vec![
        ExprKind::Int(0),
        ExprKind::Float(0.0),
        ExprKind::Bool(false),
        ExprKind::Str(String::new()),
        ExprKind::Null,
        ExprKind::This,
        ExprKind::Local(String::new(), Type::I32, false),
        ExprKind::Global(Symbol::from_full_text("")),
        ExprKind::FuncRef(Symbol::from_full_text("")),
        ExprKind::EnumMember {
            id: EnumId(0),
            member: String::new(),
            value: 0,
        },
        ExprKind::Zero,
        ExprKind::Unassigned,
        ExprKind::RawNew { class: ClassId(0) },
        ExprKind::Yield(None),
        ExprKind::AsyncSuspend,
    ];
    for kind in leaf_kinds {
        assert!(test_expr(kind).children().is_empty());
    }

    let cases = vec![
        (
            ExprKind::Unary {
                op: UnOp::Neg,
                operand: Box::new(child_expr(1)),
            },
            vec![1],
        ),
        (
            ExprKind::Binary {
                op: BinOp::Add,
                left: Box::new(child_expr(1)),
                right: Box::new(child_expr(2)),
            },
            vec![1, 2],
        ),
        (
            ExprKind::Assign {
                update: None,
                op: None,
                target: Box::new(child_expr(1)),
                value: Box::new(child_expr(2)),
            },
            vec![1, 2],
        ),
        (ExprKind::Cast(Box::new(child_expr(1))), vec![1]),
        (
            ExprKind::Call {
                callee: Callee::Value(Box::new(child_expr(1))),
                args: vec![child_expr(2)],
            },
            vec![1, 2],
        ),
        (
            ExprKind::Call {
                callee: Callee::Method {
                    recv: Box::new(child_expr(1)),
                    name: Symbol::from_full_text(""),
                },
                args: vec![child_expr(2)],
            },
            vec![1, 2],
        ),
        (
            ExprKind::New {
                class: ClassId(0),
                args: vec![child_expr(1), child_expr(2)],
            },
            vec![1, 2],
        ),
        (
            ExprKind::DescriptorLit {
                class: ClassId(0),
                fields: vec![Some(child_expr(1)), None, Some(child_expr(2))],
            },
            vec![1, 2],
        ),
        (
            ExprKind::Field {
                obj: Box::new(child_expr(1)),
                name: String::new(),
            },
            vec![1],
        ),
        (ExprKind::Length(Box::new(child_expr(1))), vec![1]),
        (
            ExprKind::Index {
                element_key: None,
                obj: Box::new(child_expr(1)),
                index: Box::new(child_expr(2)),
                checked: true,
            },
            vec![1, 2],
        ),
        (
            ExprKind::ArrayLit(vec![child_expr(1), child_expr(2)]),
            vec![1, 2],
        ),
        (
            ExprKind::ArraySpreadLit(vec![
                ArrayLitElem {
                    expr: child_expr(1),
                    spread: None,
                },
                ArrayLitElem {
                    expr: child_expr(2),
                    spread: Some(SpreadKind::Array),
                },
            ]),
            vec![1, 2],
        ),
        (
            ExprKind::Template(vec![
                TplPart::Text(String::new()),
                TplPart::Expr(child_expr(1)),
                TplPart::Expr(child_expr(2)),
            ]),
            vec![1, 2],
        ),
        (
            ExprKind::Lambda {
                is_async: false,
                id: LambdaId(0),
                params: Vec::new(),
                ret: Type::Void,
                body: vec![child_stmt(1), child_stmt(2)],
                captures: Vec::new(),
                can_raise: false,
            },
            vec![1, 2],
        ),
        (ExprKind::Yield(Some(Box::new(child_expr(1)))), vec![1]),
        (
            ExprKind::AsyncCall {
                callee: AsyncCallee::Method {
                    class: ClassId(0),
                    receiver: Box::new(child_expr(1)),
                    name: Symbol::from_full_text(""),
                },
                args: vec![child_expr(2)],
            },
            vec![1, 2],
        ),
        (
            ExprKind::AsyncHandleCreate {
                callee: AsyncCallee::Method {
                    class: ClassId(0),
                    receiver: Box::new(child_expr(1)),
                    name: Symbol::from_full_text(""),
                },
                args: vec![child_expr(2)],
                origin: 0,
            },
            vec![1, 2],
        ),
        (ExprKind::AsyncHandleAwait(Box::new(child_expr(1))), vec![1]),
        (
            ExprKind::AsyncHandleTransfer {
                value: Box::new(child_expr(1)),
                origin: 0,
            },
            vec![1],
        ),
        (
            ExprKind::Cond {
                cond: Box::new(child_expr(1)),
                then: Box::new(child_expr(2)),
                els: Box::new(child_expr(3)),
            },
            vec![1, 2, 3],
        ),
    ];
    for (kind, expected) in cases {
        assert_eq!(child_values(test_expr(kind).children()), expected);
    }
}

#[test]
fn stmt_children_yield_every_child() {
    let pos = Pos::new("children.ts", 1, 1);
    let cases = vec![
        (
            Stmt::Let {
                name: String::new(),
                ty: Type::I32,
                mutable: false,
                dispose: false,
                init: child_expr(1),
                pos: pos.clone(),
            },
            vec![1],
        ),
        (Stmt::Expr(child_expr(1)), vec![1]),
        (
            Stmt::Return {
                value: Some(child_expr(1)),
                pos: pos.clone(),
            },
            vec![1],
        ),
        (
            Stmt::Return {
                value: None,
                pos: pos.clone(),
            },
            Vec::new(),
        ),
        (
            Stmt::If {
                cond: child_expr(1),
                then: vec![child_stmt(2)],
                els: Some(vec![child_stmt(3)]),
                pos: pos.clone(),
            },
            vec![1, 2, 3],
        ),
        (
            Stmt::While {
                cond: child_expr(1),
                body: vec![child_stmt(2)],
                pos: pos.clone(),
            },
            vec![1, 2],
        ),
        (
            Stmt::For {
                init: Some(Box::new(child_stmt(1))),
                cond: Some(child_expr(2)),
                step: Some(child_expr(3)),
                body: vec![child_stmt(4)],
                pos: pos.clone(),
            },
            vec![1, 2, 3, 4],
        ),
        (
            Stmt::ForOf {
                name: String::new(),
                ty: Type::I32,
                subject: child_expr(1),
                kind: ForOfKind::ArrayValues,
                body: vec![child_stmt(2)],
                pos: pos.clone(),
            },
            vec![1, 2],
        ),
        (
            Stmt::Switch {
                disc: child_expr(1),
                cases: vec![SwitchCase {
                    test: Some(child_expr(2)),
                    body: vec![child_stmt(3)],
                    pos: pos.clone(),
                }],
                pos: pos.clone(),
            },
            vec![1, 2, 3],
        ),
        (Stmt::Block(vec![child_stmt(1), child_stmt(2)]), vec![1, 2]),
        (Stmt::Break(pos.clone()), Vec::new()),
        (Stmt::Continue(pos.clone()), Vec::new()),
        (
            Stmt::Throw {
                value: child_expr(1),
                pos: pos.clone(),
            },
            vec![1],
        ),
        (
            Stmt::Try {
                body: vec![child_stmt(1)],
                binding: Some(("e".to_string(), Type::I32)),
                handler: vec![child_stmt(2)],
                pos: pos.clone(),
            },
            vec![1, 2],
        ),
        (
            Stmt::Using {
                bindings: vec![UsingBinding::new(
                    "r".to_string(),
                    Type::I32,
                    None,
                    pos.clone(),
                )],
                body: vec![child_stmt(1), child_stmt(2)],
                pos,
            },
            vec![1, 2],
        ),
    ];
    for (stmt, expected) in cases {
        assert_eq!(child_values(stmt.children()), expected);
    }
}

#[test]
fn host_entry_trap_sites_name_each_wire_parameter() {
    let parameter_pos = Pos::new("wire-entry.ts", 3, 27);
    let function = Function {
        synthesized_helper: false,
        symbol: Symbol::from_full_text("configure"),
        name: "configure".to_string(),
        can_raise: false,
        exported: true,
        is_generator: false,
        is_async: false,
        params: vec![Param {
            escapes: false,
            default_can_raise: false,
            name: "mode".to_string(),
            ty: Type::StringAlias(crate::types::StringAliasId(0)),
            default: None,
            foreign_provenance: None,
            pos: parameter_pos.clone(),
        }],
        ret: Type::Void,
        body: Vec::new(),
        pos: Pos::new("wire-entry.ts", 3, 1),
    };
    let module = Module {
        host_entries: Vec::new(),
        entry_pos: Pos::new("test.ts", 1, 1),
        poisoned_imports: Vec::new(),
        synthesized_helpers: Default::default(),
        classes: Vec::new(),
        enums: Vec::new(),
        string_aliases: vec![StringAliasDef {
            name: "WireMode".to_string(),
            members: vec!["m0".to_string()],
            wire_values: Some(vec![16]),
            pos: Pos::new("wire-entry.ts", 1, 1),
        }],
        globals: Vec::new(),
        functions: vec![function.clone()],
        worker_entries: Vec::new(),
        operation_signatures: Vec::new(),
        foreign_fns: Vec::new(),
        foreign_mirrors: Vec::new(),
        top_level: Vec::new(),
        initializer_modules: Vec::new(),
        regex_literal_globals: Vec::new(),
        initializer_segments: Vec::new(),
        initializer_can_raise: false,
        source_bytes: 0,
    };
    assert_eq!(
        function.host_entry_trap_sites(&module),
        Some(vec![TrapSite::WireEnumValue {
            alias: crate::types::StringAliasId(0),
            pos: parameter_pos,
        }])
    );
}

#[test]
fn math_fn_all_is_indexed_by_discriminant() {
    // Runtime-import tables index by `f as usize`; the ALL order
    // must therefore equal declaration order.
    for (i, f) in MathFn::ALL.iter().enumerate() {
        assert_eq!(*f as usize, i, "MathFn::ALL out of order at {i}");
    }
}

#[test]
fn math_fn_arity_matches_the_contract() {
    assert_eq!(MathFn::Abs.arity(), 1);
    assert_eq!(MathFn::Atan2.arity(), 2);
    assert_eq!(MathFn::Hypot.arity(), 2);
    assert_eq!(MathFn::Pow.arity(), 2);
    assert_eq!(MathFn::Max.arity(), 2);
    assert_eq!(MathFn::Min.arity(), 2);
    assert_eq!(MathFn::Random.arity(), 0);
    assert_eq!(MathFn::Clz32.arity(), 1);
    assert_eq!(MathFn::Imul.arity(), 2);
    assert_eq!(MathFn::Fround.arity(), 1);
    assert_eq!(MathFn::F32ToBits.arity(), 1);
    assert_eq!(MathFn::F32FromBits.arity(), 1);
    assert_eq!(MathFn::Random.name(), "random");
    assert_eq!(MathFn::Log1p.name(), "log1p");
    assert_eq!(MathFn::Clz32.name(), "clz32");
    assert_eq!(MathFn::Imul.name(), "imul");
    assert_eq!(MathFn::Fround.name(), "fround");
    assert_eq!(MathFn::F32ToBits.name(), "f32ToBits");
    assert_eq!(MathFn::F32FromBits.name(), "f32FromBits");
    assert_eq!(MathFn::F32ToBits.symbol(), "subscript_rt_math_f32_to_bits");
    assert_eq!(
        MathFn::F32FromBits.symbol(),
        "subscript_rt_math_f32_from_bits"
    );
}

#[test]
fn num_fn_table_matches_the_section_11_contract() {
    for (index, f) in NumFn::ALL.iter().enumerate() {
        assert_eq!(*f as usize, index, "NumFn::ALL out of order at {index}");
        assert!(f.symbol().starts_with("subscript_rt_num_"));
    }
    assert!(NumFn::IsNaN.returns_bool());
    assert!(!NumFn::ParseFloat.returns_bool());
    assert!(NumFn::ParseInt.takes_pos_id());
    assert!(NumFn::ParseFloat.takes_pos_id());
    assert!(NumFn::ToFixed.takes_pos_id());
    assert!(NumFn::ToStringF32.takes_pos_id());
    assert!(NumFn::ToStringF64.takes_pos_id());
    assert!(NumFn::ToExponential.takes_pos_id());
    assert!(NumFn::ToPrecision.takes_pos_id());
    assert!(!NumFn::IsFinite.takes_pos_id());
}

#[test]
fn date_fn_field_codes_cover_the_eight_accessors_in_order() {
    // The subscript_rt_date_get field-code contract (stdlib.md §3): the
    // eight UTC accessors carry codes 0..=7 in accessor order; the
    // non-accessor operations carry none.
    let accessors = [
        DateFn::GetUtcFullYear,
        DateFn::GetUtcMonth,
        DateFn::GetUtcDate,
        DateFn::GetUtcDay,
        DateFn::GetUtcHours,
        DateFn::GetUtcMinutes,
        DateFn::GetUtcSeconds,
        DateFn::GetUtcMilliseconds,
    ];
    for (i, f) in accessors.iter().enumerate() {
        assert_eq!(f.field_code(), Some(i as u32), "field code of {}", f.name());
    }
    for f in [
        DateFn::New,
        DateFn::Utc,
        DateFn::Now,
        DateFn::ToIso,
        DateFn::ToUtcString,
    ] {
        assert_eq!(f.field_code(), None, "{} is not an accessor", f.name());
    }
    assert_eq!(DateFn::ToIso.name(), "toISOString");
    assert_eq!(DateFn::ToUtcString.name(), "toUTCString");
    assert_eq!(DateFn::Utc.name(), "UTC");
}

#[test]
fn callee_trap_policy_delegates_to_operation_predicates() {
    assert!(!Callee::Ambient(AmbientFn::Print).has_call_site());
    assert!(Callee::Ambient(AmbientFn::Unreachable).has_call_site());
    assert!(Callee::Ambient(AmbientFn::UnsafeDelete).has_call_site());
    assert!(!Callee::Math(MathFn::Abs).has_call_site());
    assert!(Callee::Num(NumFn::ParseInt).has_call_site());
    assert!(!Callee::Num(NumFn::IsFinite).has_call_site());
    assert!(Callee::Date(DateFn::New).has_call_site());
    assert!(Callee::Date(DateFn::ToUtcString).has_call_site());
    assert!(!Callee::Date(DateFn::Now).has_call_site());
    assert!(Callee::Json(JsonFn::Finish).has_call_site());
    assert!(Callee::Str(StrFn::CharCodeAt).has_call_site());
    assert!(!Callee::Str(StrFn::Includes).has_call_site());
    assert!(Callee::Arr(ArrFn::ForEach).has_call_site());
    assert!(!Callee::Arr(ArrFn::Reverse).has_call_site());
    assert!(Callee::Map(MapFn::Set).has_call_site());
    assert!(!Callee::Map(MapFn::Get).has_call_site());
    assert!(Callee::Set(SetFn::Union).has_call_site());
    assert!(!Callee::Set(SetFn::Has).has_call_site());
    assert!(Callee::Func(Symbol::from_full_text("script")).has_call_site());
    assert!(Callee::Foreign("host".to_string()).has_call_site());
}

#[test]
fn str_fn_all_is_indexed_by_discriminant() {
    // Runtime-import tables index by `f as usize`; the ALL order
    // must therefore equal declaration order.
    for (i, f) in StrFn::ALL.iter().enumerate() {
        assert_eq!(*f as usize, i, "StrFn::ALL out of order at {i}");
    }
}

#[test]
fn str_fn_shapes_match_the_section_8_contract() {
    use StrParam as P;
    // Post-normalization parameter spellings (stdlib.md §8).
    assert_eq!(StrFn::IndexOf.params(), &[P::Str, P::I32]);
    assert_eq!(StrFn::Includes.params(), &[P::Str, P::I32]);
    assert_eq!(StrFn::LastIndexOf.params(), &[P::Str, P::I32]);
    assert_eq!(StrFn::CharCodeAt.params(), &[P::I32]);
    assert_eq!(StrFn::Trim.params(), &[] as &[P]);
    assert_eq!(StrFn::PadStart.params(), &[P::I32, P::Str]);
    assert_eq!(StrFn::ReplaceAll.params(), &[P::Str, P::Str]);
    // Result spellings.
    assert_eq!(StrFn::IndexOf.ret(), StrRet::I32);
    assert_eq!(StrFn::CharCodeAt.ret(), StrRet::I32);
    assert_eq!(StrFn::Includes.ret(), StrRet::Bool);
    assert_eq!(StrFn::EndsWith.ret(), StrRet::Bool);
    assert_eq!(StrFn::Split.ret(), StrRet::StrArray);
    assert_eq!(StrFn::Trim.ret(), StrRet::Str);
    assert_eq!(StrFn::ReplaceAll.ret(), StrRet::Str);
    // pos_id: only the five pure search predicates take none.
    for f in StrFn::ALL {
        let pure = matches!(
            f,
            StrFn::IndexOf
                | StrFn::LastIndexOf
                | StrFn::Includes
                | StrFn::StartsWith
                | StrFn::EndsWith
        );
        assert_eq!(f.takes_pos_id(), !pure, "pos_id of {}", f.name());
    }
    // Symbols follow the subscript_rt_str_* convention, distinctly.
    let mut symbols: Vec<&str> = StrFn::ALL.iter().map(|f| f.symbol()).collect();
    symbols.sort_unstable();
    symbols.dedup();
    assert_eq!(symbols.len(), StrFn::ALL.len());
    assert!(StrFn::ALL
        .iter()
        .all(|f| f.symbol().starts_with("subscript_rt_str_")));
    assert_eq!(StrFn::ToUpperCase.symbol(), "subscript_rt_str_to_upper");
    assert_eq!(StrFn::CharCodeAt.name(), "charCodeAt");
}

#[test]
fn arr_fn_all_is_indexed_by_discriminant() {
    // Runtime-import tables index by `f as usize`; the ALL order
    // must therefore equal declaration order.
    for (i, f) in ArrFn::ALL.iter().enumerate() {
        assert_eq!(*f as usize, i, "ArrFn::ALL out of order at {i}");
    }
}

#[test]
fn arr_fn_shapes_match_the_section_9_contract() {
    // Symbols follow the subscript_rt_arr_* convention, distinctly.
    let mut symbols: Vec<&str> = ArrFn::ALL.iter().map(|f| f.symbol()).collect();
    symbols.sort_unstable();
    symbols.dedup();
    assert_eq!(symbols.len(), ArrFn::ALL.len());
    assert!(ArrFn::ALL
        .iter()
        .all(|f| f.symbol().starts_with("subscript_rt_arr_")));
    assert_eq!(ArrFn::ForEach.symbol(), "subscript_rt_arr_for_each");
    assert_eq!(ArrFn::FindIndex.name(), "findIndex");
    // The callback set includes each predicate and mapping operation.
    let with_cb: Vec<ArrFn> = ArrFn::ALL
        .iter()
        .copied()
        .filter(|f| f.takes_callback())
        .collect();
    assert_eq!(
        with_cb,
        [
            ArrFn::ForEach,
            ArrFn::Map,
            ArrFn::Filter,
            ArrFn::Reduce,
            ArrFn::Some,
            ArrFn::Every,
            ArrFn::FindIndex,
            ArrFn::Sort,
            ArrFn::ReduceRight,
            ArrFn::Find,
            ArrFn::FindLast,
            ArrFn::FindLastIndex,
            ArrFn::FlatMap,
        ]
    );
    for f in [
        ArrFn::ForEach,
        ArrFn::Map,
        ArrFn::Filter,
        ArrFn::Some,
        ArrFn::Every,
        ArrFn::FindIndex,
        ArrFn::Find,
        ArrFn::FindLast,
        ArrFn::FindLastIndex,
        ArrFn::FlatMap,
    ] {
        assert_eq!(f.callback_index_arity(), Some(2), "{}", f.name());
        assert!(f.api_signature().contains("index: i32"), "{}", f.name());
    }
    for f in [ArrFn::Reduce, ArrFn::ReduceRight] {
        assert_eq!(f.callback_index_arity(), Some(3), "{}", f.name());
        assert!(f.api_signature().contains("index: i32"), "{}", f.name());
    }
    assert_eq!(ArrFn::Sort.callback_index_arity(), None);
    assert!(!ArrFn::Sort.api_signature().contains("index"));
    // pos_id: the allocating operations plus the trapping shift.
    for f in ArrFn::ALL {
        let carries_pos = matches!(
            f,
            ArrFn::At
                | ArrFn::FlatMap
                | ArrFn::Join
                | ArrFn::Slice
                | ArrFn::Concat
                | ArrFn::Map
                | ArrFn::Filter
                | ArrFn::Splice
                | ArrFn::Shift
                | ArrFn::Unshift
        );
        assert_eq!(f.takes_pos_id(), carries_pos, "pos_id of {}", f.name());
        assert_eq!(
            f.can_trap(),
            carries_pos || f.takes_callback(),
            "trap check of {}",
            f.name()
        );
    }
    assert_eq!(ArrFn::END_SENTINEL, i64::from(i32::MAX));
}

#[test]
fn map_set_fn_tables_match_the_section_10_contract() {
    for (i, f) in MapFn::ALL.iter().enumerate() {
        assert_eq!(*f as usize, i, "MapFn::ALL out of order at {i}");
        assert!(
            f.symbol().starts_with("subscript_rt_map_")
                || f.symbol().starts_with("subscript_rt_assoc_")
        );
    }
    for (i, f) in SetFn::ALL.iter().enumerate() {
        assert_eq!(*f as usize, i, "SetFn::ALL out of order at {i}");
        assert!(
            f.symbol().starts_with("subscript_rt_set_")
                || f.symbol().starts_with("subscript_rt_assoc_")
        );
    }
    assert_eq!(MapFn::Size.symbol(), SetFn::Size.symbol());
    assert_eq!(MapFn::Has.symbol(), SetFn::Has.symbol());
    assert_eq!(MapFn::Delete.symbol(), SetFn::Delete.symbol());
    assert_eq!(MapFn::Clear.symbol(), SetFn::Clear.symbol());
    assert!(MapFn::New.allocates());
    assert!(MapFn::Set.allocates());
    assert!(MapFn::GroupBy.allocates());
    assert!(!MapFn::Get.allocates());
    assert!(MapFn::ForEach.can_trap());
    assert!(MapFn::GroupBy.can_trap());
    assert!(SetFn::New.allocates());
    assert!(SetFn::Add.allocates());
    assert!(SetFn::Union.allocates());
    assert!(SetFn::Intersection.allocates());
    assert!(SetFn::Difference.allocates());
    assert!(SetFn::SymmetricDifference.allocates());
    assert!(!SetFn::Has.allocates());
    assert!(!SetFn::IsSubsetOf.allocates());
    assert!(SetFn::ForEach.can_trap());
}

#[test]
fn arr_elem_kind_covers_the_marshalable_types_and_nothing_else() {
    use crate::types::FuncType;
    let value_class = |id: ClassId| id.0 == 0; // class 0 is @ValueType, class 1 is a reference
    let of = |ty: &Type| ArrElemKind::of(ty, &value_class);
    for ty in [
        Type::Bool,
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
        Type::Object,
        Type::Class(ClassId(1)),
        Type::Nullable(Box::new(Type::Class(ClassId(1)))),
        Type::Array(Box::new(Type::I32)),
    ] {
        assert_eq!(of(&ty), Some(ArrElemKind::Int), "{ty:?}");
    }
    for ty in [
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
        Type::Date,
        Type::Enum(EnumId(0)),
    ] {
        assert_eq!(of(&ty), Some(ArrElemKind::SignedInt), "{ty:?}");
    }
    assert_eq!(of(&Type::F32), Some(ArrElemKind::F32));
    assert_eq!(of(&Type::F64), Some(ArrElemKind::F64));
    assert_eq!(of(&Type::F16), Some(ArrElemKind::F16));
    assert_eq!(of(&Type::Str), Some(ArrElemKind::Str));
    // Excluded: value classes, function values, FixedArray, void.
    assert_eq!(of(&Type::Class(ClassId(0))), None);
    let ft = Type::Func(Box::new(FuncType {
        params: vec![Type::I32],
        ret: Type::I32,
    }));
    assert_eq!(of(&ft), None);
    assert_eq!(of(&Type::Nullable(Box::new(ft))), None);
    assert_eq!(of(&Type::FixedArray(Box::new(Type::I32), 3)), None);
    assert_eq!(of(&Type::Void), None);
    // Stable ABI codes.
    assert_eq!(ArrElemKind::Int.code(), 0);
    assert_eq!(ArrElemKind::F32.code(), 1);
    assert_eq!(ArrElemKind::F64.code(), 2);
    assert_eq!(ArrElemKind::Str.code(), 3);
    assert_eq!(ArrElemKind::F16.code(), 4);
    assert_eq!(ArrElemKind::SignedInt.code(), 5);
}

#[test]
fn arr_fmt_kind_matches_the_q14_interpolable_set() {
    assert_eq!(ArrFmtKind::of(&Type::I32), Some(ArrFmtKind::I32));
    assert_eq!(ArrFmtKind::of(&Type::I8), Some(ArrFmtKind::I8));
    assert_eq!(ArrFmtKind::of(&Type::U8), Some(ArrFmtKind::U8));
    assert_eq!(ArrFmtKind::of(&Type::I16), Some(ArrFmtKind::I16));
    assert_eq!(ArrFmtKind::of(&Type::U16), Some(ArrFmtKind::U16));
    assert_eq!(
        ArrFmtKind::of(&Type::Enum(EnumId(0))),
        Some(ArrFmtKind::I32)
    );
    assert_eq!(ArrFmtKind::of(&Type::U32), Some(ArrFmtKind::U32));
    assert_eq!(ArrFmtKind::of(&Type::I64), Some(ArrFmtKind::I64));
    assert_eq!(ArrFmtKind::of(&Type::U64), Some(ArrFmtKind::U64));
    assert_eq!(ArrFmtKind::of(&Type::F32), Some(ArrFmtKind::F32));
    assert_eq!(ArrFmtKind::of(&Type::F64), Some(ArrFmtKind::F64));
    assert_eq!(ArrFmtKind::of(&Type::F16), Some(ArrFmtKind::F16));
    assert_eq!(ArrFmtKind::of(&Type::Bool), Some(ArrFmtKind::Bool));
    assert_eq!(ArrFmtKind::of(&Type::Str), Some(ArrFmtKind::Str));
    // Not interpolatable (Q20 for Date; references have no Q14 form).
    assert_eq!(ArrFmtKind::of(&Type::Date), None);
    assert_eq!(ArrFmtKind::of(&Type::Class(ClassId(0))), None);
    assert_eq!(ArrFmtKind::of(&Type::Object), None);
    // Stable ABI codes, in declaration order.
    let codes: Vec<u32> = [
        ArrFmtKind::I32,
        ArrFmtKind::U32,
        ArrFmtKind::I64,
        ArrFmtKind::U64,
        ArrFmtKind::F32,
        ArrFmtKind::F64,
        ArrFmtKind::Bool,
        ArrFmtKind::Str,
        ArrFmtKind::I8,
        ArrFmtKind::U8,
        ArrFmtKind::I16,
        ArrFmtKind::U16,
        ArrFmtKind::F16,
    ]
    .iter()
    .map(|k| k.code())
    .collect();
    assert_eq!(codes, (0..13).collect::<Vec<u32>>());
}

#[test]
fn module_is_constructible_empty() {
    let m = Module {
        host_entries: Vec::new(),
        entry_pos: Pos::new("test.ts", 1, 1),
        poisoned_imports: Vec::new(),
        synthesized_helpers: Default::default(),
        classes: Vec::new(),
        enums: Vec::new(),
        string_aliases: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
        worker_entries: Vec::new(),
        operation_signatures: Vec::new(),
        foreign_fns: Vec::new(),
        foreign_mirrors: Vec::new(),
        top_level: Vec::new(),
        initializer_modules: Vec::new(),
        regex_literal_globals: Vec::new(),
        initializer_segments: Vec::new(),
        initializer_can_raise: false,
        source_bytes: 0,
    };
    assert!(m.functions.is_empty());
}

#[test]
fn worker_intrinsic_identity_uses_the_all_table_order() {
    assert_eq!(WorkerFn::ALL.len(), 8);
    assert_eq!(WorkerFn::Spawn(37).intrinsic_identity(), WorkerFn::ALL[0]);
    assert_eq!(WorkerFn::OutboxPost.intrinsic_identity(), WorkerFn::ALL[7]);
}

#[test]
fn reload_site_derivation_keeps_precise_facts_and_adds_quiet_call_edges() {
    let module = crate::check_program(&[crate::SourceFile::new(
        "sites.ts",
        r#"
function quiet(): void {}
function raises(): void { throw new Error("control"); }
export function main(): void { quiet(); raises(); }
"#,
    )])
    .expect("checked source");
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let calls = main
        .body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Expr(expr) if matches!(expr.kind, ExprKind::Call { .. }) => Some(expr),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    for reload in [false, true] {
        let quiet = calls[0].trap_sites_for_reload(&module, reload);
        assert!(quiet
            .iter()
            .any(|site| matches!(site, TrapSite::Call { .. })));
        assert_eq!(
            quiet
                .iter()
                .any(|site| matches!(site, TrapSite::Raise { .. })),
            reload
        );
        let control = calls[1].trap_sites_for_reload(&module, reload);
        assert!(control
            .iter()
            .any(|site| matches!(site, TrapSite::Raise { .. })));
    }
    assert!(
        !module
            .functions
            .iter()
            .find(|f| f.name == "quiet")
            .unwrap()
            .can_raise
    );
}
