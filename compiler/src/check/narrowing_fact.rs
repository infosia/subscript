//! A path fact carries its shared-location class (compiler.md §161).

use std::borrow::Borrow;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

/// A live path fact. Its path identifies it at a flow join.
#[derive(Debug, Clone)]
pub(crate) struct NarrowingFact {
    pub key: String,
    pub shared: bool,
}

impl Deref for NarrowingFact {
    type Target = String;
    fn deref(&self) -> &String {
        &self.key
    }
}

impl Borrow<str> for NarrowingFact {
    fn borrow(&self) -> &str {
        &self.key
    }
}

impl Borrow<String> for NarrowingFact {
    fn borrow(&self) -> &String {
        &self.key
    }
}

impl PartialEq for NarrowingFact {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for NarrowingFact {}
impl PartialOrd for NarrowingFact {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NarrowingFact {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}
impl Hash for NarrowingFact {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}
impl PartialEq<String> for NarrowingFact {
    fn eq(&self, other: &String) -> bool {
        self.key == *other
    }
}
impl PartialEq<str> for NarrowingFact {
    fn eq(&self, other: &str) -> bool {
        self.key == other
    }
}

/// Checks the location class when a fact enters a set.
pub(super) trait FactSet {
    fn fact(&self, key: &str) -> Option<&NarrowingFact>;
    fn insert_fact(&mut self, fact: NarrowingFact) -> bool;
    fn replace_fact(&mut self, fact: NarrowingFact) -> Option<NarrowingFact>;

    fn assert_class(&self, fact: &NarrowingFact) {
        if let Some(existing) = self.fact(&fact.key) {
            debug_assert_eq!(existing.shared, fact.shared, "fact class: {}", fact.key);
        }
    }

    fn extend_facts(&mut self, facts: impl IntoIterator<Item = NarrowingFact>) {
        for fact in facts {
            self.insert_fact(fact);
        }
    }
}

impl FactSet for std::collections::BTreeSet<NarrowingFact> {
    fn fact(&self, key: &str) -> Option<&NarrowingFact> {
        self.get(key)
    }

    fn insert_fact(&mut self, fact: NarrowingFact) -> bool {
        self.assert_class(&fact);
        self.insert(fact)
    }

    fn replace_fact(&mut self, fact: NarrowingFact) -> Option<NarrowingFact> {
        self.assert_class(&fact);
        self.replace(fact)
    }
}

impl FactSet for std::collections::HashSet<NarrowingFact> {
    fn fact(&self, key: &str) -> Option<&NarrowingFact> {
        self.get(key)
    }

    fn insert_fact(&mut self, fact: NarrowingFact) -> bool {
        self.assert_class(&fact);
        self.insert(fact)
    }

    fn replace_fact(&mut self, fact: NarrowingFact) -> Option<NarrowingFact> {
        self.assert_class(&fact);
        self.replace(fact)
    }
}
