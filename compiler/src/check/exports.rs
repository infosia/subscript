//! Named export dependencies and declaration identities (compiler.md §128).

use super::*;
use crate::check::rejection::{diagnostic, RejectionSite};

#[derive(Clone)]
pub(super) struct ExportBinding {
    target: ExportTarget,
    pub(super) pos: Pos,
    re_export: bool,
}

#[derive(Clone)]
enum ExportTarget {
    Declaration(ScopeItem),
    Local(String),
    Remote { module: String, name: String },
}

type ExportKey = (usize, String);
type ImportTargets = HashMap<ExportKey, (String, String)>;

#[derive(Default)]
struct ExportResolution {
    items: HashMap<ExportKey, ScopeItem>,
    // The origin owns the diagnostic; dependency edges carry only poison.
    failures: BTreeMap<(usize, u32, u32), Diagnostic>,
}

impl ExportResolution {
    fn fail(&mut self, file: usize, diagnostic: Diagnostic) -> ScopeItem {
        self.failures
            .entry((file, diagnostic.pos.line, diagnostic.pos.col))
            .or_insert(diagnostic);
        ScopeItem::Poisoned
    }
}

impl Checker<'_> {
    fn register_export(&mut self, file: usize, name: String, binding: ExportBinding) {
        if let Some(previous) = self.export_definitions[file].get(&name) {
            if !previous.re_export && !binding.re_export {
                return; // The module scope owns declaration duplicates.
            }
            self.reject_subset(
                RejectionSite::DuplicateDirectExport,
                format!("duplicate export name `{name}`"),
                binding.pos,
            );
            return;
        }
        self.export_definitions[file].insert(name, binding);
    }

    pub(super) fn collect_export_declaration(&mut self, file: usize, decl: &ast::Decl) {
        for name in declaration_names(decl) {
            let item = self.file_scopes[file]
                .get(name.sym.as_ref())
                .map_or(ScopeItem::Poisoned, |binding| binding.item.clone());
            self.register_export(
                file,
                name.sym.to_string(),
                ExportBinding {
                    target: ExportTarget::Declaration(item),
                    pos: self.pos(name.span),
                    re_export: false,
                },
            );
        }
    }

    pub(super) fn collect_named_exports(&mut self, file: usize, export: &ast::NamedExport) {
        if self.prog.files[file].dts {
            self.reject_subset(
                RejectionSite::MirrorExportList,
                "export lists are outside the mirror surface",
                self.pos(export.span),
            );
            return;
        }
        if export.type_only {
            self.reject_subset(
                RejectionSite::TypeOnlyExportDeclaration,
                "type-only exports are outside the named module surface",
                self.pos(export.span),
            );
        }
        let unsupported_form =
            export.type_only || export.specifiers.iter().any(unsupported_export_specifier);
        // A standard module has no export list to re-export (§185 rule 1).
        let standard_source = export.src.as_ref().filter(|source| {
            &*source.value == "node:fs/promises" || &*source.value == super::text_module::SPECIFIER
        });
        if let Some(source) = standard_source {
            if &*source.value == super::text_module::SPECIFIER {
                self.reject_subset(
                    RejectionSite::TextModuleImportForm,
                    super::text_module::FORMS,
                    self.pos(source.span),
                );
            } else {
                self.reject_subset(
                    RejectionSite::FileModuleImportForm,
                    super::file_module::FORMS,
                    self.pos(source.span),
                );
            }
        }
        let missing_source = !unsupported_form
            && standard_source.is_none()
            && export.src.as_ref().is_some_and(|source| {
                let stem = normalize_module_specifier(&source.value);
                !self.prog.files.iter().any(|file| file.stem == stem)
            });
        if missing_source {
            if let Some(source) = &export.src {
                if !self
                    .poison_missing_modules
                    .contains(&normalize_module_specifier(&source.value))
                {
                    self.resolution_error(
                        RejectionSite::ExportSourceModuleMissing,
                        format!(
                            "export source module `{}` is not among the program's files",
                            source.value
                        ),
                        self.pos(source.span),
                    );
                }
            }
        }
        for specifier in &export.specifiers {
            let ast::ExportSpecifier::Named(named) = specifier else {
                self.reject_subset(
                    RejectionSite::ExportSpecifierKind,
                    "the module surface requires named exports",
                    self.pos(specifier.span()),
                );
                continue;
            };
            let name = named.orig.atom().to_string();
            let exported = named.exported.as_ref().unwrap_or(&named.orig);
            let unsupported = unsupported_export_specifier(specifier);
            if unsupported && !export.type_only {
                self.reject_subset(
                    RejectionSite::TypeOnlyOrDefaultExportSpecifier,
                    "type-only and default exports are outside the named module surface",
                    self.pos(named.span),
                );
            }
            // §134 rule 5: a local re-export of a type-only import is a
            // type-only export form (§128 rule 7b).
            let type_only_local = !unsupported
                && export.src.is_none()
                && binds_type_only_import(&self.prog.files[file].module, &name);
            if type_only_local {
                self.reject_subset(RejectionSite::TypeOnlyImportReexport, format!(
                        "`{name}` was imported with `import type`; its re-export is a type-only export, outside the named module surface"
                    ), self.pos(named.orig.span()));
            }
            let target = if unsupported_form
                || missing_source
                || type_only_local
                || standard_source.is_some()
            {
                ExportTarget::Declaration(ScopeItem::Poisoned)
            } else {
                match &export.src {
                    Some(source) => ExportTarget::Remote {
                        module: source.value.to_string(),
                        name,
                    },
                    None => ExportTarget::Local(name),
                }
            };
            // The imported name locates S016; the exported name locates S017.
            let exported_name = exported.atom().to_string();
            if self.export_definitions[file].contains_key(&exported_name) {
                self.reject_subset(
                    RejectionSite::DuplicateAliasedExport,
                    format!("duplicate export name `{exported_name}`"),
                    self.pos(exported.span()),
                );
                continue;
            }
            self.register_export(
                file,
                exported_name,
                ExportBinding {
                    target,
                    pos: self.pos(named.orig.span()),
                    re_export: true,
                },
            );
        }
    }

    pub(super) fn record_discovery_export(&mut self, export: &ast::NamedExport) {
        if export.type_only || export.specifiers.iter().any(unsupported_export_specifier) {
            return;
        }
        let Some(source) = &export.src else {
            return;
        };
        let module = source.value.to_string();
        let stem = normalize_module_specifier(&module);
        if !self.poison_missing_modules.contains(&stem)
            || self.prog.files.iter().any(|file| file.stem == stem)
        {
            return;
        }
        let names: Vec<_> = export
            .specifiers
            .iter()
            .filter_map(|specifier| {
                let ast::ExportSpecifier::Named(named) = specifier else {
                    return None;
                };
                Some((
                    named.orig.atom().to_string(),
                    named
                        .exported
                        .as_ref()
                        .unwrap_or(&named.orig)
                        .atom()
                        .to_string(),
                ))
            })
            .collect();
        if !names.is_empty() {
            self.poisoned_imports.push(hir::PoisonedImport {
                module,
                names,
                namespace: None,
                pos: self.pos(source.span),
            });
        }
    }

    pub(super) fn resolve_exports(&mut self) {
        let mut imports = ImportTargets::new();
        for (file, source) in self.prog.files.iter().enumerate() {
            for item in &source.module.body {
                let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) = item else {
                    continue;
                };
                for specifier in &import.specifiers {
                    if let ast::ImportSpecifier::Named(named) = specifier {
                        let local = named.local.sym.to_string();
                        let imported = named
                            .imported
                            .as_ref()
                            .map_or_else(|| local.clone(), |name| name.atom().to_string());
                        imports
                            .entry((file, local))
                            .or_insert((import.src.value.to_string(), imported));
                    }
                }
            }
        }
        let mut resolved = ExportResolution::default();
        for file in 0..self.prog.files.len() {
            let definitions = self.export_definitions[file].clone();
            for (name, _) in definitions {
                let item = self.resolve_export_name(
                    (file, name.clone()),
                    &imports,
                    &mut Vec::new(),
                    &mut resolved,
                );
                self.exports[file].insert(name, item);
            }
        }
        for diagnostic in resolved.failures.into_values() {
            let mut diagnostic = diagnostic;
            diagnostic.resolution = true;
            self.diags.push(diagnostic);
        }
    }

    fn resolve_export_name(
        &mut self,
        key: ExportKey,
        imports: &ImportTargets,
        path: &mut Vec<ExportKey>,
        resolved: &mut ExportResolution,
    ) -> ScopeItem {
        if let Some(item) = resolved.items.get(&key) {
            return item.clone();
        }
        if let Some(start) = path.iter().position(|member| member == &key) {
            let mut cycle = path[start..].to_vec();
            let first = cycle
                .iter()
                .enumerate()
                .min_by_key(|(_, (file, name))| {
                    let pos = &self.export_definitions[*file][name].pos;
                    (*file, pos.line, pos.col)
                })
                .map_or(0, |(index, _)| index);
            cycle.rotate_left(first);
            let origin = &cycle[0];
            let chain = cycle
                .iter()
                .chain(std::iter::once(origin))
                .map(|(file, name)| format!("{}::{name}", self.prog.files[*file].name))
                .collect::<Vec<_>>()
                .join(" -> ");
            return resolved.fail(
                origin.0,
                diagnostic(
                    RejectionSite::ExportAliasCycle,
                    format!("export alias chain reaches no declaration: {chain}"),
                    self.export_definitions[origin.0][&origin.1].pos.clone(),
                ),
            );
        }
        let (file, name) = &key;
        let Some(binding) = self.export_definitions[*file].get(name).cloned() else {
            return ScopeItem::Poisoned;
        };
        path.push(key.clone());
        let result = match &binding.target {
            ExportTarget::Declaration(item) => item.clone(),
            ExportTarget::Local(local) => {
                let namespace = self.prog.files[*file].module.body.iter().any(|item| {
                    matches!(item, ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import))
                        if import.specifiers.iter().any(|specifier|
                            matches!(specifier, ast::ImportSpecifier::Namespace(ns) if ns.local.sym.as_ref() == local)))
                });
                if namespace {
                    self.reject_subset(
                        RejectionSite::ReexportSpecifierKind,
                        "the module surface requires named exports",
                        binding.pos.clone(),
                    );
                    ScopeItem::Poisoned
                } else if let Some(binding) = self.file_scopes[*file].get(local) {
                    binding.item.clone()
                } else if let Some((module, imported)) = imports.get(&(*file, local.clone())) {
                    self.resolve_export_source(module, imported, imports, path, resolved, None)
                } else {
                    resolved.fail(
                        *file,
                        diagnostic(
                            RejectionSite::ExportLocalMissing,
                            format!(
                                "`{local}` is not defined in `{}`",
                                self.prog.files[*file].name
                            ),
                            binding.pos.clone(),
                        ),
                    )
                }
            }
            ExportTarget::Remote { module, name } => self.resolve_export_source(
                module,
                name,
                imports,
                path,
                resolved,
                Some((*file, &binding.pos)),
            ),
        };
        path.pop();
        resolved.items.insert(key, result.clone());
        result
    }

    fn resolve_export_source(
        &mut self,
        module: &str,
        name: &str,
        imports: &ImportTargets,
        path: &mut Vec<ExportKey>,
        resolved: &mut ExportResolution,
        origin: Option<(usize, &Pos)>,
    ) -> ScopeItem {
        let stem = normalize_module_specifier(module);
        let Some(file) = self.prog.files.iter().position(|file| file.stem == stem) else {
            // Missing module statements own their diagnostics, including imports.
            return ScopeItem::Poisoned;
        };
        if self
            .rejected_module_exports
            .get(&stem)
            .is_some_and(|names| names.contains(name))
        {
            return ScopeItem::Poisoned;
        }
        if !self.export_definitions[file].contains_key(name) {
            if let Some((file, pos)) = origin {
                return resolved.fail(
                    file,
                    diagnostic(
                        RejectionSite::ReexportMemberMissing,
                        format!("`{name}` is not exported by `{module}`"),
                        pos.clone(),
                    ),
                );
            }
            // The import behind a local re-export owns this missing name.
            return ScopeItem::Poisoned;
        }
        self.resolve_export_name((file, name.to_string()), imports, path, resolved)
    }
}

