//! Resolution of TypeScript type annotations to language [`Type`]s,
//! including the banned-type rules (S001 `any`, S007 bare `number`,
//! S011 general unions, S012 `undefined`, S013 `Promise`).

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::RuleCode;
use crate::types::{FuncType, Type};

use super::{Checker, ScopeItem};

impl<'p> Checker<'p> {
    /// Resolves an annotation to a language type, emitting rule
    /// diagnostics for banned spellings. Errors resolve to
    /// [`Type::Error`] so one bad annotation does not cascade.
    pub(crate) fn resolve_type(&mut self, ty: &ast::TsType) -> Type {
        match ty {
            ast::TsType::TsKeywordType(kw) => self.resolve_keyword(kw),
            ast::TsType::TsTypeRef(r) => self.resolve_type_ref(r),
            ast::TsType::TsArrayType(arr) => {
                let elem = self.resolve_type(&arr.elem_type);
                Type::Array(Box::new(elem))
            }
            ast::TsType::TsUnionOrIntersectionType(u) => self.resolve_union(u),
            ast::TsType::TsFnOrConstructorType(f) => self.resolve_fn_type(f),
            ast::TsType::TsParenthesizedType(p) => self.resolve_type(&p.type_ann),
            other => {
                let pos = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "type annotation form outside the decided surface",
                    pos,
                );
                Type::Error
            }
        }
    }

    fn resolve_keyword(&mut self, kw: &ast::TsKeywordType) -> Type {
        use ast::TsKeywordTypeKind::*;
        let pos = self.pos(kw.span);
        match kw.kind {
            TsNumberKeyword => {
                self.error(
                    RuleCode::S007,
                    "bare `number` is rejected; there is no default numeric type — \
                     use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, \
                     f16, f32, f64)",
                    pos,
                );
                Type::Error
            }
            TsAnyKeyword => {
                self.error(RuleCode::S001, "`any` is not part of the language", pos);
                Type::Error
            }
            TsUndefinedKeyword => {
                self.error(
                    RuleCode::S012,
                    "`undefined` is banned; the single null story is `null`",
                    pos,
                );
                Type::Error
            }
            TsBooleanKeyword => Type::Bool,
            TsStringKeyword => Type::Str,
            TsVoidKeyword => Type::Void,
            TsNullKeyword => Type::Null,
            TsObjectKeyword => {
                // C7: the boundary-opaque `object` (and `object | null`)
                // exists only at the C boundary. It is legal while
                // resolving a mirror declaration (`in_boundary`); general
                // declarations may not spell it. The ambient
                // `unsafeDelete(value: object)` signature is hardcoded and
                // unaffected.
                if self.in_boundary
                    || self.in_assoc_key
                    || self.in_json_argument
                    || self.in_for_of_subject
                {
                    Type::Object
                } else {
                    self.error(
                        RuleCode::S011,
                        "`object` is a boundary-only type; it is not available to \
                         general declarations",
                        pos,
                    );
                    Type::Error
                }
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    "keyword type outside the decided surface",
                    pos,
                );
                Type::Error
            }
        }
    }

    fn resolve_type_ref(&mut self, r: &ast::TsTypeRef) -> Type {
        let ast::TsEntityName::Ident(ident) = &r.type_name else {
            let pos = self.pos(r.span);
            self.error(RuleCode::S100, "qualified type names are not decided", pos);
            return Type::Error;
        };
        let name = ident.sym.as_ref();
        let pos = self.pos(ident.span);

        if let Some(bound) = self.subst.get(name) {
            return bound.clone();
        }
        if let Some(sized) = crate::ambient::sized_alias(name) {
            return sized;
        }
        // Mirror `type` aliases (function-pointer typedefs, flag-set
        // `u64` aliases) resolve to their aliased language type (P5.2).
        if let Some(alias) = self.type_aliases.get(name) {
            return alias.clone();
        }
        match name {
            "Promise" => {
                self.error(
                    RuleCode::S013,
                    "`Promise` requires an event loop; the language has none",
                    pos,
                );
                return Type::Error;
            }
            "FixedArray" => {
                let Some(args) = &r.type_params else {
                    self.error(
                        RuleCode::S100,
                        "`FixedArray` requires element type and length arguments",
                        pos,
                    );
                    return Type::Error;
                };
                if args.params.len() != 2 {
                    self.error(
                        RuleCode::S100,
                        "`FixedArray` takes exactly two type arguments",
                        pos,
                    );
                    return Type::Error;
                }
                let elem = self.resolve_type(&args.params[0]);
                let len = match &*args.params[1] {
                    ast::TsType::TsLitType(ast::TsLitType {
                        lit: ast::TsLit::Number(n),
                        ..
                    }) if n.value >= 0.0 && n.value.fract() == 0.0 => {
                        if n.value > f64::from(u32::MAX) {
                            let p = self.pos(args.params[1].span());
                            self.error(
                                RuleCode::S008,
                                format!(
                                    "FixedArray length {} out of range (maximum {})",
                                    n.value,
                                    u32::MAX
                                ),
                                p,
                            );
                            return Type::Error;
                        }
                        n.value as u32
                    }
                    other => {
                        let p = self.pos(other.span());
                        self.error(
                            RuleCode::S100,
                            "`FixedArray` length must be a non-negative integer literal",
                            p,
                        );
                        return Type::Error;
                    }
                };
                let fixed = Type::FixedArray(Box::new(elem), len);
                match super::layout::class_independent_layout(&fixed) {
                    super::layout::IndependentLayout::Fits => return fixed,
                    super::layout::IndependentLayout::TooLarge => {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "`FixedArray` byte size exceeds the supported aggregate limit \
                                 of {} bytes",
                                crate::types::MAX_AGGREGATE_BYTES
                            ),
                            pos,
                        );
                        return Type::Error;
                    }
                    super::layout::IndependentLayout::DependsOnClass => {
                        self.pending_layouts
                            .push((fixed.clone(), pos, "`FixedArray` byte size"));
                        return fixed;
                    }
                }
            }
            "Array" => {
                if let Some(args) = &r.type_params {
                    if args.params.len() == 1 {
                        let elem = self.resolve_type(&args.params[0]);
                        return Type::Array(Box::new(elem));
                    }
                }
                self.error(RuleCode::S100, "`Array` takes one type argument", pos);
                return Type::Error;
            }
            "Generator" => {
                if let Some(args) = &r.type_params {
                    if let Some(first) = args.params.first() {
                        let y = self.resolve_type(first);
                        return Type::Generator(Box::new(y));
                    }
                }
                self.error(
                    RuleCode::S100,
                    "`Generator` requires at least a yield type argument",
                    pos,
                );
                return Type::Error;
            }
            _ => {}
        }

        // P13's ambient generic result reference. Like Map/Set below, the
        // language checker monomorphizes it directly; a source declaration
        // with the same name shadows the ambient class.
        if name == "JsonResult" && self.scope_item(name).is_none() {
            let Some(args) = &r.type_params else {
                self.error(
                    RuleCode::S100,
                    "generic reference class `JsonResult` requires one type argument",
                    pos,
                );
                return Type::Error;
            };
            if args.params.len() != 1 {
                self.error(
                    RuleCode::S100,
                    "`JsonResult` takes exactly one type argument",
                    pos,
                );
                return Type::Error;
            }
            let value = self.resolve_type(&args.params[0]);
            let id = self.instantiate_json_result(&value, pos);
            return Type::Class(id);
        }

        // The ES2022 lib supplies the editor declarations; the language
        // checker resolves its accepted, monomorphized subset directly.
        // A program declaration shadows the ambient name, as for Date.
        if (name == "Map" || name == "Set") && self.scope_item(name).is_none() {
            let Some(args) = &r.type_params else {
                self.error(
                    RuleCode::S100,
                    format!("generic reference class `{name}` requires explicit type arguments"),
                    pos,
                );
                return Type::Error;
            };
            let expected = if name == "Map" { 2 } else { 1 };
            if args.params.len() != expected {
                self.error(
                    RuleCode::S100,
                    format!("`{name}` takes exactly {expected} type argument(s)"),
                    pos,
                );
                return Type::Error;
            }
            let saved = self.in_assoc_key;
            self.in_assoc_key = true;
            let key = self.resolve_type(&args.params[0]);
            // Only this container's key position may temporarily admit
            // boundary-only shapes so the Q24 whitelist can issue S014.
            // A nested container's value is a general declaration even
            // when the container itself appears as an outer key.
            self.in_assoc_key = false;
            if !matches!(key, Type::Error) && self.assoc_key_kind(&key).is_none() {
                let key_pos = self.pos(args.params[0].span());
                let key_name = self.type_name(&key);
                self.error(
                    RuleCode::S014,
                    format!(
                        "`{key_name}` is not a Map/Set key kind; Q24 permits sized \
                         integers, boolean, enum, f32/f64, string, Date, and \
                         reference classes"
                    ),
                    key_pos,
                );
            }
            if name == "Map" {
                let value = self.resolve_type(&args.params[1]);
                self.in_assoc_key = saved;
                return Type::Map(Box::new(key), Box::new(value));
            }
            self.in_assoc_key = saved;
            return Type::Set(Box::new(key));
        }

        // The ambient `Date` value type (stdlib.md §3): applies only
        // when no program declaration shadows the name — a user class
        // named `Date` wins, exactly as for `Math`.
        if name == "Date" && self.scope_item(name).is_none() {
            if r.type_params.is_some() {
                self.error(RuleCode::S100, "`Date` is not generic", pos);
                return Type::Error;
            }
            return Type::Date;
        }

        match self.scope_item(name) {
            Some(ScopeItem::Class(id)) => {
                if r.type_params.is_some() {
                    self.error(
                        RuleCode::S100,
                        format!("`{}` is not generic", name),
                        pos,
                    );
                }
                Type::Class(id)
            }
            Some(ScopeItem::GenericClass(key)) => {
                let Some(args) = &r.type_params else {
                    self.error(
                        RuleCode::S100,
                        format!("generic class `{}` requires explicit type arguments", name),
                        pos,
                    );
                    return Type::Error;
                };
                let resolved: Vec<Type> =
                    args.params.iter().map(|t| self.resolve_type(t)).collect();
                match self.instantiate_class(&key, &resolved, pos) {
                    Some(id) => Type::Class(id),
                    None => Type::Error,
                }
            }
            Some(ScopeItem::Enum(id)) => Type::Enum(id),
            _ => {
                self.error(
                    RuleCode::S100,
                    format!("unknown type name `{}`", name),
                    pos,
                );
                Type::Error
            }
        }
    }

    fn resolve_union(&mut self, u: &ast::TsUnionOrIntersectionType) -> Type {
        let union = match u {
            ast::TsUnionOrIntersectionType::TsUnionType(union) => union,
            ast::TsUnionOrIntersectionType::TsIntersectionType(i) => {
                let pos = self.pos(i.span);
                self.error(
                    RuleCode::S100,
                    "intersection types are not in the decided surface",
                    pos,
                );
                return Type::Error;
            }
        };
        for member in &union.types {
            if let ast::TsType::TsKeywordType(kw) = &**member {
                if kw.kind == ast::TsKeywordTypeKind::TsUndefinedKeyword {
                    let pos = self.pos(kw.span);
                    self.error(
                        RuleCode::S012,
                        "`undefined` is banned; the single null story is `null`",
                        pos,
                    );
                    return Type::Error;
                }
            }
        }
        let is_null = |t: &ast::TsType| {
            matches!(
                t,
                ast::TsType::TsKeywordType(kw) if kw.kind == ast::TsKeywordTypeKind::TsNullKeyword
            )
        };
        if union.types.len() == 2 {
            let (base, has_null) = if is_null(&union.types[1]) {
                (&union.types[0], true)
            } else if is_null(&union.types[0]) {
                (&union.types[1], true)
            } else {
                (&union.types[0], false)
            };
            if has_null {
                let inner = self.resolve_type(base);
                if matches!(inner, Type::Error) {
                    return Type::Error;
                }
                // C7: `Ref | null` for reference shapes (classes, opaque
                // handles, functions, `object`). At a boundary position
                // (`in_boundary`), the `Struct | null` form is also legal
                // — a value-class-with-null whose `null` lowers to the
                // zeroed struct (P5.2b). It stays rejected elsewhere.
                if self.in_assoc_key {
                    return Type::Nullable(Box::new(inner));
                }
                let ok = (inner.is_reference_shape() && !self.is_value_class(&inner))
                    || (self.in_boundary && self.is_value_class(&inner));
                if ok {
                    return Type::Nullable(Box::new(inner));
                }
                let pos = self.pos(base.span());
                let name = self.type_name(&inner);
                self.error(
                    RuleCode::S011,
                    format!(
                        "unions are limited to `Ref | null`; `{} | null` is not a \
                         reference type union",
                        name
                    ),
                    pos,
                );
                return Type::Error;
            }
        }
        let pos = self.pos(union.span);
        self.error(
            RuleCode::S011,
            "unions are limited to `Ref | null`",
            pos,
        );
        Type::Error
    }

    fn resolve_fn_type(&mut self, f: &ast::TsFnOrConstructorType) -> Type {
        let fn_ty = match f {
            ast::TsFnOrConstructorType::TsFnType(fn_ty) => fn_ty,
            ast::TsFnOrConstructorType::TsConstructorType(c) => {
                let pos = self.pos(c.span);
                self.error(
                    RuleCode::S100,
                    "constructor types are not in the decided surface",
                    pos,
                );
                return Type::Error;
            }
        };
        let mut params = Vec::new();
        for p in &fn_ty.params {
            match p {
                ast::TsFnParam::Ident(binding) => match &binding.type_ann {
                    Some(ann) => params.push(self.resolve_type(&ann.type_ann)),
                    None => {
                        let pos = self.pos(binding.id.span);
                        self.error(
                            RuleCode::S100,
                            "function type parameters require annotations",
                            pos,
                        );
                        params.push(Type::Error);
                    }
                },
                other => {
                    let pos = self.pos(other.span());
                    self.error(
                        RuleCode::S100,
                        "function type parameter form outside the decided surface",
                        pos,
                    );
                    params.push(Type::Error);
                }
            }
        }
        let ret = self.resolve_type(&fn_ty.type_ann.type_ann);
        Type::Func(Box::new(FuncType { params, ret }))
    }
}
