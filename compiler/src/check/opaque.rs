//! The generic body check (§143).
//!
//! Each parameter has its own type and constraint. The check reports every
//! diagnostic and removes every instance before HIR construction (§135 rule 1).

use super::*;
use crate::types::TypeParameterType;

/// A source site: file, 1-based line, 1-based column.
type Site = (String, u32, u32);

fn site(pos: &Pos) -> Site {
    (pos.file.clone(), pos.line, pos.col)
}

/// One generic template that the opaque check instantiates.
enum OpaqueTemplate {
    Function(String),
    Class(String),
    Method {
        class: ClassId,
        name: String,
        is_static: bool,
    },
}

/// The checker state that the opaque check restores after it runs.
struct OpaqueSnapshot {
    functions: usize,
    classes: usize,
    methods: Vec<usize>,
    class_sigs: Vec<ClassSig>,
    class_ids: HashMap<String, ClassId>,
    fn_sigs: HashMap<String, FnSig>,
    instance_symbols: HashMap<String, String>,
    type_handle_classes: usize,
    pending_layouts: usize,
    globals: usize,
    global_sigs: HashMap<String, GlobalSig>,
    regex_literals: HashMap<(String, u32, u32), String>,
    worker_entries: usize,
    top_level: usize,
}

/// The constraint of one opaque parameter identity (§143 rule 1d).
#[derive(Debug, Clone, Default)]
pub(crate) struct OpaqueType {
    constraint: Option<Type>,
}

/// Restrictions whose verdict needs a concrete argument (§143 rule 2a).
#[derive(Clone, Copy)]
pub(crate) enum InstanceRestriction {
    /// The runtime formatter accepts only the project's printable types.
    TemplateInterpolation,
    /// A value-class field must have a value layout.
    ValueField,
    /// A numeric operation, assignment, or storage element needs a concrete sized type.
    SizedNumeric,
    /// Composite assignments need exact concrete component types after constraint typing.
    CompositeAssignability,
    /// Unary numeric coercion accepts every non-null value that TypeScript accepts.
    UnaryNumeric,
    /// Boolean contexts require a concrete boolean argument.
    BooleanContext,
    /// An associative key needs a concrete hash and equality kind.
    AssociativeKey,
    /// An array callback or equality search needs a concrete element kind.
    ArrayElementKind,
    /// A partial accessor needs a concrete nullable-pointer value layout.
    PartialValueLayout,
    /// A container argument needs a concrete Context-affinity kind.
    ContainerArgument,
    /// A nullable argument needs a concrete reference shape.
    NullableShape,
    /// An aggregate needs a concrete byte layout.
    AggregateLayout,
    /// A switch needs a concrete dispatch kind.
    SwitchKind,
    /// A cast needs a concrete runtime conversion.
    CastKind,
    /// JSON needs a concrete serializable graph.
    JsonSerializability,
    /// Byte access needs a concrete value layout.
    ByteAccessTarget,
    /// A relational operation needs a concrete operand kind.
    RelationalKind,
}

