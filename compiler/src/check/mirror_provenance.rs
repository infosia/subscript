use super::*;
use crate::check::rejection::RejectionSite;

impl<'p> Checker<'p> {
    /// Validates record targets in one ambient mirror and assigns its HIR
    /// header identity for foreign functions and boundary classes.
    pub(super) fn collect_mirror_provenance(&mut self, file: usize) {
        let parsed = &self.prog.files[file];
        let mut functions = HashMap::new();
        let mut aliases = HashSet::new();
        let mut classes = HashMap::new();
        for item in &parsed.module.body {
            let Some(decl) = module_decl(item) else {
                continue;
            };
            match decl {
                ast::Decl::Fn(function) => {
                    functions.insert(function.ident.sym.to_string(), &function.function);
                }
                ast::Decl::TsTypeAlias(alias) => {
                    aliases.insert(alias.id.sym.to_string());
                }
                ast::Decl::Class(class) => {
                    classes.insert(class.ident.sym.to_string(), &class.class);
                }
                _ => {}
            }
        }

        if !functions.is_empty() || (!classes.is_empty() && parsed.provenance.header.is_some()) {
            let include = match &parsed.provenance.header {
                Some(record) => record.value.clone(),
                None => {
                    self.reject_subset(
                        RejectionSite::MirrorHeaderMissing,
                        format!(
                            "mirror `{}` declares foreign functions but has no \
                             `@subscript-c-header` provenance record",
                            parsed.name
                        ),
                        Pos::new(parsed.name.clone(), 1, 1),
                    );
                    String::new()
                }
            };
            let id = hir::ForeignMirrorId(self.foreign_mirrors.len());
            self.foreign_mirrors.push(hir::ForeignMirror {
                source_name: parsed.name.clone(),
                include,
            });
            self.foreign_mirror_ids.insert(file, id);
        }

        for (name, record) in &parsed.provenance.completions {
            if !functions.contains_key(name) {
                self.reject_subset(
                    RejectionSite::ForeignReturnProvenance,
                    format!("completion directive names nonexistent foreign function `{name}`"),
                    Pos::new(parsed.name.clone(), record.line, 1),
                );
            }
        }

        for ((function_name, parameter_name), record) in &parsed.provenance.parameters {
            let exists = functions.get(function_name).is_some_and(|function| {
                function.params.iter().any(|parameter| {
                    parameter_name_from_pat(&parameter.pat)
                        .is_some_and(|name| name == parameter_name)
                })
            });
            if !exists {
                self.reject_subset(
                    RejectionSite::MirrorParameterTargetMissing,
                    format!(
                        "mirror `{}` has provenance record naming nonexistent \
                         parameter `{}.{}`: `{}`",
                        parsed.name, function_name, parameter_name, record.raw
                    ),
                    Pos::new(parsed.name.clone(), record.line, 1),
                );
            }
        }

        for (typedef_name, record) in &parsed.provenance.callbacks {
            if !aliases.contains(typedef_name) {
                self.reject_subset(
                    RejectionSite::MirrorCallbackTargetMissing,
                    format!(
                        "mirror `{}` has provenance record naming nonexistent \
                         callback typedef `{}`: `{}`",
                        parsed.name, typedef_name, record.raw
                    ),
                    Pos::new(parsed.name.clone(), record.line, 1),
                );
            }
        }

        // §111 rule 1: the selected aggregate is a boundary class of this
        // mirror, and it carries one callback field. The record and the
        // class declaration are two facts derived apart.
        let mut lifetime_errors = Vec::new();
        for (aggregate, record) in &parsed.provenance.callback_lifetimes {
            let carries_callback = classes.get(aggregate).is_some_and(|class| {
                class.body.iter().any(|member| {
                    let ast::ClassMember::ClassProp(prop) = member else {
                        return false;
                    };
                    type_reference_name(
                        prop.type_ann
                            .as_deref()
                            .map(|annotation| annotation.type_ann.as_ref()),
                    )
                    .is_some_and(|name| parsed.provenance.callbacks.contains_key(name))
                })
            });
            if !carries_callback {
                lifetime_errors.push((
                    format!(
                        "mirror `{}` has provenance record naming `{}`, which is no boundary \
                         class with a callback field: `{}`",
                        parsed.name, aggregate, record.raw
                    ),
                    Pos::new(parsed.name.clone(), record.line, 1),
                ));
            }
        }
        // The record map has no order, so the diagnostics take the mirror's
        // own line order.
        lifetime_errors.sort_by_key(|(_, pos)| pos.line);
        for (message, pos) in lifetime_errors {
            self.reject_subset(RejectionSite::MirrorLifetimeTargetMissing, message, pos);
        }
    }

