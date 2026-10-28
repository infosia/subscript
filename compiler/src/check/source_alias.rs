//! Module-level transparent type aliases (compiler.md §202).
//!
//! A `type N = T;` whose `T` is not a string-literal union is a second
//! name of `T`. The alias resolves after every module name is bound, so
//! it can name a later declaration (rule 3). A reference that reaches the
//! alias again is one S100 at that alias (rule 4).

use swc_ecma_ast as ast;

use super::{module_decl, Checker, ScopeItem};
use crate::check::rejection::RejectionSite;
use crate::diag::Pos;
use crate::types::Type;

/// One module-level transparent alias.
#[derive(Debug, Clone)]
pub(crate) struct SourceAlias {
    file: usize,
    name: String,
    pos: Pos,
    state: State,
    cycle_reported: bool,
}

#[derive(Debug, Clone)]
enum State {
    Pending,
    Resolving,
    Resolved(ScopeItem),
}

impl SourceAlias {
    /// An unresolved alias that `file` declares as `name` at `pos`.
    pub(crate) fn new(file: usize, name: &str, pos: Pos) -> Self {
        Self {
            file,
            name: name.to_owned(),
            pos,
            state: State::Pending,
            cycle_reported: false,
        }
    }
}

/// Collects each type name that `ty` references. The flag is true for a
/// direct reference: one that only unions and parentheses enclose.
/// `tsc` defers the other references, so a cycle through one of them is
/// a cycle that `tsc` accepts.
fn references<'a>(ty: &'a ast::TsType, direct: bool, out: &mut Vec<(&'a str, bool)>) {
    match ty {
        ast::TsType::TsTypeRef(reference) => {
            if let ast::TsEntityName::Ident(ident) = &reference.type_name {
                out.push((ident.sym.as_ref(), direct));
            }
            if let Some(arguments) = &reference.type_params {
                for argument in &arguments.params {
                    references(argument, false, out);
                }
            }
        }
        ast::TsType::TsParenthesizedType(inner) => references(&inner.type_ann, direct, out),
        ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(
            union,
        )) => {
            for member in &union.types {
                references(member, direct, out);
            }
        }
        ast::TsType::TsUnionOrIntersectionType(
            ast::TsUnionOrIntersectionType::TsIntersectionType(intersection),
        ) => {
            for member in &intersection.types {
                references(member, direct, out);
            }
        }
        ast::TsType::TsArrayType(array) => references(&array.elem_type, false, out),
        ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(function)) => {
            for parameter in &function.params {
                let annotation = match parameter {
                    ast::TsFnParam::Ident(binding) => binding.type_ann.as_deref(),
                    ast::TsFnParam::Rest(rest) => rest.type_ann.as_deref(),
                    ast::TsFnParam::Array(array) => array.type_ann.as_deref(),
                    ast::TsFnParam::Object(object) => object.type_ann.as_deref(),
                };
                if let Some(annotation) = annotation {
                    references(&annotation.type_ann, false, out);
                }
            }
            references(&function.type_ann.type_ann, false, out);
        }
        _ => {}
    }
}

impl<'p> Checker<'p> {
    /// The item that a scope binding names. A transparent alias names
    /// its resolved item; before it resolves, it names nothing usable.
    pub(crate) fn binding_item(&self, item: &ScopeItem) -> ScopeItem {
        let ScopeItem::SourceAlias(id) = item else {
            return item.clone();
        };
        match self.source_aliases.get(*id).map(|alias| &alias.state) {
            Some(State::Resolved(resolved)) => resolved.clone(),
            _ => ScopeItem::Poisoned,
        }
    }

    /// Resolves every transparent alias of the program (§202.1).
    pub(super) fn resolve_source_aliases(&mut self) {
        let saved = self.cur_file;
        self.subst.clear();
        for id in 0..self.source_aliases.len() {
            self.resolve_source_alias(id, &mut Vec::new());
        }
        self.cur_file = saved;
    }

    /// The aliased type of alias `id`, read from the program form that
    /// the namespace pass rewrote.
    fn source_alias_type(&self, id: usize) -> Option<&'p ast::TsType> {
        let prog = self.prog;
        let alias = &self.source_aliases[id];
        prog.files
            .get(alias.file)?
            .module
            .body
            .iter()
            .filter_map(module_decl)
            .find_map(|decl| match decl {
                ast::Decl::TsTypeAlias(declaration)
                    if declaration.id.sym.as_ref() == alias.name =>
                {
                    Some(&*declaration.type_ann)
                }
                _ => None,
            })
    }

    /// Resolves alias `id` and returns its item. `path` holds each
    /// alias whose resolution is in progress, with the directness of its
    /// reference to the next alias on the path.
    fn resolve_source_alias(&mut self, id: usize, path: &mut Vec<(usize, bool)>) -> ScopeItem {
        match &self.source_aliases[id].state {
            State::Resolved(item) => return item.clone(),
            State::Resolving => return ScopeItem::Poisoned,
            State::Pending => {}
        }
        self.source_aliases[id].state = State::Resolving;
        let file = self.source_aliases[id].file;
        let Some(aliased) = self.source_alias_type(id) else {
            self.source_aliases[id].state = State::Resolved(ScopeItem::Poisoned);
            return ScopeItem::Poisoned;
        };
        let mut names = Vec::new();
        references(aliased, true, &mut names);
        let mut failed = false;
        for (name, direct) in names {
            let Some(ScopeItem::SourceAlias(dependency)) = self.file_scopes[file]
                .get(name)
                .or_else(|| self.ambient_scope.get(name))
                .map(|binding| binding.item.clone())
            else {
                continue;
            };
            match &self.source_aliases[dependency].state {
                State::Resolved(ScopeItem::Poisoned) => failed = true,
                State::Resolved(_) => {}
                State::Resolving => {
                    self.report_alias_cycle(dependency, id, direct, path);
                    failed = true;
                }
                State::Pending => {
                    path.push((id, direct));
                    let item = self.resolve_source_alias(dependency, path);
                    path.pop();
                    failed |= matches!(item, ScopeItem::Poisoned);
                }
            }
        }
        let item = if failed {
            ScopeItem::Poisoned
        } else {
            self.cur_file = file;
            let before = self.diags.len();
            let ty = self.resolve_type(aliased);
            // §202.1 rule 2: the annotation's diagnostic is the only one.
            if self.diags.len() != before || matches!(self.apparent_type(&ty), Type::Error) {
                ScopeItem::Poisoned
            } else {
                ScopeItem::TypeAlias(ty)
            }
        };
        self.source_aliases[id].state = State::Resolved(item.clone());
        item
    }

    /// Reports the cycle that closes when alias `current` references
    /// alias `target` again (§202.1 rule 4). `tsc` rejects a cycle of
    /// direct references only.
    fn report_alias_cycle(
        &mut self,
        target: usize,
        current: usize,
        direct: bool,
        path: &[(usize, bool)],
    ) {
        if self.source_aliases[target].cycle_reported {
            return;
        }
        self.source_aliases[target].cycle_reported = true;
        let start = if target == current {
            path.len()
        } else {
            path.iter()
                .position(|(alias, _)| *alias == target)
                .unwrap_or(0)
        };
        let all_direct = direct && path[start..].iter().all(|(_, edge)| *edge);
        let alias = &self.source_aliases[target];
        let message = format!("type alias `{}` refers to itself", alias.name);
        let pos = alias.pos.clone();
        self.reject_subset(
            if all_direct {
                RejectionSite::SourceAliasCircular
            } else {
                RejectionSite::SourceAliasRecursive
            },
            message,
            pos,
        );
    }
}
