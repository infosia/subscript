//! A rejection retains its site and message until diagnostic emission.
use super::RejectionSite;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RejectionFailure {
    pub(crate) site: RejectionSite,
    pub(crate) message: String,
}

impl RejectionFailure {
    pub(crate) fn new(site: RejectionSite, message: impl Into<String>) -> Self {
        Self {
            site,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RejectionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
