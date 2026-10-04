//! Resolution of TypeScript type annotations to language [`Type`]s,
//! including the banned-type rules (S001 `any`, S007 bare `number`,
//! S011 general unions, S012 `undefined`, S013 `Promise`).

use crate::check::rejection::RejectionSite;
use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::types::Type;

use super::{Checker, ContainerSlot, ScopeItem};

impl<'p> Checker<'p> {
    /// Resolves the mandatory source-level `Promise<T>` view of an async
    /// function and returns the fulfilled `T` carried in HIR. Promise is
    /// deliberately not a general [`Type`]: no Promise value exists in the
    /// language (Q34).
    pub(crate) fn resolve_async_return(&mut self, ty: &ast::TsType) -> Type {
        let ast::TsType::TsTypeRef(reference) = ty else {
            let pos = self.pos(ty.span());
            let parenthesized_promise = match ty {
                ast::TsType::TsParenthesizedType(p) => {
                    let resolved = self.resolve_type(&p.type_ann);
                    matches!(self.apparent_type(&resolved), Type::AsyncHandle(_))
                }
                _ => false,
            };
            self.reject_subset(
                if parenthesized_promise {
                    RejectionSite::AsyncReturnNonReference
                } else {
                    RejectionSite::AsyncReturnNotPromise
                },
                "async functions must return an explicitly annotated builtin `Promise<T>`",
                pos,
            );
            return Type::Error;
        };
        let ast::TsEntityName::Ident(ident) = &reference.type_name else {
            let pos = self.pos(reference.span);
            self.reject_subset(
                RejectionSite::AsyncReturnQualifiedName,
                "async functions must return an explicitly annotated builtin `Promise<T>`",
                pos,
            );
            return Type::Error;
        };
        if matches!(
            self.type_scope_item(ident.sym.as_ref()),
            Some(ScopeItem::Poisoned)
        ) {
            return Type::Error;
        }
        if ident.sym.as_ref() != "Promise" || self.type_scope_item("Promise").is_some() {
            let pos = self.pos(ident.span);
            self.reject_subset(
                if self
                    .type_scope_item(ident.sym.as_ref())
                    .is_some_and(|item| {
                        let ScopeItem::TypeAlias(ty) = item else {
                            return false;
                        };
                        matches!(self.apparent_type(&ty), Type::AsyncHandle(_))
                    })
                {
                    RejectionSite::AsyncReturnAlias
                } else {
                    RejectionSite::AsyncReturnSourceNotPromise
                },
                "async functions must return an explicitly annotated builtin `Promise<T>`",
                pos,
            );
            return Type::Error;
        }
        let Some(args) = &reference.type_params else {
            let pos = self.pos(ident.span);
            self.reject_subset(
                RejectionSite::AsyncReturnMissingArgument,
                "`Promise` requires exactly one fulfilled-value type argument",
                pos,
            );
            return Type::Error;
        };
        if args.params.len() != 1 {
            let pos = self.pos(ident.span);
            self.reject_subset(
                RejectionSite::AsyncReturnArgumentCount,
                "`Promise` requires exactly one fulfilled-value type argument",
                pos,
            );
            return Type::Error;
        }
        self.resolve_result_type(&args.params[0])
    }

    /// Resolves an annotation to a language type, emitting rule
    /// diagnostics for banned spellings. Errors resolve to
    /// [`Type::Error`] so one bad annotation does not cascade.
    pub(crate) fn resolve_type(&mut self, ty: &ast::TsType) -> Type {
        self.resolve_type_position(ty, false)
    }

    /// Resolves a return, generator element, or Promise result type.
    pub(crate) fn resolve_result_type(&mut self, ty: &ast::TsType) -> Type {
        self.resolve_type_position(ty, true)
    }

