use subscript_compiler::hir;

/// Tracks which sites from one HIR operation a lowering consumed.
pub(crate) struct TrapSiteConsumer<'a> {
    sites: &'a [hir::TrapSite],
    consumed: Vec<bool>,
}

impl<'a> TrapSiteConsumer<'a> {
    fn new(sites: &'a [hir::TrapSite]) -> Self {
        Self {
            sites,
            consumed: vec![false; sites.len()],
        }
    }

    /// Marks and returns the first unconsumed site satisfying `predicate`.
    pub(crate) fn take(
        &mut self,
        mut predicate: impl FnMut(&hir::TrapSite) -> bool,
    ) -> Option<&'a hir::TrapSite> {
        let index = self
            .sites
            .iter()
            .zip(&self.consumed)
            .position(|(site, consumed)| !consumed && predicate(site))?;
        self.consumed[index] = true;
        Some(&self.sites[index])
    }

    /// Marks and returns a required site, using `missing` on absence.
    pub(crate) fn take_required(
        &mut self,
        predicate: impl FnMut(&hir::TrapSite) -> bool,
        missing: impl Into<String>,
    ) -> Result<&'a hir::TrapSite, String> {
        self.take(predicate).ok_or_else(|| missing.into())
    }

    fn finish(self, context: &str) -> Result<(), String> {
        let unused: Vec<String> = self
            .sites
            .iter()
            .zip(self.consumed)
            .filter_map(|(site, consumed)| {
                if consumed {
                    None
                } else {
                    Some(format!("{site:?}"))
                }
            })
            .collect();
        if unused.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{context} has unused HIR trap sites: {}",
                unused.join(", ")
            ))
        }
    }
}

/// Lowers one operation and rejects every site the consumer did not use.
pub(crate) fn lower_trap_sites<T>(
    sites: &[hir::TrapSite],
    context: &str,
    lower: impl FnOnce(&mut TrapSiteConsumer<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let mut consumer = TrapSiteConsumer::new(sites);
    let value = lower(&mut consumer)?;
    consumer.finish(context)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::Pos;

    #[test]
    fn added_site_of_an_existing_variant_fails_full_consumption() {
        let pos = Pos::new("probe.ts", 3, 7);
        let sites = vec![
            hir::TrapSite::DivisionByZero { pos: pos.clone() },
            hir::TrapSite::DivisionByZero { pos },
        ];
        let error = lower_trap_sites(&sites, "integer division", |sites| {
            sites.take_required(
                |site| matches!(site, hir::TrapSite::DivisionByZero { .. }),
                "integer division has no HIR trap site",
            )?;
            Ok(())
        })
        .expect_err("the added existing-variant site must not be dropped");
        eprintln!("{error}");
        assert!(
            error.starts_with("integer division has unused HIR trap sites:"),
            "{error}"
        );
    }
}
