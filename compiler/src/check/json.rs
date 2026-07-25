//! P13 stage 1: statically typed, call-site-monomorphized
//! `JSON.stringify<T>` serializer construction.
//!
//! No runtime type inspection is involved. Once the argument has been
//! checked, this module builds a finite graph of ordinary HIR helper
//! functions for that exact `T`. Recursive reference-class types become
//! recursive helper calls, while leaf writes go through the shared
//! `sub_rt_json_*` runtime.

use std::collections::HashSet;

use swc_ecma_ast as ast;

use crate::diag::{Pos, RuleCode};
use crate::hir::{self, BinOp, Callee, ExprKind, JsonFn};
use crate::types::{ClassId, Type};

use super::{Checker, FnCtx};

impl Checker<'_> {
    /// True when `obj` denotes the unshadowed ambient `JSON` namespace.
    pub(crate) fn is_json_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        let ast::Expr::Ident(id) = obj else {
            return false;
        };
        if id.sym.as_ref() != "JSON" {
            return false;
        }
        let locally_shadowed = fx
            .scopes
            .iter()
            .rev()
            .any(|scope| scope.vars.contains_key("JSON"));
        !locally_shadowed && self.scope_item("JSON").is_none()
    }

    /// Checks and monomorphizes one `JSON.stringify(value)` call.
    pub(crate) fn check_json_call(
        &mut self,
        member: &str,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
        member_pos: Pos,
    ) -> hir::Expr {
        if member != "stringify" {
            self.error(
                RuleCode::S014,
                format!(
                    "`JSON.{member}` is outside P13 stage 1; only `JSON.stringify` is implemented"
                ),
                member_pos,
            );
            return self.err_expr(pos);
        }
        if call.args.len() != 1 {
            self.error(
                RuleCode::S014,
                format!(
                    "`JSON.stringify` expects exactly 1 argument, got {}",
                    call.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if let Some(spread) = call.args[0].spread {
            let spread_pos = self.pos(spread);
            self.error(
                RuleCode::S100,
                "spread arguments are not decided",
                spread_pos.clone(),
            );
            return self.err_expr(spread_pos);
        }
        let saved_json_argument = self.in_json_argument;
        self.in_json_argument = true;
        let value = self.check_expr(&call.args[0].expr, None, fx);
        self.in_json_argument = saved_json_argument;
        if value.ty == Type::Error {
            return self.err_expr(pos);
        }
        if !self.json_serializable(&value.ty) {
            if let Some(rejection) = crate::ambient::json_rejection(&value.ty) {
                self.error(
                    rejection.code,
                    crate::ambient::rejection_message(rejection, "JSON.stringify"),
                    member_pos,
                );
                return self.err_expr(pos);
            }
            let name = self.type_name(&value.ty);
            self.error(
                RuleCode::S014,
                format!(
                    "`JSON.stringify` cannot serialize `{name}`; P13 accepts sized numerics \
                     except f16, boolean, string, Date, arrays, @CStruct values, reference \
                     classes, and Ref | null"
                ),
                member_pos,
            );
            return self.err_expr(pos);
        }

        let tracked = self.json_type_can_cycle(&value.ty);
        let wrapper = self.synthesize_json_serializer(&value.ty, tracked, pos.clone());
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Func(wrapper),
                args: vec![value],
            },
            ty: Type::Str,
            pos,
        }
    }

    /// P13's serializable-type predicate. A currently active class is
    /// provisionally accepted so recursive reference graphs terminate;
    /// every distinct field shape is still checked before success.
    fn json_serializable(&self, ty: &Type) -> bool {
        fn visit(
            checker: &Checker<'_>,
            ty: &Type,
            active: &mut HashSet<ClassId>,
            done: &mut HashSet<ClassId>,
        ) -> bool {
            match ty {
                Type::I8
                | Type::U8
                | Type::I16
                | Type::U16
                | Type::I32
                | Type::U32
                | Type::I64
                | Type::U64
                | Type::F32
                | Type::F64
                | Type::Bool
                | Type::Str
                | Type::Date => true,
                Type::Array(element) | Type::FixedArray(element, _) => {
                    visit(checker, element, active, done)
                }
                Type::Nullable(inner) => {
                    matches!(&**inner, Type::Class(id) if !checker.classes[id.0].is_value)
                        && visit(checker, inner, active, done)
                }
                Type::Class(id) => {
                    let Some(class) = checker.classes.get(id.0) else {
                        return false;
                    };
                    // Mirror-ingested boundary structs are not source
                    // `@CStruct` values and may contain opaque host shapes.
                    if class.is_boundary {
                        return false;
                    }
                    if done.contains(id) || active.contains(id) {
                        return true;
                    }
                    active.insert(*id);
                    let ok = class
                        .fields
                        .iter()
                        .all(|field| visit(checker, &field.ty, active, done));
                    active.remove(id);
                    if ok {
                        done.insert(*id);
                    }
                    ok
                }
                Type::F16
                | Type::Void
                | Type::Null
                | Type::Object
                | Type::Enum(_)
                | Type::Map(..)
                | Type::Set(_)
                | Type::Func(_)
                | Type::Generator(_)
                | Type::IterResult(_)
                | Type::Error => false,
            }
        }

        visit(self, ty, &mut HashSet::new(), &mut HashSet::new())
    }

    /// Statically detects whether the reachable field-type graph has a
    /// cycle containing a reference class. Only such serializers receive
    /// the tracked builder and Visit/Leave calls.
    fn json_type_can_cycle(&self, ty: &Type) -> bool {
        fn walk(
            checker: &Checker<'_>,
            ty: &Type,
            active: &mut Vec<ClassId>,
            done: &mut HashSet<ClassId>,
        ) -> bool {
            match ty {
                Type::Array(element)
                | Type::FixedArray(element, _)
                | Type::Nullable(element) => walk(checker, element, active, done),
                Type::Class(id) => {
                    if let Some(start) = active.iter().position(|candidate| candidate == id) {
                        return active[start..]
                            .iter()
                            .any(|cid| !checker.classes[cid.0].is_value);
                    }
                    if done.contains(id) {
                        return false;
                    }
                    active.push(*id);
                    let cyclic = checker.classes[id.0]
                        .fields
                        .iter()
                        .any(|field| walk(checker, &field.ty, active, done));
                    active.pop();
                    if !cyclic {
                        done.insert(*id);
                    }
                    cyclic
                }
                _ => false,
            }
        }

        walk(self, ty, &mut Vec::new(), &mut HashSet::new())
    }

    /// Adds one call-specific wrapper and its finite serializer-function
    /// graph to HIR, returning the wrapper name used at the original call
    /// site. Per-call generation keeps every possible trap pinned to the
    /// exact source position that requested serialization.
    fn synthesize_json_serializer(&mut self, root: &Type, tracked: bool, pos: Pos) -> String {
        let call_id = self.functions.len();
        let mut types = Vec::new();
        self.collect_json_types(root, &mut types);
        let names: Vec<String> = (0..types.len())
            .map(|index| format!("[[json.stringify#{call_id}.value#{index}]]"))
            .collect();

        for (index, ty) in types.iter().enumerate() {
            let body = self.json_helper_body(ty, tracked, &types, &names, &pos);
            self.functions.push(hir::Function {
                name: names[index].clone(),
                exported: false,
                is_generator: false,
                params: vec![
                    json_param("builder", Type::U64, &pos),
                    json_param("value", ty.clone(), &pos),
                ],
                ret: Type::Void,
                body,
                pos: pos.clone(),
            });
        }

        let wrapper = format!("[[json.stringify#{call_id}.root]]");
        let builder_init = json_call(
            if tracked {
                JsonFn::BeginTracked
            } else {
                JsonFn::Begin
            },
            Vec::new(),
            Type::U64,
            &pos,
        );
        let root_helper = names[json_type_index(&types, root)].clone();
        let body = vec![
            hir::Stmt::Let {
                name: "builder".to_string(),
                ty: Type::U64,
                mutable: false,
                init: builder_init,
                pos: pos.clone(),
            },
            hir::Stmt::Expr(script_call(
                root_helper,
                vec![
                    json_local("builder", Type::U64, &pos),
                    json_local("value", root.clone(), &pos),
                ],
                Type::Void,
                &pos,
            )),
            hir::Stmt::Return {
                value: Some(json_call(
                    JsonFn::Finish,
                    vec![json_local("builder", Type::U64, &pos)],
                    Type::Str,
                    &pos,
                )),
                pos: pos.clone(),
            },
        ];
        self.functions.push(hir::Function {
            name: wrapper.clone(),
            exported: false,
            is_generator: false,
            params: vec![json_param("value", root.clone(), &pos)],
            ret: Type::Str,
            body,
            pos,
        });
        wrapper
    }

    fn collect_json_types(&self, ty: &Type, out: &mut Vec<Type>) {
        if out.contains(ty) {
            return;
        }
        out.push(ty.clone());
        match ty {
            Type::Array(element)
            | Type::FixedArray(element, _)
            | Type::Nullable(element) => self.collect_json_types(element, out),
            Type::Class(id) => {
                for field in &self.classes[id.0].fields {
                    self.collect_json_types(&field.ty, out);
                }
            }
            _ => {}
        }
    }

    fn json_helper_body(
        &self,
        ty: &Type,
        tracked: bool,
        types: &[Type],
        names: &[String],
        pos: &Pos,
    ) -> Vec<hir::Stmt> {
        let builder = || json_local("builder", Type::U64, pos);
        let value = || json_local("value", ty.clone(), pos);
        let append = |function: JsonFn, argument: hir::Expr| {
            hir::Stmt::Expr(json_call(
                function,
                vec![builder(), argument],
                Type::Void,
                pos,
            ))
        };
        match ty {
            Type::I8 | Type::I16 => vec![append(
                JsonFn::I32,
                json_cast(value(), Type::I32, pos),
            )],
            Type::U8 | Type::U16 => vec![append(
                JsonFn::U32,
                json_cast(value(), Type::U32, pos),
            )],
            Type::I32 => vec![append(JsonFn::I32, value())],
            Type::U32 => vec![append(JsonFn::U32, value())],
            Type::I64 => vec![append(JsonFn::I64, value())],
            Type::U64 => vec![append(JsonFn::U64, value())],
            Type::F32 => vec![append(JsonFn::F32, value())],
            Type::F64 => vec![append(JsonFn::F64, value())],
            Type::Bool => vec![append(JsonFn::Bool, value())],
            Type::Str => vec![append(JsonFn::Str, value())],
            Type::Date => vec![append(JsonFn::Date, value())],
            Type::Array(element) | Type::FixedArray(element, _) => {
                self.json_array_body(ty, element, types, names, pos)
            }
            Type::Nullable(inner) => {
                let null = hir::Expr {
                    kind: ExprKind::Null,
                    ty: Type::Null,
                    pos: pos.clone(),
                };
                let cond = hir::Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::Eq,
                        left: Box::new(value()),
                        right: Box::new(null),
                    },
                    ty: Type::Bool,
                    pos: pos.clone(),
                };
                let narrowed = json_local("value", (**inner).clone(), pos);
                vec![hir::Stmt::If {
                    cond,
                    then: vec![hir::Stmt::Expr(json_call(
                        JsonFn::Null,
                        vec![builder()],
                        Type::Void,
                        pos,
                    ))],
                    els: Some(vec![hir::Stmt::Expr(script_call(
                        names[json_type_index(types, inner)].clone(),
                        vec![builder(), narrowed],
                        Type::Void,
                        pos,
                    ))]),
                    pos: pos.clone(),
                }]
            }
            Type::Class(id) => {
                let object = self.json_object_body(*id, types, names, pos);
                if !self.classes[id.0].is_value && tracked {
                    let visit = json_call(
                        JsonFn::Visit,
                        vec![builder(), value()],
                        Type::Bool,
                        pos,
                    );
                    let mut then = object;
                    then.push(hir::Stmt::Expr(json_call(
                        JsonFn::Leave,
                        vec![builder(), value()],
                        Type::Void,
                        pos,
                    )));
                    vec![hir::Stmt::If {
                        cond: visit,
                        then,
                        els: None,
                        pos: pos.clone(),
                    }]
                } else {
                    object
                }
            }
            other => panic!("checker generated JSON helper for rejected type {other:?}"),
        }
    }

    fn json_array_body(
        &self,
        array_ty: &Type,
        element: &Type,
        types: &[Type],
        names: &[String],
        pos: &Pos,
    ) -> Vec<hir::Stmt> {
        let builder = || json_local("builder", Type::U64, pos);
        let array = || json_local("value", array_ty.clone(), pos);
        let index = || json_local("index", Type::I32, pos);
        let raw = |text: &str| {
            hir::Stmt::Expr(json_call(
                JsonFn::Raw,
                vec![builder(), json_string(text, pos)],
                Type::Void,
                pos,
            ))
        };
        let condition = hir::Expr {
            kind: ExprKind::Binary {
                op: BinOp::Lt,
                left: Box::new(index()),
                right: Box::new(hir::Expr {
                    kind: ExprKind::Length(Box::new(array())),
                    ty: Type::I32,
                    pos: pos.clone(),
                }),
            },
            ty: Type::Bool,
            pos: pos.clone(),
        };
        let comma_condition = hir::Expr {
            kind: ExprKind::Binary {
                op: BinOp::Ne,
                left: Box::new(index()),
                right: Box::new(json_int(0, pos)),
            },
            ty: Type::Bool,
            pos: pos.clone(),
        };
        let indexed = hir::Expr {
            kind: ExprKind::Index {
                obj: Box::new(array()),
                index: Box::new(index()),
            },
            ty: element.clone(),
            pos: pos.clone(),
        };
        let step = hir::Expr {
            kind: ExprKind::Assign {
                op: Some(BinOp::Add),
                target: Box::new(index()),
                value: Box::new(json_int(1, pos)),
            },
            ty: Type::I32,
            pos: pos.clone(),
        };
        vec![
            raw("["),
            hir::Stmt::Let {
                name: "index".to_string(),
                ty: Type::I32,
                mutable: true,
                init: json_int(0, pos),
                pos: pos.clone(),
            },
            hir::Stmt::While {
                cond: condition,
                body: vec![
                    hir::Stmt::If {
                        cond: comma_condition,
                        then: vec![raw(",")],
                        els: None,
                        pos: pos.clone(),
                    },
                    hir::Stmt::Expr(script_call(
                        names[json_type_index(types, element)].clone(),
                        vec![builder(), indexed],
                        Type::Void,
                        pos,
                    )),
                    hir::Stmt::Expr(step),
                ],
                pos: pos.clone(),
            },
            raw("]"),
        ]
    }

    fn json_object_body(
        &self,
        id: ClassId,
        types: &[Type],
        names: &[String],
        pos: &Pos,
    ) -> Vec<hir::Stmt> {
        let builder = || json_local("builder", Type::U64, pos);
        let object = || json_local("value", Type::Class(id), pos);
        let raw = |text: &str| {
            hir::Stmt::Expr(json_call(
                JsonFn::Raw,
                vec![builder(), json_string(text, pos)],
                Type::Void,
                pos,
            ))
        };
        let mut body = vec![raw("{")];
        for (index, field) in self.classes[id.0].fields.iter().enumerate() {
            if index != 0 {
                body.push(raw(","));
            }
            body.push(hir::Stmt::Expr(json_call(
                JsonFn::Str,
                vec![builder(), json_string(&field.name, pos)],
                Type::Void,
                pos,
            )));
            body.push(raw(":"));
            let field_value = hir::Expr {
                kind: ExprKind::Field {
                    obj: Box::new(object()),
                    name: field.name.clone(),
                },
                ty: field.ty.clone(),
                pos: pos.clone(),
            };
            body.push(hir::Stmt::Expr(script_call(
                names[json_type_index(types, &field.ty)].clone(),
                vec![builder(), field_value],
                Type::Void,
                pos,
            )));
        }
        body.push(raw("}"));
        body
    }
}

fn json_type_index(types: &[Type], ty: &Type) -> usize {
    types
        .iter()
        .position(|candidate| candidate == ty)
        .expect("JSON type graph is closed")
}

fn json_param(name: &str, ty: Type, pos: &Pos) -> hir::Param {
    hir::Param {
        name: name.to_string(),
        ty,
        default: None,
        pos: pos.clone(),
    }
}

fn json_local(name: &str, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Local(name.to_string()),
        ty,
        pos: pos.clone(),
    }
}

fn json_string(value: &str, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Str(value.to_string()),
        ty: Type::Str,
        pos: pos.clone(),
    }
}

fn json_int(value: i64, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Int(value),
        ty: Type::I32,
        pos: pos.clone(),
    }
}

fn json_cast(value: hir::Expr, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Cast(Box::new(value)),
        ty,
        pos: pos.clone(),
    }
}

fn json_call(function: JsonFn, args: Vec<hir::Expr>, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Call {
            callee: Callee::Json(function),
            args,
        },
        ty,
        pos: pos.clone(),
    }
}

fn script_call(name: String, args: Vec<hir::Expr>, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Call {
            callee: Callee::Func(name),
            args,
        },
        ty,
        pos: pos.clone(),
    }
}