    /// Checks the selected C result against the resolved mirror signature.
    pub(super) fn check_completion_result(
        &mut self,
        file: usize,
        name: &str,
        ret: &Type,
        pos: Pos,
    ) -> Option<String> {
        let ret = self.apparent_type(ret);
        let record = self.prog.files[file]
            .provenance
            .completions
            .get(name)
            .cloned();
        let Some(record) = record else {
            if matches!(ret, Type::AsyncHandle(_)) {
                self.reject_subset(RejectionSite::ForeignReturnProvenance,
                    format!("foreign function `{name}` returns Promise<T> without a `@subscript-c-completion` directive"), pos);
            }
            return None;
        };
        let scalar = match subscript_boundary::c_kind(&record.value)
            .map(|kind| kind.language)
            .or_else(|| (record.value == "void").then_some("void"))
        {
            Some("void") => Some(Type::Void),
            _ if record.value == "string" => Some(Type::Str),
            _ if record.value == "u8[]" => Some(Type::Array(Box::new(Type::U8))),
            Some("i8") => Some(Type::I8),
            Some("u8") => Some(Type::U8),
            Some("i16") => Some(Type::I16),
            Some("u16") => Some(Type::U16),
            Some("i32") => Some(Type::I32),
            Some("u32") => Some(Type::U32),
            Some("i64") => Some(Type::I64),
            Some("u64") => Some(Type::U64),
            Some("f16") => Some(Type::F16),
            Some("f32") => Some(Type::F32),
            Some("f64") => Some(Type::F64),
            Some("boolean") => Some(Type::Bool),
            _ => self
                .type_aliases
                .get(&record.value)
                .cloned()
                .or_else(|| {
                    let alias = self.prog.files[file]
                        .provenance
                        .cenums
                        .get(&record.value)
                        .map_or(record.value.as_str(), |r| r.value.as_str());
                    match self.peek_scope_item(alias) {
                        Some(ScopeItem::StringAlias(id)) => Some(Type::StringAlias(id)),
                        Some(ScopeItem::Enum(id)) => Some(Type::Enum(id)),
                        _ => None,
                    }
                })
                .or_else(|| {
                    self.class_ids
                        .get(&self.declaration_symbol(file, &record.value))
                        .copied()
                        .map(Type::Class)
                }),
        };
        if let Some(ty) = scalar.as_ref() {
            if let Type::Class(id) = self.apparent_type(ty) {
                if let Some((class, field)) =
                    self.completion_struct_field_error(id, &mut HashSet::new())
                {
                    self.reject_subset(RejectionSite::ForeignReturnProvenance,
                    format!("completion function `{name}` struct `{class}` field `{field}` is outside §178 rule 6"), pos);
                    return None;
                }
            }
        }
        let supported = |ty: &Type| match self.apparent_type(ty) {
            Type::Str => record.value == "string",
            Type::Void
            | Type::I8
            | Type::U8
            | Type::I16
            | Type::U16
            | Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::F16
            | Type::F32
            | Type::F64
            | Type::Bool
            | Type::Enum(_) => true,
            Type::Array(element) => {
                record.value == "u8[]" && self.apparent_type(&element) == Type::U8
            }
            Type::StringAlias(_) => false,
            Type::Class(id) => self
                .completion_struct_field_error(id, &mut HashSet::new())
                .is_none(),
            _ => false,
        };
        let valid = record.value != "subscript_rt_completion"
            && matches!(ret, Type::AsyncHandle(value) if scalar.as_ref().is_some_and(|ty| ty == value.as_ref() && supported(ty)));
        if !valid {
            self.reject_subset(RejectionSite::ForeignReturnProvenance,
                format!("completion function `{name}` must return Promise<T> matching supported C result `{}`", record.value), pos);
            return None;
        }
        Some(record.value)
    }

