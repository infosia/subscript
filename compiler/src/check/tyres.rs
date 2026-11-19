//! Resolution of TypeScript type annotations to language [`Type`]s,
//! including the banned-type rules (S001 `any`, S007 bare `number`,
//! S011 general unions, S012 `undefined`, S013 `Promise`).

use crate::check::rejection::RejectionSite;
use oxc_ast::ast;
use oxc_span::GetSpan;

use crate::types::Type;

use super::{Checker, ContainerSlot, ScopeItem};

impl<'p> Checker<'p> {
    /// Resolves the mandatory source-level `Promise<T>` view of an async
    /// function and returns the fulfilled `T` carried in HIR. Promise is
    /// deliberately not a general [`Type`]: no Promise value exists in the
    /// language (Q34).
    pub(crate) fn resolve_async_return(&mut self, ty: &ast::TSType) -> Type {
        let ast::TSType::TSTypeReference(reference) = ty else {
            let pos = self.pos(ty.span());
            let parenthesized_promise = match ty {
                ast::TSType::TSParenthesizedType(p) => {
                    let resolved = self.resolve_type(&p.type_annotation);
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
        let ast::TSTypeName::IdentifierReference(ident) = &reference.type_name else {
            let pos = self.pos(reference.span);
            self.reject_subset(
                RejectionSite::AsyncReturnQualifiedName,
                "async functions must return an explicitly annotated builtin `Promise<T>`",
                pos,
            );
            return Type::Error;
        };
        if matches!(
            self.type_scope_item(ident.name.as_ref()),
            Some(ScopeItem::Poisoned)
        ) {
            return Type::Error;
        }
        if ident.name.as_ref() != "Promise" || self.type_scope_item("Promise").is_some() {
            let pos = self.pos(ident.span);
            self.reject_subset(
                if self
                    .type_scope_item(ident.name.as_ref())
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
        let Some(args) = &reference.type_arguments else {
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
    pub(crate) fn resolve_type(&mut self, ty: &ast::TSType) -> Type {
        self.resolve_type_position(ty, false)
    }

    /// Resolves a return, generator element, or Promise result type.
    pub(crate) fn resolve_result_type(&mut self, ty: &ast::TSType) -> Type {
        self.resolve_type_position(ty, true)
    }

    fn resolve_type_position(&mut self, ty: &ast::TSType, allow_void: bool) -> Type {
        // §230 rule 3: a resolved type argument entry is read.
        if !self.type_argument_watch.is_empty() {
            self.type_argument_watch.remove(&ty.span());
        }
        let undefined_result = std::mem::take(&mut self.undefined_result);
        let resolved = match ty {
            keyword if keyword.is_keyword() && !matches!(keyword, ast::TSType::TSThisType(_)) => {
                self.resolve_keyword(keyword)
            }
            ast::TSType::TSTypeReference(r) => self.resolve_type_ref(r),
            ast::TSType::TSArrayType(arr) => {
                let elem = self.resolve_type(&arr.element_type);
                let elem_pos = self.pos(arr.element_type.span());
                let elem = self.container_argument(ContainerSlot::ArrayElement, elem, elem_pos);
                Type::array(elem)
            }
            u @ (ast::TSType::TSUnionType(_) | ast::TSType::TSIntersectionType(_)) => {
                self.resolve_union(u, undefined_result)
            }
            f @ (ast::TSType::TSFunctionType(_) | ast::TSType::TSConstructorType(_)) => {
                self.resolve_fn_type(f)
            }
            ast::TSType::TSParenthesizedType(p) => {
                self.undefined_result = undefined_result;
                self.resolve_type_position(&p.type_annotation, allow_void)
            }
            other => {
                let pos = self.pos(other.span());
                self.reject_subset(
                    match other {
                        ast::TSType::TSTupleType(_) => RejectionSite::TupleAnnotation,
                        ast::TSType::TSThisType(_) => RejectionSite::ThisAnnotation,
                        ast::TSType::JSDocNullableType(nullable) if nullable.postfix => RejectionSite::OptionalAnnotation,
                        ast::TSType::TSTemplateLiteralType(_) => RejectionSite::TemplateLiteralAnnotation,
                        ast::TSType::TSTypeQuery(_) => RejectionSite::QueryAnnotation,
                        ast::TSType::TSTypeLiteral(_) => RejectionSite::StructuralAnnotation,


                        ast::TSType::TSConditionalType(_) => RejectionSite::ConditionalAnnotation,
                        ast::TSType::TSInferType(_) => RejectionSite::InferAnnotation,
                        ast::TSType::TSTypeOperatorType(op) if op.operator == ast::TSTypeOperatorOperator::Keyof => RejectionSite::KeyofAnnotation,
                        ast::TSType::TSTypeOperatorType(_) => RejectionSite::OperatorAnnotation,
                        ast::TSType::TSIndexedAccessType(_) => RejectionSite::IndexedAnnotation,
                        ast::TSType::TSMappedType(_) => RejectionSite::MappedAnnotation,
                        ast::TSType::TSTypePredicate(_) => RejectionSite::PredicateAnnotation,
                        ast::TSType::TSImportType(_) => RejectionSite::ImportAnnotation,
                        ast::TSType::TSLiteralType(lit) => match &lit.literal {
                            ast::TSLiteral::StringLiteral(_) => RejectionSite::StringLiteralAnnotation,
                            ast::TSLiteral::NumericLiteral(_) => RejectionSite::NumberLiteralAnnotation,
                            ast::TSLiteral::BooleanLiteral(_) => RejectionSite::BooleanLiteralAnnotation,
                            ast::TSLiteral::BigIntLiteral(_) => RejectionSite::BigIntLiteralAnnotation,
                            ast::TSLiteral::TemplateLiteral(_) => RejectionSite::TemplateLiteralAnnotation,
                            ast::TSLiteral::UnaryExpression(unary) => if matches!(unary.argument, ast::Expression::BigIntLiteral(_)) { RejectionSite::BigIntLiteralAnnotation } else { RejectionSite::NumberLiteralAnnotation },
                        },
                        _ => RejectionSite::UnsupportedAnnotationKind,
                    },
                    if matches!(other, ast::TSType::TSTypeOperatorType(op) if op.operator == ast::TSTypeOperatorOperator::Keyof) { "`keyof` annotations are not supported; name each accepted key explicitly" } else { "type annotation form outside the decided surface" },
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

    fn resolve_keyword(&mut self, kw: &ast::TSType) -> Type {
        let pos = self.pos(kw.span());
        match kw {
            ast::TSType::TSNumberKeyword(_) => {
                self.reject_subset(
                    RejectionSite::AnyTypeAnnotation,
                    "bare `number` is rejected; there is no default numeric type — \
                     use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, \
                     f16, f32, f64)",
                    pos,
                );
                Type::Error
            }
            ast::TSType::TSAnyKeyword(_) => {
                self.reject_subset(
                    RejectionSite::UnknownTypeAnnotation,
                    "`any` is not part of the language",
                    pos,
                );
                Type::Error
            }
            ast::TSType::TSUndefinedKeyword(_) => {
                self.reject_subset(
                    RejectionSite::UndefinedKeywordAnnotation,
                    "`undefined` is legal only in `T | undefined`, the result type of a function declaration or a class method",
                    pos,
                );
                Type::Error
            }
            ast::TSType::TSBooleanKeyword(_) => Type::Bool,
            ast::TSType::TSStringKeyword(_) => Type::Str,
            ast::TSType::TSVoidKeyword(_) => Type::Void,
            ast::TSType::TSNullKeyword(_) => Type::Null,
            ast::TSType::TSObjectKeyword(_) => {
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
                    match kw {
                        ast::TSType::TSNeverKeyword(_) => RejectionSite::NeverAnnotation,
                        ast::TSType::TSUnknownKeyword(_) => RejectionSite::UnknownAnnotation,
                        ast::TSType::TSSymbolKeyword(_) => RejectionSite::SymbolAnnotation,
                        ast::TSType::TSBigIntKeyword(_) => RejectionSite::BigIntAnnotation,
                        _ => RejectionSite::UnsupportedKeywordKind,
                    },
                    "keyword type outside the decided surface",
                    pos,
                );
                Type::Error
            }
        }
    }

    fn resolve_type_ref(&mut self, r: &ast::TSTypeReference) -> Type {
        let ast::TSTypeName::IdentifierReference(ident) = &r.type_name else {
            let pos = self.pos(r.span);
            self.reject_subset(
                RejectionSite::QualifiedSourceTypeName,
                "qualified type names are not decided",
                pos,
            );
            return Type::Error;
        };
        let name = ident.name.as_ref();
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
                // §202.1 rule 3a: an alias has no type parameters.
                if r.type_arguments.is_some() {
                    self.reject_subset(
                        RejectionSite::AliasTypeArguments,
                        format!("type alias `{name}` is not generic"),
                        pos,
                    );
                    return Type::Error;
                }
                return alias;
            }
        }
        match name {
            "Worker" | "Inbox" | "Outbox" if self.type_scope_item(name).is_none() => {
                let expected = if name == "Worker" { 2 } else { 1 };
                let Some(args) = &r.type_arguments else {
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
            "TaskGroup" if self.type_scope_item(name).is_none() => {
                if !self.task_group_type || r.type_arguments.is_some() || self.in_boundary {
                    self.reject_subset(
                        RejectionSite::TaskGroupPosition,
                        "TaskGroup requires one const local or a synchronous borrowed parameter",
                        pos,
                    );
                    return Type::Error;
                }
                return Type::TaskGroup;
            }
            "RegExp" if self.type_scope_item(name).is_none() => {
                if r.type_arguments.is_some() {
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
                let Some(args) = &r.type_arguments else {
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
                let value = match &args.params[0] {
                    ast::TSType::TSArrayType(array)
                        if matches!(&array.element_type, ast::TSType::TSVoidKeyword(_)) =>
                    {
                        Type::array(Type::Void)
                    }
                    _ => self.resolve_result_type(&args.params[0]),
                };
                return Type::async_handle(value);
            }
            "FixedArray" if self.type_scope_item(name).is_none() => {
                let Some(args) = &r.type_arguments else {
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
                let len = match &args.params[1] {
                    ast::TSType::TSLiteralType(literal) if matches!(&literal.literal, ast::TSLiteral::NumericLiteral(n) if n.value >= 0.0 && n.value.fract() == 0.0) =>
                    {
                        let ast::TSLiteral::NumericLiteral(n) = &literal.literal else {
                            return Type::Error;
                        };
                        // compiler.md §254 rule 1: C has no array with the
                        // length zero.
                        if n.value == 0.0 {
                            let p = self.pos(args.params[1].span());
                            self.reject_subset(
                                RejectionSite::FixedArrayLengthZero,
                                "a `FixedArray` length is at least 1; C has no array with the length zero",
                                p,
                            );
                            return Type::Error;
                        }
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
                if let Some(args) = &r.type_arguments {
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
                if self.in_boundary {
                    self.reject_subset(
                        RejectionSite::MirrorGeneratorType,
                        "a mirror names `Generator`; the C ABI has no generator and cannot register a generator frame",
                        pos,
                    );
                    return Type::Error;
                }
                if let Some(args) = &r.type_arguments {
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
            if r.type_arguments.is_some() {
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
            let Some(args) = &r.type_arguments else {
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
            if r.type_arguments.is_some() {
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
                if let Some(arguments) = &r.type_arguments {
                    for argument in &arguments.params {
                        let _ = self.resolve_type(argument);
                    }
                }
                Type::Error
            }
            Some(ScopeItem::Class(id)) => {
                if r.type_arguments.is_some() {
                    self.reject_subset(
                        RejectionSite::NonGenericClassTypeArguments,
                        format!("`{}` is not generic", name),
                        pos,
                    );
                }
                Type::Class(id)
            }
            Some(ScopeItem::GenericClass(key)) => {
                let Some(args) = &r.type_arguments else {
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
            Some(ScopeItem::Interface(_)) => self.reject_interface_value_type(name, pos),
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
                } else if r.type_arguments.is_some() {
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

    /// Resolves a union with an `undefined` member (compiler.md §195.1).
    fn resolve_undefined_union(
        &mut self,
        union: &ast::TSUnionType,
        span: oxc_span::Span,
        undefined_result: bool,
    ) -> Type {
        let pos = self.pos(span);
        if !undefined_result {
            self.reject_subset(
                RejectionSite::UndefinedUnionMember,
                "`T | undefined` is legal only as the result type of a function declaration or a class method",
                pos,
            );
            return Type::Error;
        }
        let has_null = union
            .types
            .iter()
            .any(|member| matches!(member, ast::TSType::TSNullKeyword(_)));
        if has_null {
            self.reject_subset(
                RejectionSite::UndefinedNullUnion,
                "`T | null | undefined` has no representation; a nullable type has one absence spelling, `null` or `undefined`",
                self.pos(union.span),
            );
            return Type::Error;
        }
        let others: Vec<_> = union
            .types
            .iter()
            .filter(|member| !matches!(*member, ast::TSType::TSUndefinedKeyword(_)))
            .collect();
        let [base] = others.as_slice() else {
            self.reject_subset(
                RejectionSite::UnionGeneralUnionAndUndefined,
                "unions are limited to `Ref | null`",
                self.pos(union.span),
            );
            return Type::Error;
        };
        let inner = self.resolve_type(base);
        if matches!(self.apparent_type(&inner), Type::Error) {
            return Type::Error;
        }
        if self.allows_nullable(&inner) {
            return Type::nullable_spelled(inner, crate::types::Absence::Undefined);
        }
        let name = self.type_name(&inner);
        self.reject_subset(
            RejectionSite::NullableNonReference,
            format!(
                "unions are limited to `Ref | null` and a `Ref | undefined` result; `{name} | undefined` is not a reference type union"
            ),
            self.pos(base.span()),
        );
        Type::Error
    }

    /// The shared acceptance rule for a written or inferred `T | null`.
    pub(super) fn allows_nullable(&self, inner: &Type) -> bool {
        self.instance_restriction(super::opaque::InstanceRestriction::NullableShape, inner)
            || self
                .apparent_type(inner)
                .is_reference_shape(&self.type_handle_classes)
    }

    fn resolve_union(&mut self, u: &ast::TSType, undefined_result: bool) -> Type {
        let union = match u {
            ast::TSType::TSUnionType(union) => union,
            ast::TSType::TSIntersectionType(i) => {
                let pos = self.pos(i.span);
                self.reject_subset(
                    RejectionSite::IntersectionTypeAnnotation,
                    "intersection types are not in the decided surface",
                    pos,
                );
                return Type::Error;
            }
            _ => return Type::Error,
        };
        let is_null = |t: &ast::TSType| matches!(t, ast::TSType::TSNullKeyword(_));
        let undefined_member = union.types.iter().find_map(|member| match member {
            ast::TSType::TSUndefinedKeyword(kw) => Some(kw.span),
            _ => None,
        });
        if let Some(span) = undefined_member {
            return self.resolve_undefined_union(union, span, undefined_result);
        }
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
                member,
                ast::TSType::TSLiteralType(literal) if matches!(literal.literal, ast::TSLiteral::StringLiteral(_))
            )
        }) {
            RejectionSite::UnionLiteralUnionAlias
        } else if union.types.iter().any(|member| matches!(member, ast::TSType::TSTypeReference(reference)
            if matches!(&reference.type_name, ast::TSTypeName::IdentifierReference(ident)
                if !self.subst.contains_key(ident.name.as_ref())
                    && self.type_scope_item(ident.name.as_ref()).is_none()
                    && crate::ambient::sized_alias(ident.name.as_ref()).is_none()
                    && !crate::ambient::lib_type_name(ident.name.as_ref())
                    && !matches!(ident.name.as_ref(), "FixedArray" | "Context" | "Worker" | "Inbox" | "Outbox" | "CEnum")))) {
            RejectionSite::UnionUnknownTypeMember
        } else {
            RejectionSite::UnionGeneralUnionAndUndefined
        };
        self.reject_subset(site, "unions are limited to `Ref | null`", pos);
        Type::Error
    }

    fn resolve_fn_type(&mut self, f: &ast::TSType) -> Type {
        let fn_ty = match f {
            ast::TSType::TSFunctionType(fn_ty) => fn_ty,
            ast::TSType::TSConstructorType(c) => {
                let pos = self.pos(c.span);
                self.reject_subset(
                    RejectionSite::ConstructorTypeAnnotation,
                    "constructor types are not in the decided surface",
                    pos,
                );
                return Type::Error;
            }
            _ => return Type::Error,
        };
        let mut params = Vec::new();
        for parameter in super::formal_parameters(
            fn_ty.this_param.as_deref(),
            &fn_ty.params,
            self.prog.allocator,
        ) {
            if parameter.optional {
                self.reject_subset(
                    RejectionSite::OptionalFunctionTypeParameter,
                    "optional parameters in function types are not supported (C7)",
                    self.pos(parameter.span),
                );
                return Type::Error;
            }
            match &parameter.pattern {
                ast::BindingPattern::BindingIdentifier(binding) => {
                    if let Some(annotation) = &parameter.type_annotation {
                        params.push(self.resolve_type(&annotation.type_annotation));
                    } else {
                        self.reject_subset(
                            RejectionSite::FunctionTypeParameterAnnotationMissing,
                            "function type parameters require annotations",
                            self.pos(binding.span),
                        );
                        params.push(Type::Error);
                    }
                }
                other => {
                    self.reject_subset(
                        if matches!(other, ast::BindingPattern::ArrayPattern(_)) {
                            RejectionSite::FunctionTypeArrayPattern
                        } else {
                            RejectionSite::FunctionTypeObjectPattern
                        },
                        "function type parameter form outside the decided surface",
                        self.pos(parameter.span),
                    );
                    params.push(Type::Error);
                }
            }
        }
        if let Some(rest) = &fn_ty.params.rest {
            self.reject_subset(
                RejectionSite::FunctionTypeRestParameter,
                "function type parameter form outside the decided surface",
                self.pos(rest.span),
            );
            params.push(Type::Error);
        }
        let ret = self.resolve_result_type(&fn_ty.return_type.type_annotation);
        Type::func(params, ret)
    }
}
