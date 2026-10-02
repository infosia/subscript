use super::*;

impl<'p> Checker<'p> {
    // ----- shared lookups -----

    /// Resolves a name used as a value at `pos` against the current
    /// file's top-level scope, then the global ambient scope (mirror
    /// declarations, §12.2). A type-only import binding reports S100 once
    /// per use site and resolves poisoned (compiler.md §134 rule 2).
    pub(crate) fn scope_item(&mut self, name: &str, pos: &Pos) -> Option<ScopeItem> {
        let type_only = self.scope_binding(name).is_some_and(|binding| {
            binding.type_only && !matches!(binding.item, ScopeItem::Poisoned)
        });
        if type_only
            && self
                .type_only_value_uses
                .insert((self.cur_file, pos.line, pos.col))
        {
            self.error(
                RuleCode::S100,
                format!(
                    "`{name}` cannot be used as a value because it was imported with `import type`"
                ),
                pos.clone(),
            );
        }
        if matches!(
            self.type_scope_item(name),
            Some(ScopeItem::Namespace { module: None, .. })
        ) {
            return Some(ScopeItem::Poisoned);
        }
        if matches!(
            self.type_scope_item(name),
            Some(ScopeItem::Namespace { .. })
        ) {
            self.error(
                RuleCode::S100,
                format!(
                    "namespace import `{name}` is a static qualifier and cannot be used as a value"
                ),
                pos.clone(),
            );
            return Some(ScopeItem::Poisoned);
        }
        self.peek_scope_item(name)
    }

    /// Resolves a name as a value without a use: an existence or shadow
    /// test. A type-only import binding resolves poisoned.
    pub(crate) fn peek_scope_item(&self, name: &str) -> Option<ScopeItem> {
        match self.scope_binding(name) {
            Some(binding) if binding.type_only => Some(ScopeItem::Poisoned),
            _ => self.type_scope_item(name),
        }
    }

    /// Resolves a name in a type position. A type-only import binding
    /// resolves to its declaration (compiler.md §134 rule 3).
    pub(crate) fn type_scope_item(&self, name: &str) -> Option<ScopeItem> {
        self.scope_binding(name)
            .map(|binding| binding.item.clone())
            .or_else(|| {
                self.type_aliases
                    .get(name)
                    .cloned()
                    .map(ScopeItem::TypeAlias)
            })
    }

    /// Resolves a top-level binding with its import status.
    pub(crate) fn scope_binding(&self, name: &str) -> Option<&ScopeBinding> {
        self.file_scopes
            .get(self.cur_file)
            .and_then(|scope| scope.get(name))
            .or_else(|| self.ambient_scope.get(name))
    }

    /// Looks a name up in the local scope stack. A hit that crosses a
    /// lambda boundary is a capture: it is recorded on every crossed
    /// lambda frame and must refer to a `const` binding (C5).
    pub(crate) fn lookup_local(&mut self, name: &str, pos: &Pos, fx: &mut FnCtx) -> Option<Local> {
        self.lookup_local_access(name, pos, fx, true)
    }

    /// Looks up a local assignment target without a read-before-declaration check.
    pub(crate) fn lookup_local_for_write(
        &mut self,
        name: &str,
        pos: &Pos,
        fx: &mut FnCtx,
    ) -> Option<Local> {
        self.lookup_local_access(name, pos, fx, false)
    }

    fn lookup_local_access(
        &mut self,
        name: &str,
        pos: &Pos,
        fx: &mut FnCtx,
        for_read: bool,
    ) -> Option<Local> {
        let mut crossed = 0usize;
        let mut found: Option<(usize, Local)> = None;
        for scope in fx.scopes.iter().rev() {
            let owns_name = scope.vars.contains_key(name)
                || scope.pending.contains(name)
                || scope.switch_declarations.contains_key(name);
            let scope_name = if scope.is_switch {
                "this switch body"
            } else {
                "this block"
            };
            if owns_name && !scope.duplicate_declarations.contains(name) {
                if let (Some(declaration_case), Some(current_case)) =
                    (scope.switch_declarations.get(name), scope.switch_case)
                {
                    if *declaration_case != current_case {
                        let message = if for_read {
                            format!("`{name}` is read from a different switch case")
                        } else {
                            format!("`{name}` is assigned in a case that does not declare it")
                        };
                        if for_read {
                            self.error(RuleCode::S100, message, pos.clone());
                        } else {
                            self.error_diverging(
                                RuleCode::S100,
                                message,
                                pos.clone(),
                                Divergence::DeclarationScope,
                            );
                        }
                        return Some(Local {
                            ty: Type::Error,
                            mutable: true,
                            async_origins: HashSet::new(),
                            caught: false,
                        });
                    }
                }
            }
            if owns_name && for_read && scope.pending.contains(name) {
                let message = format!("`{name}` is read before its declaration in {scope_name}");
                let shadows_program_item = matches!(
                    self.peek_scope_item(name),
                    Some(ScopeItem::Class(_) | ScopeItem::GenericClass(_) | ScopeItem::Func(_))
                );
                let ambient_namespace = matches!(
                    name,
                    "Math" | "Date" | "Number" | "JSON" | "Context" | "Promise"
                );
                if shadows_program_item || ambient_namespace {
                    self.error(RuleCode::S100, message, pos.clone());
                } else {
                    self.error_diverging(
                        RuleCode::S100,
                        message,
                        pos.clone(),
                        Divergence::DeclarationScope,
                    );
                }
                return Some(Local {
                    ty: Type::Error,
                    mutable: true,
                    async_origins: HashSet::new(),
                    caught: false,
                });
            }
            if let Some(local) = scope.vars.get(name) {
                found = Some((crossed, local.clone()));
                break;
            }
            if scope.pending.contains(name) {
                self.error(
                    RuleCode::S100,
                    format!("`{name}` is assigned before its declaration in {scope_name}"),
                    pos.clone(),
                );
                return Some(Local {
                    ty: Type::Error,
                    mutable: true,
                    async_origins: HashSet::new(),
                    caught: false,
                });
            }
            if scope.fn_boundary {
                crossed += 1;
            }
        }
        let (crossed, local) = found?;
        if crossed > 0 {
            if self.is_context_affine_type(&local.ty) {
                self.error(
                    RuleCode::S100,
                    format!(
                        "lambda captures Context-affine `{name}`; Worker, Inbox, and Outbox values may not be captured"
                    ),
                    pos.clone(),
                );
            }
            if local.mutable {
                self.error(
                    RuleCode::S009,
                    format!(
                        "lambda captures `{}`, which is not a `const` local; \
                         capturing lambdas may capture only const locals by value",
                        name
                    ),
                    pos.clone(),
                );
            }
            let mut remaining = crossed;
            for frame in fx.frames.iter_mut().rev() {
                if remaining == 0 {
                    break;
                }
                if frame.is_lambda {
                    if !frame.captures.iter().any(|capture| capture.name == name) {
                        frame.captures.push(hir::Capture {
                            name: name.to_string(),
                            ty: local.ty.clone(),
                        });
                    }
                    remaining -= 1;
                }
            }
        }
        Some(local)
    }
}
