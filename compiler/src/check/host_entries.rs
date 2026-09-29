//! The entry module owns the host API (compiler.md §129).

use super::*;

pub(super) fn entry_file(prog: &ParsedProgram) -> Result<Option<usize>, Vec<Diagnostic>> {
    let named: Vec<_> = prog
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| f.entry)
        .collect();
    let programs: Vec<_> = prog
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.dts)
        .collect();
    let message = match named.as_slice() {
        [(index, file)] if !file.dts => return Ok(Some(*index)),
        [(_, _)] => "an ambient declaration file cannot be the entry module",
        [] if programs.len() == 1 => return Ok(Some(programs[0].0)),
        [] if programs.is_empty() => return Ok(None),
        [] => "a program with multiple source files must name its entry module",
        _ => "a program must name exactly one entry module",
    };
    Err(vec![Diagnostic::new(
        RuleCode::S100,
        message,
        Pos::new(
            named
                .iter()
                .map(|(_, file)| file.name.as_str())
                .min()
                .or_else(|| prog.files.iter().map(|file| file.name.as_str()).min())
                .unwrap_or(""),
            1,
            1,
        ),
    )])
}

impl Checker<'_> {
    pub(super) fn host_exports(
        &self,
        entry: Option<usize>,
    ) -> Vec<(String, ScopeItem, Pos, Option<Pos>)> {
        let Some(file) = entry else { return Vec::new() };
        self.export_definitions[file]
            .iter()
            .filter_map(|(name, binding)| {
                self.exports[file].get(name).map(|target| {
                    let declaration = match target {
                        ScopeItem::GenericFunc(symbol) => self
                            .generic_fns
                            .get(symbol)
                            .map(|function| Pos::new(&self.prog.files[function.file].name, 1, 1)),
                        _ => None,
                    };
                    (
                        name.clone(),
                        target.clone(),
                        binding.pos.clone(),
                        declaration,
                    )
                })
            })
            .collect()
    }
}

pub(super) fn populate(
    module: &mut hir::Module,
    exports: Vec<(String, ScopeItem, Pos, Option<Pos>)>,
) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    for (name, target, pos, declaration) in exports {
        if matches!(target, ScopeItem::Poisoned) {
            continue;
        }
        let reason = match &target {
            ScopeItem::Func(symbol) => {
                if let Some(function) = module
                    .functions
                    .iter()
                    .find(|f| f.symbol.full_text() == symbol)
                {
                    let reason = if function.is_generator {
                        Some("generators cannot be host entries")
                    } else if function.is_async && !function.params.is_empty() {
                        Some("async host entries cannot have parameters")
                    } else if function.ret != Type::Void {
                        Some("host entries must return void")
                    } else if function.host_entry_trap_sites(module).is_none() {
                        Some("host entry boundary signature requires scalar or opaque-handle parameters")
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        format!(
                            "{reason}; target `{}` in `{}`",
                            function.name, function.pos.file
                        )
                    } else {
                        module.host_entries.push(hir::HostEntry {
                            name,
                            target: function.symbol.clone(),
                            signature: hir::HostSignature {
                                parameters: function.params.iter().map(|p| p.ty.clone()).collect(),
                                is_async: function.is_async,
                            },
                            pos,
                        });
                        continue;
                    }
                } else {
                    "the target function has no checked declaration".to_owned()
                }
            }
            ScopeItem::GenericFunc(symbol) => format!(
                "generic function `{}` in `{}` cannot be a host entry",
                source_name(symbol),
                declaration.as_ref().map_or("", |pos| pos.file.as_str())
            ),
            _ => "the entry module exports functions only".to_owned(),
        };
        let mut diagnostic = Diagnostic::new(
            RuleCode::S100,
            format!("entry export `{name}`: {reason}"),
            pos,
        );
        diagnostic.divergence = Some(Divergence::HostApiSurface);
        errors.push(diagnostic);
    }
    errors
}
