//! P13: statically typed, call-site-monomorphized JSON construction.
//!
//! No language RTTI is involved. Once an exact `T` is known, this module
//! builds a finite graph of ordinary HIR helper functions. Stringify
//! traverses language values; parse validates a transient runtime syntax
//! tree before constructing any language value. Representation-neutral
//! leaves go through the shared `subscript_rt_json_*` runtime.

use std::collections::HashSet;

use swc_ecma_ast as ast;

use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, BinOp, Callee, ExprKind, JsonFn, UnOp};
use crate::types::{ClassId, Type};

use super::{Checker, FnCtx};

impl Checker<'_> {
    fn json_call(&self, function: JsonFn, args: Vec<hir::Expr>, ty: Type, pos: &Pos) -> hir::Expr {
        let expr = hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Json(function),
                args,
            },
            ty,
            pos: pos.clone(),
        };
        self.register_operation_signature(&expr);
        expr
    }

    /// Monomorphizes P13's ambient `JsonResult<T>` reference class on
    /// first use. The zeroed payload is exactly the failed-result shape.
    pub(crate) fn instantiate_json_result(&mut self, value: &Type, pos: Pos) -> ClassId {
        let name = self.mono_name("JsonResult", std::slice::from_ref(value));
        if let Some(&id) = self.class_ids.get(&name) {
            return id;
        }
        let id = self.new_class(&name, false, false, None, pos.clone());
        self.classes[id.0].fields = vec![
            hir::Field {
                name: "ok".to_string(),
                ty: Type::Bool,
                is_defaulted: false,
                is_absence_capable: false,
                init: None,
                foreign_provenance: None,
                pos: pos.clone(),
            },
            hir::Field {
                name: "value".to_string(),
                ty: value.clone(),
                is_defaulted: false,
                is_absence_capable: false,
                init: None,
                foreign_provenance: None,
                pos,
            },
        ];
        id
    }

    pub(super) fn json_result_value_type(&self, ty: &Type) -> Option<Type> {
        let Type::Class(id) = ty else {
            return None;
        };
        let class = self.classes.get(id.0)?;
        if !class.name.starts_with("JsonResult<") || class.fields.len() != 2 {
            return None;
        }
        let ok = &class.fields[0];
        let value = &class.fields[1];
        (ok.name == "ok" && ok.ty == Type::Bool && value.name == "value").then(|| value.ty.clone())
    }

    /// True when `obj` denotes the unshadowed ambient `JSON` namespace.
    pub(crate) fn is_json_namespace(&self, obj: &ast::Expr, fx: &FnCtx) -> bool {
        self.ambient_namespace(obj, fx) == Some("JSON")
    }

    /// Checks and monomorphizes one `JSON.stringify(value)` call.
    pub(crate) fn check_json_call(
        &mut self,
        member: &str,
        call: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
        member_pos: Pos,
    ) -> hir::Expr {
        if member == "parse" {
            return self.check_json_parse(call, ctx, fx, pos, member_pos);
        }
        if member != "stringify" {
            self.error(
                RuleCode::S014,
                format!("`JSON.{member}` is outside the accepted JSON subset (Q28)"),
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
                RuleCode::S014,
                "spread arguments require variadic parameters, which the language does not have",
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
                self.error_diverging(
                    rejection.code,
                    crate::ambient::rejection_message(rejection, "JSON.stringify"),
                    member_pos,
                    Divergence::JsonSubset,
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
        let wrapper = match self.synthesize_json_serializer(&value.ty, tracked, pos.clone()) {
            Ok(wrapper) => wrapper,
            Err(detail) => {
                self.error(
                    RuleCode::S014,
                    format!("cannot generate `JSON.stringify` helper: {detail}"),
                    member_pos,
                );
                return self.err_expr(pos);
            }
        };
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Func(wrapper),
                args: vec![value],
            },
            ty: Type::Str,
            pos,
        }
    }

    fn check_json_parse(
        &mut self,
        call: &ast::CallExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
        member_pos: Pos,
    ) -> hir::Expr {
        if call.args.len() != 1 {
            self.error(
                RuleCode::S014,
                format!(
                    "`JSON.parse` expects exactly 1 argument, got {}",
                    call.args.len()
                ),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if let Some(spread) = call.args[0].spread {
            let spread_pos = self.pos(spread);
            self.error(
                RuleCode::S014,
                "spread arguments require variadic parameters, which the language does not have",
                spread_pos.clone(),
            );
            return self.err_expr(spread_pos);
        }

        let target = if let Some(type_args) = &call.type_args {
            if type_args.params.len() != 1 {
                self.error(
                    RuleCode::S014,
                    "`JSON.parse<T>` takes exactly one type argument",
                    member_pos.clone(),
                );
                return self.err_expr(pos);
            }
            let saved = self.in_json_argument;
            self.in_json_argument = true;
            let target = self.resolve_type(&type_args.params[0]);
            self.in_json_argument = saved;
            target
        } else if let Some(target) = ctx.and_then(|ty| self.json_result_value_type(ty)) {
            target
        } else {
            self.error_diverging(
                RuleCode::S014,
                "`JSON.parse` requires a target type; use `JSON.parse<T>(text)` \
                 or a contextual `JsonResult<T>` type (Q28)",
                member_pos,
                Divergence::JsonSubset,
            );
            return self.err_expr(pos);
        };
        if target == Type::Error {
            return self.err_expr(pos);
        }
        if self.json_type_contains_date(&target) {
            let rejection = crate::ambient::json_parse_date_rejection();
            let actual = if target == Type::Date {
                "JSON.parse<Date>"
            } else {
                "JSON.parse target containing Date"
            };
            self.error_diverging(
                rejection.code,
                crate::ambient::rejection_message(rejection, actual),
                member_pos,
                Divergence::JsonSubset,
            );
            return self.err_expr(pos);
        }
        if !self.json_serializable(&target) {
            let name = self.type_name(&target);
            self.error(
                RuleCode::S014,
                format!(
                    "`JSON.parse` cannot deserialize `{name}`; Q28 accepts sized numerics \
                     except f16, boolean, string, arrays, @CStruct values, reference \
                     classes, and Ref | null"
                ),
                member_pos,
            );
            return self.err_expr(pos);
        }

        let text = self.check_expr(&call.args[0].expr, Some(&Type::Str), fx);
        self.require_assignable(
            &text.ty.clone(),
            &Type::Str,
            text.pos.clone(),
            "`JSON.parse` text",
        );
        if text.ty == Type::Error {
            return self.err_expr(pos);
        }

        let result_id = self.instantiate_json_result(&target, pos.clone());
        let wrapper = match self.synthesize_json_parser(&target, result_id, pos.clone()) {
            Ok(wrapper) => wrapper,
            Err(detail) => {
                self.error(
                    RuleCode::S014,
                    format!("cannot generate `JSON.parse` helper: {detail}"),
                    member_pos,
                );
                return self.err_expr(pos);
            }
        };
        hir::Expr {
            kind: ExprKind::Call {
                callee: Callee::Func(wrapper),
                args: vec![text],
            },
            ty: Type::Class(result_id),
            pos,
        }
    }

    /// True when a parse target contains Date at any depth. JSON has no
    /// tagged Date node, so every such target is unreachable rather than
    /// merely subject to a data-dependent mismatch.
    fn json_type_contains_date(&self, ty: &Type) -> bool {
        fn visit(checker: &Checker<'_>, ty: &Type, seen: &mut HashSet<ClassId>) -> bool {
            match ty {
                Type::Date => true,
                Type::Array(element) | Type::FixedArray(element, _) | Type::Nullable(element) => {
                    visit(checker, element, seen)
                }
                Type::Class(id) => {
                    seen.insert(*id)
                        && checker.classes[id.0]
                            .fields
                            .iter()
                            .any(|field| visit(checker, &field.ty, seen))
                }
                _ => false,
            }
        }

        visit(self, ty, &mut HashSet::new())
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
                | Type::RegExp
                | Type::Enum(_)
                | Type::StringAlias(_)
                | Type::Map(..)
                | Type::Set(_)
                | Type::Worker(..)
                | Type::Inbox(_)
                | Type::Outbox(_)
                | Type::Func(_)
                | Type::Generator(_)
                | Type::AsyncHandle(_)
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
                Type::Array(element) | Type::FixedArray(element, _) | Type::Nullable(element) => {
                    walk(checker, element, active, done)
                }
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
    fn synthesize_json_serializer(
        &mut self,
        root: &Type,
        tracked: bool,
        pos: Pos,
    ) -> Result<String, String> {
        let call_id = self.functions.len();
        let mut types = Vec::new();
        self.collect_json_types(root, &mut types);
        let names: Vec<String> = (0..types.len())
            .map(|index| format!("[[json.stringify#{call_id}.value#{index}]]"))
            .collect();

        for (index, ty) in types.iter().enumerate() {
            let body = self.json_helper_body(ty, tracked, &types, &names, &pos)?;
            self.functions.push(hir::Function {
                name: names[index].clone(),
                exported: false,
                is_generator: false,
                is_async: false,
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
        let builder_init = self.json_call(
            if tracked {
                JsonFn::BeginTracked
            } else {
                JsonFn::Begin
            },
            Vec::new(),
            Type::U64,
            &pos,
        );
        let root_helper = names[json_type_index(&types, root)?].clone();
        let locals = JsonLocals::new(&pos);
        let body = vec![
            hir::Stmt::Let {
                name: "builder".to_string(),
                ty: Type::U64,
                mutable: false,
                dispose: false,
                init: builder_init,
                pos: pos.clone(),
            },
            hir::Stmt::Expr(script_call(
                root_helper,
                vec![locals.builder(), locals.value(root.clone())],
                Type::Void,
                &pos,
            )),
            hir::Stmt::Return {
                value: Some(self.json_call(
                    JsonFn::Finish,
                    vec![locals.builder()],
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
            is_async: false,
            params: vec![json_param("value", root.clone(), &pos)],
            ret: Type::Str,
            body,
            pos,
        });
        Ok(wrapper)
    }

    fn collect_json_types(&self, ty: &Type, out: &mut Vec<Type>) {
        if out.contains(ty) {
            return;
        }
        out.push(ty.clone());
        match ty {
            Type::Array(element) | Type::FixedArray(element, _) | Type::Nullable(element) => {
                self.collect_json_types(element, out)
            }
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
    ) -> Result<Vec<hir::Stmt>, String> {
        let locals = JsonLocals::new(pos);
        let append = |function: JsonFn, argument: hir::Expr| {
            hir::Stmt::Expr(self.json_call(
                function,
                vec![locals.builder(), argument],
                Type::Void,
                pos,
            ))
        };
        match ty {
            Type::I8 | Type::I16 => Ok(vec![append(
                JsonFn::I32,
                json_cast(locals.value(ty.clone()), Type::I32, pos),
            )]),
            Type::U8 | Type::U16 => Ok(vec![append(
                JsonFn::U32,
                json_cast(locals.value(ty.clone()), Type::U32, pos),
            )]),
            Type::I32 => Ok(vec![append(JsonFn::I32, locals.value(ty.clone()))]),
            Type::U32 => Ok(vec![append(JsonFn::U32, locals.value(ty.clone()))]),
            Type::I64 => Ok(vec![append(JsonFn::I64, locals.value(ty.clone()))]),
            Type::U64 => Ok(vec![append(JsonFn::U64, locals.value(ty.clone()))]),
            Type::F32 => Ok(vec![append(JsonFn::F32, locals.value(ty.clone()))]),
            Type::F64 => Ok(vec![append(JsonFn::F64, locals.value(ty.clone()))]),
            Type::Bool => Ok(vec![append(JsonFn::Bool, locals.value(ty.clone()))]),
            Type::Str => Ok(vec![append(JsonFn::Str, locals.value(ty.clone()))]),
            Type::Date => Ok(vec![append(JsonFn::Date, locals.value(ty.clone()))]),
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
                        left: Box::new(locals.value(ty.clone())),
                        right: Box::new(null),
                    },
                    ty: Type::Bool,
                    pos: pos.clone(),
                };
                let narrowed = locals.value((**inner).clone());
                Ok(vec![hir::Stmt::If {
                    cond,
                    then: vec![hir::Stmt::Expr(self.json_call(
                        JsonFn::Null,
                        vec![locals.builder()],
                        Type::Void,
                        pos,
                    ))],
                    els: Some(vec![hir::Stmt::Expr(script_call(
                        names[json_type_index(types, inner)?].clone(),
                        vec![locals.builder(), narrowed],
                        Type::Void,
                        pos,
                    ))]),
                    pos: pos.clone(),
                }])
            }
            Type::Class(id) => {
                let object = self.json_object_body(*id, types, names, pos)?;
                if !self.classes[id.0].is_value && tracked {
                    let visit = self.json_call(
                        JsonFn::Visit,
                        vec![locals.builder(), locals.value(ty.clone())],
                        Type::Bool,
                        pos,
                    );
                    let mut then = object;
                    then.push(hir::Stmt::Expr(self.json_call(
                        JsonFn::Leave,
                        vec![locals.builder(), locals.value(ty.clone())],
                        Type::Void,
                        pos,
                    )));
                    Ok(vec![hir::Stmt::If {
                        cond: visit,
                        then,
                        els: None,
                        pos: pos.clone(),
                    }])
                } else {
                    Ok(object)
                }
            }
            other => Err(format!("rejected JSON serializer type {other:?}")),
        }
    }

    fn json_array_body(
        &self,
        array_ty: &Type,
        element: &Type,
        types: &[Type],
        names: &[String],
        pos: &Pos,
    ) -> Result<Vec<hir::Stmt>, String> {
        let locals = JsonLocals::new(pos);
        let raw = |text: &str| {
            hir::Stmt::Expr(self.json_call(
                JsonFn::Raw,
                vec![locals.builder(), json_string(text, pos)],
                Type::Void,
                pos,
            ))
        };
        let condition = hir::Expr {
            kind: ExprKind::Binary {
                op: BinOp::Lt,
                left: Box::new(locals.index()),
                right: Box::new(hir::Expr {
                    kind: ExprKind::Length(Box::new(locals.value(array_ty.clone()))),
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
                left: Box::new(locals.index()),
                right: Box::new(json_int(0, pos)),
            },
            ty: Type::Bool,
            pos: pos.clone(),
        };
        let indexed = hir::Expr {
            kind: ExprKind::Index {
                obj: Box::new(locals.value(array_ty.clone())),
                index: Box::new(locals.index()),
                checked: true,
            },
            ty: element.clone(),
            pos: pos.clone(),
        };
        let step = hir::Expr {
            kind: ExprKind::Assign {
                op: Some(BinOp::Add),
                target: Box::new(locals.index()),
                value: Box::new(json_int(1, pos)),
            },
            ty: Type::I32,
            pos: pos.clone(),
        };
        let element_helper = names[json_type_index(types, element)?].clone();
        Ok(vec![
            raw("["),
            hir::Stmt::Let {
                name: "index".to_string(),
                ty: Type::I32,
                mutable: true,
                dispose: false,
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
                        element_helper,
                        vec![locals.builder(), indexed],
                        Type::Void,
                        pos,
                    )),
                    hir::Stmt::Expr(step),
                ],
                pos: pos.clone(),
            },
            raw("]"),
        ])
    }

    fn json_object_body(
        &self,
        id: ClassId,
        types: &[Type],
        names: &[String],
        pos: &Pos,
    ) -> Result<Vec<hir::Stmt>, String> {
        let locals = JsonLocals::new(pos);
        let raw = |text: &str| {
            hir::Stmt::Expr(self.json_call(
                JsonFn::Raw,
                vec![locals.builder(), json_string(text, pos)],
                Type::Void,
                pos,
            ))
        };
        let mut body = vec![raw("{")];
        for (index, field) in self.classes[id.0].fields.iter().enumerate() {
            if index != 0 {
                body.push(raw(","));
            }
            body.push(hir::Stmt::Expr(self.json_call(
                JsonFn::Str,
                vec![locals.builder(), json_string(&field.name, pos)],
                Type::Void,
                pos,
            )));
            body.push(raw(":"));
            let field_value = hir::Expr {
                kind: ExprKind::Field {
                    obj: Box::new(locals.value(Type::Class(id))),
                    name: field.name.clone(),
                },
                ty: field.ty.clone(),
                pos: pos.clone(),
            };
            body.push(hir::Stmt::Expr(script_call(
                names[json_type_index(types, &field.ty)?].clone(),
                vec![locals.builder(), field_value],
                Type::Void,
                pos,
            )));
        }
        body.push(raw("}"));
        Ok(body)
    }

    fn synthesize_json_parser(
        &mut self,
        root: &Type,
        result_id: ClassId,
        pos: Pos,
    ) -> Result<String, String> {
        let call_id = self.functions.len();
        let mut types = Vec::new();
        self.collect_json_types(root, &mut types);
        let validators: Vec<String> = (0..types.len())
            .map(|index| format!("[[json.parse#{call_id}.validate#{index}]]"))
            .collect();
        let constructors: Vec<String> = (0..types.len())
            .map(|index| format!("[[json.parse#{call_id}.construct#{index}]]"))
            .collect();

        for (index, ty) in types.iter().enumerate() {
            let body = self.json_validation_body(ty, &types, &validators, &pos)?;
            self.functions.push(hir::Function {
                name: validators[index].clone(),
                exported: false,
                is_generator: false,
                is_async: false,
                params: vec![
                    json_param("parser", Type::U64, &pos),
                    json_param("node", Type::U64, &pos),
                ],
                ret: Type::Bool,
                body,
                pos: pos.clone(),
            });
        }
        for (index, ty) in types.iter().enumerate() {
            let body = self.json_construction_body(ty, &types, &constructors, &pos)?;
            self.functions.push(hir::Function {
                name: constructors[index].clone(),
                exported: false,
                is_generator: false,
                is_async: false,
                params: vec![
                    json_param("parser", Type::U64, &pos),
                    json_param("node", Type::U64, &pos),
                ],
                ret: ty.clone(),
                body,
                pos: pos.clone(),
            });
        }

        let result_ty = Type::Class(result_id);
        let locals = JsonLocals::new(&pos);
        let root_index = json_type_index(&types, root)?;
        let assign_ok = json_assign(
            json_field(locals.result(result_ty.clone()), "ok", Type::Bool, &pos),
            json_bool(true, &pos),
            Type::Bool,
            &pos,
        );
        let constructed = script_call(
            constructors[root_index].clone(),
            vec![locals.parser(), locals.node()],
            root.clone(),
            &pos,
        );
        let assign_value = json_assign(
            json_field(
                locals.result(result_ty.clone()),
                "value",
                root.clone(),
                &pos,
            ),
            constructed,
            root.clone(),
            &pos,
        );
        let valid = script_call(
            validators[root_index].clone(),
            vec![locals.parser(), locals.node()],
            Type::Bool,
            &pos,
        );
        let parser_present = json_binary(
            BinOp::Ne,
            locals.parser(),
            json_u64(0, &pos),
            Type::Bool,
            &pos,
        );
        let wrapper = format!("[[json.parse#{call_id}.root]]");
        self.functions.push(hir::Function {
            name: wrapper.clone(),
            exported: false,
            is_generator: false,
            is_async: false,
            params: vec![json_param("text", Type::Str, &pos)],
            ret: result_ty.clone(),
            body: vec![
                hir::Stmt::Let {
                    name: "result".to_string(),
                    ty: result_ty.clone(),
                    mutable: false,
                    dispose: false,
                    init: hir::Expr {
                        kind: ExprKind::RawNew { class: result_id },
                        ty: Type::Class(result_id),
                        pos: pos.clone(),
                    },
                    pos: pos.clone(),
                },
                hir::Stmt::Let {
                    name: "parser".to_string(),
                    ty: Type::U64,
                    mutable: false,
                    dispose: false,
                    init: self.json_call(JsonFn::ParseBegin, vec![locals.text()], Type::U64, &pos),
                    pos: pos.clone(),
                },
                hir::Stmt::If {
                    cond: parser_present,
                    then: vec![
                        hir::Stmt::Let {
                            name: "node".to_string(),
                            ty: Type::U64,
                            mutable: false,
                            dispose: false,
                            init: self.json_call(
                                JsonFn::ParseRoot,
                                vec![locals.parser()],
                                Type::U64,
                                &pos,
                            ),
                            pos: pos.clone(),
                        },
                        hir::Stmt::If {
                            cond: valid,
                            then: vec![hir::Stmt::Expr(assign_value), hir::Stmt::Expr(assign_ok)],
                            els: None,
                            pos: pos.clone(),
                        },
                        hir::Stmt::Expr(self.json_call(
                            JsonFn::ParseEnd,
                            vec![locals.parser()],
                            Type::Void,
                            &pos,
                        )),
                    ],
                    els: None,
                    pos: pos.clone(),
                },
                hir::Stmt::Return {
                    value: Some(locals.result(result_ty.clone())),
                    pos: pos.clone(),
                },
            ],
            pos,
        });
        Ok(wrapper)
    }

    fn json_validation_body(
        &self,
        ty: &Type,
        types: &[Type],
        validators: &[String],
        pos: &Pos,
    ) -> Result<Vec<hir::Stmt>, String> {
        let locals = JsonLocals::new(pos);
        let kind = |code: i64| {
            self.json_call(
                JsonFn::ParseIsKind,
                vec![locals.parser(), locals.node(), json_int(code, pos)],
                Type::Bool,
                pos,
            )
        };
        let return_value = |value: hir::Expr| hir::Stmt::Return {
            value: Some(value),
            pos: pos.clone(),
        };
        if let Some(target) = json_number_target(ty) {
            return Ok(vec![return_value(self.json_call(
                JsonFn::ParseNumberFits,
                vec![locals.parser(), locals.node(), json_int(target, pos)],
                Type::Bool,
                pos,
            ))]);
        }
        match ty {
            Type::Bool => Ok(vec![return_value(kind(JsonKind::BOOL))]),
            Type::Str => Ok(vec![return_value(kind(JsonKind::STRING))]),
            Type::Nullable(inner) => Ok(vec![hir::Stmt::If {
                cond: kind(JsonKind::NULL),
                then: vec![return_value(json_bool(true, pos))],
                els: Some(vec![return_value(script_call(
                    validators[json_type_index(types, inner)?].clone(),
                    vec![locals.parser(), locals.node()],
                    Type::Bool,
                    pos,
                ))]),
                pos: pos.clone(),
            }]),
            Type::Array(element) | Type::FixedArray(element, _) => {
                self.json_array_validation_body(ty, element, types, validators, pos)
            }
            Type::Class(id) => {
                let mut body = vec![json_return_false_unless(kind(JsonKind::OBJECT), pos)];
                for field in &self.classes[id.0].fields {
                    let field_node = self.json_call(
                        JsonFn::ParseObjectGet,
                        vec![
                            locals.parser(),
                            locals.node(),
                            json_string(&field.name, pos),
                        ],
                        Type::U64,
                        pos,
                    );
                    body.push(hir::Stmt::Let {
                        name: format!("field_{}", field.name),
                        ty: Type::U64,
                        mutable: false,
                        dispose: false,
                        init: field_node,
                        pos: pos.clone(),
                    });
                    let local = json_local(&format!("field_{}", field.name), Type::U64, pos);
                    body.push(json_return_false_unless(
                        json_binary(BinOp::Ne, local.clone(), json_u64(0, pos), Type::Bool, pos),
                        pos,
                    ));
                    body.push(json_return_false_unless(
                        script_call(
                            validators[json_type_index(types, &field.ty)?].clone(),
                            vec![locals.parser(), local],
                            Type::Bool,
                            pos,
                        ),
                        pos,
                    ));
                }
                body.push(return_value(json_bool(true, pos)));
                Ok(body)
            }
            other => Err(format!("rejected JSON validator type {other:?}")),
        }
    }

    fn json_array_validation_body(
        &self,
        array_ty: &Type,
        element: &Type,
        types: &[Type],
        validators: &[String],
        pos: &Pos,
    ) -> Result<Vec<hir::Stmt>, String> {
        let locals = JsonLocals::new(pos);
        let mut body = vec![
            json_return_false_unless(
                self.json_call(
                    JsonFn::ParseIsKind,
                    vec![
                        locals.parser(),
                        locals.node(),
                        json_int(JsonKind::ARRAY, pos),
                    ],
                    Type::Bool,
                    pos,
                ),
                pos,
            ),
            hir::Stmt::Let {
                name: "length".to_string(),
                ty: Type::I32,
                mutable: false,
                dispose: false,
                init: self.json_call(
                    JsonFn::ParseArrayLen,
                    vec![locals.parser(), locals.node()],
                    Type::I32,
                    pos,
                ),
                pos: pos.clone(),
            },
        ];
        if let Type::FixedArray(_, expected) = array_ty {
            body.push(json_return_false_unless(
                json_binary(
                    BinOp::Eq,
                    locals.length(),
                    json_int(i64::from(*expected), pos),
                    Type::Bool,
                    pos,
                ),
                pos,
            ));
        } else {
            body.push(json_return_false_unless(
                json_binary(
                    BinOp::Ge,
                    locals.length(),
                    json_int(0, pos),
                    Type::Bool,
                    pos,
                ),
                pos,
            ));
        }
        body.push(hir::Stmt::Let {
            name: "index".to_string(),
            ty: Type::I32,
            mutable: true,
            dispose: false,
            init: json_int(0, pos),
            pos: pos.clone(),
        });
        let child = self.json_call(
            JsonFn::ParseArrayGet,
            vec![locals.parser(), locals.node(), locals.index()],
            Type::U64,
            pos,
        );
        let element_validator = validators[json_type_index(types, element)?].clone();
        body.push(hir::Stmt::While {
            cond: json_binary(BinOp::Lt, locals.index(), locals.length(), Type::Bool, pos),
            body: vec![
                json_return_false_unless(
                    script_call(
                        element_validator,
                        vec![locals.parser(), child],
                        Type::Bool,
                        pos,
                    ),
                    pos,
                ),
                hir::Stmt::Expr(json_increment(locals.index(), pos)),
            ],
            pos: pos.clone(),
        });
        body.push(hir::Stmt::Return {
            value: Some(json_bool(true, pos)),
            pos: pos.clone(),
        });
        Ok(body)
    }

    fn json_construction_body(
        &self,
        ty: &Type,
        types: &[Type],
        constructors: &[String],
        pos: &Pos,
    ) -> Result<Vec<hir::Stmt>, String> {
        let locals = JsonLocals::new(pos);
        let return_value = |value: hir::Expr| hir::Stmt::Return {
            value: Some(value),
            pos: pos.clone(),
        };
        if let Some(target) = json_number_target(ty) {
            if matches!(ty, Type::F32 | Type::F64) {
                let number = self.json_call(
                    JsonFn::ParseNumber,
                    vec![locals.parser(), locals.node()],
                    Type::F64,
                    pos,
                );
                return Ok(vec![return_value(if *ty == Type::F64 {
                    number
                } else {
                    json_cast(number, ty.clone(), pos)
                })]);
            }
            let integer = self.json_call(
                JsonFn::ParseInteger,
                vec![locals.parser(), locals.node(), json_int(target, pos)],
                Type::U64,
                pos,
            );
            return Ok(vec![return_value(if *ty == Type::U64 {
                integer
            } else {
                json_cast(integer, ty.clone(), pos)
            })]);
        }
        match ty {
            Type::Bool => Ok(vec![return_value(self.json_call(
                JsonFn::ParseBool,
                vec![locals.parser(), locals.node()],
                Type::Bool,
                pos,
            ))]),
            Type::Str => Ok(vec![return_value(self.json_call(
                JsonFn::ParseString,
                vec![locals.parser(), locals.node()],
                Type::Str,
                pos,
            ))]),
            Type::Nullable(inner) => Ok(vec![hir::Stmt::If {
                cond: self.json_call(
                    JsonFn::ParseIsKind,
                    vec![
                        locals.parser(),
                        locals.node(),
                        json_int(JsonKind::NULL, pos),
                    ],
                    Type::Bool,
                    pos,
                ),
                then: vec![return_value(hir::Expr {
                    kind: ExprKind::Null,
                    ty: Type::Null,
                    pos: pos.clone(),
                })],
                els: Some(vec![return_value(script_call(
                    constructors[json_type_index(types, inner)?].clone(),
                    vec![locals.parser(), locals.node()],
                    (**inner).clone(),
                    pos,
                ))]),
                pos: pos.clone(),
            }]),
            Type::Array(element) | Type::FixedArray(element, _) => {
                self.json_array_construction_body(ty, element, types, constructors, pos)
            }
            Type::Class(id) => {
                let init = if self.classes[id.0].is_value {
                    json_zero(Type::Class(*id), pos)
                } else {
                    hir::Expr {
                        kind: ExprKind::RawNew { class: *id },
                        ty: Type::Class(*id),
                        pos: pos.clone(),
                    }
                };
                let mut body = vec![hir::Stmt::Let {
                    name: "value".to_string(),
                    ty: Type::Class(*id),
                    mutable: false,
                    dispose: false,
                    init,
                    pos: pos.clone(),
                }];
                for field in &self.classes[id.0].fields {
                    let field_node = self.json_call(
                        JsonFn::ParseObjectGet,
                        vec![
                            locals.parser(),
                            locals.node(),
                            json_string(&field.name, pos),
                        ],
                        Type::U64,
                        pos,
                    );
                    let constructed = script_call(
                        constructors[json_type_index(types, &field.ty)?].clone(),
                        vec![locals.parser(), field_node],
                        field.ty.clone(),
                        pos,
                    );
                    body.push(hir::Stmt::Expr(json_assign(
                        json_field(
                            locals.value(Type::Class(*id)),
                            &field.name,
                            field.ty.clone(),
                            pos,
                        ),
                        constructed,
                        field.ty.clone(),
                        pos,
                    )));
                }
                body.push(return_value(locals.value(Type::Class(*id))));
                Ok(body)
            }
            other => Err(format!("rejected JSON constructor type {other:?}")),
        }
    }

    fn json_array_construction_body(
        &self,
        array_ty: &Type,
        element: &Type,
        types: &[Type],
        constructors: &[String],
        pos: &Pos,
    ) -> Result<Vec<hir::Stmt>, String> {
        let locals = JsonLocals::new(pos);
        let (length, init) = match array_ty {
            Type::FixedArray(_, length) => (
                json_int(i64::from(*length), pos),
                json_zero(array_ty.clone(), pos),
            ),
            Type::Array(_) => (
                self.json_call(
                    JsonFn::ParseArrayLen,
                    vec![locals.parser(), locals.node()],
                    Type::I32,
                    pos,
                ),
                hir::Expr {
                    kind: ExprKind::ArrayLit(Vec::new()),
                    ty: array_ty.clone(),
                    pos: pos.clone(),
                },
            ),
            other => {
                return Err(format!(
                    "JSON array constructor received non-array type {other:?}"
                ))
            }
        };
        let mut body = vec![
            hir::Stmt::Let {
                name: "value".to_string(),
                ty: array_ty.clone(),
                mutable: false,
                dispose: false,
                init,
                pos: pos.clone(),
            },
            hir::Stmt::Let {
                name: "length".to_string(),
                ty: Type::I32,
                mutable: false,
                dispose: false,
                init: length,
                pos: pos.clone(),
            },
            hir::Stmt::Let {
                name: "index".to_string(),
                ty: Type::I32,
                mutable: true,
                dispose: false,
                init: json_int(0, pos),
                pos: pos.clone(),
            },
        ];
        let child_node = self.json_call(
            JsonFn::ParseArrayGet,
            vec![locals.parser(), locals.node(), locals.index()],
            Type::U64,
            pos,
        );
        let child = script_call(
            constructors[json_type_index(types, element)?].clone(),
            vec![locals.parser(), child_node],
            element.clone(),
            pos,
        );
        let store = match array_ty {
            Type::Array(_) => hir::Expr {
                kind: ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(locals.value(array_ty.clone())),
                        name: "push".to_string(),
                    },
                    args: vec![child],
                },
                ty: Type::I32,
                pos: pos.clone(),
            },
            Type::FixedArray(..) => json_assign(
                hir::Expr {
                    kind: ExprKind::Index {
                        obj: Box::new(locals.value(array_ty.clone())),
                        index: Box::new(locals.index()),
                        checked: true,
                    },
                    ty: element.clone(),
                    pos: pos.clone(),
                },
                child,
                element.clone(),
                pos,
            ),
            other => {
                return Err(format!(
                    "JSON array store received non-array type {other:?}"
                ))
            }
        };
        self.register_operation_signature(&store);
        body.push(hir::Stmt::While {
            cond: json_binary(BinOp::Lt, locals.index(), locals.length(), Type::Bool, pos),
            body: vec![
                hir::Stmt::Expr(store),
                hir::Stmt::Expr(json_increment(locals.index(), pos)),
            ],
            pos: pos.clone(),
        });
        body.push(hir::Stmt::Return {
            value: Some(locals.value(array_ty.clone())),
            pos: pos.clone(),
        });
        Ok(body)
    }
}

fn json_type_index(types: &[Type], ty: &Type) -> Result<usize, String> {
    types
        .iter()
        .position(|candidate| candidate == ty)
        .ok_or_else(|| format!("JSON type graph is missing {ty:?}"))
}

fn json_param(name: &str, ty: Type, pos: &Pos) -> hir::Param {
    hir::Param {
        name: name.to_string(),
        ty,
        default: None,
        foreign_provenance: None,
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

struct JsonLocals<'a> {
    pos: &'a Pos,
}

impl<'a> JsonLocals<'a> {
    fn new(pos: &'a Pos) -> Self {
        Self { pos }
    }

    fn local(&self, name: &str, ty: Type) -> hir::Expr {
        json_local(name, ty, self.pos)
    }

    fn builder(&self) -> hir::Expr {
        self.local("builder", Type::U64)
    }

    fn parser(&self) -> hir::Expr {
        self.local("parser", Type::U64)
    }

    fn node(&self) -> hir::Expr {
        self.local("node", Type::U64)
    }

    fn index(&self) -> hir::Expr {
        self.local("index", Type::I32)
    }

    fn length(&self) -> hir::Expr {
        self.local("length", Type::I32)
    }

    fn value(&self, ty: Type) -> hir::Expr {
        self.local("value", ty)
    }

    fn result(&self, ty: Type) -> hir::Expr {
        self.local("result", ty)
    }

    fn text(&self) -> hir::Expr {
        self.local("text", Type::Str)
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

fn json_u64(value: u64, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Int(value as i64),
        ty: Type::U64,
        pos: pos.clone(),
    }
}

fn json_bool(value: bool, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Bool(value),
        ty: Type::Bool,
        pos: pos.clone(),
    }
}

fn json_zero(ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Zero,
        ty,
        pos: pos.clone(),
    }
}

fn json_field(obj: hir::Expr, name: &str, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Field {
            obj: Box::new(obj),
            name: name.to_string(),
        },
        ty,
        pos: pos.clone(),
    }
}

fn json_assign(target: hir::Expr, value: hir::Expr, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Assign {
            op: None,
            target: Box::new(target),
            value: Box::new(value),
        },
        ty,
        pos: pos.clone(),
    }
}

fn json_binary(op: BinOp, left: hir::Expr, right: hir::Expr, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        ty,
        pos: pos.clone(),
    }
}

