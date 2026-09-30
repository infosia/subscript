//! The opaque check of generic bodies (compiler.md §135).
//!
//! Every generic body is checked once with each type parameter bound to an
//! opaque type parameter type. The opaque type is a fresh reference class
//! with no member. The check keeps only the diagnostics that do not depend
//! on the type argument and drops every other one (§135.1 rule 2). The
//! per-instance check and lowering do not change: the pass removes every
//! class, function, method, and global that it made, so no opaque instance
//! reaches the HIR.

use super::*;

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
pub(crate) struct OpaqueType {
    /// True when the type parameter has a constraint (`T extends C`).
    pub constrained: bool,
}

impl<'p> Checker<'p> {
    /// True when `ty` is an opaque type parameter type.
    pub(crate) fn is_type_parameter(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if self.opaque_params.contains_key(id))
    }

    /// True when `ty` is an opaque type parameter type with no constraint
    /// (compiler.md §135.1 rule 2).
    pub(crate) fn is_unconstrained_type_parameter(&self, ty: &Type) -> bool {
        matches!(ty, Type::Class(id) if self.opaque_params.get(id).is_some_and(|opaque| !opaque.constrained))
    }

    /// True when `ty` is, or contains, an opaque type parameter type or an
    /// instance of a generic class at one (compiler.md §135.1 rule 2).
    pub(crate) fn involves_type_parameter(&self, ty: &Type) -> bool {
        if self.opaque_params.is_empty() {
            return false;
        }
        matches!(ty, Type::Class(id) if self.opaque_params.contains_key(id) || self.opaque_instances.contains(id))
            || ty
                .contained_types()
                .into_iter()
                .any(|inner| self.involves_type_parameter(inner))
    }

    /// Emits a diagnostic that does not depend on a type argument, so the
    /// opaque check keeps it (compiler.md §135.1 rule 2).
    pub(crate) fn error_independent(
        &mut self,
        code: RuleCode,
        message: impl Into<String>,
        pos: Pos,
    ) {
        let first = self.diags.len();
        self.error(code, message, pos);
        self.mark_independent(first);
    }

    /// Marks every diagnostic from index `first` on as independent of the
    /// type argument, so the opaque check keeps it (compiler.md §135.1
    /// rule 2). Outside the opaque check, it does nothing.
    pub(crate) fn mark_independent(&mut self, first: usize) {
        if self.opaque_params.is_empty() {
            return;
        }
        self.independent_diagnostics.extend(first..self.diags.len());
    }

    /// Records that the opaque type parameter type `ty` has a constraint.
    pub(crate) fn constrain_opaque_param(&mut self, ty: &Type) {
        if let Type::Class(id) = ty {
            if let Some(opaque) = self.opaque_params.get_mut(id) {
                opaque.constrained = true;
            }
        }
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
        let independent = std::mem::take(&mut self.independent_diagnostics);
        if self.narrowing_analysis.is_none() {
            self.opaque_loop_effects = self.opaque_body_loop_effects(
                snapshot.functions,
                snapshot.classes,
                &snapshot.methods,
            );
        }
        self.restore_opaque_snapshot(snapshot);
        let diagnostics = self.diags.take();
        let merged = merge_opaque_diagnostics(
            diagnostics,
            first,
            &checks,
            &independent,
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
    fn opaque_arguments(&mut self, parameters: &[String], pos: &Pos) -> Vec<Type> {
        parameters
            .iter()
            .map(|parameter| {
                let name = format!("[[identity:opaque:{}]]{parameter}", self.classes.len());
                let id = self.new_class(&name, false, false, None, pos.clone());
                self.opaque_params.insert(id, OpaqueType::default());
                Type::Class(id)
            })
            .collect()
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
                self.instantiate_fn(&key, &arguments, &[], pos);
            }
            OpaqueTemplate::Class(key) => {
                let Some(generic) = self.generic_classes.get(&key) else {
                    return;
                };
                let parameters = generic.type_params.clone();
                let pos = generic.pos.clone();
                let arguments = self.opaque_arguments(&parameters, &pos);
                self.instantiate_class(&key, &arguments, &[], pos);
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
                self.instantiate_method(class, &name, &arguments, &[], is_static, pos);
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
/// range of each template check after `first`; `independent` holds the
/// index of each opaque diagnostic that does not depend on the type
/// argument; `instance_ranges` holds the range of each per-instance check
/// before `first`. The result keeps one report per site and code:
///
/// - The opaque check keeps an unknown name (S016) and each diagnostic in
///   `independent`, and drops every other diagnostic (§135.1 rule 2).
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
    independent: &HashSet<usize>,
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
            // S016 is only ever an unknown name.
            if diagnostic.code != RuleCode::S016 && !independent.contains(&index) {
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
