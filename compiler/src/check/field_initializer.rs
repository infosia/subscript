//! Context for reads of earlier initialized fields (§147).

use std::collections::HashSet;

use super::Shared;
use crate::types::Type;

#[derive(Debug, Clone)]
pub(super) struct FieldInitializer {
    pub class_type: Type,
    pub earlier: Shared<HashSet<String>>,
    pub definite_uninitialized: Shared<HashSet<String>>,
    pub write: bool,
}