    fn resolve_type_position(&mut self, ty: &ast::TsType, allow_void: bool) -> Type {
        let resolved = match ty {
            ast::TsType::TsKeywordType(kw) => self.resolve_keyword(kw),
            ast::TsType::TsTypeRef(r) => self.resolve_type_ref(r),
            ast::TsType::TsArrayType(arr) => {
                let elem = self.resolve_type(&arr.elem_type);
                let elem_pos = self.pos(arr.elem_type.span());
                let elem = self.container_argument(ContainerSlot::ArrayElement, elem, elem_pos);
                Type::array(elem)
            }
            ast::TsType::TsUnionOrIntersectionType(u) => self.resolve_union(u),
            ast::TsType::TsFnOrConstructorType(f) => self.resolve_fn_type(f),
            ast::TsType::TsParenthesizedType(p) => {
                self.resolve_type_position(&p.type_ann, allow_void)
            }
            other => {
                let pos = self.pos(other.span());
                self.reject_subset(
                    match other {
                        ast::TsType::TsTupleType(_) => RejectionSite::TupleAnnotation,
                        ast::TsType::TsThisType(_) => RejectionSite::ThisAnnotation,
                        ast::TsType::TsTypeQuery(_) => RejectionSite::QueryAnnotation,
                        ast::TsType::TsTypeLit(_) => RejectionSite::StructuralAnnotation,
                        ast::TsType::TsOptionalType(_) => RejectionSite::OptionalAnnotation,
                        ast::TsType::TsRestType(_) => RejectionSite::RestAnnotation,
                        ast::TsType::TsConditionalType(_) => RejectionSite::ConditionalAnnotation,
                        ast::TsType::TsInferType(_) => RejectionSite::InferAnnotation,
                        ast::TsType::TsTypeOperator(_) => RejectionSite::OperatorAnnotation,
                        ast::TsType::TsIndexedAccessType(_) => RejectionSite::IndexedAnnotation,
                        ast::TsType::TsMappedType(_) => RejectionSite::MappedAnnotation,
                        ast::TsType::TsTypePredicate(_) => RejectionSite::PredicateAnnotation,
                        ast::TsType::TsImportType(_) => RejectionSite::ImportAnnotation,
                        ast::TsType::TsLitType(lit) => match &lit.lit {
                            ast::TsLit::Str(_) => RejectionSite::StringLiteralAnnotation,
                            ast::TsLit::Number(_) => RejectionSite::NumberLiteralAnnotation,
                            ast::TsLit::Bool(_) => RejectionSite::BooleanLiteralAnnotation,
                            ast::TsLit::BigInt(_) => RejectionSite::BigIntLiteralAnnotation,
                            ast::TsLit::Tpl(_) => RejectionSite::TemplateLiteralAnnotation,
                        },
                        _ => RejectionSite::UnsupportedAnnotationKind,
                    },
                    "type annotation form outside the decided surface",
                    pos,
                );
                Type::Error
            }
        };
        if !allow_void && self.apparent_type(&resolved) == Type::Void {
            self.reject_subset(
                RejectionSite::VoidTypeOutsideResult,
                "`void` is only allowed as a return, generator element, or Promise result type",
                self.pos(ty.span()),
            );
            Type::Error
        } else {
            resolved
        }
    }