impl<'p> Checker<'p> {
    /// True when a named restriction needs the instance (§143 rule 2a).
    pub(crate) fn instance_restriction(&self, restriction: InstanceRestriction, ty: &Type) -> bool {
        if !self.involves_type_parameter(ty) {
            return false;
        }
        let apparent = self.apparent_type(ty);
        match restriction {
            InstanceRestriction::SizedNumeric => {
                let mut element = apparent;
                let mut seen = std::collections::HashSet::new();
                loop {
                    // §143 rule 1d preserves identity across recursive storage constraints.
                    if let Type::TypeParameter(parameter) = &element {
                        if !seen.insert(parameter.identity) {
                            return false;
                        }
                    }
                    let shape = self.apparent_type(&element);
                    match shape {
                        Type::Array(inner) | Type::FixedArray(inner, _) => element = *inner,
                        _ => {
                            return shape.is_numeric()
                                || matches!(shape, Type::GenericNumber | Type::Enum(_))
                        }
                    }
                }
            }
            InstanceRestriction::CompositeAssignability => matches!(
                apparent,
                Type::Nullable(_)
                    | Type::Array(_)
                    | Type::FixedArray(_, _)
                    | Type::Map(_, _)
                    | Type::Set(_)
                    | Type::Class(_)
                    | Type::Func(_)
                    | Type::Worker(_, _)
                    | Type::Inbox(_)
                    | Type::Outbox(_)
                    | Type::Generator(_)
                    | Type::AsyncHandle(_)
                    | Type::IterResult(_)
            ),
            InstanceRestriction::UnaryNumeric => {
                !matches!(apparent, Type::Nullable(_) | Type::Null | Type::Void)
            }
            InstanceRestriction::BooleanContext => !matches!(apparent, Type::Func(_)),
            InstanceRestriction::TemplateInterpolation => !matches!(apparent, Type::Error),
            InstanceRestriction::JsonSerializability | InstanceRestriction::ByteAccessTarget => {
                !matches!(apparent, Type::Error)
            }
            InstanceRestriction::ValueField => !matches!(apparent, Type::Error),
            InstanceRestriction::AssociativeKey => !matches!(apparent, Type::Error),
            InstanceRestriction::ArrayElementKind => !matches!(apparent, Type::Error),
            InstanceRestriction::PartialValueLayout => !matches!(apparent, Type::Error),
            InstanceRestriction::ContainerArgument => !matches!(apparent, Type::Error),
            InstanceRestriction::NullableShape => !matches!(apparent, Type::Error),
            InstanceRestriction::AggregateLayout => {
                matches!(&self.apparent_type(ty), Type::FixedArray(_, _))
            }
            InstanceRestriction::SwitchKind => !matches!(apparent, Type::Error),
            InstanceRestriction::CastKind => !matches!(apparent, Type::Error),
            InstanceRestriction::RelationalKind => {
                !matches!(apparent, Type::Nullable(_) | Type::Void | Type::Func(_))
            }
        }
    }

    /// True when `ty` is an opaque type parameter type.
    pub(crate) fn is_type_parameter(&self, ty: &Type) -> bool {
        matches!(ty, Type::TypeParameter(_))
    }

    /// True when `ty` is an opaque type parameter type with no constraint
    /// (§143 rule 1).
    pub(crate) fn is_unconstrained_type_parameter(&self, ty: &Type) -> bool {
        matches!(ty, Type::TypeParameter(parameter) if parameter.constraint.is_none())
    }

    /// True when `ty` is, or contains, an opaque type parameter type or an
    /// instance of a generic class at one (§143 rule 2a).
    pub(crate) fn involves_type_parameter(&self, ty: &Type) -> bool {
        if self.opaque_params.is_empty() {
            return false;
        }
        matches!(ty, Type::TypeParameter(_) | Type::GenericNumber)
            || matches!(ty, Type::Class(id) if self.opaque_instances.contains(id))
            || ty
                .contained_types()
                .into_iter()
                .any(|inner| self.involves_type_parameter(inner))
    }

    /// Stores the constraint by identity and in each binding of this parameter (§143 rules 1b and 1d).
    pub(crate) fn constrain_opaque_param(&mut self, ty: &Type, constraint: Type) {
        if let Type::TypeParameter(parameter) = ty {
            if let Some(opaque) = self.opaque_params.get_mut(&ClassId(parameter.identity)) {
                opaque.constraint = Some(constraint.clone());
            }
            for binding in self.subst.values_mut() {
                if let Type::TypeParameter(other) = binding {
                    if parameter.identity == other.identity {
                        other.constraint = Some(Box::new(constraint.clone()));
                    }
                }
            }
        }
    }

    /// Resolves a value type through its constraint (§143 rule 1a).
    pub(crate) fn apparent_type(&self, ty: &Type) -> Type {
        self.resolve_apparent_type(ty, &mut Vec::new())
    }

