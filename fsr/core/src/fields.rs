//! What an object holds: its fields in insertion order, inline in one vector. A small object is scanned, which beats hashing at the sizes objects have. A big one keeps an index so it stays linear to build.

use crate::value::Value;

pub type Key = smol_str::SmolStr;

const INDEXED_PAST: usize = 8;

/// How many fields an object that outgrows what it was built with has room for at once.
const GROWN: usize = 8;

#[derive(Clone, Default)]
pub struct Fields {
  entries: Vec<(Key, Value)>,
  index: Option<Box<Index>>,
}

#[derive(Clone, Default)]
struct Index {
  at: hashbrown::HashTable<u32>,
  hasher: crate::value::ValueHasher,
}

impl Index {
  fn hash(&self, key: &str) -> u64 {
    std::hash::BuildHasher::hash_one(&self.hasher, key)
  }

  fn find(&self, entries: &[(Key, Value)], key: &str) -> Option<usize> {
    self.at.find(self.hash(key), |at| entries[*at as usize].0.as_str() == key).map(|at| *at as usize)
  }

  fn push(&mut self, entries: &[(Key, Value)], at: usize) {
    let hash = self.hash(entries[at].0.as_str());
    let hasher = &self.hasher;
    self.at.insert_unique(hash, at as u32, |at| std::hash::BuildHasher::hash_one(hasher, entries[*at as usize].0.as_str()));
  }
}

impl Fields {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn with_capacity(capacity: usize) -> Self {
    Self { entries: Vec::with_capacity(capacity), index: None }
  }

  pub fn with_capacity_and_hasher<H>(capacity: usize, _hasher: H) -> Self {
    Self::with_capacity(capacity)
  }

  /// These fields copied with room for `room` more without growing.
  pub fn copy_with_room(&self, room: usize) -> Fields {
    let mut entries = Vec::with_capacity(self.entries.len() + room);
    entries.extend(self.entries.iter().cloned());
    Fields { entries, index: self.index.clone() }
  }

  pub fn reserve(&mut self, more: usize) {
    self.entries.reserve(more);
  }

  pub fn len(&self) -> usize {
    self.entries.len()
  }

  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  #[inline]
  fn position(&self, key: &str) -> Option<usize> {
    match &self.index {
      Some(index) => index.find(&self.entries, key),
      None => self.entries.iter().position(|(k, _)| k.as_str() == key),
    }
  }

  #[inline]
  pub fn get<Q: AsRef<str> + ?Sized>(&self, key: &Q) -> Option<&Value> {
    self.position(key.as_ref()).map(|at| &self.entries[at].1)
  }

  pub fn get_mut<Q: AsRef<str> + ?Sized>(&mut self, key: &Q) -> Option<&mut Value> {
    let at = self.position(key.as_ref())?;
    Some(&mut self.entries[at].1)
  }

  pub fn get_key_value<Q: AsRef<str> + ?Sized>(&self, key: &Q) -> Option<(&Key, &Value)> {
    self.position(key.as_ref()).map(|at| (&self.entries[at].0, &self.entries[at].1))
  }

  pub fn get_index(&self, at: usize) -> Option<(&Key, &Value)> {
    self.entries.get(at).map(|(k, v)| (k, v))
  }

  pub fn contains_key<Q: AsRef<str> + ?Sized>(&self, key: &Q) -> bool {
    self.position(key.as_ref()).is_some()
  }

  pub fn insert(&mut self, key: impl Into<Key>, value: Value) -> Option<Value> {
    let key = key.into();
    if let Some(at) = self.position(key.as_str()) {
      return Some(std::mem::replace(&mut self.entries[at].1, value));
    }
    self.push_new(key, value);
    None
  }

  fn push_new(&mut self, key: Key, value: Value) {
    // An object written past what it was built with is one that keeps growing: its first growth makes room for eight fields at once.
    if self.entries.len() == self.entries.capacity() && self.entries.len() < GROWN {
      self.entries.reserve_exact(GROWN - self.entries.len());
    }
    self.entries.push((key, value));
    match &mut self.index {
      Some(index) => index.push(&self.entries, self.entries.len() - 1),
      None if self.entries.len() > INDEXED_PAST => self.reindex(),
      None => {}
    }
  }

  fn reindex(&mut self) {
    if self.entries.len() <= INDEXED_PAST {
      self.index = None;
      return;
    }
    let mut index = self.index.take().unwrap_or_default();
    index.at.clear();
    index.at.reserve(self.entries.len(), |_| 0);
    for at in 0..self.entries.len() {
      index.push(&self.entries, at);
    }
    self.index = Some(index);
  }

  pub fn shift_remove<Q: AsRef<str> + ?Sized>(&mut self, key: &Q) -> Option<Value> {
    let at = self.position(key.as_ref())?;
    let (_, value) = self.entries.remove(at);
    self.reindex();
    Some(value)
  }

