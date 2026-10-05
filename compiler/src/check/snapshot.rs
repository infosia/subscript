//! Shared lexical facts for initializer decisions and deferred expressions.
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// A shared snapshot that copies its payload on the first write.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shared<T>(Rc<T>);

impl<T: Clone> Shared<T> {
    pub(super) fn into_inner(self) -> T {
        Rc::unwrap_or_clone(self.0)
    }
}

impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self(Rc::new(value))
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone> DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        Rc::make_mut(&mut self.0)
    }
}

impl<A, T: FromIterator<A>> FromIterator<A> for Shared<T> {
    fn from_iter<I: IntoIterator<Item = A>>(values: I) -> Self {
        T::from_iter(values).into()
    }
}

impl<T: Clone + IntoIterator> IntoIterator for Shared<T> {
    type Item = T::Item;
    type IntoIter = T::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.into_inner().into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Shared<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (**self).into_iter()
    }
}

#[test]
fn a_snapshot_keeps_lexical_facts_after_the_source_changes() {
    let mut source: Shared<std::collections::HashMap<&str, i32>> =
        [("value", 1)].into_iter().collect();
    let mut snapshot = source.clone();
    assert!(Rc::ptr_eq(&source.0, &snapshot.0));
    source.insert("value", 2);
    snapshot.insert("later", 3);
    assert_eq!(source.get("value"), Some(&2));
    assert_eq!(snapshot.get("value"), Some(&1));
    assert!(!source.contains_key("later"));
}