    /// Resolves one constraint by parameter identity (§143 rule 1d).
    pub(crate) fn direct_constraint<'a>(
        &'a self,
        parameter: &'a TypeParameterType,
    ) -> Option<&'a Type> {
        parameter.constraint.as_deref().or_else(|| {
            self.opaque_params
                .get(&ClassId(parameter.identity))
                .and_then(|opaque| opaque.constraint.as_ref())
        })
    }

    /// True when direct parameter constraints form a cycle (§143 rule 1d).
    pub(crate) fn constraint_cycle(&self, ty: &Type) -> bool {
        let mut seen = Vec::new();
        let mut current = ty;
        while let Type::TypeParameter(parameter) = current {
            if seen.contains(&parameter.identity) {
                return true;
            }
            seen.push(parameter.identity);
            let Some(constraint) = self.direct_constraint(parameter) else {
                return false;
            };
            current = constraint;
        }
        false
    }

    fn resolve_apparent_type(&self, ty: &Type, seen: &mut Vec<usize>) -> Type {
        match ty {
            Type::TypeParameter(parameter) => {
                if seen.contains(&parameter.identity) {
                    return ty.clone();
                }
                seen.push(parameter.identity);
                let constraint = self.direct_constraint(parameter);
                let result = constraint.map_or_else(
                    || ty.clone(),
                    |constraint| self.resolve_apparent_type(constraint, seen),
                );
                seen.pop();
                result
            }
            Type::Nullable(inner) => Type::nullable(self.resolve_apparent_type(inner, seen)),
            Type::GenericUnion(members) => {
                let mut shapes = Vec::new();
                for member in members.iter() {
                    let shape = self.resolve_apparent_type(member, seen);
                    if !shapes.contains(&shape) {
                        shapes.push(shape);
                    }
                }
                if shapes
                    .iter()
                    .all(|shape| shape.is_numeric() || matches!(shape, Type::GenericNumber))
                {
                    return Type::GenericNumber;
                }
                if shapes.len() == 1 {
                    return shapes.remove(0);
                }
                Type::GenericUnion(shapes.into_boxed_slice())
            }
            _ => ty.clone(),
        }
    }

    /// Removes null through the apparent type and preserves parameter identity (§143 rule 1a).
    pub(crate) fn non_null_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Nullable(inner) => (**inner).clone(),
            Type::TypeParameter(parameter) => {
                let mut parameter = parameter.clone();
                parameter.constraint = parameter
                    .constraint
                    .as_deref()
                    .map(|constraint| Box::new(self.non_null_type(constraint)));
                Type::TypeParameter(parameter)
            }
            _ => ty.clone(),
        }
    }

    /// Builds the result union of a generic operation (§143 rule 1c).
    pub(crate) fn generic_union(&self, left: &Type, right: &Type) -> Type {
        if left == right {
            return left.clone();
        }
        let mut members = Vec::new();
        for ty in [left, right] {
            let types = match ty {
                Type::GenericUnion(types) => types.as_ref(),
                _ => std::slice::from_ref(ty),
            };
            for member in types {
                if !members.contains(member) {
                    members.push(member.clone());
                }
            }
        }
        Type::GenericUnion(members.into_boxed_slice())
    }

    /// Checks type overlap for generic equality and case labels (§143 rule 1a).
    pub(crate) fn generic_overlap(&self, left: &Type, right: &Type) -> bool {
        if left == right {
            return true;
        }
        if let Type::Nullable(inner) = left {
            return self.generic_overlap(inner, right);
        }
        if let Type::Nullable(inner) = right {
            return self.generic_overlap(left, inner);
        }
        if matches!(left, Type::TypeParameter(_)) && matches!(right, Type::TypeParameter(_)) {
            return self.assignable(left, right) || self.assignable(right, left);
        }
        if self.is_unconstrained_type_parameter(left) || self.is_unconstrained_type_parameter(right)
        {
            return true;
        }
        if matches!(left, Type::Null) || matches!(right, Type::Null) {
            return true;
        }
        let apparent_left = self.apparent_type(left);
        let apparent_right = self.apparent_type(right);
        let left_numeric = apparent_left.is_numeric()
            || matches!(apparent_left, Type::GenericNumber | Type::Enum(_));
        let right_numeric = apparent_right.is_numeric()
            || matches!(apparent_right, Type::GenericNumber | Type::Enum(_));
        if left_numeric && right_numeric {
            return true;
        }
        if matches!(
            (&apparent_left, &apparent_right),
            (Type::StringAlias(_), Type::Str) | (Type::Str, Type::StringAlias(_))
        ) {
            return true;
        }
        self.assignable(&apparent_left, &apparent_right)
            || self.assignable(&apparent_right, &apparent_left)
    }

    /// Resolves the receiver type through the same apparent-type function (§143 rule 1a).
    pub(crate) fn apparent_expr(&self, mut expr: hir::Expr) -> hir::Expr {
        expr.ty = self.apparent_type(&expr.ty);
        expr
    }

    /// Runs the opaque check over every generic template of the program
    /// (compiler.md §135.1 rule 1) and merges its diagnostics with the
    /// per-instance diagnostics (§135.1 rule 3).
    pub(crate) fn check_generic_bodies_opaque(&mut self) {
        let templates = self.opaque_templates();
        if templates.is_empty() {
            return;
        }
        let snapshot = self.opaque_snapshot();
        let first = self.diags.len();
        let mut checks = Vec::with_capacity(templates.len());
        for template in templates {
            let start = self.diags.len();
            self.opaque_root = true;
            self.check_opaque_template(template);
            self.opaque_root = false;
            checks.push(start..self.diags.len());
        }
        self.opaque_params.clear();
        self.opaque_instances.clear();
        if self.narrowing_analysis.is_none() {
            self.opaque_loop_effects = self.opaque_body_loop_effects(
                snapshot.functions,
                snapshot.classes,
                &snapshot.methods,
            );
        }
        self.restore_opaque_snapshot(snapshot);
        let diagnostics = self.diags.take();
        // An S011 inside a growing argument takes precedence (§140 acceptance 2).
        let suppressed: HashSet<_> = self
            .growth_reports
            .iter()
            .filter_map(|(index, start, end)| {
                diagnostics
                    .iter()
                    .any(|diagnostic| {
                        diagnostic.code == RuleCode::S011
                            && diagnostic.pos.file == start.file
                            && (diagnostic.pos.line, diagnostic.pos.col) >= (start.line, start.col)
                            && (diagnostic.pos.line, diagnostic.pos.col) < (end.line, end.col)
                    })
                    .then_some(*index)
            })
            .collect();
        let merged = merge_opaque_diagnostics(
            diagnostics,
            first,
            &checks,
            &suppressed,
            &std::mem::take(&mut self.instance_diagnostic_ranges),
        );
        self.diags.extend(merged);
    }

    /// Every generic template in source order: generic functions, generic
    /// classes, and the generic methods of non-generic classes.
    fn opaque_templates(&self) -> Vec<OpaqueTemplate> {
        let mut templates: Vec<((usize, u32, u32), OpaqueTemplate)> = Vec::new();
        for (key, template) in &self.generic_fns {
            if template.rejected {
                continue;
            }
            let pos = self.pos(template.function.span);
            templates.push((
                (template.file, pos.line, pos.col),
                OpaqueTemplate::Function(key.clone()),
            ));
        }
        for (key, template) in &self.generic_classes {
            templates.push((
                (template.file, template.pos.line, template.pos.col),
                OpaqueTemplate::Class(key.clone()),
            ));
        }
        for (index, class_sig) in self.class_sigs.iter().enumerate() {
            for (namespace, is_static) in [
                (&class_sig.generic_methods, false),
                (&class_sig.static_generic_methods, true),
            ] {
                for (name, template) in namespace {
                    if template.rejected {
                        continue;
                    }
                    let pos = self.pos(template.function.span);
                    templates.push((
                        (template.file, pos.line, pos.col),
                        OpaqueTemplate::Method {
                            class: ClassId(index),
                            name: name.clone(),
                            is_static,
                        },
                    ));
                }
            }
        }
        templates.sort_by_key(|(position, _)| *position);
        templates
            .into_iter()
            .map(|(_, template)| template)
            .collect()
    }

    /// Makes one opaque type parameter type per parameter name.
    fn opaque_arguments(&mut self, parameters: &[String], pos: &Pos) -> InstanceArguments {
        let types = parameters
            .iter()
            .map(|parameter| {
                let name = format!("[[identity:opaque:{}]]{parameter}", self.classes.len());
                let id = self.new_class(&name, false, false, None, pos.clone());
                self.opaque_params
                    .insert(id, OpaqueType { constraint: None });
                Type::TypeParameter(Box::new(TypeParameterType {
                    identity: id.0,
                    name: parameter.clone(),
                    constraint: None,
                }))
            })
            .collect();
        InstanceArguments {
            types,
            positions: Vec::new(),
            edges: vec![Vec::new(); parameters.len()],
            ends: Vec::new(),
        }
    }

    fn check_opaque_template(&mut self, template: OpaqueTemplate) {
        match template {
            OpaqueTemplate::Function(key) => {
                let Some(generic) = self.generic_fns.get(&key) else {
                    return;
                };
                let parameters = generic.type_params.clone();
                let pos = self.pos(generic.function.span);
                let arguments = self.opaque_arguments(&parameters, &pos);
                self.instantiate_fn(&key, &arguments, pos);
            }
            OpaqueTemplate::Class(key) => {
                let Some(generic) = self.generic_classes.get(&key) else {
                    return;
                };
                let parameters = generic.type_params.clone();
                let pos = generic.pos.clone();
                let arguments = self.opaque_arguments(&parameters, &pos);
                self.instantiate_class(&key, &arguments, pos);
            }
            OpaqueTemplate::Method {
                class,
                name,
                is_static,
            } => {
                let signatures = &self.class_sigs[class.0];
                let generic = if is_static {
                    signatures.static_generic_methods.get(&name)
                } else {
                    signatures.generic_methods.get(&name)
                };
                let Some(generic) = generic else {
                    return;
                };
                let parameters = generic.type_params.clone();
                let pos = self.pos(generic.function.span);
                let arguments = self.opaque_arguments(&parameters, &pos);
                self.instantiate_method(class, &name, &arguments, is_static, pos);
            }
        }
    }

    fn opaque_snapshot(&self) -> OpaqueSnapshot {
        OpaqueSnapshot {
            functions: self.functions.len(),
            classes: self.classes.len(),
            methods: self
                .classes
                .iter()
                .map(|class| class.methods.len())
                .collect(),
            class_sigs: self.class_sigs.clone(),
            class_ids: self.class_ids.clone(),
            fn_sigs: self.fn_sigs.clone(),
            instance_symbols: self.instance_symbols.clone(),
            type_handle_classes: self.type_handle_classes.len(),
            pending_layouts: self.pending_layouts.len(),
            globals: self.globals.len(),
            global_sigs: self.global_sigs.clone(),
            regex_literals: self.regex_literals.clone(),
            worker_entries: self.worker_entries.len(),
            top_level: self.top_level.len(),
        }
    }

    fn restore_opaque_snapshot(&mut self, snapshot: OpaqueSnapshot) {
        self.functions.truncate(snapshot.functions);
        self.classes.truncate(snapshot.classes);
        for (class, methods) in self.classes.iter_mut().zip(snapshot.methods) {
            class.methods.truncate(methods);
        }
        self.class_sigs = snapshot.class_sigs;
        self.class_ids = snapshot.class_ids;
        self.fn_sigs = snapshot.fn_sigs;
        self.instance_symbols = snapshot.instance_symbols;
        self.type_handle_classes
            .truncate(snapshot.type_handle_classes);
        self.pending_layouts.truncate(snapshot.pending_layouts);
        self.globals.truncate(snapshot.globals);
        self.global_sigs = snapshot.global_sigs;
        self.regex_literals = snapshot.regex_literals;
        self.worker_entries.truncate(snapshot.worker_entries);
        self.top_level.truncate(snapshot.top_level);
        let classes = snapshot.classes;
        self.handle_classes.retain(|id| id.0 < classes);
        self.declared_classes.retain(|id| id.0 < classes);
        self.boundary_classes.retain(|id| id.0 < classes);
        self.instance_arguments.retain(|id, _| id.0 < classes);
    }
}

