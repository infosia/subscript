#![warn(missing_docs)]
//! subscript compiler front end: SWC parse, the semantic checker for the
//! collision rules (`specs/blocks/collisions.md`), and the typed HIR.
//!
//! The primary public entry point is [`check_program`]: it takes one or
//! more source files (multi-file programs use `import`/`export`, e.g.
//! the `a19-modules` corpus entry) and returns either a typed
//! [`hir::Module`] or a non-empty list of [`Diagnostic`]s with stable
//! rule codes (S001–S013, S100) and TS positions. Loaders can use
//! [`parse_import_specifiers`] to discover imports with the same parser.

#[cfg(test)]
#[path = "../tests/corpus/interop.rs"]
mod test_interop;

use crate::check::rejection::{diagnostic, RejectionSite};
pub mod api_reference;
pub mod boundary_pass;
pub mod crossing;
pub mod diag;
mod diag_render;
pub mod divergence;
pub mod hir;
pub mod language_reference;
mod lifetime;
pub mod lir;
pub mod lir_text;
pub mod types;

mod ambient;
mod check;
mod parse;
mod provenance;
mod raise_sites;
mod regex;
mod trap_sites;
mod warn;

pub use check::fallthrough::sequence_can_fall_through;
pub use diag::{Diagnostic, Pos, RuleCode};
pub use diag_render::{render_diagnostics, render_warnings};
pub use parse::{discover_module_sources, parse_import_specifiers};
pub use types::{CallbackLifetime, ClassId, EnumId, FuncType, StringAliasId, Type};
pub use warn::{check_warnings, WarnCode, Warning};

/// The repository-relative name of `absolute`, always with `/` separators.
/// Returns `None` when the path is outside the root.
/// A directory walk yields the host separator and a `tsc` diagnostic yields
/// `/`, so both spellings pass through here and name one entry on every
/// host.
pub fn repository_relative(root: &std::path::Path, absolute: &std::path::Path) -> Option<String> {
    Some(
        absolute
            .strip_prefix(root)
            .ok()?
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// The standard module specifiers that `CheckOptions::enabled_modules` accepts.
pub const STANDARD_MODULES: &[&str] = &["node:fs/promises"];

/// Options that control program checking.
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// Standard module specifiers enabled by this build.
    pub enabled_modules: Vec<String>,
    /// Import specifiers to bind as poisoned when absent.
    pub poison_missing_modules: Vec<String>,
}

/// One source file of a program.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// File name used in diagnostics and for import resolution (the
    /// base name without `.ts` is the module stem).
    pub name: String,
    /// Full source text.
    pub source: String,
    /// True for an ambient declaration file (`.d.ts`): parsed in ambient
    /// mode, and its top-level declarations become a global ambient
    /// source (the generated C-header mirror, §12.2) rather than a
    /// checked program module.
    pub dts: bool,
    /// True when the program input names this file as its entry module.
    pub entry: bool,
}

impl SourceFile {
    /// Builds a program source file (ordinary `.ts`).
    #[must_use]
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        SourceFile {
            name: name.into(),
            source: source.into(),
            dts: false,
            entry: false,
        }
    }

    /// Builds the explicitly named entry module of a program.
    #[must_use]
    pub fn entry(name: impl Into<String>, source: impl Into<String>) -> Self {
        let mut file = Self::new(name, source);
        file.entry = true;
        file.dts = file.name.ends_with(".d.ts");
        file
    }

    /// Builds an ambient declaration source (`.d.ts`): parsed in ambient
    /// mode; its declarations join the global ambient surface (mirror
    /// ingestion, §12.2), visible to every program file without import.
    #[must_use]
    pub fn ambient(name: impl Into<String>, source: impl Into<String>) -> Self {
        SourceFile {
            name: name.into(),
            source: source.into(),
            dts: true,
            entry: false,
        }
    }
}

/// Checks a program.
///
/// On success every accepted construct resolves to a typed HIR module:
/// every expression carries its resolved type and TS position, and
/// generic declarations are already monomorphized. On rejection the
/// diagnostic list is non-empty; each entry carries a stable rule code
/// and the position of the offending construct.
///
/// # Errors
///
/// Returns the diagnostic list when the program parses with errors or
/// violates any language rule.
pub fn check_program(files: &[SourceFile]) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program_with(files, &CheckOptions::default())
}

/// Checks a program with the specified options.
///
/// The work runs on the thread that calls it
/// (`specs/blocks/compiler.md` §114.2 rule 1).
///
/// # Errors
///
/// Returns the diagnostic list when the program parses with errors,
/// violates any language rule, or `options.enabled_modules` names a module
/// outside [`STANDARD_MODULES`].
pub fn check_program_with(
    files: &[SourceFile],
    options: &CheckOptions,
) -> Result<hir::Module, Vec<Diagnostic>> {
    if files.is_empty() {
        return Err(vec![diagnostic(
            RejectionSite::SourceFilesEmpty,
            "no source files given",
            Pos::new(String::new(), 1, 1),
        )]);
    }
    let unknown: Vec<_> = options
        .enabled_modules
        .iter()
        .filter(|module| !STANDARD_MODULES.contains(&module.as_str()))
        .map(|module| {
            diagnostic(
                RejectionSite::EnabledModuleUnknown,
                format!(
                    "unknown standard module `{module}`; the enabled standard module must be node:fs/promises"
                ),
                Pos::new(String::new(), 1, 1),
            )
        })
        .collect();
    if !unknown.is_empty() {
        return Err(unknown);
    }
    swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
        let parsed = parse::parse_program(files)?;
        check::run(&parsed, options)
    })
}

#[cfg(test)]
mod tests;
