//! Mutation journals for bounded loop retries (§315 rule 4, §161 rule 1).
use std::borrow::Borrow;
use std::collections::{hash_map, HashMap};
use std::hash::Hash;
use std::ops::{Deref, Index, IndexMut};

/// A map records only the entries that a speculative check can change.
pub(crate) struct Map<K, V> {
    values: HashMap<K, V>,
    undo: Vec<(K, Option<V>)>,
    marks: Vec<usize>,
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            undo: Vec::new(),
            marks: Vec::new(),
        }
    }
}

impl<K, V> From<HashMap<K, V>> for Map<K, V> {
    fn from(values: HashMap<K, V>) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }
}

impl<K, V> Deref for Map<K, V> {
    type Target = HashMap<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<K: Clone + Eq + Hash, V: Clone> Map<K, V> {
    pub(super) fn begin(&mut self) {
        self.marks.push(self.undo.len());
    }

    pub(super) fn commit(&mut self) {
        self.marks.pop();
        if self.marks.is_empty() {
            self.undo.clear();
        }
    }

    pub(super) fn rollback(&mut self) {
        let Some(mark) = self.marks.pop() else {
            return;
        };
        while self.undo.len() > mark {
            if let Some((key, value)) = self.undo.pop() {
                if let Some(value) = value {
                    self.values.insert(key, value);
                } else {
                    self.values.remove(&key);
                }
            }
        }
    }

    fn record(&mut self, key: &K) {
        if !self.marks.is_empty() {
            self.undo.push((key.clone(), self.values.get(key).cloned()));
        }
    }

    pub(super) fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.record(&key);
        self.values.insert(key, value)
    }

    pub(super) fn get_mut<Q: ?Sized + Eq + Hash>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        if let Some((key, _)) = self.values.get_key_value(key) {
            let key = key.clone();
            self.record(&key);
        }
        self.values.get_mut(key)
    }

    pub(super) fn remove<Q: ?Sized + Eq + Hash>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        if let Some((key, _)) = self.values.get_key_value(key) {
            let key = key.clone();
            self.record(&key);
        }
        self.values.remove(key)
    }

    pub(super) fn entry(&mut self, key: K) -> hash_map::Entry<'_, K, V> {
        self.record(&key);
        self.values.entry(key)
    }

    pub(super) fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.values.retain(|key, value| {
            let retained = keep(key, value);
            if !retained && !self.marks.is_empty() {
                self.undo.push((key.clone(), Some(value.clone())));
            }
            retained
        });
    }

    /// The opaque check takes a separate module snapshot (§143).
    pub(super) fn snapshot(&self) -> HashMap<K, V> {
        self.values.clone()
    }
}

impl<'a, K, V> IntoIterator for &'a Map<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = hash_map::Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

impl<K, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = hash_map::IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl<K: Eq + Hash, V> FromIterator<(K, V)> for Map<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        HashMap::from_iter(iter).into()
    }
}

enum Change<T> {
    Pop,
    Insert(usize, T),
    Set(usize, T),
}

/// A vector records appends and the individual records that a check changes.
pub(crate) struct Vector<T> {
    values: Vec<T>,
    undo: Vec<Change<T>>,
    marks: Vec<(usize, usize)>,
}

impl<T> Default for Vector<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            undo: Vec::new(),
            marks: Vec::new(),
        }
    }
}

impl<T> Vector<T> {
    pub(super) fn into_inner(self) -> Vec<T> {
        self.values
    }
}

impl<T> From<Vec<T>> for Vector<T> {
    fn from(values: Vec<T>) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }
}

impl<T> Deref for Vector<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T: Clone> Vector<T> {
    pub(super) fn begin(&mut self) {
        self.marks.push((self.undo.len(), self.values.len()));
    }
    pub(super) fn transaction_len(&self) -> usize {
        self.marks.last().map_or(self.values.len(), |(_, len)| *len)
    }
    pub(super) fn commit(&mut self) {
        self.marks.pop();
        if self.marks.is_empty() {
            self.undo.clear();
        }
    }
    pub(super) fn rollback(&mut self) {
        let Some((mark, _)) = self.marks.pop() else {
            return;
        };
        while self.undo.len() > mark {
            if let Some(change) = self.undo.pop() {
                match change {
                    Change::Pop => {
                        self.values.pop();
                    }
                    Change::Insert(index, value) => {
                        self.values.insert(index, value);
                    }
                    Change::Set(index, value) => {
                        if let Some(slot) = self.values.get_mut(index) {
                            *slot = value;
                        }
                    }
                }
            }
        }
    }
    pub(super) fn push(&mut self, value: T) {
        if !self.marks.is_empty() {
            self.undo.push(Change::Pop);
        }
        self.values.push(value);
    }
    pub(super) fn pop(&mut self) -> Option<T> {
        let value = self.values.pop()?;
        if !self.marks.is_empty() {
            self.undo
                .push(Change::Insert(self.values.len(), value.clone()));
        }
        Some(value)
    }
    pub(super) fn remove(&mut self, index: usize) -> T {
        let value = self.values.remove(index);
        if !self.marks.is_empty() {
            self.undo.push(Change::Insert(index, value.clone()));
        }
        value
    }
    pub(super) fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if !self.marks.is_empty() {
            if let Some(value) = self.values.get(index) {
                self.undo.push(Change::Set(index, value.clone()));
            }
        }
        self.values.get_mut(index)
    }
    pub(super) fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        if !self.marks.is_empty() {
            self.undo.extend(
                self.values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| Change::Set(index, value.clone())),
            );
        }
        self.values.iter_mut()
    }
    pub(super) fn truncate(&mut self, len: usize) {
        while self.values.len() > len {
            self.pop();
        }
    }
    pub(super) fn append(&mut self, other: &mut Vec<T>) {
        for value in other.drain(..) {
            self.push(value);
        }
    }
    pub(super) fn take(&mut self) -> Vec<T> {
        if self.marks.is_empty() {
            return std::mem::take(&mut self.values);
        }
        let values = self.values.clone();
        self.truncate(0);
        values
    }
    pub(super) fn replace(&mut self, values: Vec<T>) -> Vec<T> {
        let before = self.take();
        for value in values {
            self.push(value);
        }
        before
    }
    pub(super) fn snapshot(&self) -> Vec<T> {
        self.values.clone()
    }
}

