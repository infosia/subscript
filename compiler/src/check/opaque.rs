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

/// One opaque type parameter type of the opaque check (compiler.md §135.1).
#[derive(Debug, Clone, Default)]
pub(crate) struct OpaqueType;

/// Restrictions whose verdict needs a concrete argument (§143 rule 2a).
#[derive(Clone, Copy)]
pub(crate) enum InstanceRestriction {
    /// The runtime formatter accepts only the project's printable types.
    TemplateInterpolation,
    /// A value-class field must have a value layout.
    ValueField,
    /// A numeric operation or assignment needs a concrete sized type.
    SizedNumeric,
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
    /// A relational operation needs a concrete operand kind.
    RelationalKind,
}

impl<'p> Checker<'p> {
    /// True when a named restriction needs the instance (§143 rule 2a).
    pub(crate) fn instance_restriction(&self, restriction: InstanceRestriction, ty: &Type) -> bool {
        match restriction {
            InstanceRestriction::TemplateInterpolation
            | InstanceRestriction::ValueField
            | InstanceRestriction::SizedNumeric
            | InstanceRestriction::BooleanContext
            | InstanceRestriction::AssociativeKey
            | InstanceRestriction::ArrayElementKind
            | InstanceRestriction::PartialValueLayout
            | InstanceRestriction::ContainerArgument
            | InstanceRestriction::NullableShape
            | InstanceRestriction::AggregateLayout
            | InstanceRestriction::SwitchKind
            | InstanceRestriction::CastKind
            | InstanceRestriction::RelationalKind => self.involves_type_parameter(ty),
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

    /// Stores the constraint in each binding of this parameter (§143 rule 1).
    pub(crate) fn constrain_opaque_param(&mut self, ty: &Type, constraint: Type) {
        if let Type::TypeParameter(parameter) = ty {
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
        match ty {
            Type::TypeParameter(parameter) => match &parameter.constraint {
                Some(constraint) => self.apparent_type(constraint),
                None => ty.clone(),
            },
            Type::GenericNumber => Type::F64,
            Type::Nullable(inner) => Type::nullable(self.apparent_type(inner)),
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
        self.assignable(&self.apparent_type(left), &self.apparent_type(right))
            || self.assignable(&self.apparent_type(right), &self.apparent_type(left))
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
                self.opaque_params.insert(id, OpaqueType);
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