    fn resolve_keyword(&mut self, kw: &ast::TsKeywordType) -> Type {
        use ast::TsKeywordTypeKind::*;
        let pos = self.pos(kw.span);
        match kw.kind {
            TsNumberKeyword => {
                self.reject_subset(
                    RejectionSite::AnyTypeAnnotation,
                    "bare `number` is rejected; there is no default numeric type — \
                     use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, \
                     f16, f32, f64)",
                    pos,
                );
                Type::Error
            }
            TsAnyKeyword => {
                self.reject_subset(
                    RejectionSite::UnknownTypeAnnotation,
                    "`any` is not part of the language",
                    pos,
                );
                Type::Error
            }
            TsUndefinedKeyword => {
                self.reject_subset(
                    RejectionSite::UndefinedKeywordAnnotation,
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
                // `Context.free(value: object)` signature is hardcoded and
                // unaffected.
                if self.in_boundary
                    || self.in_assoc_key
                    || self.in_json_argument
                    || self.in_for_of_subject
                {
                    Type::Object
                } else {
                    self.reject_subset(
                        RejectionSite::ObjectTypeOutsideBoundary,
                        "`object` is a boundary-only type; it is not available to \
                         general declarations",
                        pos,
                    );
                    Type::Error
                }
            }
            _ => {
                self.reject_subset(
                    match kw.kind {
                        TsNeverKeyword => RejectionSite::NeverAnnotation,
                        TsUnknownKeyword => RejectionSite::UnknownAnnotation,
                        TsSymbolKeyword => RejectionSite::SymbolAnnotation,
                        TsBigIntKeyword => RejectionSite::BigIntAnnotation,
                        _ => RejectionSite::UnsupportedKeywordKind,
                    },
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
            self.reject_subset(
                RejectionSite::QualifiedSourceTypeName,
                "qualified type names are not decided",
                pos,
            );
            return Type::Error;
        };
        let name = ident.sym.as_ref();
        let pos = self.pos(ident.span);

        if let Some(bound) = self.subst.get(name) {
            return bound.clone();
        }
        if self.type_scope_item(name).is_none() {
            if let Some(sized) = crate::ambient::sized_alias(name) {
                return sized;
            }
        }
        // Mirror `type` aliases (function-pointer typedefs, flag-set
        // `u64` aliases) resolve to their aliased language type (§12.2).
        if name != "Array" {
            if let Some(ScopeItem::TypeAlias(alias)) = self.type_scope_item(name) {
                return alias;
            }
        }
        match name {
            "Worker" | "Inbox" | "Outbox" if self.type_scope_item(name).is_none() => {
                let expected = if name == "Worker" { 2 } else { 1 };
                let Some(args) = &r.type_params else {
                    self.reject_subset(
                        RejectionSite::WorkerTypeArgumentsMissing,
                        format!(
                            "generic reference class `{name}` requires explicit type arguments"
                        ),
                        pos,
                    );
                    return Type::Error;
                };
                if args.params.len() != expected {
                    self.reject_subset(
                        RejectionSite::WorkerTypeArgumentCount,
                        format!("`{name}` takes exactly {expected} type argument(s)"),
                        pos,
                    );
                    return Type::Error;
                }
                let mut messages = Vec::with_capacity(expected);
                for argument in &args.params {
                    let message = self.resolve_type(argument);
                    let plain_reference = match self.apparent_type(&message) {
                        Type::Class(id) => self.classes.get(id.0).is_some_and(|class| {
                            !class.is_value
                                && !class.is_descriptor
                                && !class.is_boundary
                                && !self.handle_classes.contains(&id)
                        }),
                        Type::Error => true,
                        _ => false,
                    };
                    if !plain_reference && !matches!(self.apparent_type(&message), Type::Error) {
                        let type_name = self.type_name(&message);
                        self.reject_subset(
                            if matches!(self.apparent_type(&message), Type::Class(_)) {
                                RejectionSite::WorkerMessagePlainClass
                            } else {
                                RejectionSite::WorkerMessageNonClass
                            },
                            format!(
                                "worker message type `{type_name}` must be a plain reference class"
                            ),
                            self.pos(argument.span()),
                        );
                        messages.push(Type::Error);
                    } else {
                        messages.push(message);
                    }
                }
                return match name {
                    "Worker" => Type::worker(messages[0].clone(), messages[1].clone()),
                    "Inbox" => Type::inbox(messages[0].clone()),
                    "Outbox" => Type::outbox(messages[0].clone()),
                    _ => unreachable!("matched worker ambient name"),
                };
            }
            "RegExp" if self.type_scope_item(name).is_none() => {
                if r.type_params.is_some() {
                    self.reject_subset(
                        RejectionSite::RegExpTypeArguments,
                        "`RegExp` is not generic",
                        pos,
                    );
                    return Type::Error;
                }
                return Type::RegExp;
            }
            "RegExpMatchArray" if self.type_scope_item(name).is_none() => {
                let message = "`RegExpMatchArray` is rejected: `groups` requires an object with dynamic keys, which the language does not have (Q31)";
                self.reject_subset(RejectionSite::RegexMatchType, message, pos);
                return Type::Error;
            }
            "Promise" if self.type_scope_item(name).is_none() => {
                let Some(args) = &r.type_params else {
                    self.reject_subset(
                        RejectionSite::PromiseTypeArgumentMissing,
                        "`Promise` requires exactly one fulfilled-value type argument",
                        pos,
                    );
                    return Type::Error;
                };
                if args.params.len() != 1 {
                    self.reject_subset(
                        RejectionSite::PromiseTypeArgumentCount,
                        "`Promise` requires exactly one fulfilled-value type argument",
                        pos,
                    );
                    return Type::Error;
                }
                let value = self.resolve_result_type(&args.params[0]);
                return Type::async_handle(value);
            }
            "FixedArray" if self.type_scope_item(name).is_none() => {
                let Some(args) = &r.type_params else {
                    self.reject_subset(
                        RejectionSite::FixedArrayTypeArgumentsMissing,
                        "`FixedArray` requires element type and length arguments",
                        pos,
                    );
                    return Type::Error;
                };
                if args.params.len() != 2 {
                    self.reject_subset(
                        RejectionSite::FixedArrayTypeArgumentCount,
                        "`FixedArray` takes exactly two type arguments",
                        pos,
                    );
                    return Type::Error;
                }
                let elem = self.resolve_type(&args.params[0]);
                let elem_pos = self.pos(args.params[0].span());
                let elem = self.container_argument(ContainerSlot::ArrayElement, elem, elem_pos);
                let len = match &*args.params[1] {
                    ast::TsType::TsLitType(ast::TsLitType {
                        lit: ast::TsLit::Number(n),
                        ..
                    }) if n.value >= 0.0 && n.value.fract() == 0.0 => {
                        if n.value > f64::from(u32::MAX) {
                            let p = self.pos(args.params[1].span());
                            self.reject_subset(
                                RejectionSite::FixedArrayLengthRange,
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
                        self.reject_subset(
                            RejectionSite::FixedArrayLengthLiteral,
                            "`FixedArray` length must be a non-negative integer literal",
                            p,
                        );
                        return Type::Error;
                    }
                };
                let fixed = Type::fixed_array(elem, len);
                if self.instance_restriction(
                    super::opaque::InstanceRestriction::AggregateLayout,
                    &fixed,
                ) {
                    return fixed;
                }
                match super::layout::class_independent_layout(&fixed) {
                    super::layout::IndependentLayout::Fits => return fixed,
                    super::layout::IndependentLayout::TooLarge => {
                        let message = format!(
                            "`FixedArray` byte size exceeds the supported aggregate limit \
                             of {} bytes",
                            crate::types::MAX_AGGREGATE_BYTES
                        );
                        if let Some(site) = self.aggregate_type_site {
                            self.reject_subset(site, message, pos);
                        } else {
                            self.reject_subset(RejectionSite::FixedArrayByteLimit, message, pos);
                        }
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
                        let elem_pos = self.pos(args.params[0].span());
                        let elem =
                            self.container_argument(ContainerSlot::ArrayElement, elem, elem_pos);
                        return Type::array(elem);
                    }
                }
                self.reject_subset(
                    if self.scope_binding(name).is_some() {
                        RejectionSite::ArrayTypeArgumentCount
                    } else {
                        RejectionSite::BuiltinArrayTypeArgumentCount
                    },
                    "`Array` takes one type argument",
                    pos,
                );
                return Type::Error;
            }
            "Generator" if self.type_scope_item(name).is_none() => {
                if let Some(args) = &r.type_params {
                    if let Some(first) = args.params.first() {
                        let y = self.resolve_result_type(first);
                        for argument in args.params.iter().skip(1) {
                            self.resolve_type(argument);
                        }
                        return Type::generator(y);
                    }
                }
                self.reject_subset(
                    RejectionSite::GeneratorYieldTypeMissing,
                    "`Generator` requires at least a yield type argument",
                    pos,
                );
                return Type::Error;
            }
            _ => {}
        }

        // compiler.md §115.1: the three ambient Error classes are one
        // class; a program declaration shadows the ambient name.
        if crate::check::exception::ErrorKind::from_name(name).is_some()
            && self.type_scope_item(name).is_none()
        {
            if r.type_params.is_some() {
                self.reject_subset(
                    RejectionSite::ErrorTypeArguments,
                    format!("`{name}` is not generic"),
                    pos,
                );
                return Type::Error;
            }
            return Type::Class(self.error_class);
        }

        // The ES2022 lib supplies the editor declarations; the language
        // checker resolves its accepted, monomorphized subset directly.
        // A program declaration shadows the ambient name, as for Date.
        if (name == "Map" || name == "Set") && self.type_scope_item(name).is_none() {
            let Some(args) = &r.type_params else {
                self.reject_subset(
                    RejectionSite::MapTypeArgumentsMissing,
                    format!("generic reference class `{name}` requires explicit type arguments"),
                    pos,
                );
                return Type::Error;
            };
            let expected = if name == "Map" { 2 } else { 1 };
            if args.params.len() != expected {
                self.reject_subset(
                    RejectionSite::MapTypeArgumentCount,
                    format!("`{name}` takes exactly {expected} type argument(s)"),
                    pos,
                );
                return Type::Error;
            }
            let saved = self.in_assoc_key;
            self.in_assoc_key = true;
            let key = self.resolve_type(&args.params[0]);
            let key_slot = if name == "Map" {
                ContainerSlot::MapKey
            } else {
                ContainerSlot::SetElement
            };
            let key_pos = self.pos(args.params[0].span());
            let key = self.container_argument(key_slot, key, key_pos);
            // Only this container's key position may temporarily admit
            // boundary-only shapes so the Q24 whitelist can issue S014.
            // A nested container's value is a general declaration even
            // when the container itself appears as an outer key.
            self.in_assoc_key = false;
            if !self.instance_restriction(
                crate::check::opaque::InstanceRestriction::AssociativeKey,
                &key,
            ) && !matches!(self.apparent_type(&key), Type::Error)
                && self.assoc_key_kind(&key).is_none()
            {
                let key_pos = self.pos(args.params[0].span());
                let key_name = self.type_name(&key);
                self.reject_subset(
                    RejectionSite::MapSetTypeKey,
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
                let value_pos = self.pos(args.params[1].span());
                let value = self.container_argument(ContainerSlot::MapValue, value, value_pos);
                self.in_assoc_key = saved;
                return Type::map(key, value);
            }
            self.in_assoc_key = saved;
            return Type::set(key);
        }

        // The ambient `Date` value type (stdlib.md §3): applies only
        // when no program declaration shadows the name — a user class
        // named `Date` wins, exactly as for `Math`.
        if name == "Date" && self.type_scope_item(name).is_none() {
            if r.type_params.is_some() {
                self.reject_subset(
                    RejectionSite::DateTypeArguments,
                    "`Date` is not generic",
                    pos,
                );
                return Type::Error;
            }
            return Type::Date;
        }

        match self.type_scope_item(name) {
            Some(ScopeItem::Poisoned) => {
                if let Some(arguments) = &r.type_params {
                    for argument in &arguments.params {
                        let _ = self.resolve_type(argument);
                    }
                }
                Type::Error
            }
            Some(ScopeItem::Class(id)) => {
                if r.type_params.is_some() {
                    self.reject_subset(
                        RejectionSite::NonGenericClassTypeArguments,
                        format!("`{}` is not generic", name),
                        pos,
                    );
                }
                Type::Class(id)
            }
            Some(ScopeItem::GenericClass(key)) => {
                let Some(args) = &r.type_params else {
                    self.reject_subset(
                        RejectionSite::GenericClassTypeArgumentsMissing,
                        format!("generic class `{}` requires explicit type arguments", name),
                        pos,
                    );
                    return Type::Error;
                };
                let arguments = self.resolve_instance_arguments(args);
                match self.instantiate_class(&key, &arguments, pos) {
                    Some(id) => Type::Class(id),
                    None => Type::Error,
                }
            }
            Some(ScopeItem::Enum(id)) => Type::Enum(id),
            Some(ScopeItem::StringAlias(id)) => {
                if self.in_boundary {
                    let wire_mapped = self
                        .string_aliases
                        .get(id.0)
                        .is_some_and(|alias| alias.wire_values.is_some());
                    if wire_mapped && self.allow_wire_alias_boundary {
                        Type::StringAlias(id)
                    } else {
                        self.reject_subset(RejectionSite::BoundaryLiteralAlias, format!(
                                "string-literal union alias `{name}` cannot appear in a boundary signature"
                            ), pos);
                        Type::Error
                    }
                } else if r.type_params.is_some() {
                    self.reject_subset(
                        RejectionSite::LiteralAliasTypeArguments,
                        format!("string-literal union alias `{name}` is not generic"),
                        pos,
                    );
                    Type::Error
                } else {
                    Type::StringAlias(id)
                }
            }
            _ => {
                self.reject_subset(
                    if crate::ambient::lib_type_name(name) {
                        RejectionSite::TypeNameUnknown
                    } else {
                        RejectionSite::UnboundTypeName
                    },
                    format!("unknown type name `{}`", name),
                    pos,
                );
                Type::Error
            }
        }
    }

    /// The shared acceptance rule for a written or inferred `T | null`.
    pub(super) fn allows_nullable(&self, inner: &Type) -> bool {
        self.instance_restriction(super::opaque::InstanceRestriction::NullableShape, inner)
            || self
                .apparent_type(inner)
                .is_reference_shape(&self.type_handle_classes)
    }

    fn resolve_union(&mut self, u: &ast::TsUnionOrIntersectionType) -> Type {
        let union = match u {
            ast::TsUnionOrIntersectionType::TsUnionType(union) => union,
            ast::TsUnionOrIntersectionType::TsIntersectionType(i) => {
                let pos = self.pos(i.span);
                self.reject_subset(
                    RejectionSite::IntersectionTypeAnnotation,
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
                    self.reject_subset(
                        RejectionSite::UndefinedUnionMember,
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
                if matches!(self.apparent_type(&inner), Type::Error) {
                    return Type::Error;
                }
                // C7 and §33.5: nullable reference shapes and nullable
                // boundary value classes are reference-sized handles. Plain
                // script value classes remain outside the union surface.
                if self.in_assoc_key {
                    return Type::nullable(inner);
                }
                if self.allows_nullable(&inner) {
                    return Type::nullable(inner);
                }
                let pos = self.pos(base.span());
                let name = self.type_name(&inner);
                self.reject_subset(
                    RejectionSite::NullableNonReference,
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
        let site = if union.types.iter().all(|member| {
            matches!(
                &**member,
                ast::TsType::TsLitType(ast::TsLitType {
                    lit: ast::TsLit::Str(_),
                    ..
                })
            )
        }) {
            RejectionSite::UnionLiteralUnionAlias
        } else if union.types.iter().any(|member| matches!(&**member, ast::TsType::TsTypeRef(reference)
            if matches!(&reference.type_name, ast::TsEntityName::Ident(ident)
                if !self.subst.contains_key(ident.sym.as_ref())
                    && self.type_scope_item(ident.sym.as_ref()).is_none()
                    && crate::ambient::sized_alias(ident.sym.as_ref()).is_none()
                    && !crate::ambient::lib_type_name(ident.sym.as_ref())
                    && !matches!(ident.sym.as_ref(), "FixedArray" | "Context" | "Worker" | "Inbox" | "Outbox" | "CEnum")))) {
            RejectionSite::UnionUnknownTypeMember
        } else {
            RejectionSite::UnionGeneralUnionAndUndefined
        };
        self.reject_subset(site, "unions are limited to `Ref | null`", pos);
        Type::Error
    }

    fn resolve_fn_type(&mut self, f: &ast::TsFnOrConstructorType) -> Type {
        let fn_ty = match f {
            ast::TsFnOrConstructorType::TsFnType(fn_ty) => fn_ty,
            ast::TsFnOrConstructorType::TsConstructorType(c) => {
                let pos = self.pos(c.span);
                self.reject_subset(
                    RejectionSite::ConstructorTypeAnnotation,
                    "constructor types are not in the decided surface",
                    pos,
                );
                return Type::Error;
            }
        };
        let mut params = Vec::new();
        for p in &fn_ty.params {
            let optional = match p {
                ast::TsFnParam::Ident(binding) => binding.id.optional,
                ast::TsFnParam::Array(pattern) => pattern.optional,
                ast::TsFnParam::Object(pattern) => pattern.optional,
                ast::TsFnParam::Rest(_) => false,
            };
            if optional {
                self.reject_subset(
                    RejectionSite::OptionalFunctionTypeParameter,
                    "optional parameters in function types are not supported (C7)",
                    self.pos(p.span()),
                );
                return Type::Error;
            }
            match p {
                ast::TsFnParam::Ident(binding) => match &binding.type_ann {
                    Some(ann) => params.push(self.resolve_type(&ann.type_ann)),
                    None => {
                        let pos = self.pos(binding.id.span);
                        self.reject_subset(
                            RejectionSite::FunctionTypeParameterAnnotationMissing,
                            "function type parameters require annotations",
                            pos,
                        );
                        params.push(Type::Error);
                    }
                },
                other => {
                    let pos = self.pos(other.span());
                    self.reject_subset(
                        match other {
                            ast::TsFnParam::Rest(_) => RejectionSite::FunctionTypeRestParameter,
                            ast::TsFnParam::Array(_) => RejectionSite::FunctionTypeArrayPattern,
                            _ => RejectionSite::FunctionTypeObjectPattern,
                        },
                        "function type parameter form outside the decided surface",
                        pos,
                    );
                    params.push(Type::Error);
                }
            }
        }
        let ret = self.resolve_result_type(&fn_ty.type_ann.type_ann);
        Type::func(params, ret)
    }
}