  pub fn swap_remove<Q: AsRef<str> + ?Sized>(&mut self, key: &Q) -> Option<Value> {
    let at = self.position(key.as_ref())?;
    let (_, value) = self.entries.swap_remove(at);
    self.reindex();
    Some(value)
  }

  pub fn remove<Q: AsRef<str> + ?Sized>(&mut self, key: &Q) -> Option<Value> {
    self.shift_remove(key)
  }

  pub fn clear(&mut self) {
    self.entries.clear();
    self.index = None;
  }

  pub fn retain(&mut self, mut keep: impl FnMut(&Key, &mut Value) -> bool) {
    self.entries.retain_mut(|(k, v)| keep(k, v));
    self.reindex();
  }

  pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&Key, &Value)> + ExactSizeIterator + Clone {
    self.entries.iter().map(|(k, v)| (k, v))
  }

  pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = (&Key, &mut Value)> + ExactSizeIterator {
    self.entries.iter_mut().map(|(k, v)| (&*k, v))
  }

  pub fn keys(&self) -> impl DoubleEndedIterator<Item = &Key> + ExactSizeIterator + Clone {
    self.entries.iter().map(|(k, _)| k)
  }

  pub fn values(&self) -> impl DoubleEndedIterator<Item = &Value> + ExactSizeIterator + Clone {
    self.entries.iter().map(|(_, v)| v)
  }

  pub fn values_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Value> + ExactSizeIterator {
    self.entries.iter_mut().map(|(_, v)| v)
  }

  pub fn into_keys(self) -> impl DoubleEndedIterator<Item = Key> + ExactSizeIterator {
    self.entries.into_iter().map(|(k, _)| k)
  }

  pub fn into_values(self) -> impl DoubleEndedIterator<Item = Value> + ExactSizeIterator {
    self.entries.into_iter().map(|(_, v)| v)
  }

  pub fn entry(&mut self, key: impl Into<Key>) -> Entry<'_> {
    Entry { fields: self, key: key.into() }
  }

  pub fn sort_keys(&mut self) {
    self.entries.sort_by(|a, b| a.0.cmp(&b.0));
    self.reindex();
  }
}

pub struct Entry<'a> {
  fields: &'a mut Fields,
  key: Key,
}

impl<'a> Entry<'a> {
  pub fn or_insert(self, value: Value) -> &'a mut Value {
    self.or_insert_with(|| value)
  }

  pub fn or_insert_with(self, make: impl FnOnce() -> Value) -> &'a mut Value {
    let at = match self.fields.position(self.key.as_str()) {
      Some(at) => at,
      None => {
        let at = self.fields.entries.len();
        self.fields.push_new(self.key, make());
        at
      }
    };
    &mut self.fields.entries[at].1
  }

  pub fn or_default(self) -> &'a mut Value {
    self.or_insert_with(|| Value::Null)
  }
}

impl std::fmt::Debug for Fields {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_map().entries(self.entries.iter().map(|(k, v)| (k, v))).finish()
  }
}

/// Fields compare whatever their order, as an object's do.
impl PartialEq for Fields {
  fn eq(&self, other: &Self) -> bool {
    self.len() == other.len() && self.entries.iter().all(|(k, v)| other.get(k.as_str()) == Some(v))
  }
}

impl<K: Into<Key>> FromIterator<(K, Value)> for Fields {
  fn from_iter<T: IntoIterator<Item = (K, Value)>>(iter: T) -> Self {
    let iter = iter.into_iter();
    let mut out = Fields::with_capacity(iter.size_hint().0);
    for (k, v) in iter {
      out.insert(k, v);
    }
    out
  }
}

impl<K: Into<Key>> Extend<(K, Value)> for Fields {
  fn extend<T: IntoIterator<Item = (K, Value)>>(&mut self, iter: T) {
    for (k, v) in iter {
      self.insert(k, v);
    }
  }
}

impl IntoIterator for Fields {
  type Item = (Key, Value);
  type IntoIter = std::vec::IntoIter<(Key, Value)>;
  fn into_iter(self) -> Self::IntoIter {
    self.entries.into_iter()
  }
}

impl<'a> IntoIterator for &'a Fields {
  type Item = (&'a Key, &'a Value);
  type IntoIter = std::iter::Map<std::slice::Iter<'a, (Key, Value)>, fn(&'a (Key, Value)) -> (&'a Key, &'a Value)>;
  fn into_iter(self) -> Self::IntoIter {
    self.entries.iter().map(|(k, v)| (k, v))
  }
}

impl std::ops::Index<&str> for Fields {
  type Output = Value;
  fn index(&self, key: &str) -> &Value {
    self.get(key).expect("no field of that name")
  }
}
