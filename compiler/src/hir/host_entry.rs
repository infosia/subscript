use super::*;

/// The checked C boundary of one host entry.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HostSignature {
    /// Boundary parameter types, in call order.
    pub parameters: Vec<Type>,
    /// True for a zero-argument async entry.
    pub is_async: bool,
}

/// One public name resolved by the entry module's export graph.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HostEntry {
    /// Name visible to the host.
    pub name: String,
    /// Stable declaration symbol of the implementation.
    pub target: String,
    /// Checked boundary signature; the boundary always returns void.
    pub signature: HostSignature,
    /// Position of the exposing export.
    pub pos: Pos,
}

impl Module {
    /// Resolves the zero-argument host entry used by standard runners.
    ///
    /// # Errors
    /// Returns S100 at the entry module if main is absent or needs host arguments.
    pub fn runner_main(&self) -> Result<&HostEntry, crate::Diagnostic> {
        let entry = self.host_entries.iter().find(|entry| entry.name == "main");
        match entry {
            Some(entry) if entry.signature.parameters.is_empty() => Ok(entry),
            _ => Err(crate::Diagnostic::new(
                crate::RuleCode::S100,
                if entry.is_some() {
                    "runner main must have signature `main(): void`"
                } else {
                    "entry module exports no host entry `main`"
                },
                self.entry_pos.clone(),
            )),
        }
    }
}
