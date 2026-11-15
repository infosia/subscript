//! Site-selected examples retain the record and specialize the reason where needed (§311 rules 1 and 3).
use super::rejection::RejectionSite;
use crate::ambient::ApiRejectionId;
use crate::divergence::{Divergence, DivergenceEntry};

mod api;
mod declarations;
mod expressions;
mod forms;
mod related;

impl RejectionSite {
    pub(crate) fn example(self) -> Option<&'static DivergenceEntry> {
        forms::example(self)
            .or_else(|| related::example(self))
            .or_else(|| api::example(self))
            .or_else(|| expressions::example(self))
            .or_else(|| declarations::example(self))
    }
}