// Exhaustive declaration coverage keeps rejected names in the dependency graph (§128 rule 5b).
pub(super) fn declaration_names(decl: &ast::Decl) -> Vec<ast::Ident> {
    match decl {
        ast::Decl::Fn(f) => vec![f.ident.clone()],
        ast::Decl::Class(c) => vec![c.ident.clone()],
        ast::Decl::TsEnum(e) => vec![e.id.clone()],
        ast::Decl::TsTypeAlias(a) => vec![a.id.clone()],
        ast::Decl::TsInterface(i) => vec![i.id.clone()],
        ast::Decl::TsModule(m) => match &m.id {
            ast::TsModuleName::Ident(id) => vec![id.clone()],
            ast::TsModuleName::Str(_) => Vec::new(),
        },
        ast::Decl::Var(v) => v
            .decls
            .iter()
            .flat_map(|d| pattern::collect_names(&d.name))
            .map(|b| b.id.clone())
            .collect(),
        ast::Decl::Using(u) => u
            .decls
            .iter()
            .flat_map(|d| pattern::collect_names(&d.name))
            .map(|b| b.id.clone())
            .collect(),
    }
}

/// Answers whether a type-only import of `module` binds `local`.
fn binds_type_only_import(module: &ast::Module, local: &str) -> bool {
    module.body.iter().any(|item| {
        let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) = item else {
            return false;
        };
        import.specifiers.iter().any(|specifier| {
            matches!(specifier, ast::ImportSpecifier::Named(named)
                if named.local.sym.as_ref() == local
                    && signatures::type_only_import(import, named))
        })
    })
}

fn unsupported_export_specifier(specifier: &ast::ExportSpecifier) -> bool {
    match specifier {
        ast::ExportSpecifier::Named(named) => {
            named.is_type_only
                || named.orig.atom().as_ref() == "default"
                || named
                    .exported
                    .as_ref()
                    .is_some_and(|name| name.atom().as_ref() == "default")
        }
        _ => true,
    }
}