    /// Derives completion layout eligibility from resolved class fields.
    fn completion_struct_field_error(
        &self,
        id: crate::ClassId,
        visiting: &mut HashSet<crate::ClassId>,
    ) -> Option<(String, String)> {
        let class = &self.classes[id.0];
        if !self.boundary_classes.contains(&id) || !class.is_value || !visiting.insert(id) {
            return Some((class.name.clone(), "<layout>".into()));
        }
        for field in &class.fields {
            let supported = match self.apparent_type(&field.ty) {
                Type::Bool
                | Type::I8
                | Type::U8
                | Type::I16
                | Type::U16
                | Type::I32
                | Type::U32
                | Type::I64
                | Type::U64
                | Type::F16
                | Type::F32
                | Type::F64
                | Type::Enum(_) => true,
                Type::Class(nested) => {
                    if let Some(error) = self.completion_struct_field_error(nested, visiting) {
                        return Some(error);
                    }
                    true
                }
                _ => false,
            };
            if !supported {
                return Some((class.name.clone(), field.name.clone()));
            }
        }
        visiting.remove(&id);
        None
    }

    /// Converts parameter provenance into the consumer-ready HIR shape and
    /// rejects missing or type-incompatible records.
    pub(super) fn foreign_parameter_provenance(
        &mut self,
        file: usize,
        function_name: &str,
        parameter_name: &str,
        ty: &Type,
        pos: Pos,
    ) -> Option<hir::ForeignTypeProvenance> {
        let parsed = &self.prog.files[file];
        let key = (function_name.to_string(), parameter_name.to_string());
        let record = parsed.provenance.parameters.get(&key);
        match (ty, record.map(|record| &record.value)) {
            (
                Type::Array(_),
                Some(provenance::Parameter::Descriptor {
                    aggregate,
                    element,
                    element_const,
                }),
            ) => Some(hir::ForeignTypeProvenance::Descriptor {
                aggregate: aggregate.clone(),
                element: element.clone(),
                element_const: *element_const,
            }),
            (
                Type::Array(_),
                Some(provenance::Parameter::ScalarPair {
                    element,
                    element_const,
                }),
            ) => Some(hir::ForeignTypeProvenance::ScalarPair {
                element: element.clone(),
                element_const: *element_const,
            }),
            (Type::Str, Some(provenance::Parameter::StringView { aggregate })) => {
                Some(hir::ForeignTypeProvenance::StringView {
                    aggregate: aggregate.clone(),
                })
            }
            (Type::Array(_), None) => {
                self.reject_subset(
                    RejectionSite::MirrorArrayProvenanceMissing,
                    format!(
                        "mirror `{}` parameter `{}.{}` absorbs an array descriptor \
                         or scalar parameter pair but has no \
                         `@subscript-c-descriptor` or `@subscript-c-scalar-pair` \
                         provenance record",
                        parsed.name, function_name, parameter_name
                    ),
                    pos,
                );
                None
            }
            (Type::Str, None) => {
                self.reject_subset(
                    RejectionSite::MirrorStringProvenanceMissing,
                    format!(
                        "mirror `{}` parameter `{}.{}` absorbs a string view but \
                         has no `@subscript-c-string-view` provenance record",
                        parsed.name, function_name, parameter_name
                    ),
                    pos,
                );
                None
            }
            (Type::Array(_), Some(_)) | (Type::Str, Some(_)) | (_, Some(_)) => {
                let raw = record.map(|record| record.raw.as_str()).unwrap_or_default();
                self.reject_subset(
                    RejectionSite::MirrorParameterProvenanceMismatch,
                    format!(
                        "mirror `{}` has provenance record incompatible with \
                         parameter `{}.{}`: `{}`",
                        parsed.name, function_name, parameter_name, raw
                    ),
                    pos,
                );
                None
            }
            (_, None) => None,
        }
    }

    /// Resolves one mirrored function type directly to its C typedef.
    pub(super) fn callback_provenance(
        &mut self,
        file: usize,
        type_ann: Option<&ast::TsType>,
        pos: Pos,
    ) -> Option<hir::ForeignTypeProvenance> {
        let parsed = &self.prog.files[file];
        let Some(typedef_name) = type_reference_name(type_ann) else {
            self.reject_subset(
                RejectionSite::MirrorAnonymousCallback,
                format!(
                    "mirror `{}` has an anonymous callback type without \
                     `@subscript-c-callback` provenance",
                    parsed.name
                ),
                pos,
            );
            return None;
        };
        match parsed.provenance.callbacks.get(typedef_name) {
            Some(record) => Some(hir::ForeignTypeProvenance::Callback {
                typedef_name: record.value.clone(),
            }),
            None => {
                self.reject_subset(
                    RejectionSite::MirrorCallbackProvenanceMissing,
                    format!(
                        "mirror `{}` callback type `{}` has no \
                         `@subscript-c-callback` provenance record",
                        parsed.name, typedef_name
                    ),
                    pos,
                );
                None
            }
        }
    }
}