fn json_return_false_unless(condition: hir::Expr, pos: &Pos) -> hir::Stmt {
    hir::Stmt::If {
        cond: hir::Expr {
            kind: ExprKind::Unary {
                op: UnOp::Not,
                operand: Box::new(condition),
            },
            ty: Type::Bool,
            pos: pos.clone(),
        },
        then: vec![hir::Stmt::Return {
            value: Some(json_bool(false, pos)),
            pos: pos.clone(),
        }],
        els: None,
        pos: pos.clone(),
    }
}

fn json_increment(index: hir::Expr, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Assign {
            op: Some(BinOp::Add),
            target: Box::new(index),
            value: Box::new(json_int(1, pos)),
        },
        ty: Type::I32,
        pos: pos.clone(),
    }
}

struct JsonKind;

impl JsonKind {
    const NULL: i64 = 0;
    const BOOL: i64 = 1;
    const STRING: i64 = 3;
    const ARRAY: i64 = 4;
    const OBJECT: i64 = 5;
}

/// Stable target codes mirrored by runtime::json's `NUMBER_*` constants.
fn json_number_target(ty: &Type) -> Option<i64> {
    Some(match ty {
        Type::I8 => 0,
        Type::U8 => 1,
        Type::I16 => 2,
        Type::U16 => 3,
        Type::I32 => 4,
        Type::U32 => 5,
        Type::I64 => 6,
        Type::U64 => 7,
        Type::F32 => 8,
        Type::F64 => 9,
        _ => return None,
    })
}

fn json_cast(value: hir::Expr, ty: Type, pos: &Pos) -> hir::Expr {
    hir::Expr {
        kind: ExprKind::Cast(Box::new(value)),
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
