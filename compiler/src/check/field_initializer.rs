//! Context for reads of earlier initialized fields (§147).

use std::collections::HashSet;

use crate::types::Type;

#[derive(Debug)]
pub(super) struct FieldInitializer {
    pub class_type: Type,
    pub earlier: HashSet<String>,
    pub write: bool,
}