/// The identity of a diagnostic for compiler.md §135.1 rule 3: the site,
/// the code, and the message.
type Identity = (Site, RuleCode, String);

fn identity(diagnostic: &Diagnostic) -> Identity {
    (
        site(&diagnostic.pos),
        diagnostic.code,
        diagnostic.message.clone(),
    )
}

/// Merges the diagnostics of the opaque check with the earlier ones
/// (compiler.md §135.1 rule 3).
///
/// `diagnostics[..first]` precede the opaque check; `checks` holds the
/// range of each template check after `first`; `instance_ranges` holds
/// the range of each per-instance check
/// before `first`. The result keeps the diagnostics of §143 rule 2:
///
/// - The opaque check reports every diagnostic (§143 rule 2).
/// - A site that an earlier template check reports is not reported again
///   by a later template check.
/// - Two kept diagnostics with the same code, message, and position are
///   one, whichever check produced them.
/// - A per-instance diagnostic at a site that the opaque check reports
///   with the same code is removed, and the opaque diagnostics of that
///   site take the place of the first one removed. A per-instance
///   diagnostic with a different code stays.
/// - A per-instance diagnostic that repeats an earlier per-instance
///   diagnostic (same code, message, and position) is removed.
fn merge_opaque_diagnostics(
    diagnostics: Vec<Diagnostic>,
    first: usize,
    checks: &[std::ops::Range<usize>],
    suppressed: &HashSet<usize>,
    instance_ranges: &[std::ops::Range<usize>],
) -> Vec<Diagnostic> {
    let mut opaque: Vec<Option<Diagnostic>> = Vec::new();
    let mut opaque_sites: HashMap<Site, Vec<usize>> = HashMap::new();
    let mut opaque_codes: HashMap<Site, Vec<RuleCode>> = HashMap::new();
    let mut seen_opaque: HashSet<Identity> = HashSet::new();
    for range in checks {
        let mut reported: Vec<(Site, RuleCode)> = Vec::new();
        for (index, diagnostic) in diagnostics[range.clone()]
            .iter()
            .enumerate()
            .map(|(offset, diagnostic)| (range.start + offset, diagnostic))
        {
            if suppressed.contains(&index) {
                continue;
            }
            let key = site(&diagnostic.pos);
            if opaque_codes.contains_key(&key) || !seen_opaque.insert(identity(diagnostic)) {
                continue;
            }
            reported.push((key.clone(), diagnostic.code));
            opaque_sites.entry(key).or_default().push(opaque.len());
            opaque.push(Some(diagnostic.clone()));
        }
        for (key, code) in reported {
            opaque_codes.entry(key).or_default().push(code);
        }
    }

    let mut in_instance = vec![false; first];
    for range in instance_ranges {
        for flag in in_instance
            .iter_mut()
            .take(range.end.min(first))
            .skip(range.start)
        {
            *flag = true;
        }
    }

    let mut seen_instance: HashSet<Identity> = HashSet::new();
    let mut merged = Vec::with_capacity(diagnostics.len());
    for (index, diagnostic) in diagnostics[..first].iter().enumerate() {
        if suppressed.contains(&index) {
            continue;
        }
        if !in_instance[index] {
            merged.push(diagnostic.clone());
            continue;
        }
        let key = site(&diagnostic.pos);
        if opaque_codes
            .get(&key)
            .is_some_and(|codes| codes.contains(&diagnostic.code))
        {
            // The first removed instance diagnostic of the site takes the
            // opaque diagnostics of that site.
            if let Some(indices) = opaque_sites.remove(&key) {
                merged.extend(indices.into_iter().filter_map(|at| opaque[at].take()));
            }
            continue;
        }
        if !seen_instance.insert(identity(diagnostic)) {
            continue;
        }
        merged.push(diagnostic.clone());
    }
    merged.extend(opaque.into_iter().flatten());
    merged
}