impl<T, I: std::slice::SliceIndex<[T]>> Index<I> for Vector<T> {
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        &self.values[index]
    }
}

impl<T: Clone> IndexMut<usize> for Vector<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        if !self.marks.is_empty() {
            if let Some(value) = self.values.get(index) {
                self.undo.push(Change::Set(index, value.clone()));
            }
        }
        &mut self.values[index]
    }
}

impl<'a, T> IntoIterator for &'a Vector<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

impl<T> IntoIterator for Vector<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

/// A set journals its inserted and removed keys.
pub(crate) struct Set<K>(Map<K, ()>);
impl<K> Default for Set<K> {
    fn default() -> Self {
        Self(Map::default())
    }
}
impl<K: Clone + Hash + Eq> Set<K> {
    pub(super) fn begin(&mut self) {
        self.0.begin();
    }
    pub(super) fn commit(&mut self) {
        self.0.commit();
    }
    pub(super) fn rollback(&mut self) {
        self.0.rollback();
    }
    pub(super) fn remove<Q: ?Sized + Eq + Hash>(&mut self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.0.remove(key).is_some()
    }
    pub(super) fn extend(&mut self, keys: impl IntoIterator<Item = K>) {
        for key in keys {
            self.insert(key);
        }
    }
    pub(super) fn insert(&mut self, key: K) -> bool {
        self.0.insert(key, ()).is_none()
    }
    pub(super) fn contains<Q: ?Sized + Eq + Hash>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.0.contains_key(key)
    }
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc, time::Instant};

    struct Counted(Rc<Cell<usize>>);
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Self(self.0.clone())
        }
    }

    /// Measured cost: 0.0003 seconds; no compiler or engine subprocess.
    #[test]
    fn a_checkpoint_copies_only_changed_records() {
        let started = Instant::now();
        for size in [1, 1000] {
            let copies = Rc::new(Cell::new(0));
            let mut map: Map<usize, Counted> = (0..size)
                .map(|key| (key, Counted(copies.clone())))
                .collect();
            let mut vector: Vector<Counted> = (0..size)
                .map(|_| Counted(copies.clone()))
                .collect::<Vec<_>>()
                .into();
            map.begin();
            vector.begin();
            assert_eq!(copies.get(), 0);
            map.insert(size, Counted(copies.clone()));
            vector.push(Counted(copies.clone()));
            assert_eq!(copies.get(), 0);
            map.get_mut(&0);
            vector.get_mut(0);
            assert_eq!(copies.get(), 2);
            map.commit();
            vector.commit();
            assert_eq!(copies.get(), 2);
        }
        eprintln!("s315 journal copy cost: {:?}", started.elapsed());
    }

    /// Measured cost: 0.00003 seconds; no compiler or engine subprocess.
    #[test]
    fn nested_commit_and_rollback_restore_maps_and_vectors() {
        let started = Instant::now();
        let mut map: Map<i32, Vec<i32>> = HashMap::from([(1, vec![1]), (2, vec![2])]).into();
        let mut vector: Vector<i32> = vec![1, 2, 3].into();
        map.begin();
        vector.begin();
        map.entry(1).or_default().push(4);
        map.remove(&2);
        vector[0] = 4;
        vector.remove(1);
        map.begin();
        vector.begin();
        map.insert(3, vec![3]);
        vector.push(5);
        map.commit();
        vector.commit();
        map.rollback();
        vector.rollback();
        assert_eq!(map.snapshot(), HashMap::from([(1, vec![1]), (2, vec![2])]));
        assert_eq!(&*vector, &[1, 2, 3]);
        map.begin();
        vector.begin();
        for key in [1, 2] {
            map.get_mut(&key).unwrap().push(0);
        }
        map.retain(|_, _| false);
        assert_eq!(vector.take(), vec![1, 2, 3]);
        vector.replace(vec![5, 6]);
        map.rollback();
        vector.rollback();
        assert_eq!(map.snapshot(), HashMap::from([(1, vec![1]), (2, vec![2])]));
        assert_eq!(&*vector, &[1, 2, 3]);
        eprintln!("s315 nested journal cost: {:?}", started.elapsed());
    }
}
